//! Someone's shared screen, decoded: H264 access units the host hands over,
//! one decoder on a thread of its own (`rune-video-decode`), and the newest
//! pictures as RGBA in the same queue of two the previews use. The decoder
//! is the GPU's ([`super::gpu`]: VA-API, then NVDEC) among the engines the
//! host allows, or OpenH264 when none is there or one fails; after a switch
//! nothing is shown until a key frame.
//!
//! It knows no service: units come from `receive` and a key frame is asked
//! for through `key`, both supplied by the host. A unit waiting longer than
//! [`MAX_AGE`] is dropped, and after any drop or decoding error nothing is
//! shown until a key frame arrives. Dropping the [`Decoding`] handle stops
//! the thread within one poll.

use std::sync::{Arc, Mutex};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use openh264::decoder::{DecodedYUV, Decoder};
use openh264::formats::YUVSource;
use rune_i18n::t;

use super::gpu::{self, Engine};
use super::{convert_within, fit, lock, Frame, Pixels, Plane, Shared};

/// The largest picture shown; bigger streams are scaled down to fit.
pub const STREAM_MAX: (u32, u32) = (1920, 1080);
/// A unit older than this is not shown: latency over completeness.
pub const MAX_AGE: Duration = Duration::from_millis(150);
/// The largest picture decoded at all: H264 level 5.2, what OpenH264 takes.
pub(crate) const MAX_PICTURE: (usize, usize) = (3840, 2160);
/// Key frames are asked for at most this often.
const KEY_GAP: Duration = Duration::from_millis(500);
/// How long the thread sleeps when nothing waits.
const POLL: Duration = Duration::from_millis(4);

/// One access unit: Annex B, as the provider assembled it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unit {
    /// It holds an IDR picture.
    pub keyframe: bool,
    /// When it was complete.
    pub at: Instant,
    pub data: Vec<u8>,
}

/// Who decodes, and the last GPU engine that failed on the way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DecodeEngine {
    pub engine: Engine,
    pub failed: Option<Engine>,
}

/// One stream being decoded. Creating it starts the thread; dropping it
/// stops the thread, which then releases the decoder and both closures.
pub struct Decoding {
    shared: Arc<Shared>,
    engine: Arc<Mutex<DecodeEngine>>,
    worker: Option<Thread>,
}

impl Decoding {
    /// Decode what `receive` gives with the first of the GPU's `engines`
    /// that works ([`gpu::HARDWARE`], or none), OpenH264 after them.
    pub fn start(
        engines: &'static [Engine],
        receive: impl FnMut() -> Option<Unit> + Send + 'static,
        key: impl FnMut() + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared::new());
        let engine = Arc::new(Mutex::new(DecodeEngine::default()));
        let (worker, told) = (shared.clone(), engine.clone());
        let spawned = thread::Builder::new()
            .name("rune-video-decode".into())
            .spawn(move || work(&worker, &told, engines, receive, key));
        let worker = match spawned {
            Ok(handle) => Some(handle.thread().clone()),
            Err(_) => {
                shared.end(Some(
                    t("No se pudo iniciar el video de la transmisión.").into(),
                ));
                None
            }
        };
        Self {
            shared,
            engine,
            worker,
        }
    }

    /// Who decodes now.
    pub fn engine(&self) -> DecodeEngine {
        *lock(&self.engine)
    }

    /// `Running` once a picture was decoded; `Ended` with why if it failed.
    #[cfg(test)]
    pub fn state(&self) -> super::VideoState {
        super::lock(&self.shared.state).clone()
    }

    /// The newest picture since the last call, if any.
    pub fn take_frame(&self) -> Option<Frame> {
        self.shared.queue.newest()
    }
}

impl Drop for Decoding {
    fn drop(&mut self) {
        self.shared.end(None);
        if let Some(worker) = &self.worker {
            worker.unpark();
        }
    }
}

/// What decodes: OpenH264 on this thread, or a GPU's decoder.
enum Codec {
    Software(Decoder),
    Gpu(gpu::Decoder),
}

/// The first of `engines` that starts, or OpenH264.
fn codec(engines: &[Engine]) -> Option<Codec> {
    engines
        .iter()
        .find_map(|engine| gpu::Decoder::start(*engine))
        .map(Codec::Gpu)
        .or_else(|| Decoder::new().ok().map(Codec::Software))
}

fn work(
    shared: &Shared,
    told: &Mutex<DecodeEngine>,
    mut engines: &'static [Engine],
    mut receive: impl FnMut() -> Option<Unit>,
    mut key: impl FnMut(),
) {
    let failed = || {
        shared.end(Some(
            t("No se pudo iniciar el decodificador de video.").into(),
        ))
    };
    let Some(mut decoder) = codec(engines) else {
        return failed();
    };
    let mut asked: Option<Instant> = None;
    let mut ask = |asked: &mut Option<Instant>| {
        if asked.is_none_or(|at| at.elapsed() >= KEY_GAP) {
            key();
            *asked = Some(Instant::now());
        }
    };
    // Decoding starts at a key frame.
    let mut waiting = true;
    let mut broken = false;
    ask(&mut asked);
    while !shared.stopped() {
        let engine = match &decoder {
            Codec::Software(_) => Engine::Software,
            Codec::Gpu(gpu) => gpu.engine,
        };
        if std::mem::take(&mut broken) {
            // The next engine, from a key frame; OpenH264 when none is left.
            engines = gpu::after(engines, engine);
            let Some(next) = codec(engines) else {
                return failed();
            };
            decoder = next;
            lock(told).failed = Some(engine);
            waiting = true;
            ask(&mut asked);
            continue;
        }
        lock(told).engine = engine;
        if let Codec::Gpu(gpu) = &mut decoder {
            loop {
                match gpu.pull() {
                    Ok(Some(frame)) => shared.push(frame),
                    Ok(None) => break,
                    Err(()) => {
                        broken = true;
                        break;
                    }
                }
            }
        }
        let Some(unit) = receive() else {
            thread::park_timeout(POLL);
            continue;
        };
        // A late key frame still restarts the picture; a late delta is skipped,
        // as is one the GPU has no room for.
        let behind = matches!(&decoder, Codec::Gpu(gpu) if gpu.behind());
        if !unit.keyframe && (waiting || behind || unit.at.elapsed() > MAX_AGE) {
            waiting = true;
            ask(&mut asked);
            continue;
        }
        waiting = false;
        match &mut decoder {
            Codec::Gpu(gpu) => broken = !gpu.push(unit.data),
            Codec::Software(decoder) => match decoder.decode(&unit.data) {
                Ok(Some(picture)) => {
                    if let Some(frame) = rgba(&picture) {
                        shared.push(frame);
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    waiting = true;
                    ask(&mut asked);
                }
            },
        }
    }
}

/// An RGBA picture as a frame within [`STREAM_MAX`]; `None` if it is empty,
/// larger than anything a stream sends or not all in `plane`.
#[cfg_attr(windows, allow(dead_code))]
pub(super) fn within(width: u32, height: u32, plane: Plane) -> Option<Frame> {
    if width == 0
        || height == 0
        || width as usize > MAX_PICTURE.0
        || height as usize > MAX_PICTURE.1
    {
        return None;
    }
    let row = width as usize * 4;
    if fit(width, height, STREAM_MAX) == (width, height) && plane.stride == row {
        let rgba = plane.bytes.get(..row * height as usize)?.to_vec();
        return Some(Frame {
            width,
            height,
            rgba,
        });
    }
    convert_within(Pixels::Rgb4, width, height, &[plane], STREAM_MAX)
}

/// A decoded picture as RGBA within [`STREAM_MAX`]; `None` if it is empty
/// or larger than anything a stream sends.
pub(crate) fn rgba(picture: &DecodedYUV) -> Option<Frame> {
    let (width, height) = picture.dimensions();
    if width == 0 || height == 0 || width > MAX_PICTURE.0 || height > MAX_PICTURE.1 {
        return None;
    }
    let (w, h) = (width as u32, height as u32);
    if fit(w, h, STREAM_MAX) == (w, h) {
        // Shown as is: OpenH264's own conversion, vectorized.
        let mut rgba = vec![0; width * height * 4];
        picture.write_rgba8(&mut rgba);
        return Some(Frame {
            width: w,
            height: h,
            rgba,
        });
    }
    let (y, u, v) = picture.strides();
    convert_within(
        Pixels::I420,
        w,
        h,
        &[
            Plane {
                bytes: picture.y(),
                stride: y,
            },
            Plane {
                bytes: picture.u(),
                stride: u,
            },
            Plane {
                bytes: picture.v(),
                stride: v,
            },
        ],
        STREAM_MAX,
    )
}

/// A decoder for a file's pictures: they may come out later than they go
/// in, as B-frames reorder them, and none is forced out early.
pub(crate) fn file_decoder() -> Result<Decoder, openh264::Error> {
    use openh264::decoder::{DecoderConfig, Flush};
    Decoder::with_api_config(
        openh264::OpenH264API::from_source(),
        DecoderConfig::new().flush_after_decode(Flush::NoFlush),
    )
}

/// Every picture of `file`, an MP4 with H264 pictures such as a GIF site's
/// clip, in the order they show: `each` gets when one shows, from the
/// first, and the picture, and returns whether to go on. Returns how long
/// the clip lasts. Nothing decodes past `within`.
pub fn clip_pictures(
    file: &[u8],
    within: Duration,
    mut each: impl FnMut(Duration, Frame) -> bool,
) -> Result<Duration, crate::MediaError> {
    let failed = || crate::MediaError(t("RUNE no pudo leer este clip.").into());
    let started = Instant::now();
    let track = crate::mp4::video_track(file)
        .map_err(|_| failed())?
        .ok_or_else(failed)?;
    let mut shows: Vec<i64> = track.samples.iter().map(|sample| sample.show).collect();
    shows.sort_unstable();
    let first = shows[0];
    let scale = f64::from(track.timescale);
    let at = |show: i64| Duration::from_secs_f64((show - first).max(0) as f64 / scale);
    let mut shows = shows.into_iter();
    let mut decoder = file_decoder().map_err(|_| failed())?;
    for index in 0..track.samples.len() {
        if started.elapsed() > within {
            return Err(failed());
        }
        let unit = track.annex_b(file, index).ok_or_else(failed)?;
        // A broken picture loses itself, not the clip.
        if let Ok(Some(picture)) = decoder.decode(&unit) {
            let (Some(show), Some(frame)) = (shows.next(), rgba(&picture)) else {
                return Err(failed());
            };
            if !each(at(show), frame) {
                return Ok(Duration::from_secs_f64(track.seconds()));
            }
        }
    }
    for picture in decoder.flush_remaining().map_err(|_| failed())? {
        let (Some(show), Some(frame)) = (shows.next(), rgba(&picture)) else {
            break;
        };
        if !each(at(show), frame) {
            break;
        }
    }
    Ok(Duration::from_secs_f64(track.seconds()))
}

#[cfg(test)]
mod tests {
    use super::super::VideoState;
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use openh264::encoder::Encoder;
    use openh264::formats::YUVBuffer;

    /// `count` frames of a flat color, encoded: the first is a key frame.
    fn encoded(width: usize, height: usize, rgb: [u8; 3], count: usize) -> Vec<Vec<u8>> {
        let mut encoder = Encoder::new().unwrap();
        let pixels: Vec<u8> = rgb.repeat(width * height);
        let yuv = YUVBuffer::from_rgb8_source(openh264::formats::RgbSliceU8::new(
            &pixels,
            (width, height),
        ));
        (0..count)
            .map(|_| encoder.encode(&yuv).unwrap().to_vec())
            .collect()
    }

    fn wait<T>(mut found: impl FnMut() -> Option<T>) -> T {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(found) = found() {
                return found;
            }
            assert!(Instant::now() < until, "timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// With `engines`, a key frame decodes to its size and color, and late
    /// deltas ask for another; who decoded it.
    fn key_frames_decode_and_late_deltas_ask_for_another(engines: &'static [Engine]) -> Engine {
        let units = encoded(320, 180, [200, 30, 30], 3);
        let queue = Arc::new(Mutex::new(std::collections::VecDeque::new()));
        let keys = Arc::new(AtomicUsize::new(0));
        let (feed, asked) = (queue.clone(), keys.clone());
        let decoding = Decoding::start(
            engines,
            move || feed.lock().unwrap().pop_front(),
            move || {
                asked.fetch_add(1, Ordering::SeqCst);
            },
        );
        wait(|| (keys.load(Ordering::SeqCst) == 1).then_some(()));
        // A delta before any key frame is never shown.
        queue.lock().unwrap().push_back(Unit {
            keyframe: false,
            at: Instant::now(),
            data: units[1].clone(),
        });
        queue.lock().unwrap().push_back(Unit {
            keyframe: true,
            at: Instant::now(),
            data: units[0].clone(),
        });
        let frame = wait(|| decoding.take_frame());
        assert_eq!((frame.width, frame.height), (320, 180));
        let [r, g, b, a] = frame.rgba[..4] else {
            unreachable!()
        };
        assert!(r > 170 && g < 70 && b < 70 && a == 255, "{r} {g} {b}");
        assert_eq!(decoding.state(), VideoState::Running);
        // A delta that waited too long is dropped, and a key frame asked for
        // once the gap allows it.
        thread::sleep(KEY_GAP);
        queue.lock().unwrap().push_back(Unit {
            keyframe: false,
            at: Instant::now() - MAX_AGE * 2,
            data: units[2].clone(),
        });
        wait(|| (keys.load(Ordering::SeqCst) == 2).then_some(()));
        assert!(decoding.take_frame().is_none());
        let engine = decoding.engine();
        assert_eq!(engine.failed, None);
        drop(decoding);
        // The thread let go of its closures.
        wait(|| (Arc::strong_count(&queue) == 1).then_some(()));
        engine.engine
    }

    #[test]
    fn a_key_frame_decodes_to_its_size_and_color_and_late_deltas_ask_for_another() {
        let engine = key_frames_decode_and_late_deltas_ask_for_another(&[]);
        assert_eq!(engine, Engine::Software);
        // A GPU when this machine has one, OpenH264 when not (as in CI).
        key_frames_decode_and_late_deltas_ask_for_another(gpu::HARDWARE);
    }

    #[test]
    #[ignore = "needs a GPU with VA-API H264 decoding (Mesa)"]
    fn va_api_decodes() {
        let engine = key_frames_decode_and_late_deltas_ask_for_another(&[Engine::VaApi]);
        assert_eq!(engine, Engine::VaApi);
    }

    #[test]
    #[ignore = "needs an NVIDIA GPU with NVDEC"]
    fn nvdec_decodes() {
        let engine = key_frames_decode_and_late_deltas_ask_for_another(&[Engine::Nvidia]);
        assert_eq!(engine, Engine::Nvidia);
    }

    #[test]
    fn a_gpu_picture_is_kept_within_the_largest_shown() {
        let row = |width: usize, stride: usize| {
            let mut line = [10u8, 20, 200, 255].repeat(width);
            line.resize(stride, 0);
            line
        };
        // As it is, its padding left out.
        let padded = row(4, 32).repeat(2);
        let plane = Plane {
            bytes: &padded,
            stride: 32,
        };
        let frame = within(4, 2, plane).unwrap();
        assert_eq!((frame.width, frame.height, frame.rgba.len()), (4, 2, 32));
        assert_eq!(&frame.rgba[..4], &[10, 20, 200, 255]);
        // Larger than a stream is shown: scaled to fit.
        let large = row(2560, 2560 * 4).repeat(1440);
        let plane = Plane {
            bytes: &large,
            stride: 2560 * 4,
        };
        let frame = within(2560, 1440, plane).unwrap();
        assert_eq!((frame.width, frame.height), (1920, 1080));
        let short = Plane {
            bytes: &large[..100],
            stride: 2560 * 4,
        };
        assert!(within(2560, 1440, short).is_none());
        assert!(within(
            0,
            10,
            Plane {
                bytes: &[],
                stride: 0
            }
        )
        .is_none());
    }

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn a_clip_yields_every_picture_in_the_order_they_show() {
        // Baseline, as a GIF site's loop, and High with B-frames.
        for (name, count, seconds) in [("loop.mp4", 10, 1.0), ("clip.mp4", 30, 2.0)] {
            let mut shown = Vec::new();
            let length = clip_pictures(&fixture(name), Duration::from_secs(10), |at, frame| {
                assert_eq!((frame.width, frame.height), (160, 90), "{name}");
                shown.push(at);
                true
            })
            .unwrap();
            assert_eq!(shown.len(), count, "{name}");
            assert!(shown.windows(2).all(|pair| pair[0] < pair[1]), "{name}");
            assert_eq!(shown[0], Duration::ZERO);
            assert!((length.as_secs_f64() - seconds).abs() < 0.01, "{name}");
        }
        let mut first = 0;
        clip_pictures(&fixture("loop.mp4"), Duration::from_secs(10), |_, _| {
            first += 1;
            false
        })
        .unwrap();
        assert_eq!(first, 1, "it stops when asked");
        assert!(clip_pictures(&fixture("hevc.mp4"), Duration::from_secs(10), |_, _| true).is_err());
        assert!(clip_pictures(b"no", Duration::from_secs(10), |_, _| true).is_err());
    }

    #[test]
    fn a_large_picture_is_scaled_down_to_fit() {
        let units = encoded(2560, 1440, [20, 20, 200], 1);
        let mut decoder = Decoder::new().unwrap();
        let picture = decoder.decode(&units[0]).unwrap().unwrap();
        let frame = rgba(&picture).unwrap();
        assert_eq!((frame.width, frame.height), (1920, 1080));
        assert!(frame.rgba[2] > 160 && frame.rgba[0] < 60);
    }
}
