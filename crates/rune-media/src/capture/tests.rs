//! Synthetic inputs and a fake audio backend. These tests never open hardware.

use std::sync::mpsc::{self, Receiver, Sender};

use super::*;

fn config(rate: u32, format: SampleFormat) -> cpal::SupportedStreamConfig {
    cpal::SupportedStreamConfig::new(2, rate, cpal::SupportedBufferSize::Unknown, format)
}

fn offered(format: SampleFormat) -> cpal::SupportedStreamConfigRange {
    cpal::SupportedStreamConfigRange::new(
        2,
        PLAYBACK_RATE,
        PLAYBACK_RATE,
        cpal::SupportedBufferSize::Unknown,
        format,
    )
}

#[test]
fn preferred_rate_skips_formats_the_callback_cannot_convert() {
    let ranges = [offered(SampleFormat::I24), offered(SampleFormat::I16)];
    let chosen = preferred_input_config(config(44_100, SampleFormat::F32), &ranges);
    assert_eq!(chosen.sample_rate(), PLAYBACK_RATE);
    assert_eq!(chosen.sample_format(), SampleFormat::I16);
    // Some endpoints even report packed 24-bit as their own format.
    let chosen = preferred_input_config(config(PLAYBACK_RATE, SampleFormat::I24), &ranges);
    assert_eq!(chosen.sample_format(), SampleFormat::I16);
}

#[test]
fn preferred_rate_keeps_the_devices_format_or_its_supported_default() {
    let ranges = [offered(SampleFormat::I16), offered(SampleFormat::F32)];
    let chosen = preferred_input_config(config(44_100, SampleFormat::F32), &ranges);
    assert_eq!(chosen.sample_format(), SampleFormat::F32);
    let chosen = preferred_input_config(config(PLAYBACK_RATE, SampleFormat::F32), &ranges);
    assert_eq!(chosen.sample_format(), SampleFormat::F32);
    let chosen = preferred_input_config(config(44_100, SampleFormat::F32), &ranges[..0]);
    assert_eq!(chosen.sample_rate(), 44_100);
}

#[test]
fn unsupported_default_uses_a_compatible_format_at_its_own_rate() {
    let ranges = [cpal::SupportedStreamConfigRange::new(
        2,
        44_100,
        44_100,
        cpal::SupportedBufferSize::Unknown,
        SampleFormat::I16,
    )];
    let chosen = preferred_input_config(config(44_100, SampleFormat::I24), &ranges);
    assert_eq!(chosen.sample_rate(), 44_100);
    assert_eq!(chosen.sample_format(), SampleFormat::I16);
    // A valid default still wins when no 48 kHz offer can replace it.
    let chosen = preferred_input_config(config(44_100, SampleFormat::F32), &ranges);
    assert_eq!(chosen.sample_format(), SampleFormat::F32);
}

#[test]
fn oversized_default_and_offers_do_not_bypass_ingress_bounds() {
    let default = cpal::SupportedStreamConfig::new(
        MAX_CHANNELS + 1,
        PLAYBACK_RATE,
        cpal::SupportedBufferSize::Unknown,
        SampleFormat::F32,
    );
    assert!(!input_config_supported(&default));
    let ranges = [
        cpal::SupportedStreamConfigRange::new(
            MAX_CHANNELS + 1,
            PLAYBACK_RATE,
            PLAYBACK_RATE,
            cpal::SupportedBufferSize::Unknown,
            SampleFormat::F32,
        ),
        cpal::SupportedStreamConfigRange::new(
            2,
            44_100,
            44_100,
            cpal::SupportedBufferSize::Unknown,
            SampleFormat::I16,
        ),
    ];
    let chosen = preferred_input_config(default, &ranges);
    assert_eq!(chosen.channels(), 2);
    assert_eq!(chosen.sample_rate(), 44_100);
    assert!(input_config_supported(&chosen));
}

#[test]
fn unsupported_default_can_use_another_rate_within_an_offered_range() {
    let ranges = [cpal::SupportedStreamConfigRange::new(
        2,
        48_001,
        MAX_RATE * 2,
        cpal::SupportedBufferSize::Unknown,
        SampleFormat::F32,
    )];
    let chosen = preferred_input_config(config(MAX_RATE * 2, SampleFormat::I24), &ranges);
    assert_eq!(chosen.sample_rate(), MAX_RATE);
    assert!(input_config_supported(&chosen));
}

#[test]
fn microphone_diagnostics_are_periodic_peaks_and_counter_deltas() {
    let start = Instant::now();
    let mut diagnostics = CaptureDiagnostics::new(start);
    let mut snapshot = CaptureSnapshot {
        processed_frames: 500,
        dropped_samples: 4,
        ..CaptureSnapshot::default()
    };
    diagnostics.observe(0.7, 0.1);
    diagnostics.observe(0.4, 0.2);
    assert!(diagnostics
        .take(
            start + DIAGNOSTIC_INTERVAL - Duration::from_millis(1),
            &snapshot
        )
        .is_none());
    assert_eq!(
        diagnostics.take(start + DIAGNOSTIC_INTERVAL, &snapshot),
        Some(CaptureReport {
            frames: 500,
            dropped: 4,
            raw_peak: 0.7,
            processed_peak: 0.2,
        })
    );
    snapshot.processed_frames += 500;
    snapshot.dropped_samples += 2;
    assert_eq!(
        diagnostics.take(start + DIAGNOSTIC_INTERVAL * 2, &snapshot),
        Some(CaptureReport {
            frames: 500,
            dropped: 2,
            raw_peak: 0.0,
            processed_peak: 0.0,
        }),
        "no audio samples or old peaks survive the reporting interval"
    );
}

#[test]
fn microphone_diagnostics_report_an_open_but_silent_device() {
    let start = Instant::now();
    let mut diagnostics = CaptureDiagnostics::new(start);
    assert_eq!(
        diagnostics.take(start + DIAGNOSTIC_INTERVAL, &CaptureSnapshot::default()),
        Some(CaptureReport {
            frames: 0,
            dropped: 0,
            raw_peak: 0.0,
            processed_peak: 0.0,
        })
    );
}

fn shared() -> Arc<Shared> {
    let shared = Arc::new(Shared::new());
    shared.requested.store(1, Ordering::Release);
    shared.ready.store(1, Ordering::Release);
    shared.muted.store(false, Ordering::Release);
    shared
}

fn request(device: &str) -> CaptureRequest {
    CaptureRequest {
        device: Some(device.into()),
        mode: ProcessingMode::Off,
        muted: false,
        echo_cancellation: false,
        automatic_gain: false,
        monitor: false,
        speech_gate: false,
        system_echo: false,
        output: None,
    }
}

fn active(shared: Arc<Shared>, channels: u16) -> (Ingress, Active<()>) {
    let (ingress, consumer) = ring(48_000, channels, shared).unwrap();
    let opened = Opened {
        stream: (),
        consumer,
        signals: ingress.signals.clone(),
        rate: 48_000,
    };
    (ingress, Active::new(opened, &request("synthetic")).unwrap())
}

#[test]
fn callback_downmixes_split_interleaved_frames_and_sanitizes_values() {
    let (mut input, mut output) = ring(8_000, 2, shared()).unwrap();
    input.write(&[1.0_f32]);
    assert!(output.pop().is_err());
    input.write(&[-1.0_f32, 2.0, f32::NAN, f32::INFINITY]);
    input.write(&[-4.0_f32]);
    let samples = (0..3).map(|_| output.pop().unwrap()).collect::<Vec<_>>();
    assert_eq!(
        samples
            .iter()
            .map(|sample| sample.value)
            .collect::<Vec<_>>(),
        [0.0, 0.5, -0.5]
    );
    assert_eq!(
        samples
            .iter()
            .map(|sample| sample.sequence)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert!(output.pop().is_err());
}

#[test]
fn callback_bounds_and_overflow_are_explicit() {
    for (rate, channels) in [(0, 1), (44_101, 1), (192_100, 1), (48_000, 0), (48_000, 9)] {
        assert!(ring(rate, channels, shared()).is_err());
    }
    let (mut input, mut output) = ring(8_000, 1, shared()).unwrap();
    input.write(&vec![0.1_f32; 1_607]);
    assert_eq!(input.signals.dropped.load(Ordering::Relaxed), 7);
    for sequence in 0..1_600 {
        assert_eq!(output.pop().unwrap().sequence, sequence);
    }
    assert!(output.pop().is_err());
    input.write(&[0.2_f32]);
    assert_eq!(
        output.pop().unwrap().sequence,
        1_607,
        "a consumer can detect the missing interval"
    );
}

#[test]
fn partial_dsp_frames_survive_callback_boundaries_and_report_only_complete_audio() {
    let shared = shared();
    let (mut input, mut active) = active(shared.clone(), 1);
    let now = Instant::now();
    input.write(&[0.25_f32; 127]);
    assert!(!active.step(&shared, 1, now, &mut Links::default()).unwrap());
    assert_eq!(active.snapshot.processed_frames, 0);
    input.write(&[0.25_f32; 353]);
    assert!(active.step(&shared, 1, now, &mut Links::default()).unwrap());
    assert_eq!(active.snapshot.processed_frames, 1);
    assert_eq!(
        active.snapshot.raw_level,
        crate::meter_position(crate::dbfs(0.25))
    );
    assert_eq!(active.snapshot.raw_level, active.snapshot.processed_level);
    assert!(active.frame.is_empty());
}

#[test]
fn mute_and_generation_changes_discard_queued_and_partial_old_audio() {
    let shared = shared();
    let (mut input, mut active) = active(shared.clone(), 2);
    let now = Instant::now();
    input.write(&[0.9_f32; 201]);
    active.step(&shared, 1, now, &mut Links::default()).unwrap();
    assert_eq!(active.frame.len(), 100);
    shared.requested.store(2, Ordering::Release);
    shared.muted.store(true, Ordering::Release);
    input.write(&[0.8_f32; 2_000]);
    shared.ready.store(0, Ordering::Release);
    active.reconfigure(&request("synthetic"));
    shared.ready.store(2, Ordering::Release);
    assert!(!active.step(&shared, 2, now, &mut Links::default()).unwrap());
    shared.requested.store(3, Ordering::Release);
    shared.muted.store(false, Ordering::Release);
    active.reconfigure(&request("synthetic"));
    shared.ready.store(3, Ordering::Release);
    input.write(&[0.0_f32; 960]);
    active.step(&shared, 3, now, &mut Links::default()).unwrap();
    assert_eq!(active.snapshot.processed_frames, 1);
    assert_eq!(active.snapshot.raw_level, 0.0);
    assert_eq!(active.snapshot.processed_level, 0.0);
}

#[test]
fn a_sequence_gap_resets_partial_framing_instead_of_splicing_audio() {
    let shared = shared();
    let (mut input, mut active) = active(shared.clone(), 1);
    let now = Instant::now();
    input.write(&[0.9_f32; 100]);
    active.step(&shared, 1, now, &mut Links::default()).unwrap();
    input.sequence += 1;
    input.write(&[0.0_f32; 480]);
    active.step(&shared, 1, now, &mut Links::default()).unwrap();
    assert_eq!(active.snapshot.processed_frames, 1);
    assert_eq!(active.snapshot.raw_level, 0.0);
    assert!(active.frame.is_empty());
}

#[test]
fn stalled_capture_clears_levels_and_partial_audio_once_without_waiting() {
    let shared = shared();
    let (mut input, mut active) = active(shared.clone(), 1);
    let now = Instant::now();
    input.write(&[0.5_f32; 490]);
    active.step(&shared, 1, now, &mut Links::default()).unwrap();
    assert!(active.snapshot.raw_level > 0.0);
    assert_eq!(active.frame.len(), 10);
    assert!(active
        .step(&shared, 1, now + STALE_AFTER, &mut Links::default())
        .unwrap());
    assert_eq!(active.snapshot.raw_level, 0.0);
    assert_eq!(active.snapshot.processed_level, 0.0);
    assert!(active.frame.is_empty());
    assert!(!active
        .step(&shared, 1, now + STALE_AFTER * 2, &mut Links::default())
        .unwrap());
    input.write(&[0.0_f32; 480]);
    active
        .step(&shared, 1, now + STALE_AFTER * 3, &mut Links::default())
        .unwrap();
    assert_eq!(active.snapshot.raw_level, 0.0);
    assert_eq!(active.snapshot.processed_frames, 2);
}

#[derive(Debug, PartialEq)]
enum Trace {
    Open(String),
    Play(String),
    Drop(String),
    Loopback(String),
}

struct FakeStream {
    device: String,
    trace: Sender<Trace>,
}

impl InputStream for FakeStream {
    fn play(&self) -> Result<(), MediaError> {
        self.trace.send(Trace::Play(self.device.clone())).unwrap();
        Ok(())
    }
}

impl Drop for FakeStream {
    fn drop(&mut self) {
        let _ = self.trace.send(Trace::Drop(self.device.clone()));
    }
}

struct FakeBackend {
    trace: Sender<Trace>,
    inputs: Sender<Ingress>,
    /// Where a loopback's sound goes in, for a test that plays some.
    loopbacks: Sender<rtrb::Producer<f32>>,
    first_open_gate: Option<Receiver<()>>,
    fail_first: bool,
}

impl Backend for FakeBackend {
    type Stream = FakeStream;

    fn open(
        &mut self,
        device: Option<&str>,
        shared: Arc<Shared>,
    ) -> Result<Opened<Self::Stream>, MediaError> {
        let device = device.unwrap_or("default").to_owned();
        self.trace.send(Trace::Open(device.clone())).unwrap();
        let stream = FakeStream {
            device,
            trace: self.trace.clone(),
        };
        if let Some(gate) = self.first_open_gate.take() {
            gate.recv().unwrap();
        }
        if std::mem::take(&mut self.fail_first) {
            return Err(MediaError("synthetic open failed".into()));
        }
        let (ingress, consumer) = ring(48_000, 1, shared)?;
        let signals = ingress.signals.clone();
        self.inputs
            .send(ingress)
            .unwrap_or_else(|_| panic!("test input receiver closed"));
        Ok(Opened {
            stream,
            consumer,
            signals,
            rate: 48_000,
        })
    }

    fn open_loopback(&mut self, output: Option<&str>) -> Result<Loopback, MediaError> {
        let output = output.unwrap_or("default").to_owned();
        self.trace.send(Trace::Loopback(output.clone())).unwrap();
        let (played, reference) = EchoReference::pair(9_600);
        let _ = self.loopbacks.send(played);
        Ok(Loopback {
            _stream: Box::new(()),
            failed: Arc::new(AtomicBool::new(false)),
            reference,
        })
    }
}

fn fake(
    blocked: bool,
    fail_first: bool,
) -> (Capture, Receiver<Trace>, Receiver<Ingress>, Sender<()>) {
    let (trace, traces) = mpsc::channel();
    let (inputs, input) = mpsc::channel();
    let (release, gate) = mpsc::channel();
    let capture = Capture::with_backend(FakeBackend {
        trace,
        inputs,
        loopbacks: mpsc::channel().0,
        first_open_gate: blocked.then_some(gate),
        fail_first,
    })
    .unwrap();
    (capture, traces, input, release)
}

fn receive<T>(receiver: &Receiver<T>) -> T {
    receiver
        .recv_timeout(Duration::from_secs(2))
        .expect("fake worker did not respond")
}

fn wait(capture: &Capture, predicate: impl Fn(&CaptureSnapshot) -> bool) -> CaptureSnapshot {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let snapshot = capture.snapshot();
        if predicate(&snapshot) {
            return snapshot;
        }
        assert!(
            Instant::now() < deadline,
            "fake worker stalled: {snapshot:?}"
        );
        thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn latest_request_wins_and_a_delayed_obsolete_open_is_not_played() {
    let (mut capture, trace, _inputs, release) = fake(true, false);
    capture.configure(Some(request("old")));
    assert_eq!(receive(&trace), Trace::Open("old".into()));
    capture.configure(Some(request("new")));
    assert_eq!(capture.snapshot().state, CaptureState::Starting);
    release.send(()).unwrap();
    assert_eq!(receive(&trace), Trace::Drop("old".into()));
    assert_eq!(receive(&trace), Trace::Open("new".into()));
    assert_eq!(receive(&trace), Trace::Play("new".into()));
    wait(&capture, |snapshot| snapshot.state == CaptureState::Running);
}

#[test]
fn identical_requests_do_not_reopen_and_mode_mute_changes_share_one_stream() {
    let (mut capture, trace, inputs, _release) = fake(false, false);
    let mut desired = request("same");
    capture.configure(Some(desired.clone()));
    let mut input = receive(&inputs);
    wait(&capture, |snapshot| snapshot.state == CaptureState::Running);
    let generation = capture.generation;
    capture.configure(Some(desired.clone()));
    assert_eq!(capture.generation, generation);
    input.write(&[0.25_f32; 480]);
    wait(&capture, |snapshot| snapshot.processed_frames == 1);
    desired.muted = true;
    capture.configure(Some(desired.clone()));
    assert_eq!(capture.snapshot().raw_level, 0.0);
    wait(&capture, |snapshot| snapshot.state == CaptureState::Running);
    // Every suppressor and the gain switch on the stream that is open.
    for (mode, gain) in [
        (ProcessingMode::WebRtc, false),
        (ProcessingMode::RnNoise, true),
        (ProcessingMode::Off, false),
    ] {
        desired.mode = mode;
        desired.automatic_gain = gain;
        capture.configure(Some(desired.clone()));
        wait(&capture, |snapshot| snapshot.state == CaptureState::Running);
    }
    assert_eq!(
        trace.try_iter().collect::<Vec<_>>(),
        [Trace::Open("same".into()), Trace::Play("same".into())]
    );
    capture.configure(None);
    assert_eq!(capture.snapshot(), CaptureSnapshot::default());
    assert_eq!(receive(&trace), Trace::Drop("same".into()));
}

#[test]
fn obsolete_failures_cannot_replace_idle_and_drop_does_not_wait_for_open() {
    let (mut capture, trace, _inputs, release) = fake(true, true);
    capture.configure(Some(request("blocked")));
    assert_eq!(receive(&trace), Trace::Open("blocked".into()));
    capture.configure(None);
    assert_eq!(capture.snapshot(), CaptureSnapshot::default());
    let shared = capture.shared.clone();
    // If Drop joined, this would deadlock before the fake open was released.
    drop(capture);
    assert!(shared.shutdown.load(Ordering::Acquire));
    assert_eq!(shared.admitted_generation(), None);
    release.send(()).unwrap();
    assert_eq!(receive(&trace), Trace::Drop("blocked".into()));
}

#[test]
fn failed_requests_need_an_explicit_retry_and_stream_errors_close_capture() {
    let (mut capture, trace, inputs, _release) = fake(false, true);
    let desired = request("retry");
    capture.configure(Some(desired.clone()));
    wait(&capture, |snapshot| {
        matches!(snapshot.state, CaptureState::Failed(_))
    });
    let generation = capture.generation;
    capture.configure(Some(desired.clone()));
    assert_eq!(capture.generation, generation);
    assert!(matches!(capture.snapshot().state, CaptureState::Failed(_)));
    capture.configure(None);
    capture.configure(Some(desired));
    let input = receive(&inputs);
    wait(&capture, |snapshot| snapshot.state == CaptureState::Running);
    input.signals.failed.store(true, Ordering::Release);
    wait(&capture, |snapshot| {
        matches!(snapshot.state, CaptureState::Failed(_))
    });
    assert_eq!(capture.snapshot().raw_level, 0.0);
    assert_eq!(
        trace
            .try_iter()
            .filter(|event| matches!(event, Trace::Open(_)))
            .count(),
        2
    );
}

fn frames(
    input: &mut Ingress,
    active: &mut Active<()>,
    shared: &Shared,
    links: &mut Links,
    count: usize,
) {
    let now = Instant::now();
    for _ in 0..count {
        input.write(&[0.1_f32; 480]);
        active.step(shared, 1, now, links).unwrap();
    }
}

fn push(played: &mut rtrb::Producer<f32>, samples: usize, value: f32) {
    for _ in 0..samples {
        played.push(value).unwrap();
    }
}

#[test]
fn echo_cancellation_takes_all_that_played_before_each_frame_and_stays_on() {
    let shared = shared();
    let (mut input, mut active) = active(shared.clone(), 1);
    let (mut played, reference) = EchoReference::pair(16_000);
    let mut links = Links {
        reference: Some(reference),
        monitor: None,
        transmit: None,
    };
    active.echo = true;
    frames(&mut input, &mut active, &shared, &mut links, 1);
    assert!(
        active.snapshot.echo_cancelling,
        "on with a reference to read"
    );
    // A burst of what the output played goes in before the next frame,
    // not one frame each: waiting would put it after its own echo.
    push(&mut played, 3 * REFERENCE_FRAME, 0.2);
    frames(&mut input, &mut active, &shared, &mut links, 1);
    assert_eq!(links.reference.as_ref().unwrap().available(), 0);
    // Silence between words, or none played: it keeps what it learned.
    frames(&mut input, &mut active, &shared, &mut links, 100);
    assert!(active.snapshot.echo_cancelling, "no hold that turns it off");
    // Not requested: a reference alone never turns it on.
    active.echo = false;
    push(&mut played, 2 * REFERENCE_FRAME, 0.2);
    frames(&mut input, &mut active, &shared, &mut links, 2);
    assert!(!active.snapshot.echo_cancelling);
    // Without a reference there is nothing to cancel with.
    active.echo = true;
    links.reference = None;
    frames(&mut input, &mut active, &shared, &mut links, 1);
    assert!(!active.snapshot.echo_cancelling);
}

#[test]
fn old_reference_is_dropped_to_keep_alignment_bounded() {
    let shared = shared();
    let (mut input, mut active) = active(shared.clone(), 1);
    let (mut played, reference) = EchoReference::pair(16_000);
    let mut links = Links {
        reference: Some(reference),
        monitor: None,
        transmit: None,
    };
    active.echo = true;
    // What played before the capture started goes without counting.
    push(&mut played, 10_000, 0.2);
    frames(&mut input, &mut active, &shared, &mut links, 1);
    assert_eq!(active.snapshot.reference_dropped, 0);
    assert_eq!(links.reference.as_ref().unwrap().available(), 0);
    // A stalled capture: the oldest beyond the backlog goes, the rest in.
    push(&mut played, 10_000, 0.2);
    frames(&mut input, &mut active, &shared, &mut links, 1);
    assert_eq!(
        active.snapshot.reference_dropped,
        (10_000 - REFERENCE_BACKLOG) as u64
    );
    assert_eq!(links.reference.as_ref().unwrap().available(), 0);
    // A restart (a mute, another request) forgets what played before it.
    push(&mut played, 4 * REFERENCE_FRAME, 0.2);
    active.reconfigure(&CaptureRequest {
        echo_cancellation: true,
        ..request("synthetic")
    });
    frames(&mut input, &mut active, &shared, &mut links, 1);
    assert_eq!(links.reference.as_ref().unwrap().available(), 0);
    assert_eq!(active.snapshot.reference_dropped, 0, "cleared, not trimmed");
}

fn loopback_backend() -> (FakeBackend, Receiver<Trace>, Receiver<rtrb::Producer<f32>>) {
    let (trace, traces) = mpsc::channel();
    let (loopbacks, opened) = mpsc::channel();
    (
        FakeBackend {
            trace,
            inputs: mpsc::channel().0,
            loopbacks,
            first_open_gate: None,
            fail_first: false,
        },
        traces,
        opened,
    )
}

#[test]
fn the_system_loopback_replaces_runes_reference_and_hands_back_when_silent() {
    let shared = shared();
    let (mut input, mut active) = active(shared.clone(), 1);
    let (mut backend, traces, opened) = loopback_backend();
    let (mut played, reference) = EchoReference::pair(16_000);
    let mut links = Links {
        reference: Some(reference),
        monitor: None,
        transmit: None,
    };
    let wanted = CaptureRequest {
        echo_cancellation: true,
        system_echo: true,
        output: Some("headset".into()),
        ..request("synthetic")
    };
    active.reconfigure(&wanted);
    active.steer_system(&mut backend, &wanted);
    assert_eq!(receive(&traces), Trace::Loopback("headset".into()));
    let mut system = receive(&opened);
    frames(&mut input, &mut active, &shared, &mut links, 1);
    push(&mut system, 2 * REFERENCE_FRAME, 0.3);
    push(&mut played, 2 * REFERENCE_FRAME, 0.2);
    frames(&mut input, &mut active, &shared, &mut links, 1);
    assert!(active.snapshot.system_echo && active.snapshot.echo_cancelling);
    // It holds RUNE's sound too: RUNE's own reference is not fed as well.
    assert_eq!(links.reference.as_ref().unwrap().available(), 0);
    assert!(system.slots() == system.buffer().capacity(), "all read");
    // The same request keeps it open; nothing is opened twice.
    active.steer_system(&mut backend, &wanted);
    assert!(traces.try_recv().is_err());
    // Half a second without anything from it: RUNE's own stands in.
    frames(
        &mut input,
        &mut active,
        &shared,
        &mut links,
        SYSTEM_IDLE_FRAMES as usize,
    );
    push(&mut played, REFERENCE_FRAME, 0.2);
    frames(&mut input, &mut active, &shared, &mut links, 1);
    assert!(!active.snapshot.system_echo && active.snapshot.echo_cancelling);
    assert_eq!(links.reference.as_ref().unwrap().available(), 0);
    // Turned off, or no echo cancellation: the loopback closes.
    let off = CaptureRequest {
        system_echo: false,
        ..wanted.clone()
    };
    active.steer_system(&mut backend, &off);
    assert!(active.system.is_none());
    active.steer_system(&mut backend, &wanted);
    assert_eq!(receive(&traces), Trace::Loopback("headset".into()));
    active.steer_system(
        &mut backend,
        &CaptureRequest {
            echo_cancellation: false,
            ..wanted
        },
    );
    assert!(active.system.is_none());
}

#[test]
fn what_the_system_plays_is_never_sent_or_heard_back() {
    let shared = shared();
    let (mut input, mut active) = active(shared.clone(), 1);
    let (mut backend, _traces, opened) = loopback_backend();
    let (feed, mut tap) = crate::tap(48_000);
    let (heard, mut monitor) = Feed::pair(48_000);
    let mut links = Links {
        reference: None,
        monitor: Some(heard),
        transmit: Some(feed),
    };
    let wanted = CaptureRequest {
        echo_cancellation: true,
        system_echo: true,
        monitor: true,
        ..request("synthetic")
    };
    active.reconfigure(&wanted);
    active.steer_system(&mut backend, &wanted);
    let mut system = receive(&opened);
    let now = Instant::now();
    let mut loud = crate::processing::eval::Rng::new(9);
    for _ in 0..50 {
        // Loud noise on the output; a silent microphone.
        for _ in 0..REFERENCE_FRAME {
            system.push(0.8 * loud.signed()).unwrap();
        }
        input.write(&[0.0_f32; 480]);
        active.step(&shared, 1, now, &mut links).unwrap();
    }
    assert!(active.snapshot.system_echo);
    let mut sent = vec![0.0; tap.available()];
    assert!(tap.read(&mut sent) && sent.len() >= 40 * 480);
    // Under the meter's floor, −60 dBFS, where −2 dBFS played.
    assert!(crate::rms(&sent) < 1e-3, "sent {}", crate::rms(&sent));
    let looped: Vec<f32> = std::iter::from_fn(|| monitor.pop().ok()).collect();
    assert!(crate::rms(&looped) < 1e-3, "heard {}", crate::rms(&looped));
}

#[test]
fn the_input_volume_applies_from_the_next_frame_without_a_restart() {
    let shared = shared();
    let (mut input, mut active) = active(shared.clone(), 1);
    let mut links = Links::default();
    frames(&mut input, &mut active, &shared, &mut links, 1);
    let usual = active.snapshot.raw_level;
    shared
        .input_volume
        .store(0.5_f32.to_bits(), Ordering::Relaxed);
    frames(&mut input, &mut active, &shared, &mut links, 1);
    // Half the amplitude is 6 dB, a tenth of the meter's 60.
    assert!((usual - active.snapshot.raw_level - 0.1).abs() < 0.001);
    assert_eq!(active.snapshot.processed_frames, 2, "the same run goes on");
}

#[test]
fn a_microphone_at_another_rate_is_processed_at_48_khz_so_rnnoise_runs() {
    let shared = shared();
    let (mut input, consumer) = ring(44_100, 1, shared.clone()).unwrap();
    let opened = Opened {
        stream: (),
        consumer,
        signals: input.signals.clone(),
        rate: 44_100,
    };
    let mut active = Active::new(
        opened,
        &CaptureRequest {
            mode: ProcessingMode::RnNoise,
            ..request("synthetic")
        },
    )
    .unwrap();
    assert!(active.processor.rnnoise(), "not WebRTC standing in");
    assert_eq!(active.processor.frame_len(), 480);
    let (feed, tap) = crate::tap(9_600);
    let mut links = Links {
        transmit: Some(feed),
        ..Links::default()
    };
    // 200 ms at 44.1 kHz, the ring's size, is 200 ms at 48 kHz.
    input.write(&[0.1_f32; 8_820]);
    active.step(&shared, 1, Instant::now(), &mut links).unwrap();
    let frames = active.snapshot.processed_frames;
    assert!((19..=20).contains(&frames), "{frames}");
    assert_eq!(tap.available(), frames as usize * 480);
}

#[test]
fn the_monitor_hears_processed_frames_only_when_asked() {
    let shared = shared();
    let (mut input, mut active) = active(shared.clone(), 1);
    let (feed, mut heard) = Feed::pair(4_800);
    let mut links = Links {
        reference: None,
        monitor: Some(feed),
        transmit: None,
    };
    frames(&mut input, &mut active, &shared, &mut links, 2);
    assert_eq!(heard.slots(), 0);
    active.monitor = true;
    frames(&mut input, &mut active, &shared, &mut links, 2);
    assert!((958..=960).contains(&heard.slots()), "{}", heard.slots());
    heard.pop().unwrap();
    assert!((heard.pop().unwrap() - 0.1).abs() < 1e-6);
}

#[test]
fn a_call_reads_processed_frames_until_its_tap_goes() {
    let shared = shared();
    let (mut input, mut active) = active(shared.clone(), 1);
    let (feed, mut tap) = crate::tap(4_800);
    let mut links = Links {
        transmit: Some(feed),
        ..Links::default()
    };
    frames(&mut input, &mut active, &shared, &mut links, 2);
    assert!(
        (958..=960).contains(&tap.available()),
        "{}",
        tap.available()
    );
    let mut frame = [0.0; 480];
    assert!(tap.read(&mut frame));
    assert!((frame[479] - 0.1).abs() < 1e-6);
    drop(tap);
    frames(&mut input, &mut active, &shared, &mut links, 1);
    assert!(
        links.transmit.is_none(),
        "a dropped reader releases the link"
    );
}
