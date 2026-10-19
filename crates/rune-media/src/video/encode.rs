//! The account's shared screen or its camera, encoded: the newest captured
//! frame, one encoder on a thread of its own (`rune-video-encode`), and H264
//! access units in Annex B handed to the host one at a time. The encoder is
//! the GPU's ([`super::gpu`]: VA-API, then NVENC) among the engines
//! [`Settings::engines`] allows, or OpenH264 when none is there, starts or
//! keeps working: a GPU that fails mid-stream hands over to the next one,
//! and the stream goes on with a key frame.
//!
//! It knows no service: frames come from `frames`, units go to `send` once
//! `room` says one fits (until then frames are skipped, never encoded and
//! dropped), and what the network asks for (a key frame, another bit rate)
//! comes from `asks`, all supplied by the host. Constrained Baseline, so no picture
//! is ever reordered. A key frame starts the stream, answers
//! [`Ask::KeyFrame`] and follows any unit `send` could not take (what a GPU
//! made after it waits for that key frame); a new bit rate goes to the
//! running encoder, with no key frame. A screen
//! that stops changing is encoded again every [`REPEAT`], so those who
//! start watching get a picture. Dropping the [`Encoding`] handle stops the
//! thread within one poll.

use std::sync::{Arc, Mutex};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use openh264::encoder::{
    BitRate, Encoder, EncoderConfig, FrameRate, FrameType, IntraFramePeriod, Profile,
    RateControlMode, UsageType,
};
use openh264::formats::{RgbaSliceU8, YUVBuffer};
use openh264::OpenH264API;
use rune_i18n::{es, t};

pub use super::decode::Unit;
pub use super::gpu::{Engine, HARDWARE};
use super::{gpu, lock, Frame, Shared, VideoState};

/// The lowest bit rate asked of a screen's encoder.
pub const MIN_BITRATE: u32 = 250_000;
/// The last frame is encoded again after this long without a new one.
pub const REPEAT: Duration = Duration::from_secs(1);
/// How long the thread sleeps when nothing waits.
const POLL: Duration = Duration::from_millis(4);

/// What the network asks of the encoder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    KeyFrame,
    /// Bits a second, kept within the least and the most allowed.
    Bitrate(u32),
}

/// How to encode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Frames a second at most.
    pub fps: u32,
    /// Bits a second to start at, and never to pass.
    pub bitrate: u32,
    pub min_bitrate: u32,
    pub max_bitrate: u32,
    /// A camera rather than a screen: OpenH264 tunes for each.
    pub camera: bool,
    /// The GPU's engines to try, in order, before OpenH264: [`HARDWARE`],
    /// or none to encode on the CPU only.
    pub engines: &'static [Engine],
}

/// What actually goes out, for the interface: CPU may not keep up.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sent {
    /// Units taken by `send` in the last whole second.
    pub fps: u32,
    /// The bit rate encoded at now.
    pub bitrate: u32,
    /// Who encodes: OpenH264 until the first picture says otherwise.
    pub engine: Engine,
}

/// What the encoder did, counted since it started: for the shared screen's
/// diagnostic line (`rune: voice share: video …`). No pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EncodeStats {
    /// The size of the pictures encoded.
    pub size: (u32, u32),
    /// Pictures encoded, the key frames among them, and those OpenH264's
    /// rate control skipped to keep to the bit rate.
    pub encoded: u64,
    pub keyframes: u64,
    pub skipped: u64,
    /// Units `send` took, their bytes, and those it could not take (or
    /// that waited for a key frame after one it could not).
    pub sent: u64,
    pub bytes: u64,
    pub refused: u64,
    /// Time converting to I420 and encoding, in microseconds; with a GPU,
    /// from handing it a picture to having its unit back.
    pub encoding_micros: u64,
    /// Bit-rate changes, and those that took a new encoder (and a key frame).
    pub rate_changes: u64,
    pub restarts: u64,
    /// Who encodes now, and the last GPU engine that failed on the way.
    pub engine: Engine,
    pub failed: Option<Engine>,
}

struct State {
    shared: Shared,
    sent: Mutex<Sent>,
    stats: Mutex<EncodeStats>,
}

/// One screen being encoded. Creating it starts the thread; dropping it
/// stops the thread, which then releases the encoder and the closures.
pub struct Encoding {
    state: Arc<State>,
    worker: Option<Thread>,
}

impl Encoding {
    pub fn start(
        settings: Settings,
        frames: impl FnMut() -> Option<Frame> + Send + 'static,
        room: impl FnMut() -> bool + Send + 'static,
        send: impl FnMut(Unit) -> bool + Send + 'static,
        asks: impl FnMut() -> Option<Ask> + Send + 'static,
    ) -> Self {
        let state = Arc::new(State {
            shared: Shared::new(),
            sent: Mutex::new(Sent::default()),
            stats: Mutex::new(EncodeStats::default()),
        });
        let worker = state.clone();
        let spawned = thread::Builder::new()
            .name("rune-video-encode".into())
            .spawn(move || work(&worker, settings, frames, room, send, asks));
        let worker = match spawned {
            Ok(handle) => Some(handle.thread().clone()),
            Err(_) => {
                state.shared.end(Some(
                    t("No se pudo iniciar el envío de la pantalla.").into(),
                ));
                None
            }
        };
        Self { state, worker }
    }

    /// `Running` once a unit went out; `Ended` with why if it failed.
    pub fn state(&self) -> VideoState {
        lock(&self.state.shared.state).clone()
    }

    pub fn sent(&self) -> Sent {
        *lock(&self.state.sent)
    }

    /// What it did so far.
    pub fn stats(&self) -> EncodeStats {
        *lock(&self.state.stats)
    }
}

impl Drop for Encoding {
    fn drop(&mut self) {
        self.state.shared.end(None);
        if let Some(worker) = &self.worker {
            worker.unpark();
        }
    }
}

/// An encoder at `bitrate`: Constrained Baseline, a key frame only when
/// asked for.
fn encoder(settings: Settings, bitrate: u32) -> Option<Encoder> {
    let config = if settings.camera {
        EncoderConfig::new().usage_type(UsageType::CameraVideoRealTime)
    } else {
        EncoderConfig::new()
            .usage_type(UsageType::ScreenContentRealTime)
            // Neither applies to screen content: OpenH264 turns them off and says so.
            .adaptive_quantization(false)
            .background_detection(false)
    };
    let config = config
        .profile(Profile::Baseline)
        .rate_control_mode(RateControlMode::Bitrate)
        .bitrate(BitRate::from_bps(bitrate))
        .max_frame_rate(FrameRate::from_hz(settings.fps.max(1) as f32))
        .intra_frame_period(IntraFramePeriod::from_num_frames(0));
    Encoder::with_api_config(OpenH264API::from_source(), config).ok()
}

/// Change the running encoder's bit rate, as WebRTC does with OpenH264:
/// no new encoder, so no key frame. The safe API has no way to. Its
/// ceiling goes to `max` first, since the wrapper starts it at the first
/// rate and OpenH264 refuses a rate over it. `false` when OpenH264
/// refused, as it does before the first picture.
fn set_bitrate(encoder: &mut Encoder, bitrate: u32, max: u32) -> bool {
    use openh264_sys2::{
        SBitrateInfo, ENCODER_OPTION_BITRATE, ENCODER_OPTION_MAX_BITRATE, SPATIAL_LAYER_0,
    };
    let mut set = |option, bitrate: u32| {
        let mut info = SBitrateInfo {
            iLayer: SPATIAL_LAYER_0,
            iBitrate: bitrate.min(i32::MAX as u32) as i32,
        };
        // SAFETY: OpenH264 checks the encoder is initialized; these options
        // read one `SBitrateInfo`, which outlives the call, and change no
        // setting the wrapper relies on.
        unsafe {
            encoder
                .raw_api()
                .set_option(option, (&mut info as *mut SBitrateInfo).cast())
                == 0
        }
    };
    set(ENCODER_OPTION_MAX_BITRATE, max.max(bitrate)) && set(ENCODER_OPTION_BITRATE, bitrate)
}

/// Whether a bit rate this far from the encoder's is worth a new encoder,
/// which starts with a key frame, when OpenH264 refused to change it in
/// place: small steps wait until they add up (as Serein does).
fn worth(current: u32, wanted: u32) -> bool {
    let (current, wanted) = (u64::from(current), u64::from(wanted));
    wanted * 100 <= current * 85 || wanted * 100 >= current * 125
}

/// `frame` as I420, its odd last column or row left out.
fn yuv(frame: &Frame) -> Option<YUVBuffer> {
    let (width, height) = (frame.width as usize & !1, frame.height as usize & !1);
    let row = frame.width as usize * 4;
    if width == 0 || height == 0 || frame.rgba.len() < row * frame.height as usize {
        return None;
    }
    if width * 4 == row && height == frame.height as usize {
        return Some(YUVBuffer::from_rgba8_source(RgbaSliceU8::new(
            &frame.rgba,
            (width, height),
        )));
    }
    let even: Vec<u8> = frame
        .rgba
        .chunks_exact(row)
        .take(height)
        .flat_map(|line| &line[..width * 4])
        .copied()
        .collect();
    Some(YUVBuffer::from_rgba8_source(RgbaSliceU8::new(
        &even,
        (width, height),
    )))
}

/// What encodes: OpenH264 on this thread, or a GPU's encoder.
enum Codec {
    Software {
        encoder: Box<Encoder>,
        /// Whether it has a picture yet: before it, it starts with a key frame.
        started: bool,
        /// What the last picture made, until it is taken.
        made: Option<Made>,
    },
    Gpu(gpu::Encoder),
}

/// What a picture made: a unit, or nothing when rate control skipped it;
/// and how long it took.
enum Made {
    Unit(Unit, Duration),
    Skipped(Duration),
}

impl Codec {
    /// The first of `engines` that starts for pictures of `size`, or OpenH264.
    fn start(
        settings: Settings,
        engines: &[Engine],
        size: (u32, u32),
        bitrate: u32,
    ) -> Option<Self> {
        engines
            .iter()
            .find_map(|engine| gpu::Encoder::start(*engine, size, settings.fps, bitrate))
            .map(Self::Gpu)
            .or_else(|| {
                encoder(settings, bitrate).map(|encoder| Self::Software {
                    encoder: Box::new(encoder),
                    started: false,
                    made: None,
                })
            })
    }

    fn engine(&self) -> Engine {
        match self {
            Self::Software { .. } => Engine::Software,
            Self::Gpu(gpu) => gpu.engine,
        }
    }

    fn started(&self) -> bool {
        match self {
            Self::Software { started, .. } => *started,
            Self::Gpu(_) => true,
        }
    }

    /// Whether it takes a picture of `size` as it is.
    fn fits(&self, size: (u32, u32)) -> bool {
        match self {
            Self::Software { .. } => true,
            Self::Gpu(gpu) => gpu.size == size,
        }
    }

    fn busy(&self) -> bool {
        match self {
            Self::Software { .. } => false,
            Self::Gpu(gpu) => gpu.busy(),
        }
    }

    /// Change the bit rate from `current` to `bitrate`: `Ok(Some(restarted))`
    /// once it took it, `Ok(None)` while OpenH264's small steps add up,
    /// `Err` when a new OpenH264 encoder could not start.
    fn rate(
        &mut self,
        settings: Settings,
        current: u32,
        bitrate: u32,
        max: u32,
    ) -> Result<Option<bool>, ()> {
        match self {
            // VA-API's element starts a new sequence, with a key frame, for
            // any new rate: small steps wait until they add up, as OpenH264's.
            Self::Gpu(gpu) if gpu.engine == Engine::VaApi => {
                if !worth(current, bitrate) {
                    return Ok(None);
                }
                gpu.set_bitrate(bitrate);
                Ok(Some(true))
            }
            Self::Gpu(gpu) => {
                gpu.set_bitrate(bitrate);
                Ok(Some(false))
            }
            Self::Software {
                encoder: running,
                started,
                ..
            } => {
                if set_bitrate(running, bitrate, max) {
                    return Ok(Some(false));
                }
                if !worth(current, bitrate) {
                    return Ok(None);
                }
                **running = encoder(settings, bitrate).ok_or(())?;
                *started = false;
                Ok(Some(true))
            }
        }
    }

    /// Encode `frame`, taken `at`, a key frame when `key`. `Err` when it failed.
    fn push(&mut self, frame: &Arc<Frame>, key: bool, at: Instant) -> Result<(), ()> {
        match self {
            Self::Gpu(gpu) => gpu.push(frame, key, at).then_some(()).ok_or(()),
            Self::Software {
                encoder,
                started,
                made,
            } => {
                let begun = Instant::now();
                let Some(picture) = yuv(frame) else {
                    return Ok(());
                };
                if key && *started {
                    encoder.force_intra_frame();
                }
                let stream = encoder.encode(&picture).map_err(|_| ())?;
                *made = Some(match stream.frame_type() {
                    // Rate control skipped it: the next one follows it as well.
                    FrameType::Skip | FrameType::Invalid => Made::Skipped(begun.elapsed()),
                    kind => Made::Unit(
                        Unit {
                            keyframe: kind == FrameType::IDR,
                            at,
                            data: stream.to_vec(),
                        },
                        begun.elapsed(),
                    ),
                });
                *started = true;
                Ok(())
            }
        }
    }

    /// What it made since, one at a time; `Err` when it failed.
    fn pull(&mut self) -> Result<Option<Made>, ()> {
        match self {
            Self::Software { made, .. } => Ok(made.take()),
            Self::Gpu(gpu) => Ok(gpu.pull()?.map(|(unit, took)| Made::Unit(unit, took))),
        }
    }
}

fn work(
    state: &State,
    settings: Settings,
    mut frames: impl FnMut() -> Option<Frame>,
    mut room: impl FnMut() -> bool,
    mut send: impl FnMut(Unit) -> bool,
    mut asks: impl FnMut() -> Option<Ask>,
) {
    const FAILED: &str = es("No se pudo codificar la pantalla.");
    let shared = &state.shared;
    let (min, max) = (
        settings.min_bitrate,
        settings.max_bitrate.max(settings.min_bitrate),
    );
    let mut target = settings.bitrate.clamp(min, max);
    // Started at the first picture's size, among the engines not failed yet.
    let mut codec: Option<Codec> = None;
    let mut engines = settings.engines;
    let mut broken = false;
    let mut encoded_at = target;
    let mut key = true;
    // A unit was not taken: what a GPU made after it waits for a key frame.
    let mut waiting = false;
    let mut last: Option<(Arc<Frame>, Instant)> = None;
    let mut second = (Instant::now(), 0);
    lock(&state.sent).bitrate = target;
    let count = |change: &dyn Fn(&mut EncodeStats)| change(&mut lock(&state.stats));
    while !shared.stopped() {
        if std::mem::take(&mut broken) {
            // A GPU that failed hands over to the next engine; OpenH264 cannot.
            let engine = codec
                .take()
                .map_or(Engine::Software, |codec| codec.engine());
            if engine == Engine::Software {
                return shared.end(Some(t(FAILED).into()));
            }
            engines = gpu::after(engines, engine);
            count(&|stats| stats.failed = Some(engine));
            key = true;
        }
        while let Some(ask) = asks() {
            match ask {
                Ask::KeyFrame => key = true,
                Ask::Bitrate(bitrate) => target = bitrate.clamp(min, max),
            }
        }
        if target != encoded_at && codec.is_none() {
            // Nothing encodes yet: the encoder starts at it.
            encoded_at = target;
            lock(&state.sent).bitrate = target;
        }
        if let Some(running) = codec.as_mut().filter(|_| target != encoded_at) {
            match running.rate(settings, encoded_at, target, max) {
                Ok(Some(restarted)) => {
                    encoded_at = target;
                    lock(&state.sent).bitrate = target;
                    count(&|stats| {
                        stats.rate_changes += 1;
                        stats.restarts += u64::from(restarted);
                    });
                }
                Ok(None) => {}
                Err(()) => return shared.end(Some(t(FAILED).into())),
            }
        }
        let now = Instant::now();
        if now.duration_since(second.0) >= Duration::from_secs(1) {
            lock(&state.sent).fps = second.1;
            second = (now, 0);
        }
        // What the encoder made.
        while let Some(running) = codec.as_mut() {
            let unit = match running.pull() {
                Ok(None) => break,
                Err(()) => {
                    broken = true;
                    break;
                }
                Ok(Some(Made::Skipped(took))) => {
                    count(&|stats| {
                        stats.skipped += 1;
                        stats.encoding_micros += took.as_micros() as u64;
                    });
                    continue;
                }
                Ok(Some(Made::Unit(unit, took))) => {
                    count(&|stats| {
                        stats.encoded += 1;
                        stats.keyframes += u64::from(unit.keyframe);
                        stats.encoding_micros += took.as_micros() as u64;
                    });
                    unit
                }
            };
            if unit.data.is_empty() {
                continue;
            }
            if waiting && !unit.keyframe {
                // It refers to a picture nobody has.
                count(&|stats| stats.refused += 1);
                continue;
            }
            let (keyframe, bytes) = (unit.keyframe, unit.data.len() as u64);
            if send(unit) {
                if keyframe {
                    (key, waiting) = (false, false);
                }
                second.1 += 1;
                count(&|stats| {
                    stats.sent += 1;
                    stats.bytes += bytes;
                });
                let mut now = lock(&shared.state);
                if *now == VideoState::Starting {
                    *now = VideoState::Running;
                }
            } else {
                // Not taken: what follows would refer to a picture nobody has.
                (key, waiting) = (true, true);
                count(&|stats| stats.refused += 1);
            }
        }
        if broken {
            continue;
        }
        // The network is behind, or the GPU is: the newest frame waits in the capture.
        if !room() || codec.as_ref().is_some_and(Codec::busy) {
            thread::park_timeout(POLL);
            continue;
        }
        // A new frame; or the last one again, when a key frame is due (with
        // no encoder yet: one failed) or the screen has been still too long.
        let started = codec.as_ref().is_none_or(Codec::started);
        let picture = match frames() {
            Some(frame) => Some(Arc::new(frame)),
            None => last
                .as_ref()
                .filter(|(_, at)| (key && started) || now.duration_since(*at) >= REPEAT)
                .map(|(picture, _)| picture.clone()),
        };
        let Some(picture) = picture else {
            thread::park_timeout(POLL);
            continue;
        };
        last = Some((picture.clone(), now));
        let size = (picture.width & !1, picture.height & !1);
        // Started for the first picture, and again for a new size.
        if codec.as_ref().is_none_or(|codec| !codec.fits(size)) {
            let Some(started) = Codec::start(settings, engines, size, target) else {
                return shared.end(Some(t(FAILED).into()));
            };
            let engine = started.engine();
            lock(&state.sent).engine = engine;
            count(&|stats| stats.engine = engine);
            (codec, encoded_at) = (Some(started), target);
        }
        count(&|stats| stats.size = size);
        broken = codec
            .as_mut()
            .is_some_and(|running| running.push(&picture, key, now).is_err());
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use openh264::decoder::Decoder;

    fn frame(rgb: [u8; 3], width: u32, height: u32) -> Frame {
        Frame {
            width,
            height,
            rgba: [rgb[0], rgb[1], rgb[2], 255].repeat((width * height) as usize),
        }
    }

    fn wait<T>(mut found: impl FnMut() -> Option<T>) -> T {
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(found) = found() {
                return found;
            }
            assert!(Instant::now() < until, "timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    const SETTINGS: Settings = Settings {
        fps: 30,
        bitrate: 600_000,
        min_bitrate: MIN_BITRATE,
        max_bitrate: 3_500_000,
        camera: false,
        engines: &[],
    };

    #[test]
    fn bit_rate_changes_wait_until_they_add_up() {
        assert!(!worth(1_000_000, 1_000_000));
        assert!(!worth(1_000_000, 1_050_000));
        assert!(!worth(1_000_000, 900_000));
        assert!(worth(1_000_000, 800_000), "a cut for loss");
        assert!(worth(1_000_000, 1_276_000), "five steps up");
    }

    #[test]
    fn odd_frames_lose_their_last_column_and_row() {
        let picture = yuv(&frame([10, 200, 10], 5, 3)).unwrap();
        assert_eq!(openh264::formats::YUVSource::dimensions(&picture), (4, 2));
        assert!(yuv(&frame([0, 0, 0], 1, 1)).is_none());
        let short = Frame {
            width: 4,
            height: 4,
            rgba: vec![0; 8],
        };
        assert!(yuv(&short).is_none());
    }

    #[test]
    fn it_starts_with_a_key_frame_answers_one_and_follows_a_drop_with_one() {
        let frames = Arc::new(Mutex::new(VecDeque::new()));
        let units = Arc::new(Mutex::new(Vec::<Unit>::new()));
        let asks = Arc::new(Mutex::new(VecDeque::new()));
        let refuse = Arc::new(AtomicUsize::new(0));
        let (feed, out, asked, refused) =
            (frames.clone(), units.clone(), asks.clone(), refuse.clone());
        let encoding = Encoding::start(
            SETTINGS,
            move || feed.lock().unwrap().pop_front(),
            || true,
            move |unit| {
                if refused.load(Ordering::SeqCst) > 0 {
                    refused.fetch_sub(1, Ordering::SeqCst);
                    return false;
                }
                out.lock().unwrap().push(unit);
                true
            },
            move || asked.lock().unwrap().pop_front(),
        );
        let push = |shade| {
            frames
                .lock()
                .unwrap()
                .push_back(frame([shade, 40, 90], 320, 180))
        };
        let count = |n: usize| {
            let units = units.clone();
            move || (units.lock().unwrap().len() >= n).then_some(())
        };
        push(10);
        wait(count(1));
        push(20);
        wait(count(2));
        {
            let units = units.lock().unwrap();
            assert!(units[0].keyframe && !units[1].keyframe);
            assert!(units[0].data.starts_with(&[0, 0, 0, 1]));
            // It decodes, to the size sent.
            let mut decoder = Decoder::new().unwrap();
            let picture = decoder.decode(&units[0].data).unwrap().unwrap();
            assert_eq!(
                openh264::formats::YUVSource::dimensions(&picture),
                (320, 180)
            );
        }
        // Asked for: the next is a key frame, even with no new frame.
        asks.lock().unwrap().push_back(Ask::KeyFrame);
        wait(count(3));
        assert!(units.lock().unwrap()[2].keyframe);
        // One not taken: the one after it is a key frame.
        refuse.store(1, Ordering::SeqCst);
        push(30);
        wait(|| (refuse.load(Ordering::SeqCst) == 0).then_some(()));
        push(40);
        wait(count(4));
        assert!(units.lock().unwrap()[3].keyframe);
        assert_eq!(encoding.state(), VideoState::Running);
        drop(encoding);
        wait(|| (Arc::strong_count(&frames) == 1).then_some(()));
    }

    #[test]
    fn without_room_nothing_is_encoded_and_the_frame_waits() {
        let frames = Arc::new(Mutex::new(VecDeque::from([frame([1, 2, 3], 64, 36)])));
        let room = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let sent = Arc::new(AtomicUsize::new(0));
        let (feed, open, out) = (frames.clone(), room.clone(), sent.clone());
        let _encoding = Encoding::start(
            SETTINGS,
            move || feed.lock().unwrap().pop_front(),
            move || open.load(Ordering::SeqCst),
            move |_| {
                out.fetch_add(1, Ordering::SeqCst);
                true
            },
            || None,
        );
        thread::sleep(Duration::from_millis(100));
        assert_eq!(sent.load(Ordering::SeqCst), 0);
        assert_eq!(frames.lock().unwrap().len(), 1, "still waiting");
        room.store(true, Ordering::SeqCst);
        wait(|| (sent.load(Ordering::SeqCst) == 1).then_some(()));
    }

    #[test]
    fn a_new_bit_rate_is_kept_in_bounds_and_reported() {
        let asks = Arc::new(Mutex::new(VecDeque::from([Ask::Bitrate(50_000_000)])));
        let asked = asks.clone();
        let encoding = Encoding::start(
            SETTINGS,
            || None,
            || true,
            |_| true,
            move || asked.lock().unwrap().pop_front(),
        );
        wait(|| (encoding.sent().bitrate == 3_500_000).then_some(()));
        asks.lock().unwrap().push_back(Ask::Bitrate(1));
        wait(|| (encoding.sent().bitrate == MIN_BITRATE).then_some(()));
    }

    #[test]
    fn a_new_bit_rate_goes_to_the_running_encoder_without_a_key_frame() {
        let frames = Arc::new(Mutex::new(VecDeque::new()));
        let units = Arc::new(Mutex::new(Vec::<Unit>::new()));
        let asks = Arc::new(Mutex::new(VecDeque::new()));
        let (feed, out, asked) = (frames.clone(), units.clone(), asks.clone());
        let encoding = Encoding::start(
            SETTINGS,
            move || feed.lock().unwrap().pop_front(),
            || true,
            move |unit| {
                out.lock().unwrap().push(unit);
                true
            },
            move || asked.lock().unwrap().pop_front(),
        );
        let count = |n: usize| {
            let units = units.clone();
            move || (units.lock().unwrap().len() >= n).then_some(())
        };
        frames
            .lock()
            .unwrap()
            .push_back(frame([10, 40, 90], 320, 180));
        wait(count(1));
        // Up three times, then down by half: each in place. The pictures
        // change little, or OpenH264 would see a new scene and start over.
        for (at, bitrate) in [700_000, 900_000, 2_000_000, 1_000_000]
            .into_iter()
            .enumerate()
        {
            asks.lock().unwrap().push_back(Ask::Bitrate(bitrate));
            wait(|| (encoding.sent().bitrate == bitrate).then_some(()));
            frames
                .lock()
                .unwrap()
                .push_back(frame([11 + at as u8, 40, 90], 320, 180));
            wait(count(at + 2));
        }
        let units = units.lock().unwrap();
        assert!(units[0].keyframe);
        assert!(units[1..].iter().all(|unit| !unit.keyframe), "no key frame");
        let stats = encoding.stats();
        assert_eq!((stats.rate_changes, stats.restarts), (4, 0));
        assert_eq!((stats.sent, stats.keyframes, stats.refused), (5, 1, 0));
        assert_eq!(stats.size, (320, 180));
        assert_eq!(
            stats.bytes,
            units.iter().map(|unit| unit.data.len() as u64).sum::<u64>()
        );
    }

    #[test]
    fn a_camera_is_tuned_as_one_and_kept_within_its_own_bounds() {
        let frames = Arc::new(Mutex::new(VecDeque::from([frame(
            [200, 120, 40],
            320,
            180,
        )])));
        let units = Arc::new(Mutex::new(Vec::<Unit>::new()));
        let asks = Arc::new(Mutex::new(VecDeque::from([Ask::Bitrate(1)])));
        let (feed, out, asked) = (frames.clone(), units.clone(), asks.clone());
        let encoding = Encoding::start(
            Settings {
                min_bitrate: 150_000,
                max_bitrate: 2_500_000,
                camera: true,
                ..SETTINGS
            },
            move || feed.lock().unwrap().pop_front(),
            || true,
            move |unit| {
                out.lock().unwrap().push(unit);
                true
            },
            move || asked.lock().unwrap().pop_front(),
        );
        wait(|| (encoding.sent().bitrate == 150_000).then_some(()));
        wait(|| units.lock().unwrap().first().cloned());
        let first = units.lock().unwrap()[0].clone();
        assert!(first.keyframe);
        let mut decoder = Decoder::new().unwrap();
        let picture = decoder.decode(&first.data).unwrap().unwrap();
        assert_eq!(
            openh264::formats::YUVSource::dimensions(&picture),
            (320, 180)
        );
        asks.lock().unwrap().push_back(Ask::Bitrate(50_000_000));
        wait(|| (encoding.sent().bitrate == 2_500_000).then_some(()));
    }

    type Queue<T> = Arc<Mutex<VecDeque<T>>>;
    type Units = Arc<Mutex<Vec<Unit>>>;

    /// Start encoding `frames` with `settings`: the queues to feed and ask,
    /// and the units sent.
    fn encode(
        settings: Settings,
        frames: Vec<Frame>,
    ) -> (Encoding, Queue<Frame>, Units, Queue<Ask>) {
        let frames = Arc::new(Mutex::new(VecDeque::from(frames)));
        let units = Arc::new(Mutex::new(Vec::<Unit>::new()));
        let asks = Arc::new(Mutex::new(VecDeque::new()));
        let (feed, out, asked) = (frames.clone(), units.clone(), asks.clone());
        let encoding = Encoding::start(
            settings,
            move || feed.lock().unwrap().pop_front(),
            || true,
            move |unit| {
                out.lock().unwrap().push(unit);
                true
            },
            move || asked.lock().unwrap().pop_front(),
        );
        (encoding, frames, units, asks)
    }

    fn decodes_to(unit: &Unit, size: (usize, usize)) {
        let mut decoder = Decoder::new().unwrap();
        let picture = decoder.decode(&unit.data).unwrap().unwrap();
        assert_eq!(openh264::formats::YUVSource::dimensions(&picture), size);
    }

    #[test]
    fn with_the_gpu_allowed_whatever_encodes_makes_a_stream_openh264_reads() {
        // A GPU when this machine has one, OpenH264 when not (as in CI).
        let frames = (0..3)
            .map(|at| frame([10 + at, 40, 90], 320, 180))
            .collect();
        let (encoding, _, units, _) = encode(
            Settings {
                engines: HARDWARE,
                ..SETTINGS
            },
            frames,
        );
        wait(|| (units.lock().unwrap().len() >= 3).then_some(()));
        let units = units.lock().unwrap();
        assert!(units[0].keyframe && units[0].data.starts_with(&[0, 0, 0, 1]));
        decodes_to(&units[0], (320, 180));
        assert_eq!(encoding.sent().engine, encoding.stats().engine);
    }

    #[test]
    fn a_gpu_that_fails_hands_over_to_openh264_and_the_stream_goes_on() {
        // NVENC takes nothing under 160×64: where it is, it fails at the first
        // picture and OpenH264 encodes it again; where it is not, OpenH264 at once.
        let (encoding, _, units, _) = encode(
            Settings {
                engines: &[Engine::Nvidia],
                ..SETTINGS
            },
            vec![frame([1, 2, 3], 64, 36)],
        );
        let first = wait(|| units.lock().unwrap().first().cloned());
        assert!(first.keyframe);
        decodes_to(&first, (64, 36));
        let stats = encoding.stats();
        assert_eq!(stats.engine, Engine::Software);
        assert!(matches!(stats.failed, None | Some(Engine::Nvidia)));
        assert_eq!(gpu::after(HARDWARE, Engine::VaApi), &[Engine::Nvidia]);
        assert!(gpu::after(HARDWARE, Engine::Nvidia).is_empty());
    }

    /// The GPU `engines`' stream: Constrained Baseline from a key frame, a
    /// new rate (in place, with no key frame, when `in_place`), a key frame
    /// when asked for.
    fn gpu_keeps_the_contract(engines: &'static [Engine], in_place: bool) {
        let (encoding, frames, units, asks) = encode(
            Settings {
                engines,
                ..SETTINGS
            },
            Vec::new(),
        );
        let push = |shade: u8| {
            frames
                .lock()
                .unwrap()
                .push_back(frame([shade, 40, 90], 640, 360))
        };
        let count = |n: usize| {
            let units = units.clone();
            move || (units.lock().unwrap().len() >= n).then_some(())
        };
        for shade in 0..3 {
            push(10 + shade);
            wait(count(shade as usize + 1));
        }
        assert_eq!(encoding.stats().engine, engines[0], "it did not start");
        let first = units.lock().unwrap()[0].clone();
        assert!(first.keyframe);
        // The SPS: profile 66 with constraint_set1, as OpenH264's.
        let sps = first
            .data
            .windows(6)
            .find(|nal| nal[..3] == [0, 0, 1] && nal[3] & 0x1f == 7)
            .expect("parameter sets before the key frame");
        assert_eq!((sps[4], sps[5] & 0x40), (66, 0x40));
        decodes_to(&first, (640, 360));
        asks.lock().unwrap().push_back(Ask::Bitrate(1_000_000));
        wait(|| (encoding.sent().bitrate == 1_000_000).then_some(()));
        for shade in 3..6 {
            push(10 + shade);
            wait(count(shade as usize + 1));
        }
        let keys = units.lock().unwrap()[1..]
            .iter()
            .filter(|unit| unit.keyframe)
            .count();
        assert_eq!(keys, usize::from(!in_place), "key frames for a new rate");
        asks.lock().unwrap().push_back(Ask::KeyFrame);
        push(20);
        wait(|| {
            units.lock().unwrap()[6..]
                .iter()
                .any(|unit| unit.keyframe)
                .then_some(())
        });
        let stats = encoding.stats();
        assert_eq!((stats.restarts, stats.failed), (u64::from(!in_place), None));
    }

    #[test]
    #[ignore = "needs a GPU with VA-API H264 encoding (Mesa)"]
    fn va_api_keeps_the_contract() {
        gpu_keeps_the_contract(&[Engine::VaApi], false);
    }

    #[test]
    #[ignore = "needs an NVIDIA GPU with NVENC"]
    fn nvenc_keeps_the_contract() {
        gpu_keeps_the_contract(&[Engine::Nvidia], true);
    }
}
