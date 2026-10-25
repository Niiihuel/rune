//! Local output: a latest-request controller, a worker that owns the device
//! stream, and a real-time callback that only pops a bounded ring.
//!
//! Everything RUNE plays is mono at [`PLAYBACK_RATE`]. A source (the test
//! tone, or a [`Feed`] another thread fills) reaches the callback through the
//! worker, which keeps the device ring near a small target and drops the
//! oldest queued audio when a feed runs ahead (overrun). The callback applies
//! volume and deafen from atomics, converts rate and channels, writes silence
//! when the ring runs dry (underrun) and hands exactly what it played,
//! silence included, to the [`EchoReference`].

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use rtrb::{Consumer, Producer, RingBuffer};
use rune_i18n::t;

use crate::{CaptureState, MediaError};

/// Sources, the device ring and the echo reference are mono at this rate.
pub const PLAYBACK_RATE: u32 = 48_000;
/// Samples in a millisecond at [`PLAYBACK_RATE`].
const MS: usize = PLAYBACK_RATE as usize / 1_000;
/// What the worker keeps queued for the callback, unless its period is longer.
const TARGET: usize = 40 * MS;
const DEVICE_RING: usize = 200 * MS;
const FEED_RING: usize = 200 * MS;
/// A feed further ahead than this loses its oldest audio, down to [`TARGET`].
const FEED_BACKLOG: usize = 120 * MS;
/// Sounds mixed over the feed at once, as many as a call mixes; a new one
/// past this disconnects the oldest.
const MAX_SOUNDS: usize = 4;
const REFERENCE_RING: usize = 200 * MS;
/// The highest gain: 200 %.
pub const MAX_VOLUME: f32 = 2.0;
const MAX_RATE: u32 = 192_000;
const MIN_RATE: u32 = 8_000;
const MAX_CHANNELS: u16 = 8;
const POLL_INTERVAL: Duration = Duration::from_millis(4);
/// A lost device is reopened this often, this many times, before the system
/// default takes over (or playback fails when there is no chosen device).
#[cfg(not(test))]
pub(crate) const RETRY: Duration = Duration::from_millis(500);
#[cfg(test)]
pub(crate) const RETRY: Duration = Duration::from_millis(5);
pub(crate) const RETRIES: u32 = 10;
/// How often the default is checked while following it, and while on the
/// default for a chosen device that may be plugged in again.
#[cfg(not(test))]
pub(crate) const DEFAULT_CHECK: Duration = Duration::from_secs(1);
#[cfg(test)]
pub(crate) const DEFAULT_CHECK: Duration = Duration::from_millis(10);

/// What plays.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlaybackSource {
    /// A soft two-note chime, once a second, generated on the worker.
    #[default]
    Tone,
    /// Whatever the latest [`Playback::feed`] receives, and the
    /// [`Playback::sound_feed`]s mixed over it.
    Feed,
}

/// The complete desired output. Volume and deafen apply in the next callback;
/// a new device or source goes through the worker.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackRequest {
    /// `None` follows the system's default output, also when it changes.
    pub device: Option<String>,
    pub source: PlaybackSource,
    /// Linear gain from 0 to [`MAX_VOLUME`].
    pub volume: f32,
    /// Plays silence, which is what the echo reference gets.
    pub deafened: bool,
}

/// The output's state and counters. Counters accumulate for the life of the
/// [`Playback`]; latencies are the latest measured.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlaybackSnapshot {
    pub state: CaptureState,
    /// The device's rate and channels while running.
    pub rate: u32,
    pub channels: u16,
    /// Samples (at [`PLAYBACK_RATE`]) of silence played because a playing
    /// source ran dry.
    pub underruns: u64,
    /// Source samples dropped: the oldest beyond the backlog, or a full feed.
    pub overruns: u64,
    /// Callback to the device's output, as the audio system predicts it.
    pub output_latency_micros: u64,
    /// Audio queued for the callback when last measured.
    pub queued_micros: u64,
    /// The chosen output is missing: the system default plays in its place,
    /// and the chosen one is opened again once it is back.
    pub fallback: bool,
}

struct Desired {
    generation: u64,
    request: Option<PlaybackRequest>,
}

struct Published {
    generation: u64,
    snapshot: PlaybackSnapshot,
}

struct Shared {
    desired: ArcSwap<Desired>,
    published: ArcSwap<Published>,
    requested: AtomicU64,
    /// f32 bits of the gain the callback applies; zero while deafened.
    gain: AtomicU32,
    /// A source is configured, so silence counts as an underrun.
    playing: AtomicBool,
    shutdown: AtomicBool,
    worker_failed: AtomicBool,
    underruns: AtomicU64,
    overruns: Arc<AtomicU64>,
    latency_micros: Arc<AtomicU64>,
    /// The longest callback seen, in samples at [`PLAYBACK_RATE`].
    period: AtomicU32,
    /// Hand-offs to the worker. Only non-real-time threads take these locks.
    feed: Mutex<Option<Consumer<f32>>>,
    sounds: Mutex<Vec<Consumer<f32>>>,
    reference: Mutex<Option<Producer<f32>>>,
}

impl Shared {
    fn new() -> Self {
        Self {
            desired: ArcSwap::from_pointee(Desired {
                generation: 0,
                request: None,
            }),
            published: ArcSwap::from_pointee(Published {
                generation: 0,
                snapshot: PlaybackSnapshot::default(),
            }),
            requested: AtomicU64::new(0),
            gain: AtomicU32::new(0),
            playing: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            worker_failed: AtomicBool::new(false),
            underruns: AtomicU64::new(0),
            overruns: Arc::new(AtomicU64::new(0)),
            latency_micros: Arc::new(AtomicU64::new(0)),
            period: AtomicU32::new(0),
            feed: Mutex::new(None),
            sounds: Mutex::new(Vec::new()),
            reference: Mutex::new(None),
        }
    }

    fn current(&self, generation: u64) -> bool {
        !self.shutdown.load(Ordering::Acquire)
            && self.requested.load(Ordering::Acquire) == generation
    }

    fn publish(&self, generation: u64, snapshot: PlaybackSnapshot) {
        if self.current(generation) {
            self.published.store(Arc::new(Published {
                generation,
                snapshot,
            }));
        }
    }
}

/// The gain for `request`: zero when deafened, and never out of range.
fn gain(request: &PlaybackRequest) -> f32 {
    if request.deafened || !request.volume.is_finite() {
        0.0
    } else {
        request.volume.clamp(0.0, MAX_VOLUME)
    }
}

/// Mono audio at [`PLAYBACK_RATE`] for the output. Push from one non-real-time
/// thread; a newer [`Playback::feed`] disconnects this one.
pub struct Feed {
    producer: Producer<f32>,
    overruns: Arc<AtomicU64>,
}

impl Feed {
    /// Queue `samples`, never blocking. What does not fit is dropped and
    /// counted as an overrun; returns how many were dropped.
    pub fn push(&mut self, samples: &[f32]) -> usize {
        let mut dropped = 0;
        for &sample in samples {
            let sample = if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            if self.producer.push(sample).is_err() {
                dropped += 1;
            }
        }
        if dropped != 0 {
            self.overruns.fetch_add(dropped as u64, Ordering::Relaxed);
        }
        dropped
    }

    /// Whether the output still reads this feed.
    pub fn connected(&self) -> bool {
        !self.producer.is_abandoned()
    }

    #[cfg(test)]
    pub(crate) fn pair(capacity: usize) -> (Self, Consumer<f32>) {
        let (producer, consumer) = RingBuffer::new(capacity);
        (
            Self {
                producer,
                overruns: Arc::new(AtomicU64::new(0)),
            },
            consumer,
        )
    }
}

/// A bounded queue of mono samples between two non-real-time threads: a
/// [`Feed`] another component fills and the [`Tap`] that reads it.
pub fn tap(capacity: usize) -> (Feed, Tap) {
    let (producer, consumer) = RingBuffer::new(capacity);
    (
        Feed {
            producer,
            overruns: Arc::new(AtomicU64::new(0)),
        },
        Tap(consumer),
    )
}

/// The reading end of [`tap`].
pub struct Tap(Consumer<f32>);

impl Tap {
    /// Samples waiting to be read.
    pub fn available(&self) -> usize {
        self.0.slots()
    }

    /// Fill `frame` if that many samples are waiting; otherwise leave both.
    pub fn read(&mut self, frame: &mut [f32]) -> bool {
        let Ok(chunk) = self.0.read_chunk(frame.len()) else {
            return false;
        };
        let (first, second) = chunk.as_slices();
        frame[..first.len()].copy_from_slice(first);
        frame[first.len()..].copy_from_slice(second);
        chunk.commit_all();
        true
    }

    /// Drop the oldest samples so at most `keep` remain; returns how many went.
    pub fn trim(&mut self, keep: usize) -> usize {
        let excess = self.0.slots().saturating_sub(keep);
        if let Ok(chunk) = self.0.read_chunk(excess) {
            chunk.commit_all();
        }
        excess
    }

    /// Whether its [`Feed`] is gone; what it queued can still be read.
    pub fn abandoned(&self) -> bool {
        self.0.is_abandoned()
    }
}

/// What the output played, mono at [`PLAYBACK_RATE`], for echo cancellation:
/// every sample, silence included, so its timeline is the speaker's. Read it
/// from one non-real-time thread.
pub struct EchoReference {
    consumer: Consumer<f32>,
    latency_micros: Arc<AtomicU64>,
}

impl EchoReference {
    /// Samples waiting to be read.
    pub fn available(&self) -> usize {
        self.consumer.slots()
    }

    /// Drop the oldest samples so at most `keep` remain; returns how many went.
    pub fn trim(&mut self, keep: usize) -> usize {
        let excess = self.consumer.slots().saturating_sub(keep);
        if let Ok(chunk) = self.consumer.read_chunk(excess) {
            chunk.commit_all();
        }
        excess
    }

    /// Fill `frame` if that many samples are waiting; otherwise leave both.
    pub fn read(&mut self, frame: &mut [f32]) -> bool {
        let Ok(chunk) = self.consumer.read_chunk(frame.len()) else {
            return false;
        };
        let (first, second) = chunk.as_slices();
        frame[..first.len()].copy_from_slice(first);
        frame[first.len()..].copy_from_slice(second);
        chunk.commit_all();
        true
    }

    /// The output's predicted latency, callback to speaker, in milliseconds.
    pub fn output_latency_ms(&self) -> u32 {
        (self.latency_micros.load(Ordering::Relaxed) / 1_000).min(u32::MAX as u64) as u32
    }

    /// Drop everything waiting; returns how many went.
    pub fn clear(&mut self) -> usize {
        self.trim(0)
    }

    /// A reference read from `consumer`, with no latency to report.
    pub(crate) fn from_consumer(consumer: Consumer<f32>) -> Self {
        Self {
            consumer,
            latency_micros: Arc::new(AtomicU64::new(0)),
        }
    }

    #[cfg(test)]
    pub(crate) fn pair(capacity: usize) -> (Producer<f32>, Self) {
        let (producer, consumer) = RingBuffer::new(capacity);
        (producer, Self::from_consumer(consumer))
    }
}

/// Streaming linear interpolation between two rates.
///
/// shortcut: linear interpolation aliases on music and bright tones; replace
/// it with a band-limited resampler if a device without 48 kHz sounds wrong.
#[derive(Clone, Debug)]
pub(crate) struct Linear {
    step: f64,
    phase: f64,
    previous: f32,
    current: f32,
}

impl Linear {
    /// Both rates must be positive; callers admit 8 to 192 kHz.
    pub(crate) fn new(from: u32, to: u32) -> Self {
        Self {
            step: f64::from(from) / f64::from(to.max(1)),
            phase: 1.0,
            previous: 0.0,
            current: 0.0,
        }
    }

    /// One output sample, pulling at most `1 + ceil(from / to)` inputs. When
    /// `pull` runs out it returns `None` and keeps its place for the next call.
    pub(crate) fn next(&mut self, mut pull: impl FnMut() -> Option<f32>) -> Option<f32> {
        while self.phase >= 1.0 {
            let sample = pull()?;
            self.previous = self.current;
            self.current = sample;
            self.phase -= 1.0;
        }
        let sample = self.previous + (self.current - self.previous) * self.phase as f32;
        self.phase += self.step;
        Some(sample)
    }
}

#[derive(Default)]
struct StreamSignals {
    failed: AtomicBool,
}

/// The only mutable state a real-time output callback owns.
struct Egress {
    consumer: Consumer<f32>,
    reference: Producer<f32>,
    resampler: Linear,
    channels: usize,
    /// Samples at [`PLAYBACK_RATE`] per output frame, to size the period.
    step: f64,
    shared: Arc<Shared>,
    /// The last sample played: a dry ring fades from it instead of
    /// dropping to zero at once, which clicks (2026-10-21).
    held: f32,
}

/// How much of the last sample a dry ring keeps each sample: down to a
/// thousandth in about 3 ms.
const DRY_DECAY: f32 = 0.95;

impl Egress {
    /// Fill `output` (interleaved), never blocking or allocating.
    fn render<T>(&mut self, output: &mut [T], latency: Duration)
    where
        T: SizedSample + FromSample<f32>,
    {
        let gain = f32::from_bits(self.shared.gain.load(Ordering::Acquire));
        let (consumer, reference, held) = (&mut self.consumer, &mut self.reference, &mut self.held);
        let mut missing = 0_u64;
        for frame in output.chunks_mut(self.channels) {
            // A dry ring fades into silence, and the reference gets it too: a
            // reference that skipped it would run ahead of the speaker, and
            // the canceller would lose the echo's delay (2026-10-19).
            let pulled = self.resampler.next(|| {
                let sample = match consumer.pop() {
                    Ok(sample) => sample,
                    Err(_) => {
                        missing += 1;
                        *held * DRY_DECAY
                    }
                };
                *held = sample;
                // A full reference ring only means nobody is cancelling echo.
                let _ = reference.push((sample * gain).clamp(-1.0, 1.0));
                Some(sample)
            });
            let value = (pulled.unwrap_or(0.0) * gain).clamp(-1.0, 1.0);
            frame.fill(T::from_sample(value));
        }
        if missing != 0 && self.shared.playing.load(Ordering::Relaxed) {
            self.shared.underruns.fetch_add(missing, Ordering::Relaxed);
        }
        let frames = output.len() / self.channels.max(1);
        let period = (frames as f64 * self.step).ceil().min(f64::from(u32::MAX)) as u32;
        self.shared.period.fetch_max(period, Ordering::Relaxed);
        self.shared.latency_micros.store(
            latency.as_micros().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
    }
}

/// The output's callback alone, at [`PLAYBACK_RATE`] and full volume, for
/// measurements: what it plays, and what it hands to the echo reference.
#[cfg(test)]
pub(crate) struct Speaker {
    egress: Egress,
    ends: Ends,
}

#[cfg(test)]
impl Speaker {
    pub(crate) fn new() -> Self {
        let shared = Arc::new(Shared::new());
        shared.gain.store(1.0_f32.to_bits(), Ordering::Release);
        shared.playing.store(true, Ordering::Release);
        let (egress, ends) = rings(PLAYBACK_RATE, 1, shared).expect("48 kHz mono is admitted");
        Self { egress, ends }
    }

    /// What the worker queued for the callback.
    pub(crate) fn queue(&mut self, samples: &[f32]) {
        for sample in samples {
            let _ = self.ends.producer.push(*sample);
        }
    }

    /// One callback: fill `output` with what plays.
    pub(crate) fn play(&mut self, output: &mut [f32]) {
        self.egress.render(output, Duration::ZERO);
    }

    /// What the callback handed on as played since the last call.
    pub(crate) fn played(&mut self, into: &mut Vec<f32>) {
        while let Ok(sample) = self.ends.references.pop() {
            into.push(sample);
        }
    }
}

/// The rings of one output stream. The callback gets [`Egress`]; the worker
/// keeps the other ends.
fn rings(rate: u32, channels: u16, shared: Arc<Shared>) -> Result<(Egress, Ends), MediaError> {
    if !(MIN_RATE..=MAX_RATE).contains(&rate) {
        return Err(MediaError(
            t("La frecuencia de la salida de audio no es compatible.").into(),
        ));
    }
    if !(1..=MAX_CHANNELS).contains(&channels) {
        return Err(MediaError(
            t("La salida de audio tiene una cantidad de canales no compatible.").into(),
        ));
    }
    let (producer, consumer) = RingBuffer::new(DEVICE_RING);
    let (reference, references) = RingBuffer::new(REFERENCE_RING);
    Ok((
        Egress {
            consumer,
            reference,
            resampler: Linear::new(PLAYBACK_RATE, rate),
            channels: usize::from(channels),
            step: f64::from(PLAYBACK_RATE) / f64::from(rate),
            shared,
            held: 0.0,
        },
        Ends {
            producer,
            references,
            signals: Arc::new(StreamSignals::default()),
            rate,
            channels,
        },
    ))
}

/// The worker's ends of one stream's rings.
struct Ends {
    producer: Producer<f32>,
    references: Consumer<f32>,
    signals: Arc<StreamSignals>,
    rate: u32,
    channels: u16,
}

trait OutputStream {
    fn start(&self) -> Result<(), MediaError>;
}

impl OutputStream for cpal::Stream {
    fn start(&self) -> Result<(), MediaError> {
        StreamTrait::start(self).map_err(|error| {
            eprintln!("rune: speaker: start failed: {error}");
            MediaError(t("No se pudo activar la salida de audio.").into())
        })
    }
}

struct Opened<S> {
    stream: S,
    ends: Ends,
    /// The system default this stream opened, when it follows the default.
    default: Option<String>,
}

trait Backend {
    type Stream: OutputStream;
    fn open(
        &mut self,
        device: Option<&str>,
        shared: Arc<Shared>,
    ) -> Result<Opened<Self::Stream>, MediaError>;
    /// The system's default output now.
    fn default_output(&mut self) -> Option<String>;
    /// Whether the output with this id is connected now.
    fn present(&mut self, id: &str) -> bool;
}

struct CpalBackend;

pub(crate) fn device_id(device: &cpal::Device) -> String {
    device
        .id()
        .map(|id| id.to_string())
        .unwrap_or_else(|_| device.to_string())
}

impl Backend for CpalBackend {
    type Stream = cpal::Stream;

    fn open(
        &mut self,
        id: Option<&str>,
        shared: Arc<Shared>,
    ) -> Result<Opened<Self::Stream>, MediaError> {
        let host = cpal::default_host();
        let device = match id {
            Some(id) => {
                let invalid =
                    || MediaError(t("El identificador de la salida no es válido.").into());
                if id.len() > 4096 {
                    return Err(invalid());
                }
                let id = id.parse().map_err(|_| invalid())?;
                host.device_by_id(&id).ok_or_else(|| {
                    MediaError(t("La salida de audio elegida no está disponible.").into())
                })?
            }
            None => host
                .default_output_device()
                .ok_or_else(|| MediaError(t("No hay una salida de audio disponible.").into()))?,
        };
        let default = id.is_none().then(|| device_id(&device));
        let supported = output_config(&device)?;
        let rate = supported.sample_rate();
        let mut config = supported.config();
        // A 20 ms period keeps the queue, and the echo reference, short; the
        // audio system's default can be far longer.
        if let cpal::SupportedBufferSize::Range { min, max } = supported.buffer_size() {
            let period = rate / 50;
            if (*min..=*max).contains(&period) {
                config.buffer_size = cpal::BufferSize::Fixed(period);
            }
        }
        let (egress, ends) = rings(rate, config.channels, shared)?;
        let signals = ends.signals.clone();
        let stream = match supported.sample_format() {
            SampleFormat::F32 => open_stream::<f32>(&device, config, egress, signals)?,
            SampleFormat::F64 => open_stream::<f64>(&device, config, egress, signals)?,
            SampleFormat::I8 => open_stream::<i8>(&device, config, egress, signals)?,
            SampleFormat::I16 => open_stream::<i16>(&device, config, egress, signals)?,
            SampleFormat::I32 => open_stream::<i32>(&device, config, egress, signals)?,
            SampleFormat::I64 => open_stream::<i64>(&device, config, egress, signals)?,
            SampleFormat::U8 => open_stream::<u8>(&device, config, egress, signals)?,
            SampleFormat::U16 => open_stream::<u16>(&device, config, egress, signals)?,
            SampleFormat::U32 => open_stream::<u32>(&device, config, egress, signals)?,
            SampleFormat::U64 => open_stream::<u64>(&device, config, egress, signals)?,
            _ => {
                return Err(MediaError(
                    t("El formato de la salida de audio no es compatible.").into(),
                ))
            }
        };
        Ok(Opened {
            stream,
            ends,
            default,
        })
    }

    fn default_output(&mut self) -> Option<String> {
        cpal::default_host()
            .default_output_device()
            .map(|device| device_id(&device))
    }

    fn present(&mut self, id: &str) -> bool {
        id.parse()
            .ok()
            .and_then(|id| cpal::default_host().device_by_id(&id))
            .is_some()
    }
}

/// 48 kHz when the device offers it, so the callback does not resample, with
/// the fewest channels; otherwise the device's own default.
fn output_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, MediaError> {
    let preferred = device.supported_output_configs().ok().and_then(|configs| {
        configs
            .filter(|config| (1..=MAX_CHANNELS).contains(&config.channels()))
            .filter_map(|config| config.try_with_sample_rate(PLAYBACK_RATE))
            .min_by_key(|config| {
                (
                    config.sample_format() != SampleFormat::F32,
                    config.channels(),
                )
            })
    });
    match preferred {
        Some(config) => Ok(config),
        None => device.default_output_config().map_err(|error| {
            eprintln!("rune: speaker: no output format: {error}");
            MediaError(t("No se pudo consultar la salida de audio.").into())
        }),
    }
}

fn open_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut egress: Egress,
    signals: Arc<StreamSignals>,
) -> Result<cpal::Stream, MediaError>
where
    T: SizedSample + FromSample<f32>,
{
    device
        .build_output_stream::<T, _, _>(
            config,
            move |output, info| {
                let timestamp = info.timestamp();
                egress.render(
                    output,
                    timestamp
                        .device
                        .saturating_duration_since(timestamp.callback),
                );
            },
            move |error| {
                // CPAL 0.19 reports xruns with the affected data callback;
                // its error callback therefore means the stream was lost.
                eprintln!("rune: speaker: stream lost: {error}");
                signals.failed.store(true, Ordering::Release);
            },
            Some(Duration::from_secs(2)),
        )
        .map_err(|error| {
            eprintln!("rune: speaker: open failed: {error}");
            MediaError(t("No se pudo abrir la salida de audio elegida.").into())
        })
}

/// The test chime: 660 Hz then 880 Hz, 200 ms each with 5 ms fades, then
/// 600 ms of quiet, at about -12 dBFS.
#[derive(Default)]
struct Tone {
    position: u32,
}

impl Tone {
    const NOTE: u32 = 200 * MS as u32;
    const FADE: f32 = 5.0 * MS as f32;
    const AMPLITUDE: f32 = 0.25;

    fn next(&mut self) -> f32 {
        let n = self.position;
        self.position = (n + 1) % PLAYBACK_RATE;
        let (start, frequency) = match n / Self::NOTE {
            0 => (0, 660.0),
            1 => (Self::NOTE, 880.0),
            _ => return 0.0,
        };
        let t = (n - start) as f32;
        let envelope = (t / Self::FADE)
            .min((Self::NOTE as f32 - t) / Self::FADE)
            .min(1.0);
        let phase = std::f32::consts::TAU * frequency * n as f32 / PLAYBACK_RATE as f32;
        Self::AMPLITUDE * envelope * phase.sin()
    }
}

struct Active<S> {
    // Dropped first: the callback stops before its rings go.
    _stream: S,
    ends: Ends,
    device: Option<String>,
    default: Option<String>,
}

impl<S> Active<S> {
    fn queued(&self) -> usize {
        self.ends.producer.buffer().capacity() - self.ends.producer.slots()
    }

    /// Top the device ring up to the target from the source, after dropping
    /// a feed's oldest audio beyond the backlog. The sounds are summed with
    /// the feed, each sample with what is there; one that ended and played
    /// out is let go.
    fn fill(
        &mut self,
        shared: &Shared,
        source: PlaybackSource,
        tone: &mut Tone,
        feed: &mut Option<Consumer<f32>>,
        sounds: &mut Vec<Consumer<f32>>,
    ) {
        let period = shared.period.load(Ordering::Relaxed) as usize;
        let target = TARGET.max(period.saturating_mul(2)).min(DEVICE_RING);
        let mut want = target.saturating_sub(self.queued());
        match source {
            PlaybackSource::Tone => {
                for _ in 0..want {
                    if self.ends.producer.push(tone.next()).is_err() {
                        break;
                    }
                }
            }
            PlaybackSource::Feed => {
                sounds.retain(|sound| !(sound.is_abandoned() && sound.is_empty()));
                for queue in feed.iter_mut().chain(sounds.iter_mut()) {
                    let backlog = queue.slots();
                    if backlog > FEED_BACKLOG {
                        let excess = backlog - TARGET;
                        if let Ok(chunk) = queue.read_chunk(excess) {
                            chunk.commit_all();
                            shared.overruns.fetch_add(excess as u64, Ordering::Relaxed);
                        }
                    }
                }
                while want > 0 {
                    let mut mixed = None;
                    for queue in feed.iter_mut().chain(sounds.iter_mut()) {
                        if let Ok(sample) = queue.pop() {
                            *mixed.get_or_insert(0.0) += sample;
                        }
                    }
                    // The callback clamps after its gain.
                    let Some(sample) = mixed else {
                        break;
                    };
                    if self.ends.producer.push(sample).is_err() {
                        break;
                    }
                    want -= 1;
                }
            }
        }
    }

    /// Move what the callback played to the persistent reference, if any.
    fn drain_reference(&mut self, reference: &mut Option<Producer<f32>>) {
        let available = self.ends.references.slots();
        let Ok(chunk) = self.ends.references.read_chunk(available) else {
            return;
        };
        match reference {
            Some(reference) => {
                for sample in chunk {
                    // A full reference means nobody reads it; it stays bounded.
                    let _ = reference.push(sample);
                }
            }
            None => chunk.commit_all(),
        }
    }
}

/// Nonblocking handle to one output worker. Creating it opens no device.
/// Drop signals shutdown and never joins on the caller.
pub struct Playback {
    shared: Arc<Shared>,
    worker: Thread,
    generation: u64,
    request: Option<PlaybackRequest>,
}

impl Playback {
    pub fn new() -> Result<Self, MediaError> {
        Self::with_backend(CpalBackend)
    }

    fn with_backend<B: Backend + Send + 'static>(backend: B) -> Result<Self, MediaError> {
        let shared = Arc::new(Shared::new());
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("rune-playback".into())
            .spawn(move || run(backend, worker_shared))
            .map_err(|_| MediaError(t("No se pudo iniciar la salida de audio.").into()))?;
        let worker = worker.thread().clone();
        Ok(Self {
            shared,
            worker,
            generation: 0,
            request: None,
        })
    }

    /// Replace the desired output. Volume and deafen take effect in the next
    /// callback; a new device or source restarts it on the worker. To retry an
    /// identical failed request, configure `None` first.
    pub fn configure(&mut self, request: Option<PlaybackRequest>) {
        if self.request == request {
            return;
        }
        self.shared.gain.store(
            request.as_ref().map_or(0.0, gain).to_bits(),
            Ordering::Release,
        );
        let same_stream = match (&self.request, &request) {
            (Some(old), Some(new)) => old.device == new.device && old.source == new.source,
            _ => false,
        };
        self.request = request;
        if same_stream {
            return;
        }
        self.generation = self.generation.wrapping_add(1).max(1);
        self.shared
            .requested
            .store(self.generation, Ordering::Release);
        self.shared.desired.store(Arc::new(Desired {
            generation: self.generation,
            request: self.request.clone(),
        }));
        self.worker.unpark();
    }

    pub fn snapshot(&self) -> PlaybackSnapshot {
        if self.request.is_none() {
            return PlaybackSnapshot::default();
        }
        if self.shared.worker_failed.load(Ordering::Acquire) {
            return PlaybackSnapshot {
                state: CaptureState::Failed(t("La salida de audio se detuvo.").into()),
                ..PlaybackSnapshot::default()
            };
        }
        let published = self.shared.published.load();
        if published.generation == self.generation {
            published.snapshot.clone()
        } else {
            PlaybackSnapshot {
                state: CaptureState::Starting,
                ..PlaybackSnapshot::default()
            }
        }
    }

    /// A new input for [`PlaybackSource::Feed`]; the previous one disconnects.
    pub fn feed(&self) -> Feed {
        let (producer, consumer) = RingBuffer::new(FEED_RING);
        *self
            .shared
            .feed
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(consumer);
        self.worker.unpark();
        Feed {
            producer,
            overruns: self.shared.overruns.clone(),
        }
    }

    /// A new input mixed over the [`Playback::feed`], which it leaves
    /// alone: a short sound while a voice message plays. It goes once
    /// dropped and played out, or when [`MAX_SOUNDS`] newer ones came.
    pub fn sound_feed(&self) -> Feed {
        let (producer, consumer) = RingBuffer::new(FEED_RING);
        self.shared
            .sounds
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(consumer);
        self.worker.unpark();
        Feed {
            producer,
            overruns: self.shared.overruns.clone(),
        }
    }

    /// A new reader of what plays; the previous one stops receiving.
    pub fn echo_reference(&self) -> EchoReference {
        let (producer, consumer) = RingBuffer::new(REFERENCE_RING);
        *self
            .shared
            .reference
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(producer);
        self.worker.unpark();
        EchoReference {
            consumer,
            latency_micros: self.shared.latency_micros.clone(),
        }
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        self.shared.gain.store(0, Ordering::Release);
        self.worker.unpark();
    }
}

struct WorkerLife(Arc<Shared>);

impl Drop for WorkerLife {
    fn drop(&mut self) {
        self.0.playing.store(false, Ordering::Release);
        if !self.0.shutdown.load(Ordering::Acquire) {
            self.0.worker_failed.store(true, Ordering::Release);
        }
    }
}

/// When to try the device again, how many tries are left after the attempt
/// at `at` fails, and whether it opens the system default in place of the
/// chosen device, which is missing (`fallback`).
#[derive(Clone)]
pub(crate) struct Reopen {
    pub(crate) at: Instant,
    pub(crate) left: u32,
    pub(crate) fallback: bool,
}

/// After a failed attempt described by `from`: another try later while tries
/// are left; then, for a chosen device, the system default once; else nothing.
pub(crate) fn next_try(from: &Reopen, chosen: bool, now: Instant) -> Option<Reopen> {
    if from.left > 0 {
        return Some(Reopen {
            at: now + RETRY,
            left: from.left - 1,
            fallback: from.fallback,
        });
    }
    (chosen && !from.fallback).then_some(Reopen {
        at: now,
        left: 0,
        fallback: true,
    })
}

fn take<T>(slot: &Mutex<Option<T>>, into: &mut Option<T>) {
    if let Ok(mut slot) = slot.try_lock() {
        if let Some(value) = slot.take() {
            *into = Some(value);
        }
    }
}

fn run<B: Backend>(mut backend: B, shared: Arc<Shared>) {
    let _life = WorkerLife(shared.clone());
    let mut generation = 0;
    let mut request: Option<PlaybackRequest> = None;
    let mut active: Option<Active<B::Stream>> = None;
    let mut reopen: Option<Reopen> = None;
    // The chosen device is missing: the default plays until it is back.
    let mut fallback = false;
    let mut feed = None;
    let mut sounds = Vec::new();
    let mut reference = None;
    let mut tone = Tone::default();
    let mut checked = Instant::now();
    let mut snapshot = PlaybackSnapshot::default();
    loop {
        if shared.shutdown.load(Ordering::Acquire) {
            break;
        }
        take(&shared.feed, &mut feed);
        if let Ok(mut new) = shared.sounds.try_lock() {
            sounds.append(&mut new);
            let excess = sounds.len().saturating_sub(MAX_SOUNDS);
            sounds.drain(..excess);
        }
        take(&shared.reference, &mut reference);
        let now = Instant::now();
        let desired = shared.desired.load_full();
        if desired.generation != generation {
            generation = desired.generation;
            let previous = request.take().map(|wanted| wanted.device);
            request = desired.request.clone();
            if request.as_ref().map(|wanted| &wanted.device) != previous.as_ref() {
                fallback = false;
            }
            tone = Tone::default();
            snapshot = PlaybackSnapshot::default();
            match &request {
                None => {
                    active = None;
                    reopen = None;
                }
                Some(wanted) => {
                    let target = if fallback {
                        None
                    } else {
                        wanted.device.clone()
                    };
                    if active.as_ref().is_none_or(|active| active.device != target) {
                        active = None;
                        reopen = Some(Reopen {
                            at: now,
                            left: 0,
                            fallback,
                        });
                    }
                }
            }
            shared.playing.store(false, Ordering::Release);
        }
        if let (Some(wanted), Some(due)) = (&request, reopen.clone()) {
            if now >= due.at {
                snapshot.state = CaptureState::Starting;
                shared.publish(generation, snapshot.clone());
                fallback = due.fallback;
                let target = if fallback {
                    None
                } else {
                    wanted.device.clone()
                };
                let chosen = wanted.device.is_some();
                match backend.open(target.as_deref(), shared.clone()) {
                    Ok(opened) => {
                        if !shared.current(generation) {
                            continue;
                        }
                        match opened.stream.start() {
                            Ok(()) => {
                                snapshot.rate = opened.ends.rate;
                                snapshot.channels = opened.ends.channels;
                                snapshot.fallback = fallback;
                                checked = now;
                                active = Some(Active {
                                    _stream: opened.stream,
                                    ends: opened.ends,
                                    device: target,
                                    default: opened.default,
                                });
                                reopen = None;
                            }
                            Err(error) => {
                                reopen = retry(
                                    &shared,
                                    generation,
                                    &mut snapshot,
                                    &due,
                                    chosen,
                                    error,
                                    now,
                                );
                            }
                        }
                    }
                    Err(error) => {
                        reopen =
                            retry(&shared, generation, &mut snapshot, &due, chosen, error, now);
                    }
                }
            }
        }
        if let (Some(current), Some(wanted)) = (&mut active, &request) {
            let lost = current.ends.signals.failed.load(Ordering::Acquire);
            // On the default, look at it, and at the chosen device while it is missing.
            let mut returned = false;
            let mut moved = false;
            if current.device.is_none() && now.saturating_duration_since(checked) >= DEFAULT_CHECK {
                checked = now;
                returned = fallback
                    && wanted
                        .device
                        .as_deref()
                        .is_some_and(|id| backend.present(id));
                moved = !returned && backend.default_output() != current.default;
            }
            if lost || moved || returned {
                active = None;
                shared.playing.store(false, Ordering::Release);
                // A chosen device that is back opens at once, as does a new
                // default; a lost device gets retries first.
                reopen = Some(if returned {
                    Reopen {
                        at: now,
                        left: RETRIES,
                        fallback: false,
                    }
                } else if lost {
                    Reopen {
                        at: now + RETRY,
                        left: RETRIES,
                        fallback,
                    }
                } else {
                    Reopen {
                        at: now,
                        left: 0,
                        fallback,
                    }
                });
                snapshot.state = CaptureState::Starting;
                shared.publish(generation, snapshot.clone());
                continue;
            }
            shared.playing.store(true, Ordering::Release);
            current.fill(&shared, wanted.source, &mut tone, &mut feed, &mut sounds);
            current.drain_reference(&mut reference);
            let next = PlaybackSnapshot {
                state: CaptureState::Running,
                rate: current.ends.rate,
                channels: current.ends.channels,
                underruns: shared.underruns.load(Ordering::Relaxed),
                overruns: shared.overruns.load(Ordering::Relaxed),
                output_latency_micros: shared.latency_micros.load(Ordering::Relaxed),
                queued_micros: (current.queued() * 1_000 / MS) as u64,
                fallback,
            };
            if next != snapshot {
                snapshot = next;
                shared.publish(generation, snapshot.clone());
            }
        }
        // A backend call may consume the unpark token: recheck before sleeping.
        if !shared.current(generation) {
            continue;
        }
        if active.is_some() {
            thread::park_timeout(POLL_INTERVAL);
        } else if let Some(due) = &reopen {
            thread::park_timeout(due.at.saturating_duration_since(Instant::now()));
        } else {
            thread::park();
        }
    }
}

/// After a failed attempt: the next try, else the default, else failure.
fn retry(
    shared: &Shared,
    generation: u64,
    snapshot: &mut PlaybackSnapshot,
    due: &Reopen,
    chosen: bool,
    error: MediaError,
    now: Instant,
) -> Option<Reopen> {
    if let Some(next) = next_try(due, chosen, now) {
        return Some(next);
    }
    *snapshot = PlaybackSnapshot {
        state: CaptureState::Failed(error.0),
        ..PlaybackSnapshot::default()
    };
    shared.publish(generation, snapshot.clone());
    None
}

#[cfg(test)]
mod tests;
