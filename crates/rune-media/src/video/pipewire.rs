//! Reads one video node of a portal's PipeWire remote into the preview
//! queue. Runs on its own thread until the stop flag, an error or the
//! source going away; the stream, core and remote are released on return.

use std::cell::{Cell, RefCell};
use std::os::fd::OwnedFd;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pipewire as pw;
use pw::spa;
use pw::spa::param::format::MediaSubtype;
use pw::spa::param::video::VideoFormat;
use pw::stream::{StreamFlags, StreamState};
use rune_i18n::{es, t};

use super::{
    convert_within, lock, CameraInfo, Pace, Pixels, Plane, Shared, VideoKind, PREVIEW_INTERVAL,
};

/// How often the stop flag and the deadlines are checked.
const CHECK: Duration = Duration::from_millis(50);
/// A source that has not sent a frame by then is not going to.
const FIRST_FRAME: Duration = Duration::from_secs(15);
/// A camera sends frames all the time; a screen only when it changes.
const CAMERA_STALL: Duration = Duration::from_secs(5);

const GONE: &str = es("La fuente dejó de estar disponible.");

/// The format negotiated for the stream.
#[derive(Clone, Copy)]
struct Format {
    pixels: Pixels,
    width: u32,
    height: u32,
    /// Each buffer is one JPEG picture: a camera that only gives MJPEG.
    jpeg: bool,
}

#[derive(Default)]
struct Reader {
    format: Option<Format>,
    /// Which frames to keep, and the last one previewed while sending.
    pace: Pace,
    previewed: Option<Instant>,
    connected: bool,
}

pub(super) fn read(
    remote: OwnedFd,
    node: Option<u32>,
    kind: VideoKind,
    device: Option<String>,
    shared: &Arc<Shared>,
) -> Result<(), String> {
    let unavailable = |_| t("No se pudo conectar con PipeWire.").to_owned();
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(unavailable)?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(unavailable)?;
    let core = context.connect_fd_rc(remote, None).map_err(unavailable)?;
    let failure: Rc<RefCell<Option<String>>> = Rc::default();
    let fail = {
        let (mainloop, failure) = (mainloop.clone(), failure.clone());
        move |reason: String| {
            failure.borrow_mut().get_or_insert(reason);
            mainloop.quit();
        }
    };

    let _core = core
        .add_listener_local()
        .error({
            let fail = fail.clone();
            move |id, _, _, message| {
                if id == pw::core::PW_ID_CORE {
                    fail(format!("PipeWire: {message}"));
                }
            }
        })
        .register();

    // The cameras the remote shows, for the device choice; and the source
    // read going away (unplugged, or a window closed).
    let registry = core.get_registry_rc().map_err(unavailable)?;
    let cameras: Rc<RefCell<Vec<(u32, CameraInfo)>>> = Rc::default();
    let chosen: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(node));
    let _registry = registry
        .add_listener_local()
        .global({
            let (shared, cameras, chosen) = (shared.clone(), cameras.clone(), chosen.clone());
            let device = device.clone();
            move |global| {
                if kind != VideoKind::Camera || global.type_ != pw::types::ObjectType::Node {
                    return;
                }
                let Some(props) = global.props else { return };
                if props.get("media.class") != Some("Video/Source") {
                    return;
                }
                let Some(id) = props.get("node.name").filter(|name| name.len() <= 256) else {
                    return;
                };
                let name = props
                    .get("node.description")
                    .or_else(|| props.get("node.nick"))
                    .unwrap_or(id)
                    .chars()
                    .take(128)
                    .collect();
                if device.as_deref() == Some(id) {
                    chosen.set(Some(global.id));
                }
                let mut cameras = cameras.borrow_mut();
                cameras.push((
                    global.id,
                    CameraInfo {
                        id: id.to_owned(),
                        name,
                    },
                ));
                *super::lock(&shared.cameras) =
                    cameras.iter().map(|(_, camera)| camera.clone()).collect();
            }
        })
        .global_remove({
            let (shared, cameras, chosen) = (shared.clone(), cameras.clone(), chosen.clone());
            let fail = fail.clone();
            move |id| {
                let mut cameras = cameras.borrow_mut();
                let before = cameras.len();
                cameras.retain(|(node, _)| *node != id);
                if cameras.len() != before {
                    *super::lock(&shared.cameras) =
                        cameras.iter().map(|(_, camera)| camera.clone()).collect();
                }
                let last_camera = kind == VideoKind::Camera && before > 0 && cameras.is_empty();
                if chosen.get() == Some(id) || last_camera {
                    fail(t(GONE).to_owned());
                }
            }
        })
        .register();

    let mut properties = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Video",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => match kind {
            VideoKind::Screen => "Screen",
            VideoKind::Camera => "Camera",
        },
    };
    if let (VideoKind::Camera, Some(device)) = (kind, &device) {
        properties.insert(*pw::keys::TARGET_OBJECT, device.as_str());
    }
    let stream =
        pw::stream::StreamBox::new(&core, "rune-preview", properties).map_err(unavailable)?;
    let last_frame = Rc::new(Cell::new(None::<Instant>));
    let _stream = stream
        .add_local_listener_with_user_data(Reader::default())
        .state_changed({
            let fail = fail.clone();
            move |_, reader, _, state| match state {
                StreamState::Error(message) => fail(format!("PipeWire: {message}")),
                StreamState::Unconnected if reader.connected => fail(t(GONE).to_owned()),
                StreamState::Paused | StreamState::Streaming => reader.connected = true,
                _ => {}
            }
        })
        .param_changed({
            let shared = shared.clone();
            move |_, reader, id, param| {
                if id != spa::param::ParamType::Format.as_raw() {
                    return;
                }
                reader.format = param.and_then(|param| {
                    let (_, subtype) = spa::param::format_utils::parse_format(param).ok()?;
                    let jpeg = subtype == MediaSubtype::Mjpg;
                    // Its size and rate are read the same way for MJPEG.
                    let mut info = spa::param::video::VideoInfoRaw::new();
                    info.parse(param).ok()?;
                    let (pixels, name) = match info.format() {
                        _ if jpeg => (Pixels::Rgb4, "MJPG"),
                        VideoFormat::BGRx => (Pixels::Bgr4, "BGRx"),
                        VideoFormat::BGRA => (Pixels::Bgr4, "BGRA"),
                        VideoFormat::RGBx => (Pixels::Rgb4, "RGBx"),
                        VideoFormat::RGBA => (Pixels::Rgb4, "RGBA"),
                        VideoFormat::YUY2 => (Pixels::Yuy2, "YUY2"),
                        VideoFormat::NV12 => (Pixels::Nv12, "NV12"),
                        _ => return None,
                    };
                    let size = info.size();
                    let fraction = |rate: spa::utils::Fraction| (rate.num, rate.denom);
                    let mut stats = lock(&shared.stats);
                    stats.size = (size.width, size.height);
                    stats.format = name;
                    stats.rate = fraction(info.framerate());
                    stats.max_rate = fraction(info.max_framerate());
                    Some(Format {
                        pixels,
                        width: size.width,
                        height: size.height,
                        jpeg,
                    })
                });
            }
        })
        .process({
            let (shared, last_frame) = (shared.clone(), last_frame.clone());
            move |stream, reader| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let now = Instant::now();
                last_frame.set(Some(now));
                let (max, interval) = shared.capture();
                let since = |at: Option<Instant>| at.map(|at| now.saturating_duration_since(at));
                let memory = buffer.datas_mut().first().map(|data| data.type_());
                {
                    let mut stats = lock(&shared.stats);
                    stats.delivered += 1;
                    stats.memory = match memory {
                        Some(spa::buffer::DataType::DmaBuf) => "dmabuf",
                        Some(spa::buffer::DataType::MemFd) => "memfd",
                        Some(spa::buffer::DataType::MemPtr) => "memptr",
                        _ => "other",
                    };
                }
                // Dropping the buffer queues it back: skipped frames cost nothing.
                if !reader.pace.keep(now, interval) {
                    return;
                }
                let Some(format) = reader.format else { return };
                let Some(frame) = frame(format, buffer.datas_mut(), max) else {
                    return;
                };
                {
                    let mut stats = lock(&shared.stats);
                    stats.kept += 1;
                    stats.converting_micros += now.elapsed().as_micros() as u64;
                }
                if shared.sending.is_none() {
                    return shared.push(frame);
                }
                if since(reader.previewed).is_none_or(|since| since >= PREVIEW_INTERVAL) {
                    reader.previewed = Some(now);
                    shared.push(frame.clone());
                }
                shared.keep(frame);
            }
        })
        .register()
        .map_err(unavailable)?;

    let formats = match kind {
        VideoKind::Screen => vec![
            VideoFormat::BGRx,
            VideoFormat::BGRx,
            VideoFormat::BGRA,
            VideoFormat::RGBx,
            VideoFormat::RGBA,
        ],
        VideoKind::Camera => vec![
            VideoFormat::YUY2,
            VideoFormat::YUY2,
            VideoFormat::NV12,
            VideoFormat::BGRx,
            VideoFormat::RGBx,
        ],
    };
    // Asked for at least at the rate kept (30 as before, 60 for 60 fps): a
    // source that settles lower could only be sent slower.
    let fps = (1.0 / shared.capture().1.as_secs_f64())
        .round()
        .clamp(30.0, 60.0) as u32;
    let raw = format_pod(MediaSubtype::Raw, formats, fps)
        .map_err(|_| unavailable(pw::Error::CreationFailed))?;
    // A camera that gives only MJPEG is read too, after what it gives raw.
    let mjpg = format_pod(MediaSubtype::Mjpg, Vec::new(), fps)
        .map_err(|_| unavailable(pw::Error::CreationFailed))?;
    let pods = match kind {
        VideoKind::Screen => vec![raw],
        VideoKind::Camera => vec![raw, mjpg],
    };
    let mut params = pods
        .iter()
        .map(|pod| spa::pod::Pod::from_bytes(pod))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| unavailable(pw::Error::CreationFailed))?;
    stream
        .connect(
            spa::utils::Direction::Input,
            node,
            StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .map_err(unavailable)?;

    let started = Instant::now();
    let timer = mainloop.loop_().add_timer({
        let (mainloop, shared, fail) = (mainloop.clone(), shared.clone(), fail.clone());
        move |_| {
            let now = Instant::now();
            if shared.stopped() {
                mainloop.quit();
            } else if !shared.running() && now.duration_since(started) > FIRST_FRAME {
                fail(t("La fuente no envió imágenes.").to_owned());
            } else if kind == VideoKind::Camera
                && last_frame
                    .get()
                    .is_some_and(|last| now.duration_since(last) > CAMERA_STALL)
            {
                fail(t("La cámara dejó de enviar imágenes.").to_owned());
            }
        }
    });
    let _ = timer.update_timer(Some(CHECK), Some(CHECK));
    mainloop.run();
    let _ = stream.disconnect();
    let failure = failure.borrow_mut().take();
    match failure {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

/// What the stream accepts: `subtype` pictures (raw ones in `formats`, the
/// first preferred, as SPA choices repeat their default), any size and rate
/// the source has, `fps` preferred.
fn format_pod(
    subtype: MediaSubtype,
    formats: Vec<VideoFormat>,
    fps: u32,
) -> Result<Vec<u8>, spa::pod::serialize::GenError> {
    use spa::param::format::{FormatProperties, MediaType};
    use spa::pod::{ChoiceValue, Property, PropertyFlags, Value};
    use spa::utils::{Choice, ChoiceEnum, ChoiceFlags, Fraction, Id, Rectangle, SpaTypes};
    let id = |key: FormatProperties, value: u32| Property {
        key: key.as_raw(),
        flags: PropertyFlags::empty(),
        value: Value::Id(Id(value)),
    };
    let mut properties = vec![
        id(FormatProperties::MediaType, MediaType::Video.as_raw()),
        id(FormatProperties::MediaSubtype, subtype.as_raw()),
    ];
    let mut formats = formats.into_iter().map(|format| Id(format.as_raw()));
    if let Some(default) = formats.next() {
        properties.push(Property {
            key: FormatProperties::VideoFormat.as_raw(),
            flags: PropertyFlags::empty(),
            value: Value::Choice(ChoiceValue::Id(Choice(
                ChoiceFlags::empty(),
                ChoiceEnum::Enum {
                    default,
                    alternatives: formats.collect(),
                },
            ))),
        });
    }
    properties.extend([
        Property {
            key: FormatProperties::VideoSize.as_raw(),
            flags: PropertyFlags::empty(),
            value: Value::Choice(ChoiceValue::Rectangle(Choice(
                ChoiceFlags::empty(),
                ChoiceEnum::Range {
                    default: Rectangle {
                        width: 1280,
                        height: 720,
                    },
                    min: Rectangle {
                        width: 1,
                        height: 1,
                    },
                    max: Rectangle {
                        width: super::MAX_SOURCE_SIDE,
                        height: super::MAX_SOURCE_SIDE,
                    },
                },
            ))),
        },
        Property {
            key: FormatProperties::VideoFramerate.as_raw(),
            flags: PropertyFlags::empty(),
            value: Value::Choice(ChoiceValue::Fraction(Choice(
                ChoiceFlags::empty(),
                ChoiceEnum::Range {
                    default: Fraction { num: fps, denom: 1 },
                    min: Fraction { num: 0, denom: 1 },
                    max: Fraction {
                        num: 1000,
                        denom: 1,
                    },
                },
            ))),
        },
    ]);
    let object = spa::pod::Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties,
    };
    Ok(spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &Value::Object(object),
    )?
    .0
    .into_inner())
}

/// One mapped buffer within `max`, if its planes hold a whole frame.
fn frame(format: Format, datas: &mut [spa::buffer::Data], max: (u32, u32)) -> Option<super::Frame> {
    if format.jpeg {
        let data = datas.first_mut()?;
        let chunk = data.chunk();
        let (offset, size) = (chunk.offset() as usize, chunk.size() as usize);
        let bytes: &[u8] = data.data()?;
        return jpeg_frame(bytes.get(offset..offset.checked_add(size)?)?, max);
    }
    let bytes_per_row = match format.pixels {
        Pixels::Bgr4 | Pixels::Rgb4 => format.width as usize * 4,
        Pixels::Yuy2 => (format.width as usize).div_ceil(2) * 4,
        // PipeWire is never asked for I420: it is how decoded streams arrive.
        Pixels::Nv12 | Pixels::I420 => format.width as usize,
    };
    let (first, rest) = datas.split_first_mut()?;
    let (main, after) = plane(first, bytes_per_row)?;
    let mut planes = vec![main];
    if format.pixels == Pixels::Nv12 {
        // The chroma plane in its own data, or after the luma rows in one.
        let chroma_row = (format.width as usize).div_ceil(2) * 2;
        match rest.first_mut() {
            Some(second) => planes.push(plane(second, chroma_row)?.0),
            None => {
                let luma = planes[0].stride.checked_mul(format.height as usize)?;
                let stride = planes[0].stride;
                planes.push(Plane {
                    bytes: after.get(luma..)?,
                    stride,
                });
            }
        }
    }
    convert_within(format.pixels, format.width, format.height, &planes, max)
}

/// One camera's JPEG picture within `max`. A camera's MJPEG often leaves
/// out the Huffman tables when they are JPEG's standard ones (MJPEG in AVI
/// does too): its `AVI1` mark has the decoder fill in those missing.
fn jpeg_frame(jpeg: &[u8], max: (u32, u32)) -> Option<super::Frame> {
    use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
    let rest = jpeg.strip_prefix(&[0xFF, 0xD8])?;
    let mut marked = Vec::with_capacity(jpeg.len() + 9);
    marked.extend_from_slice(b"\xFF\xD8\xFF\xE0\x00\x07AVI1\x00");
    marked.extend_from_slice(rest);
    let side = super::MAX_SOURCE_SIDE as usize;
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGBA)
        .set_max_width(side)
        .set_max_height(side);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(&marked[..]), options);
    let rgba = decoder.decode().ok()?;
    let (width, height) = decoder.dimensions()?;
    let plane = Plane {
        bytes: &rgba,
        stride: width * 4,
    };
    convert_within(Pixels::Rgb4, width as u32, height as u32, &[plane], max)
}

/// The bytes of a data plane from its chunk's offset, and its stride.
fn plane(data: &mut spa::buffer::Data, row: usize) -> Option<(Plane<'_>, &[u8])> {
    let chunk = data.chunk();
    let offset = chunk.offset() as usize;
    let stride = usize::try_from(chunk.stride())
        .ok()
        .filter(|stride| *stride > 0)
        .unwrap_or(row);
    let bytes: &[u8] = data.data()?;
    let bytes = bytes.get(offset..)?;
    Some((Plane { bytes, stride }, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic camera picture: FFmpeg's test pattern, 64×48, 4:2:2 as
    /// USB cameras send it, with JPEG's standard Huffman tables.
    const CAMERA: &[u8] = include_bytes!("../../fixtures/camera.jpg");

    /// `jpeg` without its Huffman tables, as many cameras send it.
    fn without_tables(jpeg: &[u8]) -> Vec<u8> {
        let mut out = jpeg[..2].to_vec();
        let mut at = 2;
        loop {
            let marker = jpeg[at + 1];
            if marker == 0xDA {
                out.extend_from_slice(&jpeg[at..]);
                return out;
            }
            let length = usize::from(u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]));
            if marker != 0xC4 {
                out.extend_from_slice(&jpeg[at..at + 2 + length]);
            }
            at += 2 + length;
        }
    }

    #[test]
    fn a_camera_s_mjpeg_decodes_with_or_without_its_tables() {
        let max = super::super::PREVIEW_MAX;
        let frame = jpeg_frame(CAMERA, max).unwrap();
        assert_eq!((frame.width, frame.height), (64, 48));
        assert!(frame.rgba.chunks(4).all(|pixel| pixel[3] == 255));
        // Not one flat color: the pattern is there.
        assert!(frame.rgba.chunks(4).any(|pixel| pixel != &frame.rgba[..4]));
        let bare = without_tables(CAMERA);
        assert!(bare.len() < CAMERA.len(), "the tables were taken out");
        assert_eq!(jpeg_frame(&bare, max), Some(frame));
        let small = jpeg_frame(CAMERA, (32, 32)).unwrap();
        assert_eq!((small.width, small.height), (32, 24));
        assert!(jpeg_frame(b"not a picture", max).is_none());
        assert!(jpeg_frame(&[0xFF, 0xD8], max).is_none());
    }

    #[test]
    fn cameras_are_offered_mjpeg_besides_raw_pictures() {
        use spa::param::format::FormatProperties as Key;
        use spa::pod::{deserialize::PodDeserializer, Value};
        let pod = format_pod(MediaSubtype::Mjpg, Vec::new(), 30).unwrap();
        let Ok((_, Value::Object(object))) = PodDeserializer::deserialize_any_from(&pod) else {
            panic!("not an object");
        };
        let value = |key: Key| {
            object
                .properties
                .iter()
                .find(|property| property.key == key.as_raw())
                .map(|property| property.value.clone())
        };
        assert_eq!(
            value(Key::MediaSubtype),
            Some(Value::Id(spa::utils::Id(MediaSubtype::Mjpg.as_raw())))
        );
        assert_eq!(value(Key::VideoFormat), None, "MJPEG names no pixel layout");
        assert!(value(Key::VideoSize).is_some() && value(Key::VideoFramerate).is_some());
        let raw = format_pod(MediaSubtype::Raw, vec![VideoFormat::YUY2], 30).unwrap();
        let Ok((_, Value::Object(object))) = PodDeserializer::deserialize_any_from(&raw) else {
            panic!("not an object");
        };
        assert!(object
            .properties
            .iter()
            .any(|property| property.key == Key::VideoFormat.as_raw()));
    }
}
