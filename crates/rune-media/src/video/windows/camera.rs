//! A camera through Media Foundation: the cameras listed by name, the one
//! asked for opened, and of the pictures it offers the one that best fits
//! what is kept ([`choose`]) read as the camera gives it (NV12, YUY2, I420
//! or MJPEG), converted here as on Linux, never by Windows. Reading is
//! asynchronous: a camera that stalls is noticed, and stopping never waits
//! on it.

use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use rune_i18n::{es, t};
use windows::core::{implement, Interface, Ref, GUID, HRESULT, PWSTR};
use windows::Win32::Foundation::E_ACCESSDENIED;
use windows::Win32::Media::KernelStreaming::{
    IKsControl, KSIDENTIFIER, KSIDENTIFIER_0, KSIDENTIFIER_0_0,
    KSPROPERTY_CAMERACONTROL_AUTO_EXPOSURE_PRIORITY, KSPROPERTY_CAMERACONTROL_FLAGS_MANUAL,
    KSPROPERTY_CAMERACONTROL_S, KSPROPERTY_TYPE_SET, PROPSETID_VIDCAP_CAMERACONTROL,
};
use windows::Win32::Media::MediaFoundation::{
    IMF2DBuffer2, IMFActivate, IMFAttributes, IMFMediaEvent, IMFMediaSource, IMFMediaType,
    IMFSample, IMFSourceReader, IMFSourceReaderCallback, IMFSourceReaderCallback_Impl,
    MF2DBuffer_LockFlags_Read, MFCreateAttributes, MFCreateSourceReaderFromMediaSource,
    MFEnumDeviceSources, MFShutdown, MFStartup, MFVideoFormat_I420, MFVideoFormat_IYUV,
    MFVideoFormat_MJPG, MFVideoFormat_NV12, MFVideoFormat_YUY2, MFSTARTUP_NOSOCKET,
    MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME, MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
    MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK, MF_E_HW_MFT_FAILED_START_STREAMING,
    MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED, MF_E_VIDEO_RECORDING_DEVICE_PREEMPTED,
    MF_MT_DEFAULT_STRIDE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_SUBTYPE,
    MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED, MF_SOURCE_READERF_ENDOFSTREAM,
    MF_SOURCE_READERF_ERROR, MF_SOURCE_READER_ASYNC_CALLBACK, MF_SOURCE_READER_FIRST_VIDEO_STREAM,
    MF_VERSION,
};
use windows::Win32::System::Com::CoTaskMemFree;

use super::super::{
    convert_within, jpeg_frame, lock, CameraInfo, Frame, Keeper, Pixels, Plane, Shared,
    MAX_SOURCE_SIDE,
};
use super::{bounded, Com, CHECK, FIRST_FRAME, GONE, NO_FRAMES, UNAVAILABLE};

/// A camera sends frames all the time: one that stops this long stopped.
const STALL: Duration = Duration::from_secs(5);
/// The longest wait for a read on its way when the camera stops.
const FLUSH: Duration = Duration::from_secs(1);
/// Cameras listed at most, and native formats looked at.
const MAX_CAMERAS: usize = 32;
const MAX_OFFERS: u32 = 512;
const VIDEO: u32 = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;

const NO_CAMERA: &str = es("No hay ninguna cámara conectada.");
const DENIED: &str = es(
    "Windows no deja que las aplicaciones usen la cámara: permitilo en Configuración › Privacidad y seguridad › Cámara.",
);
const BUSY: &str = es("La cámara está en uso por otro programa.");
const NO_FORMAT: &str = es("La cámara no ofrece imágenes que RUNE pueda leer.");
const NO_MEDIA: &str = es(
    "Este Windows no tiene Media Foundation (en las ediciones N, instalá el Media Feature Pack).",
);
const STALLED: &str = es("La cámara dejó de enviar imágenes.");

/// How a camera's pictures come: raw in one of the layouts the preview
/// reads, or each one a JPEG.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Raw(Pixels),
    Jpeg,
}

impl Kind {
    fn of(subtype: GUID) -> Option<Self> {
        Some(match subtype {
            s if s == MFVideoFormat_NV12 => Self::Raw(Pixels::Nv12),
            s if s == MFVideoFormat_YUY2 => Self::Raw(Pixels::Yuy2),
            s if s == MFVideoFormat_I420 || s == MFVideoFormat_IYUV => Self::Raw(Pixels::I420),
            s if s == MFVideoFormat_MJPG => Self::Jpeg,
            _ => return None,
        })
    }

    fn name(self) -> &'static str {
        match self {
            Self::Raw(Pixels::Nv12) => "NV12",
            Self::Raw(Pixels::Yuy2) => "YUY2",
            Self::Raw(_) => "I420",
            Self::Jpeg => "MJPG",
        }
    }

    /// Raw pictures first, the cheapest to read first; JPEG last.
    fn order(self) -> u8 {
        match self {
            Self::Raw(Pixels::Nv12) => 0,
            Self::Raw(Pixels::Yuy2) => 1,
            Self::Raw(_) => 2,
            Self::Jpeg => 3,
        }
    }
}

/// One way a camera offers its pictures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Offer {
    kind: Kind,
    width: u32,
    height: u32,
    /// Frames a second as a fraction; `0/0` when the camera does not say.
    rate: (u32, u32),
}

/// The offer that best fits pictures kept within `max` at `fps` a second:
/// first one that reaches that rate, then one at least as large as what is
/// kept (the smallest such, else the largest), then raw before JPEG.
fn choose(offers: &[Offer], max: (u32, u32), fps: u32) -> Option<usize> {
    let rank = |offer: &Offer| {
        let rate = match offer.rate {
            (_, 0) => 0.0,
            (numerator, denominator) => f64::from(numerator) / f64::from(denominator),
        };
        // 29.97 is 30.
        let fast = rate + 0.5 >= f64::from(fps);
        let covers = offer.width >= max.0 || offer.height >= max.1;
        let area = u64::from(offer.width) * u64::from(offer.height);
        let size = if covers { area } else { u64::MAX - area };
        let excess = (rate - f64::from(fps)).abs().round() as u64;
        (!fast, !covers, size, offer.kind.order(), excess)
    };
    offers
        .iter()
        .enumerate()
        .filter(|(_, offer)| {
            (1..=MAX_SOURCE_SIDE).contains(&offer.width)
                && (1..=MAX_SOURCE_SIDE).contains(&offer.height)
        })
        .min_by_key(|(_, offer)| rank(offer))
        .map(|(at, _)| at)
}

/// The planes of a raw picture `stride` bytes a row, `height` rows high,
/// in one buffer: NV12's and I420's chroma after their luma.
fn planes(pixels: Pixels, bytes: &[u8], stride: usize, height: u32) -> Option<Vec<Plane<'_>>> {
    let rows = height as usize;
    let luma = stride.checked_mul(rows)?;
    Some(match pixels {
        Pixels::Nv12 => vec![
            Plane { bytes, stride },
            Plane {
                bytes: bytes.get(luma..)?,
                stride,
            },
        ],
        Pixels::I420 => {
            let half = stride / 2;
            let chroma = half.checked_mul(rows.div_ceil(2))?;
            vec![
                Plane { bytes, stride },
                Plane {
                    bytes: bytes.get(luma..)?,
                    stride: half,
                },
                Plane {
                    bytes: bytes.get(luma.checked_add(chroma)?..)?,
                    stride: half,
                },
            ]
        }
        _ => vec![Plane { bytes, stride }],
    })
}

/// What the reader gives now: how its pictures come and their rows.
#[derive(Clone, Copy, Debug)]
struct Format {
    kind: Kind,
    width: u32,
    height: u32,
    /// Bytes a row in a buffer that is not two-dimensional.
    stride: usize,
}

/// Media Foundation on this thread, until dropped.
struct MediaFoundation;

impl MediaFoundation {
    fn start() -> Result<Self, String> {
        // SAFETY: once, undone once by the drop.
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) }
            .map(|()| Self)
            .map_err(|_| t(NO_MEDIA).to_owned())
    }
}

impl Drop for MediaFoundation {
    fn drop(&mut self) {
        // SAFETY: after every object of it was let go.
        let _ = unsafe { MFShutdown() };
    }
}

/// The camera opened, shut (and so let go) when dropped.
struct Opened(IMFMediaSource);

impl Drop for Opened {
    fn drop(&mut self) {
        // SAFETY: the source this owns; shutting it releases the camera.
        let _ = unsafe { self.0.Shutdown() };
    }
}

fn attributes(count: u32) -> windows::core::Result<IMFAttributes> {
    let mut attributes = None;
    // SAFETY: a valid out-pointer.
    unsafe { MFCreateAttributes(&mut attributes, count) }?;
    attributes.ok_or_else(windows::core::Error::empty)
}

/// The text attribute `key` of a camera.
fn text(camera: &IMFActivate, key: &GUID) -> Option<String> {
    let (mut value, mut length) = (PWSTR::null(), 0);
    // SAFETY: the string Media Foundation allocates is copied and freed once.
    unsafe {
        camera
            .GetAllocatedString(key, &mut value, &mut length)
            .ok()?;
        let text = value.to_string().ok();
        CoTaskMemFree(Some(value.0 as *const _));
        text.filter(|text| !text.is_empty() && text.len() <= 4096)
    }
}

/// The cameras connected now, as the system names them.
fn cameras() -> windows::core::Result<Vec<(IMFActivate, CameraInfo)>> {
    let attributes = attributes(1)?;
    let (mut list, mut count) = (std::ptr::null_mut(), 0u32);
    // SAFETY: valid pointers; the array Media Foundation allocates holds
    // `count` cameras, each released, and is freed once.
    unsafe {
        attributes.SetGUID(
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE,
            &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_GUID,
        )?;
        MFEnumDeviceSources(&attributes, &mut list, &mut count)?;
        if list.is_null() {
            return Ok(Vec::new());
        }
        let entries = std::slice::from_raw_parts_mut(list, count as usize);
        let found = entries
            .iter_mut()
            .filter_map(Option::take)
            .take(MAX_CAMERAS)
            .filter_map(|camera| {
                let id = text(
                    &camera,
                    &MF_DEVSOURCE_ATTRIBUTE_SOURCE_TYPE_VIDCAP_SYMBOLIC_LINK,
                )?;
                let name = text(&camera, &MF_DEVSOURCE_ATTRIBUTE_FRIENDLY_NAME)
                    .map_or_else(|| id.clone(), |name| bounded(&name, 128));
                Some((camera, CameraInfo { id, name }))
            })
            .collect();
        for entry in entries {
            entry.take();
        }
        CoTaskMemFree(Some(list as *const _));
        Ok(found)
    }
}

/// Why opening or reading the camera failed, as the person reads it.
fn reason(code: HRESULT) -> String {
    t(match code {
        E_ACCESSDENIED => DENIED,
        MF_E_HW_MFT_FAILED_START_STREAMING | MF_E_VIDEO_RECORDING_DEVICE_PREEMPTED => BUSY,
        MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED => GONE,
        _ => UNAVAILABLE,
    })
    .to_owned()
}

/// What `media` says it is, if it is one RUNE reads.
fn offer(media: &IMFMediaType) -> Option<Offer> {
    // SAFETY: plain attribute reads of a media type this thread owns.
    unsafe {
        let kind = Kind::of(media.GetGUID(&MF_MT_SUBTYPE).ok()?)?;
        let size = media.GetUINT64(&MF_MT_FRAME_SIZE).ok()?;
        let rate = media.GetUINT64(&MF_MT_FRAME_RATE).unwrap_or(0);
        Some(Offer {
            kind,
            width: (size >> 32) as u32,
            height: size as u32,
            rate: ((rate >> 32) as u32, rate as u32),
        })
    }
}

/// The reader's current pictures.
fn current(reader: &IMFSourceReader) -> Option<(Format, Offer)> {
    // SAFETY: a plain query of this thread's reader.
    let media = unsafe { reader.GetCurrentMediaType(VIDEO) }.ok()?;
    let offer = offer(&media)?;
    let row = match offer.kind {
        Kind::Raw(Pixels::Yuy2) => offer.width.div_ceil(2) as usize * 4,
        _ => offer.width as usize,
    };
    // SAFETY: as above. A stride below zero is bottom-up: not a camera's.
    let stride = unsafe { media.GetUINT32(&MF_MT_DEFAULT_STRIDE) }
        .ok()
        .and_then(|stride| usize::try_from(stride as i32).ok())
        .filter(|stride| *stride >= row)
        .unwrap_or(row);
    Some((
        Format {
            kind: offer.kind,
            width: offer.width,
            height: offer.height,
            stride,
        },
        offer,
    ))
}

/// A raw picture of `format`, `stride` bytes a row, within `max`.
fn raw(
    pixels: Pixels,
    bytes: &[u8],
    stride: usize,
    format: Format,
    max: (u32, u32),
) -> Option<Frame> {
    let planes = planes(pixels, bytes, stride, format.height)?;
    convert_within(pixels, format.width, format.height, &planes, max)
}

/// One picture of `sample` within `max`.
fn picture(sample: &IMFSample, format: Format, max: (u32, u32)) -> Option<Frame> {
    // SAFETY: the buffer is read only while locked, within the length the
    // lock gave, and unlocked on every way out.
    unsafe {
        let buffer = sample.ConvertToContiguousBuffer().ok()?;
        if let (Kind::Raw(pixels), Ok(two)) = (format.kind, buffer.cast::<IMF2DBuffer2>()) {
            let (mut line, mut pitch, mut start, mut length) =
                (std::ptr::null_mut(), 0, std::ptr::null_mut(), 0);
            two.Lock2DSize(
                MF2DBuffer_LockFlags_Read,
                &mut line,
                &mut pitch,
                &mut start,
                &mut length,
            )
            .ok()?;
            let offset = (line as usize).checked_sub(start as usize);
            let frame = match (offset, usize::try_from(pitch)) {
                (Some(offset), Ok(stride)) if !start.is_null() => {
                    let bytes = std::slice::from_raw_parts(start, length as usize);
                    bytes
                        .get(offset..)
                        .and_then(|bytes| raw(pixels, bytes, stride, format, max))
                }
                _ => None,
            };
            let _ = two.Unlock2D();
            return frame;
        }
        let (mut start, mut length) = (std::ptr::null_mut(), 0);
        buffer.Lock(&mut start, None, Some(&mut length)).ok()?;
        let frame = (!start.is_null())
            .then(|| std::slice::from_raw_parts(start, length as usize))
            .and_then(|bytes| match format.kind {
                Kind::Jpeg => jpeg_frame(bytes, max),
                Kind::Raw(pixels) => raw(pixels, bytes, format.stride, format, max),
            });
        let _ = buffer.Unlock();
        frame
    }
}

/// What a read the reader finished said.
enum Read {
    /// A picture came (`true`), or only a tick of the stream.
    Sample(bool),
    /// The pictures come another way from now on.
    Changed,
    Failed(HRESULT),
    /// What was being read was dropped, as asked.
    Flushed,
}

/// The reader's callback: each picture straight into the queues, then a
/// word to the thread that asks for the next.
#[implement(IMFSourceReaderCallback)]
struct Reading {
    shared: Arc<Shared>,
    keeper: Mutex<Keeper>,
    format: Arc<Mutex<Option<Format>>>,
    told: mpsc::SyncSender<Read>,
}

impl IMFSourceReaderCallback_Impl for Reading_Impl {
    fn OnReadSample(
        &self,
        status: HRESULT,
        _: u32,
        flags: u32,
        _: i64,
        sample: Ref<IMFSample>,
    ) -> windows::core::Result<()> {
        let ended = (MF_SOURCE_READERF_ERROR.0 | MF_SOURCE_READERF_ENDOFSTREAM.0) as u32;
        let read = if status.is_err() {
            Read::Failed(status)
        } else if flags & ended != 0 {
            Read::Failed(MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED)
        } else if flags & MF_SOURCE_READERF_CURRENTMEDIATYPECHANGED.0 as u32 != 0 {
            Read::Changed
        } else {
            let format = *lock(&self.format);
            match (sample.as_ref(), format) {
                (Some(sample), Some(format)) => {
                    self.shared
                        .deliver(&mut lock(&self.keeper), Instant::now(), |max| {
                            picture(sample, format, max)
                        });
                    Read::Sample(true)
                }
                _ => Read::Sample(false),
            }
        };
        let _ = self.told.try_send(read);
        Ok(())
    }

    fn OnFlush(&self, _: u32) -> windows::core::Result<()> {
        let _ = self.told.try_send(Read::Flushed);
        Ok(())
    }

    fn OnEvent(&self, _: u32, _: Ref<IMFMediaEvent>) -> windows::core::Result<()> {
        Ok(())
    }
}

/// Without the camera slowing down to expose longer in dim light: many
/// webcams halve their frame rate for it ("auto exposure priority"), so a
/// format of 30 a second came at 15 (2026-10-21: «delivered 14.8/s» at
/// 30/1). A camera without the control, or that refuses it, stays as it
/// is.
fn steady_rate(source: &IMFMediaSource) {
    let Ok(control) = source.cast::<IKsControl>() else {
        return;
    };
    let mut set = KSPROPERTY_CAMERACONTROL_S {
        Property: KSIDENTIFIER {
            Anonymous: KSIDENTIFIER_0 {
                Anonymous: KSIDENTIFIER_0_0 {
                    Set: PROPSETID_VIDCAP_CAMERACONTROL,
                    Id: KSPROPERTY_CAMERACONTROL_AUTO_EXPOSURE_PRIORITY.0 as u32,
                    Flags: KSPROPERTY_TYPE_SET,
                },
            },
        },
        Value: 0,
        Flags: KSPROPERTY_CAMERACONTROL_FLAGS_MANUAL as u32,
        Capabilities: 0,
    };
    let size = std::mem::size_of::<KSPROPERTY_CAMERACONTROL_S>() as u32;
    let property = &mut set as *mut KSPROPERTY_CAMERACONTROL_S;
    let mut returned = 0;
    // SAFETY: the property leads the struct that is also its data, alive
    // through the call, `size` bytes each.
    let _ = unsafe {
        control.KsProperty(
            property.cast::<KSIDENTIFIER>(),
            size,
            property.cast(),
            size,
            &mut returned,
        )
    };
}

/// Read the camera `device` (or the first) into `shared` until it stops,
/// goes away or fails.
pub(super) fn read(device: Option<&str>, shared: &Arc<Shared>) -> Result<(), String> {
    let _com = Com::start()?;
    let _media = MediaFoundation::start()?;
    let unavailable = |error: windows::core::Error| reason(error.code());
    let mut cameras = cameras().map_err(unavailable)?;
    *lock(&shared.cameras) = cameras.iter().map(|(_, info)| info.clone()).collect();
    let at = cameras
        .iter()
        .position(|(_, info)| Some(info.id.as_str()) == device)
        .unwrap_or(0);
    if cameras.is_empty() {
        return Err(t(NO_CAMERA).into());
    }
    let (camera, _) = cameras.swap_remove(at);
    drop(cameras);
    // SAFETY: activated once, on the person's asking; shut when dropped.
    let opened = Opened(unsafe { camera.ActivateObject::<IMFMediaSource>() }.map_err(unavailable)?);
    let (told, heard) = mpsc::sync_channel(8);
    let format = Arc::new(Mutex::new(None));
    let callback: IMFSourceReaderCallback = Reading {
        shared: shared.clone(),
        keeper: Mutex::default(),
        format: format.clone(),
        told,
    }
    .into();
    let reader = {
        let attributes = attributes(1).map_err(unavailable)?;
        // SAFETY: the callback outlives the reader, which holds it.
        unsafe {
            attributes
                .SetUnknown(&MF_SOURCE_READER_ASYNC_CALLBACK, &callback)
                .map_err(unavailable)?;
            MFCreateSourceReaderFromMediaSource(&opened.0, &attributes).map_err(unavailable)?
        }
    };
    // What the camera offers, and the one that fits what is kept.
    let (max, interval) = shared.capture();
    let fps = (1.0 / interval.as_secs_f64()).round() as u32;
    // SAFETY: plain queries of this thread's reader, up to its last.
    let offered: Vec<(IMFMediaType, Offer)> = (0..MAX_OFFERS)
        .map_while(|at| unsafe { reader.GetNativeMediaType(VIDEO, at) }.ok())
        .filter_map(|media| {
            let offer = offer(&media)?;
            Some((media, offer))
        })
        .collect();
    let offers: Vec<Offer> = offered.iter().map(|(_, offer)| *offer).collect();
    let chosen = choose(&offers, max, fps).ok_or_else(|| t(NO_FORMAT).to_owned())?;
    // SAFETY: a native type of this reader's own video stream.
    unsafe { reader.SetCurrentMediaType(VIDEO, None, &offered[chosen].0) }.map_err(unavailable)?;
    steady_rate(&opened.0);
    let refresh = |format: &Mutex<Option<Format>>| {
        let now = current(&reader);
        if let Some((_, offer)) = now {
            let mut stats = lock(&shared.stats);
            stats.size = (offer.width, offer.height);
            stats.format = offer.kind.name();
            stats.memory = "mf";
            stats.rate = offer.rate;
            stats.max_rate = offer.rate;
        }
        *lock(format) = now.map(|(format, _)| format);
    };
    refresh(&format);
    let next = || unsafe { reader.ReadSample(VIDEO, 0, None, None, None, None) };
    next().map_err(unavailable)?;
    let started = Instant::now();
    let mut last: Option<Instant> = None;
    let result = loop {
        match heard.recv_timeout(CHECK) {
            Ok(Read::Sample(picture)) => {
                if picture {
                    last = Some(Instant::now());
                }
                if shared.stopped() {
                    break Ok(());
                }
                if let Err(error) = next() {
                    break Err(unavailable(error));
                }
            }
            Ok(Read::Changed) => {
                refresh(&format);
                if let Err(error) = next() {
                    break Err(unavailable(error));
                }
            }
            Ok(Read::Failed(code)) => break Err(reason(code)),
            Ok(Read::Flushed) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break Err(t(GONE).into()),
        }
        if shared.stopped() {
            break Ok(());
        }
        let now = Instant::now();
        if last.is_none() && now.duration_since(started) > FIRST_FRAME {
            break Err(t(NO_FRAMES).into());
        }
        if last.is_some_and(|last| now.duration_since(last) > STALL) {
            break Err(t(STALLED).into());
        }
    };
    // A read may still be on its way: flushed, and told so, before the
    // reader, the camera and Media Foundation go.
    // SAFETY: this thread's reader.
    if unsafe { reader.Flush(VIDEO) }.is_ok() {
        let until = Instant::now() + FLUSH;
        while let Some(left) = until.checked_duration_since(Instant::now()) {
            if matches!(heard.recv_timeout(left), Ok(Read::Flushed) | Err(_)) {
                break;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn offer(kind: Kind, width: u32, height: u32, fps: u32) -> Offer {
        Offer {
            kind,
            width,
            height,
            rate: (fps, 1),
        }
    }

    const YUY2: Kind = Kind::Raw(Pixels::Yuy2);
    const NV12: Kind = Kind::Raw(Pixels::Nv12);

    #[test]
    fn the_camera_s_offer_that_fits_is_chosen() {
        // A USB 2 camera: raw 720p only at 10 a second, MJPEG at 30.
        let usb = [
            offer(YUY2, 640, 480, 30),
            offer(YUY2, 1280, 720, 10),
            offer(Kind::Jpeg, 1280, 720, 30),
            offer(Kind::Jpeg, 1920, 1080, 30),
        ];
        assert_eq!(choose(&usb, (1280, 720), 30), Some(2), "the rate first");
        // A preview at 15 a second: raw 720p does not reach it either.
        assert_eq!(choose(&usb, (1280, 720), 15), Some(2));
        // Small enough, raw reaches it: raw, not JPEG.
        assert_eq!(choose(&usb, (640, 360), 30), Some(0));
        // Raw at the rate and size: NV12 before YUY2, the smallest that covers.
        let raw = [
            offer(YUY2, 1920, 1080, 30),
            offer(YUY2, 1280, 720, 30),
            offer(NV12, 1280, 720, 30),
            offer(NV12, 1280, 720, 60),
        ];
        assert_eq!(choose(&raw, (1280, 720), 30), Some(2));
        // 29.97 is 30; 4:3 that reaches the height covers.
        let ntsc = [
            Offer {
                kind: NV12,
                width: 960,
                height: 720,
                rate: (30_000, 1001),
            },
            offer(NV12, 640, 360, 30),
        ];
        assert_eq!(choose(&ntsc, (1280, 720), 30), Some(0));
        // Nothing reaches the size: the largest that keeps the rate.
        let small = [
            offer(NV12, 320, 240, 30),
            offer(NV12, 640, 480, 30),
            offer(NV12, 800, 600, 5),
        ];
        assert_eq!(choose(&small, (1280, 720), 30), Some(1));
        // Rates a camera does not say come last; sizes past the bound never.
        let odd = [
            Offer {
                kind: NV12,
                width: 1280,
                height: 720,
                rate: (0, 0),
            },
            offer(NV12, 640, 480, 30),
            offer(NV12, MAX_SOURCE_SIDE + 1, 720, 30),
        ];
        assert_eq!(choose(&odd, (1280, 720), 30), Some(1));
        assert_eq!(choose(&[], (1280, 720), 30), None);
    }

    #[test]
    fn a_contiguous_picture_s_planes_follow_its_luma() {
        // 4×2 NV12 with 8-byte rows: white luma, neutral chroma.
        let mut nv12 = vec![235u8; 8 * 2];
        nv12.extend([128u8; 8]);
        let found = planes(Pixels::Nv12, &nv12, 8, 2).unwrap();
        assert_eq!(
            (found.len(), found[1].bytes.len(), found[1].stride),
            (2, 8, 8)
        );
        let frame = convert_within(Pixels::Nv12, 4, 2, &found, (64, 64)).unwrap();
        assert!(frame
            .rgba
            .chunks(4)
            .all(|pixel| pixel == [255, 255, 255, 255]));
        // I420: U then V, each a quarter, at half the stride.
        let mut i420 = vec![16u8; 4 * 2];
        i420.extend([128u8, 128, 128, 128]);
        let found = planes(Pixels::I420, &i420, 4, 2).unwrap();
        assert_eq!(found[1].stride, 2);
        assert_eq!(found[2].bytes.len(), 2);
        let frame = convert_within(Pixels::I420, 4, 2, &found, (64, 64)).unwrap();
        assert!(frame.rgba.chunks(4).all(|pixel| pixel == [0, 0, 0, 255]));
        // A buffer shorter than its luma has no chroma to give.
        assert!(planes(Pixels::Nv12, &nv12[..8], 8, 2).is_none());
        // YUY2 is one plane.
        assert_eq!(planes(Pixels::Yuy2, &[0; 16], 8, 2).unwrap().len(), 1);
    }

    #[test]
    fn subtypes_are_read_as_the_preview_reads_them() {
        assert_eq!(Kind::of(MFVideoFormat_NV12), Some(NV12));
        assert_eq!(Kind::of(MFVideoFormat_IYUV), Some(Kind::Raw(Pixels::I420)));
        assert_eq!(Kind::of(MFVideoFormat_MJPG), Some(Kind::Jpeg));
        assert_eq!(Kind::of(GUID::zeroed()), None);
        assert_eq!(Kind::Jpeg.name(), "MJPG");
    }

    #[test]
    fn why_a_camera_failed_is_said_plainly() {
        assert_eq!(reason(E_ACCESSDENIED), DENIED);
        assert_eq!(reason(MF_E_HW_MFT_FAILED_START_STREAMING), BUSY);
        assert_eq!(reason(MF_E_VIDEO_RECORDING_DEVICE_INVALIDATED), GONE);
        assert_eq!(reason(HRESULT(-1)), UNAVAILABLE);
    }

    // Wine's Media Foundation: it lists the cameras it has (none, most
    // likely) without opening any.
    #[test]
    fn the_cameras_are_listed_without_opening_one() {
        let _com = Com::start().unwrap();
        let Ok(_media) = MediaFoundation::start() else {
            return;
        };
        let cameras = cameras().unwrap_or_default();
        assert!(cameras.len() <= MAX_CAMERAS);
        assert!(cameras.iter().all(|(_, camera)| !camera.id.is_empty()));
    }
}
