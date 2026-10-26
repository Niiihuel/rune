//! A latest-request capture controller and a bounded real-time ingress.

use std::any::Any;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample};
use rtrb::{Consumer, Producer, RingBuffer};
use rune_i18n::t;

use crate::playback::{device_id, next_try, Linear, Reopen, DEFAULT_CHECK, RETRIES};
use crate::{
    CaptureProcessor, EchoReference, Feed, MediaError, ProcessingMode, PLAYBACK_RATE,
    REFERENCE_FRAME,
};

const POLL_INTERVAL: Duration = Duration::from_millis(4);
/// A lost loopback waits this long, doubling per loss up to 16 times it.
#[cfg(not(test))]
const LOOPBACK_RETRY: Duration = Duration::from_secs(2);
#[cfg(test)]
const LOOPBACK_RETRY: Duration = Duration::from_millis(5);
/// Reference queued beyond this is old: dropping it bounds the misalignment
/// the canceller's own delay estimate has to absorb. Running steadily it is
/// never reached: everything waiting is fed before each capture frame.
const REFERENCE_BACKLOG: usize = 80 * REFERENCE_FRAME / 10;
/// Reference frames fed before one capture frame, at most: a stalled worker
/// catches up over a few frames instead of flooding the canceller.
const MAX_RENDER_FRAMES: usize = REFERENCE_BACKLOG / REFERENCE_FRAME;
/// Capture frames with nothing from the system's loopback before RUNE's own
/// reference stands in, 500 ms: the output stopped or the stream died.
const SYSTEM_IDLE_FRAMES: u32 = 50;
const STALE_AFTER: Duration = Duration::from_millis(250);
const MAX_RATE: u32 = 192_000;
const MAX_CHANNELS: u16 = 8;
/// Worker-side diagnostics only: aggregate levels, never samples.
const DIAGNOSTIC_INTERVAL: Duration = Duration::from_secs(5);

/// The complete desired configuration. Identical requests are a no-op.
#[derive(Clone, Debug, PartialEq)]
pub struct CaptureRequest {
    pub device: Option<String>,
    pub mode: ProcessingMode,
    pub muted: bool,
    /// Cancel the echo of what the [`EchoReference`] given to
    /// [`Capture::set_echo_reference`] plays; with [`Self::system_echo`],
    /// of what the whole system plays.
    pub echo_cancellation: bool,
    /// Bring speech to a steady level, digitally; see
    /// [`CaptureProcessor::set_automatic_gain`].
    pub automatic_gain: bool,
    /// Send the processed microphone to the [`Feed`] given to
    /// [`Capture::set_monitor`], to hear oneself.
    pub monitor: bool,
    /// Silence what RNNoise does not hear as speech; see
    /// [`CaptureProcessor::set_speech_gate`].
    pub speech_gate: bool,
    /// With echo cancellation, cancel also what other applications play on
    /// [`Self::output`] (a game, music): its loopback is the canceller's
    /// reference instead of RUNE's own, which it already holds. It is only
    /// a reference: never sent, played, measured or kept.
    pub system_echo: bool,
    /// The output RUNE plays to, `None` for the system's default: the one
    /// [`Self::system_echo`] listens to.
    pub output: Option<String>,
}

/// Device state; starting/opening never blocks the caller.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum CaptureState {
    #[default]
    Idle,
    Starting,
    Running,
    Failed(String),
}

/// Coherent metrics for one configuration generation. No audio samples leave
/// the worker. Levels are meter positions in 0..=1, not PCM amplitudes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CaptureSnapshot {
    pub state: CaptureState,
    pub raw_level: f32,
    pub processed_level: f32,
    /// Mono samples dropped because the capture ring was full.
    pub dropped_samples: u64,
    pub processed_frames: u64,
    /// Processing time of the latest complete frame, not end-to-end latency.
    pub processing_micros: u64,
    /// The latest frame went through echo cancellation with a played reference.
    pub echo_cancelling: bool,
    /// Echo reference samples dropped as too old to align.
    pub reference_dropped: u64,
    /// The canceller's reference is the system's loopback: what every
    /// application plays on the output, RUNE included.
    pub system_echo: bool,
    /// The chosen microphone is missing: the system default is captured in
    /// its place, and the chosen one is opened again once it is back.
    pub fallback: bool,
}

struct Desired {
    generation: u64,
    request: Option<CaptureRequest>,
}

struct Published {
    generation: u64,
    snapshot: CaptureSnapshot,
}

struct Shared {
    desired: ArcSwap<Desired>,
    published: ArcSwap<Published>,
    requested: AtomicU64,
    ready: AtomicU64,
    muted: AtomicBool,
    /// The input volume's `f32` bits: it changes while a slider is dragged,
    /// so it is read every frame instead of restarting the processing.
    input_volume: AtomicU32,
    shutdown: AtomicBool,
    worker_failed: AtomicBool,
    /// Hand-offs to the worker; only non-real-time threads take this lock.
    links: Mutex<Links>,
}

#[derive(Default)]
struct Links {
    reference: Option<EchoReference>,
    monitor: Option<Feed>,
    /// Where a call takes the processed microphone; dropped once its reader goes.
    transmit: Option<Feed>,
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
                snapshot: CaptureSnapshot::default(),
            }),
            requested: AtomicU64::new(0),
            ready: AtomicU64::new(0),
            muted: AtomicBool::new(true),
            input_volume: AtomicU32::new(1.0_f32.to_bits()),
            shutdown: AtomicBool::new(false),
            worker_failed: AtomicBool::new(false),
            links: Mutex::new(Links::default()),
        }
    }

    fn current(&self, generation: u64) -> bool {
        !self.shutdown.load(Ordering::Acquire)
            && self.requested.load(Ordering::Acquire) == generation
    }

    fn publish(&self, generation: u64, snapshot: CaptureSnapshot) {
        if self.current(generation) {
            self.published.store(Arc::new(Published {
                generation,
                snapshot,
            }));
        }
    }

    /// Only atomics are used here; the callback never loads an ArcSwap guard.
    fn admitted_generation(&self) -> Option<u64> {
        let generation = self.requested.load(Ordering::Acquire);
        (generation != 0
            && !self.shutdown.load(Ordering::Acquire)
            && !self.muted.load(Ordering::Acquire)
            && self.ready.load(Ordering::Acquire) == generation)
            .then_some(generation)
    }
}

/// Nonblocking handle to one capture/control worker. Creating it does not open
/// a microphone. Stop/mute invalidate admission immediately; stream teardown
/// happens on the worker. Drop signals shutdown and never joins on the UI.
pub struct Capture {
    shared: Arc<Shared>,
    worker: Thread,
    generation: u64,
    request: Option<CaptureRequest>,
}

impl Capture {
    pub fn new() -> Result<Self, MediaError> {
        Self::with_backend(CpalBackend)
    }

    fn with_backend<B: Backend + Send + 'static>(backend: B) -> Result<Self, MediaError> {
        let shared = Arc::new(Shared::new());
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("rune-capture".into())
            .spawn(move || run(backend, worker_shared))
            .map_err(|_| MediaError(t("No se pudo iniciar el audio local.").into()))?;
        // Detach; audio backend teardown must never be joined by the UI.
        let worker = worker.thread().clone();
        Ok(Self {
            shared,
            worker,
            generation: 0,
            request: None,
        })
    }

    /// Replace the latest desired state, without a queue backlog or waiting for
    /// the audio system. To retry an identical failed request, configure None
    /// before configuring it again; both changes get a new generation.
    pub fn configure(&mut self, request: Option<CaptureRequest>) {
        if self.request == request {
            return;
        }
        self.generation = self.generation.wrapping_add(1).max(1);
        // Invalidate the old callback before publishing the new request.
        self.shared
            .requested
            .store(self.generation, Ordering::Release);
        self.shared.muted.store(
            request.as_ref().is_none_or(|request| request.muted),
            Ordering::Release,
        );
        self.request = request;
        self.shared.desired.store(Arc::new(Desired {
            generation: self.generation,
            request: self.request.clone(),
        }));
        self.worker.unpark();
    }

    /// The input volume, from 0 to [`crate::MAX_INPUT_VOLUME`]; it applies
    /// from the next frame, without restarting anything.
    pub fn set_input_volume(&self, volume: f32) {
        self.shared
            .input_volume
            .store(volume.to_bits(), Ordering::Relaxed);
    }

    /// What the output plays, for [`CaptureRequest::echo_cancellation`].
    pub fn set_echo_reference(&self, reference: EchoReference) {
        self.links().reference = Some(reference);
        self.worker.unpark();
    }

    /// Where [`CaptureRequest::monitor`] sends the processed microphone.
    pub fn set_monitor(&self, feed: Feed) {
        self.links().monitor = Some(feed);
        self.worker.unpark();
    }

    /// Where a call reads the processed microphone, at [`PLAYBACK_RATE`],
    /// while capture runs unmuted. It stops when the [`crate::Tap`] is dropped.
    pub fn set_transmit(&self, feed: Feed) {
        self.links().transmit = Some(feed);
        self.worker.unpark();
    }

    fn links(&self) -> std::sync::MutexGuard<'_, Links> {
        self.shared
            .links
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn snapshot(&self) -> CaptureSnapshot {
        let Some(request) = &self.request else {
            return CaptureSnapshot::default();
        };
        if self.shared.worker_failed.load(Ordering::Acquire) {
            return CaptureSnapshot {
                state: CaptureState::Failed(t("El procesamiento de audio se detuvo.").into()),
                ..CaptureSnapshot::default()
            };
        }
        let published = self.shared.published.load();
        let mut snapshot = if published.generation == self.generation {
            published.snapshot.clone()
        } else {
            CaptureSnapshot {
                state: CaptureState::Starting,
                ..CaptureSnapshot::default()
            }
        };
        // Mute takes effect even if the worker is opening a device or processing.
        if request.muted {
            snapshot.raw_level = 0.0;
            snapshot.processed_level = 0.0;
        }
        snapshot
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        self.shared.ready.store(0, Ordering::Release);
        self.worker.unpark();
    }
}

#[derive(Default)]
struct StreamSignals {
    failed: AtomicBool,
    dropped: AtomicU64,
}

#[derive(Clone, Copy)]
struct Sample {
    value: f32,
    generation: u64,
    sequence: u64,
}

/// The only mutable state owned by a real-time callback. Partial interleaved
/// channel frames survive callback boundaries, but never a generation change.
struct Ingress {
    producer: Producer<Sample>,
    shared: Arc<Shared>,
    signals: Arc<StreamSignals>,
    channels: u16,
    channel: u16,
    sum: f32,
    generation: u64,
    sequence: u64,
}

impl Ingress {
    fn write<T>(&mut self, samples: &[T])
    where
        T: SizedSample,
        f32: FromSample<T>,
    {
        let Some(generation) = self.shared.admitted_generation() else {
            self.channel = 0;
            self.sum = 0.0;
            return;
        };
        if self.generation != generation {
            self.channel = 0;
            self.sum = 0.0;
            self.generation = generation;
        }
        let mut dropped = 0;
        for sample in samples {
            let sample = sample.to_sample::<f32>();
            self.sum += if sample.is_finite() {
                sample.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            self.channel += 1;
            if self.channel == self.channels {
                let sample = Sample {
                    value: self.sum / f32::from(self.channels),
                    generation,
                    sequence: self.sequence,
                };
                self.sequence = self.sequence.wrapping_add(1);
                self.channel = 0;
                self.sum = 0.0;
                if self.producer.push(sample).is_err() {
                    dropped += 1;
                }
            }
        }
        if dropped != 0 {
            self.signals.dropped.fetch_add(dropped, Ordering::Relaxed);
        }
    }
}

fn ring(
    rate: u32,
    channels: u16,
    shared: Arc<Shared>,
) -> Result<(Ingress, Consumer<Sample>), MediaError> {
    // This bounds the ring to 200ms (under 1 MiB at the highest admitted rate),
    // and the frame accumulator to 1920 mono samples. Exotic rates need an
    // explicit fractional-frame adapter before being admitted.
    if !(8_000..=MAX_RATE).contains(&rate) || !rate.is_multiple_of(100) {
        return Err(MediaError(
            t("La frecuencia del micrófono no es compatible.").into(),
        ));
    }
    if !(1..=MAX_CHANNELS).contains(&channels) {
        return Err(MediaError(
            t("El micrófono tiene una cantidad de canales no compatible.").into(),
        ));
    }
    let (producer, consumer) = RingBuffer::new(rate as usize / 5);
    Ok((
        Ingress {
            producer,
            shared,
            signals: Arc::new(StreamSignals::default()),
            channels,
            channel: 0,
            sum: 0.0,
            generation: 0,
            sequence: 0,
        },
        consumer,
    ))
}

trait InputStream {
    fn start(&self) -> Result<(), MediaError>;
}

impl InputStream for cpal::Stream {
    fn start(&self) -> Result<(), MediaError> {
        StreamTrait::start(self).map_err(|error| {
            eprintln!("rune: microphone: start failed: {error}");
            MediaError(t("No se pudo activar el micrófono.").into())
        })
    }
}

struct Opened<S> {
    stream: S,
    consumer: Consumer<Sample>,
    signals: Arc<StreamSignals>,
    rate: u32,
    /// The system default this stream opened, when it captures the default.
    default: Option<String>,
}

/// An open loopback of an output: the stream, kept only to stay open, a
/// flag its error callback raises, and what it records, as a reference.
struct Loopback {
    _stream: Box<dyn Any>,
    failed: Arc<AtomicBool>,
    reference: EchoReference,
}

trait Backend {
    type Stream: InputStream;
    fn open(
        &mut self,
        device: Option<&str>,
        shared: Arc<Shared>,
    ) -> Result<Opened<Self::Stream>, MediaError>;
    /// What the system plays on `output` (`None`: the default output).
    fn open_loopback(&mut self, output: Option<&str>) -> Result<Loopback, MediaError>;
    /// The system's default microphone now.
    fn default_input(&mut self) -> Option<String>;
    /// Whether the microphone with this id is connected now.
    fn present(&mut self, id: &str) -> bool;
}

struct CpalBackend;

impl Backend for CpalBackend {
    type Stream = cpal::Stream;

    fn open(
        &mut self,
        id: Option<&str>,
        shared: Arc<Shared>,
    ) -> Result<Opened<Self::Stream>, MediaError> {
        let host = cpal::default_host();
        let chosen = match id {
            Some(id) => {
                if id.len() > 4096 {
                    return Err(MediaError(
                        t("El identificador del micrófono no es válido.").into(),
                    ));
                }
                let id = id.parse().map_err(|_| {
                    MediaError(t("El identificador del micrófono no es válido.").into())
                })?;
                Some(host.device_by_id(&id).ok_or_else(|| {
                    MediaError(t("El micrófono seleccionado no está disponible.").into())
                })?)
            }
            None => None,
        };
        // An output's monitor saved before `devices` hid them would send
        // what this machine plays: the default microphone instead.
        let device = match chosen.filter(|device| !crate::output_monitor(&host, device)) {
            Some(device) => device,
            None => host
                .default_input_device()
                .ok_or_else(|| MediaError(t("No hay un micrófono disponible.").into()))?,
        };
        let config = input_config(&device)?;
        eprintln!(
            "rune: microphone: opening backend={:?} device={:?} rate={} channels={} format={:?}",
            host.id(),
            device.to_string(),
            config.sample_rate(),
            config.channels(),
            config.sample_format()
        );
        let rate = config.sample_rate();
        let (ingress, consumer) = ring(rate, config.channels(), shared)?;
        let signals = ingress.signals.clone();
        let stream = match config.sample_format() {
            SampleFormat::F32 => open_stream::<f32>(&device, config.into(), ingress)?,
            SampleFormat::F64 => open_stream::<f64>(&device, config.into(), ingress)?,
            SampleFormat::I8 => open_stream::<i8>(&device, config.into(), ingress)?,
            SampleFormat::I16 => open_stream::<i16>(&device, config.into(), ingress)?,
            SampleFormat::I32 => open_stream::<i32>(&device, config.into(), ingress)?,
            SampleFormat::I64 => open_stream::<i64>(&device, config.into(), ingress)?,
            SampleFormat::U8 => open_stream::<u8>(&device, config.into(), ingress)?,
            SampleFormat::U16 => open_stream::<u16>(&device, config.into(), ingress)?,
            SampleFormat::U32 => open_stream::<u32>(&device, config.into(), ingress)?,
            SampleFormat::U64 => open_stream::<u64>(&device, config.into(), ingress)?,
            _ => {
                return Err(MediaError(
                    t("El formato de audio del micrófono no es compatible.").into(),
                ));
            }
        };
        Ok(Opened {
            stream,
            consumer,
            signals,
            rate,
            default: id.is_none().then(|| device_id(&device)),
        })
    }

    fn open_loopback(&mut self, output: Option<&str>) -> Result<Loopback, MediaError> {
        let (stream, failed, reference) = crate::loopback::open(output)?;
        Ok(Loopback {
            _stream: Box::new(stream),
            failed,
            reference,
        })
    }

    fn default_input(&mut self) -> Option<String> {
        cpal::default_host()
            .default_input_device()
            .map(|device| device_id(&device))
    }

    fn present(&mut self, id: &str) -> bool {
        id.parse()
            .ok()
            .and_then(|id| cpal::default_host().device_by_id(&id))
            .is_some()
    }
}

/// The device's own configuration if it is at [`PLAYBACK_RATE`], else the
/// same at that rate when the device offers it, as Serein asks: RNNoise works
/// only there, and resampling is then not needed. Otherwise its own, which
/// the worker resamples.
fn input_config(device: &cpal::Device) -> Result<cpal::SupportedStreamConfig, MediaError> {
    let default = device.default_input_config().map_err(|error| {
        eprintln!("rune: microphone: no input format: {error}");
        MediaError(t("No se pudo consultar el micrófono.").into())
    })?;
    if default.sample_rate() == PLAYBACK_RATE && input_config_supported(&default) {
        return Ok(default);
    }
    let offered = device
        .supported_input_configs()
        .map(|configs| configs.take(64).collect::<Vec<_>>())
        .unwrap_or_default();
    Ok(preferred_input_config(default, &offered))
}

/// Only choose formats the input callback can convert. WASAPI also offers
/// packed 24-bit samples, which cannot use the integer callbacks below.
fn input_format_supported(format: SampleFormat) -> bool {
    matches!(
        format,
        SampleFormat::F32
            | SampleFormat::F64
            | SampleFormat::I8
            | SampleFormat::I16
            | SampleFormat::I32
            | SampleFormat::I64
            | SampleFormat::U8
            | SampleFormat::U16
            | SampleFormat::U32
            | SampleFormat::U64
    )
}

fn input_rate_supported(rate: u32) -> bool {
    (8_000..=MAX_RATE).contains(&rate) && rate.is_multiple_of(100)
}

fn input_config_supported(config: &cpal::SupportedStreamConfig) -> bool {
    input_rate_supported(config.sample_rate())
        && (1..=MAX_CHANNELS).contains(&config.channels())
        && input_format_supported(config.sample_format())
}

fn preferred_input_config(
    default: cpal::SupportedStreamConfig,
    offered: &[cpal::SupportedStreamConfigRange],
) -> cpal::SupportedStreamConfig {
    if default.sample_rate() == PLAYBACK_RATE && input_config_supported(&default) {
        return default;
    }
    let compatible = |range: &&cpal::SupportedStreamConfigRange| {
        (1..=MAX_CHANNELS).contains(&range.channels())
            && input_format_supported(range.sample_format())
    };
    let original = (default.channels(), default.sample_format());
    let at_rate = |rate| {
        if !input_rate_supported(rate) {
            return None;
        }
        offered
            .iter()
            .filter(compatible)
            .filter(|range| (range.channels(), range.sample_format()) == original)
            .chain(offered.iter().filter(compatible))
            .find_map(|range| range.try_with_sample_rate(rate))
    };
    if let Some(preferred) = at_rate(PLAYBACK_RATE) {
        return preferred;
    }
    if input_config_supported(&default) {
        return default;
    }
    // An unsupported default format may have a compatible alternative at
    // the same rate. The worker already resamples non-48 kHz capture.
    if let Some(at_default_rate) = at_rate(default.sample_rate()) {
        return at_default_rate;
    }
    // Otherwise stay inside both the offered range and our ingress bounds.
    // Its highest admitted whole-frame rate is enough; no device is opened
    // while choosing it. With no compatible offer, the normal open path
    // reports why the original configuration cannot be used.
    offered
        .iter()
        .filter(compatible)
        .find_map(|range| {
            let rate = range.max_sample_rate().min(MAX_RATE) / 100 * 100;
            input_rate_supported(rate)
                .then(|| range.try_with_sample_rate(rate))
                .flatten()
        })
        .unwrap_or(default)
}

fn open_stream<T>(
    device: &cpal::Device,
    config: cpal::StreamConfig,
    mut ingress: Ingress,
) -> Result<cpal::Stream, MediaError>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let signals = ingress.signals.clone();
    device
        .build_input_stream::<T, _, _>(
            config,
            move |samples, _| ingress.write(samples),
            move |error| {
                // The worker reopens the device: a default change, an unplug
                // or a sleep all end the stream here.
                eprintln!("rune: microphone: stream lost: {error}");
                signals.failed.store(true, Ordering::Release);
            },
            Some(Duration::from_secs(2)),
        )
        .map_err(|error| {
            eprintln!("rune: microphone: open failed: {error}");
            MediaError(t("No se pudo abrir el micrófono seleccionado.").into())
        })
}

struct Active<S> {
    opened: Opened<S>,
    device: Option<String>,
    /// At [`PLAYBACK_RATE`], whatever the microphone's: RNNoise runs at no
    /// other, and what it hands on needs no conversion.
    processor: CaptureProcessor,
    mode: ProcessingMode,
    echo: bool,
    monitor: bool,
    /// The system's loopback while asked for and open, the output it
    /// listens to, and capture frames since it last gave anything.
    system: Option<(Loopback, Option<String>, u32)>,
    /// The output whose loopback could not be opened: not tried again
    /// until the request changes.
    system_failed: Option<Option<String>>,
    /// A loopback that died is opened again at this time, after a pause
    /// that doubles with each loss.
    system_retry: Option<Instant>,
    system_lost: u32,
    /// The capture started over (opened, a new request such as a mute, a
    /// gap): what waits in the references played before it.
    fresh: bool,
    reference: Vec<f32>,
    /// From the microphone's rate to the processor's, when they differ.
    resampler: Option<Linear>,
    frame: Vec<f32>,
    next_sequence: Option<u64>,
    dropped_at_start: u64,
    snapshot: CaptureSnapshot,
    last_complete: Instant,
    stale: bool,
    diagnostics: CaptureDiagnostics,
}

/// Peaks and counter deltas since the worker's previous diagnostic line.
/// Nothing in the real-time callback reads or writes this state.
struct CaptureDiagnostics {
    at: Instant,
    frames: u64,
    dropped: u64,
    raw_peak: f32,
    processed_peak: f32,
}

#[derive(Debug, PartialEq)]
struct CaptureReport {
    frames: u64,
    dropped: u64,
    raw_peak: f32,
    processed_peak: f32,
}

impl CaptureDiagnostics {
    fn new(now: Instant) -> Self {
        Self {
            at: now,
            frames: 0,
            dropped: 0,
            raw_peak: 0.0,
            processed_peak: 0.0,
        }
    }

    fn observe(&mut self, raw: f32, processed: f32) {
        self.raw_peak = self.raw_peak.max(raw);
        self.processed_peak = self.processed_peak.max(processed);
    }

    fn take(&mut self, now: Instant, snapshot: &CaptureSnapshot) -> Option<CaptureReport> {
        if now.saturating_duration_since(self.at) < DIAGNOSTIC_INTERVAL {
            return None;
        }
        let report = CaptureReport {
            frames: snapshot.processed_frames.saturating_sub(self.frames),
            dropped: snapshot.dropped_samples.saturating_sub(self.dropped),
            raw_peak: std::mem::take(&mut self.raw_peak),
            processed_peak: std::mem::take(&mut self.processed_peak),
        };
        self.at = now;
        self.frames = snapshot.processed_frames;
        self.dropped = snapshot.dropped_samples;
        Some(report)
    }
}

impl<S> Active<S> {
    fn new(opened: Opened<S>, request: &CaptureRequest) -> Result<Self, MediaError> {
        let mut processor = CaptureProcessor::new(PLAYBACK_RATE, request.mode)?;
        processor.set_automatic_gain(request.automatic_gain);
        processor.set_speech_gate(request.speech_gate);
        let frame = Vec::with_capacity(processor.frame_len());
        Ok(Self {
            resampler: resampler(opened.rate),
            opened,
            device: request.device.clone(),
            processor,
            mode: request.mode,
            echo: request.echo_cancellation,
            monitor: request.monitor,
            system: None,
            system_failed: None,
            system_retry: None,
            system_lost: 0,
            fresh: true,
            reference: vec![0.0; REFERENCE_FRAME],
            frame,
            next_sequence: None,
            dropped_at_start: 0,
            snapshot: CaptureSnapshot {
                state: CaptureState::Running,
                ..CaptureSnapshot::default()
            },
            last_complete: Instant::now(),
            stale: false,
            diagnostics: CaptureDiagnostics::new(Instant::now()),
        })
    }

    fn reconfigure(&mut self, request: &CaptureRequest) {
        let mode = request.mode;
        self.echo = request.echo_cancellation;
        self.monitor = request.monitor;
        self.processor.set_automatic_gain(request.automatic_gain);
        self.processor.set_speech_gate(request.speech_gate);
        self.resampler = resampler(self.opened.rate);
        if self.mode == mode {
            self.processor.reset();
        } else {
            self.processor.set_mode(mode);
            self.mode = mode;
        }
        self.frame.clear();
        self.next_sequence = None;
        self.fresh = true;
        self.system_failed = None;
        self.system_retry = None;
        // Admission is closed while reconfiguring. An in-flight callback can
        // still finish; generation tags reject its samples in step().
        for _ in 0..self.opened.rate as usize / 5 {
            if self.opened.consumer.pop().is_err() {
                break;
            }
        }
        self.dropped_at_start = self.opened.signals.dropped.load(Ordering::Relaxed);
        self.snapshot = CaptureSnapshot {
            state: CaptureState::Running,
            ..CaptureSnapshot::default()
        };
        self.last_complete = Instant::now();
        self.stale = false;
        self.diagnostics = CaptureDiagnostics::new(self.last_complete);
    }

    /// Open or close the system's loopback as `request` asks: with echo
    /// cancellation and [`CaptureRequest::system_echo`], on its output. A
    /// failed one is not retried until the request changes; a lost one is
    /// retried after a pause (see [`Self::system_due`]).
    fn steer_system<B: Backend>(
        &mut self,
        backend: &mut B,
        request: &CaptureRequest,
        now: Instant,
    ) {
        let wanted = (request.echo_cancellation && request.system_echo).then_some(&request.output);
        let lost = self.system.as_ref().is_some_and(|(loopback, output, _)| {
            Some(output) == wanted && loopback.failed.load(Ordering::Acquire)
        });
        if self.system.as_ref().is_some_and(|(loopback, output, _)| {
            Some(output) != wanted || loopback.failed.load(Ordering::Acquire)
        }) {
            self.system = None;
        }
        if lost {
            self.system_lost = self.system_lost.saturating_add(1);
            self.system_retry = Some(now + LOOPBACK_RETRY * (1_u32 << self.system_lost.min(4)));
        }
        let Some(output) = wanted.filter(|output| {
            self.system.is_none()
                && self.system_failed.as_ref() != Some(*output)
                && self.system_retry.is_none_or(|at| now >= at)
        }) else {
            return;
        };
        match backend.open_loopback(output.as_deref()) {
            Ok(loopback) => {
                self.system = Some((loopback, output.clone(), 0));
                self.system_retry = None;
            }
            Err(_) => self.system_failed = Some(output.clone()),
        }
    }

    /// Whether the loopback needs [`Self::steer_system`] now: it died, or its
    /// pause after a loss is over.
    fn system_due(&self, now: Instant) -> bool {
        match &self.system {
            Some((loopback, ..)) => loopback.failed.load(Ordering::Acquire),
            None => self.system_retry.is_some_and(|at| now >= at),
        }
    }

    /// Before a capture frame, give the canceller everything played since
    /// the last one, as soon as it is there: the echo always comes after
    /// what made it, by however long the devices take, and the canceller
    /// finds that delay itself. One reference only: the system's loopback
    /// while it gives sound (it holds RUNE's too), else RUNE's own.
    fn cancel_echo(&mut self, reference: Option<&mut EchoReference>) -> Result<(), MediaError> {
        if !self.echo {
            self.processor.set_echo_cancellation(false);
            self.snapshot.system_echo = false;
            return Ok(());
        }
        if let Some((loopback, _, idle)) = &mut self.system {
            *idle = if loopback.reference.available() > 0 {
                0
            } else {
                idle.saturating_add(1)
            };
        }
        let system = self
            .system
            .as_mut()
            .filter(|(_, _, idle)| *idle < SYSTEM_IDLE_FRAMES)
            .map(|(loopback, ..)| &mut loopback.reference);
        let using_system = system.is_some();
        let (source, unused) = match system {
            Some(system) => (Some(system), reference),
            None => (
                reference,
                self.system
                    .as_mut()
                    .map(|(loopback, ..)| &mut loopback.reference),
            ),
        };
        if let Some(unused) = unused {
            unused.clear();
        }
        self.snapshot.system_echo = using_system;
        let Some(source) = source else {
            self.processor.set_echo_cancellation(false);
            return Ok(());
        };
        self.processor.set_echo_cancellation(true);
        // What played while nothing was read goes. Here, at the first frame
        // after a restart, as when the capture first opened: the canceller
        // then finds what follows where it was, and keeps the echo's delay.
        if std::mem::take(&mut self.fresh) {
            source.clear();
        }
        let dropped = source.trim(REFERENCE_BACKLOG) as u64;
        self.snapshot.reference_dropped = self.snapshot.reference_dropped.saturating_add(dropped);
        for _ in 0..MAX_RENDER_FRAMES {
            if !source.read(&mut self.reference) {
                break;
            }
            self.processor
                .render(&self.reference, source.output_latency_ms())?;
        }
        Ok(())
    }

    /// Send the processed frame, at the output's rate already, to whoever
    /// listens: the monitor when asked, and a call's transmission.
    fn loop_back(&mut self, links: &mut Links) {
        if links
            .transmit
            .as_ref()
            .is_some_and(|feed| !feed.connected())
        {
            links.transmit = None;
        }
        if let Some(feed) = links.monitor.as_mut().filter(|_| self.monitor) {
            feed.push(&self.frame);
        }
        if let Some(feed) = links.transmit.as_mut() {
            feed.push(&self.frame);
        }
    }

    /// Process the frame once full: echo, DSP, then where it goes.
    fn frame_full(
        &mut self,
        shared: &Shared,
        now: Instant,
        links: &mut Links,
    ) -> Result<(), MediaError> {
        self.cancel_echo(links.reference.as_mut())?;
        self.processor
            .set_input_volume(f32::from_bits(shared.input_volume.load(Ordering::Relaxed)));
        let metrics = self.processor.process(&mut self.frame)?;
        self.diagnostics
            .observe(metrics.raw_level, metrics.processed_level);
        self.loop_back(links);
        self.frame.clear();
        self.snapshot.echo_cancelling = self.processor.echo_cancellation();
        self.snapshot.raw_level = metrics.raw_level;
        self.snapshot.processed_level = metrics.processed_level;
        self.snapshot.processing_micros = metrics.elapsed_micros;
        self.snapshot.processed_frames = self.snapshot.processed_frames.saturating_add(1);
        self.last_complete = now;
        self.stale = false;
        Ok(())
    }

    fn step(
        &mut self,
        shared: &Shared,
        generation: u64,
        now: Instant,
        links: &mut Links,
    ) -> Result<bool, MediaError> {
        if self.opened.signals.failed.load(Ordering::Acquire) {
            return Err(MediaError(
                t("El micrófono dejó de responder. Vuelve a probar.").into(),
            ));
        }
        let mut changed = false;
        for _ in 0..self.opened.rate as usize / 5 {
            if shared.admitted_generation() != Some(generation) {
                break;
            }
            let Ok(sample) = self.opened.consumer.pop() else {
                break;
            };
            if sample.generation != generation {
                continue;
            }
            if self
                .next_sequence
                .is_some_and(|next| next != sample.sequence)
            {
                // Never join audio from either side of a lost interval.
                self.frame.clear();
                self.processor.reset();
                self.fresh = true;
                self.resampler = resampler(self.opened.rate);
            }
            self.next_sequence = Some(sample.sequence.wrapping_add(1));
            let mut pending = Some(sample.value);
            loop {
                let next = match self.resampler.as_mut() {
                    Some(resampler) => resampler.next(|| pending.take()),
                    None => pending.take(),
                };
                let Some(value) = next else { break };
                self.frame.push(value);
                if self.frame.len() == self.processor.frame_len() {
                    self.frame_full(shared, now, links)?;
                    changed = true;
                }
            }
        }
        if !self.stale && now.saturating_duration_since(self.last_complete) >= STALE_AFTER {
            self.frame.clear();
            self.next_sequence = None;
            self.processor.reset();
            self.fresh = true;
            self.snapshot.raw_level = 0.0;
            self.snapshot.processed_level = 0.0;
            self.stale = true;
            changed = true;
        }
        let dropped = self
            .opened
            .signals
            .dropped
            .load(Ordering::Relaxed)
            .saturating_sub(self.dropped_at_start);
        changed |= self.snapshot.dropped_samples != dropped;
        self.snapshot.dropped_samples = dropped;
        if let Some(report) = self.diagnostics.take(now, &self.snapshot) {
            eprintln!(
                "rune: microphone: frames={} dropped={} raw_peak={:.3} processed_peak={:.3} stale={} echo={} system_echo={}",
                report.frames,
                report.dropped,
                report.raw_peak,
                report.processed_peak,
                self.stale,
                self.snapshot.echo_cancelling,
                self.snapshot.system_echo
            );
        }
        Ok(changed)
    }
}

/// From a microphone at `rate` to [`PLAYBACK_RATE`], unless it is there.
fn resampler(rate: u32) -> Option<Linear> {
    (rate != PLAYBACK_RATE).then(|| Linear::new(rate, PLAYBACK_RATE))
}

struct WorkerLife(Arc<Shared>);

impl Drop for WorkerLife {
    fn drop(&mut self) {
        self.0.ready.store(0, Ordering::Release);
        if !self.0.shutdown.load(Ordering::Acquire) {
            self.0.worker_failed.store(true, Ordering::Release);
        }
    }
}

/// Opens and starts `target` for `request`: `None` when the request went
/// obsolete meanwhile, and nothing is kept then.
fn open_active<B: Backend>(
    backend: &mut B,
    shared: &Arc<Shared>,
    generation: u64,
    request: &CaptureRequest,
    target: Option<String>,
) -> Result<Option<Active<B::Stream>>, MediaError> {
    let opened = backend.open(target.as_deref(), shared.clone())?;
    // A delayed device open cannot activate an obsolete request.
    if !shared.current(generation) {
        return Ok(None);
    }
    let mut next = Active::new(opened, request)?;
    next.device = target;
    // Admission stays closed until play returns and is checked again, so even
    // a concurrent stop cannot admit samples.
    if !shared.current(generation) {
        return Ok(None);
    }
    next.opened.stream.start()?;
    Ok(shared.current(generation).then_some(next))
}

fn run<B: Backend>(mut backend: B, shared: Arc<Shared>) {
    let _life = WorkerLife(shared.clone());
    let mut generation = 0;
    let mut active: Option<Active<B::Stream>> = None;
    let mut links = Links::default();
    // The device the latest request named, and whether the system default
    // stands in for it while it is missing.
    let mut requested: Option<Option<String>> = None;
    let mut fallback = false;
    // The next attempt to open the device: after a failure or a lost stream.
    let mut reopen: Option<Reopen> = None;
    let mut checked = Instant::now();
    loop {
        if shared.shutdown.load(Ordering::Acquire) {
            if active.is_some() {
                eprintln!("rune: microphone: capture stopped (shutdown)");
            }
            break;
        }
        if let Ok(mut offered) = shared.links.try_lock() {
            if let Some(reference) = offered.reference.take() {
                links.reference = Some(reference);
            }
            if let Some(feed) = offered.monitor.take() {
                links.monitor = Some(feed);
            }
            if let Some(feed) = offered.transmit.take() {
                links.transmit = Some(feed);
            }
        }
        let desired = shared.desired.load_full();
        if desired.generation != generation {
            generation = desired.generation;
            shared.ready.store(0, Ordering::Release);
            let Some(request) = &desired.request else {
                if active.is_some() {
                    eprintln!("rune: microphone: capture stopped");
                }
                active = None;
                reopen = None;
                requested = None;
                fallback = false;
                shared.publish(generation, CaptureSnapshot::default());
                continue;
            };
            if requested.as_ref() != Some(&request.device) {
                fallback = false;
            }
            requested = Some(request.device.clone());
            reopen = None;
            shared.publish(
                generation,
                CaptureSnapshot {
                    state: CaptureState::Starting,
                    ..CaptureSnapshot::default()
                },
            );
            eprintln!(
                "rune: microphone: configured mode={:?} muted={} echo={} automatic_gain={} speech_gate={}",
                request.mode,
                request.muted,
                request.echo_cancellation,
                request.automatic_gain,
                request.speech_gate
            );
            let target = if fallback {
                None
            } else {
                request.device.clone()
            };
            let result = if active.as_ref().is_some_and(|active| {
                active.device == target && !active.opened.signals.failed.load(Ordering::Acquire)
            }) {
                active
                    .as_mut()
                    .expect("active checked above")
                    .reconfigure(request);
                Ok(())
            } else {
                active = None;
                open_active(&mut backend, &shared, generation, request, target).map(|next| {
                    active = next;
                })
            };
            if let Some(current) = active.as_mut().filter(|_| result.is_ok()) {
                current.steer_system(&mut backend, request, Instant::now());
                current.snapshot.fallback = fallback;
            }
            match result {
                Ok(()) if shared.current(generation) && active.is_some() => {
                    shared.ready.store(generation, Ordering::Release);
                    shared.publish(
                        generation,
                        active
                            .as_ref()
                            .expect("active checked above")
                            .snapshot
                            .clone(),
                    );
                }
                Err(error) => {
                    eprintln!("rune: microphone: capture failed: {error}");
                    active = None;
                    // A device that will not open is tried again, then the
                    // default stands in for a chosen one, then it fails.
                    let now = Instant::now();
                    let failed = Reopen {
                        at: now,
                        left: RETRIES,
                        fallback,
                    };
                    reopen = next_try(&failed, request.device.is_some(), now);
                    if reopen.is_none() {
                        shared.publish(
                            generation,
                            CaptureSnapshot {
                                state: CaptureState::Failed(error.0),
                                ..CaptureSnapshot::default()
                            },
                        );
                    }
                }
                _ => {}
            }
        }
        if let Some(due) = reopen.take_if(|due| Instant::now() >= due.at) {
            if let Some(request) = &desired.request {
                fallback = due.fallback;
                shared.publish(
                    generation,
                    CaptureSnapshot {
                        state: CaptureState::Starting,
                        ..CaptureSnapshot::default()
                    },
                );
                let target = if fallback {
                    None
                } else {
                    request.device.clone()
                };
                match open_active(&mut backend, &shared, generation, request, target) {
                    Ok(Some(mut next)) => {
                        next.snapshot.fallback = fallback;
                        shared.ready.store(generation, Ordering::Release);
                        shared.publish(generation, next.snapshot.clone());
                        active = Some(next);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!("rune: microphone: capture failed: {error}");
                        reopen = next_try(&due, request.device.is_some(), Instant::now());
                        if reopen.is_none() {
                            shared.publish(
                                generation,
                                CaptureSnapshot {
                                    state: CaptureState::Failed(error.0),
                                    ..CaptureSnapshot::default()
                                },
                            );
                        }
                    }
                }
            }
        }
        // Set when the open stream has to be opened again.
        let mut restart: Option<Reopen> = None;
        if let (Some(current), Some(request)) = (&mut active, &desired.request) {
            let now = Instant::now();
            if current.system_due(now) {
                current.steer_system(&mut backend, request, now);
            }
            // On the default: follow it, and look for a chosen microphone that
            // was missing; either one reopens the stream at once.
            if current.device.is_none() && now.saturating_duration_since(checked) >= DEFAULT_CHECK {
                checked = now;
                let returned = fallback
                    && request
                        .device
                        .as_deref()
                        .is_some_and(|id| backend.present(id));
                if returned || backend.default_input() != current.opened.default {
                    restart = Some(Reopen {
                        at: now,
                        left: RETRIES,
                        fallback: fallback && !returned,
                    });
                }
            }
            if restart.is_none() {
                match current.step(&shared, generation, now, &mut links) {
                    Ok(true) => shared.publish(generation, current.snapshot.clone()),
                    Ok(false) => {}
                    Err(error) => {
                        // The stream was lost: open it again at once.
                        eprintln!("rune: microphone: capture failed: {error}");
                        restart = Some(Reopen {
                            at: now,
                            left: RETRIES,
                            fallback,
                        });
                    }
                }
            }
        }
        if let Some(due) = restart {
            active = None;
            shared.ready.store(0, Ordering::Release);
            shared.publish(
                generation,
                CaptureSnapshot {
                    state: CaptureState::Starting,
                    ..CaptureSnapshot::default()
                },
            );
            reopen = Some(due);
        }
        // A backend call may itself park and consume the thread's unpark token.
        // Recheck desired state after all such calls, before going idle.
        if !shared.current(generation) {
            continue;
        }
        if let Some(due) = &reopen {
            thread::park_timeout(due.at.saturating_duration_since(Instant::now()));
        } else if active.is_none() || shared.muted.load(Ordering::Acquire) {
            thread::park();
        } else {
            thread::park_timeout(POLL_INTERVAL);
        }
    }
}

#[cfg(test)]
mod tests;

/// The echo the capture path leaves, timed as the devices run.
#[cfg(test)]
pub(crate) mod eval;
