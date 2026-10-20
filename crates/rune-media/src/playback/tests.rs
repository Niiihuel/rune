//! Synthetic output: rings, the callback and a fake backend. Nothing here
//! opens a device or plays sound.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::mpsc::{self, Receiver, Sender};

use super::*;

/// Counts allocations made on the current thread while armed, so the
/// callback's path can be checked for allocation.
struct Counting;

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ARMED.with(Cell::get) {
            ALLOCATIONS.with(|count| count.set(count.get() + 1));
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

fn allocations_during(run: impl FnOnce()) -> usize {
    ALLOCATIONS.with(|count| count.set(0));
    ARMED.with(|armed| armed.set(true));
    run();
    ARMED.with(|armed| armed.set(false));
    ALLOCATIONS.with(Cell::get)
}

fn shared(gain: f32) -> Arc<Shared> {
    let shared = Arc::new(Shared::new());
    shared.gain.store(gain.to_bits(), Ordering::Release);
    shared
}

fn render(egress: &mut Egress, frames: usize) -> Vec<f32> {
    let mut output = vec![0.0_f32; frames * egress.channels];
    egress.render(&mut output, Duration::from_millis(12));
    output
}

#[test]
fn linear_resampling_keeps_the_rate_and_bounds_what_one_sample_pulls() {
    for (from, to) in [
        (48_000, 48_000),
        (48_000, 44_100),
        (48_000, 8_000),
        (48_000, 192_000),
        (8_000, 48_000),
        (44_100, 48_000),
        (192_000, 48_000),
    ] {
        let mut resampler = Linear::new(from, to);
        let bound = 1 + from.div_ceil(to) as usize;
        let inputs = from as usize / 10;
        let mut fed = 0;
        let mut outputs = 0;
        loop {
            let mut pulls = 0;
            let sample = resampler.next(|| {
                pulls += 1;
                (fed < inputs).then(|| {
                    fed += 1;
                    0.5
                })
            });
            assert!(pulls <= bound, "{from}->{to}: {pulls} pulls for one sample");
            if sample.is_none() {
                break;
            }
            outputs += 1;
        }
        let expected = inputs as f64 * f64::from(to) / f64::from(from);
        assert!(
            (outputs as f64 - expected).abs() <= 2.0,
            "{from}->{to}: {outputs} samples, expected {expected}"
        );
    }
}

#[test]
fn equal_rates_pass_samples_through_and_resume_after_running_dry() {
    let mut resampler = Linear::new(48_000, 48_000);
    let mut input = [0.1_f32, 0.2, 0.3].into_iter();
    let mut out = Vec::new();
    while let Some(sample) = resampler.next(|| input.next()) {
        out.push(sample);
    }
    assert_eq!(out, [0.0, 0.1, 0.2], "one sample of delay, nothing lost");
    let mut more = [0.4_f32].into_iter();
    assert_eq!(resampler.next(|| more.next()), Some(0.3));
}

#[test]
fn the_callback_applies_gain_and_fills_every_channel_without_allocating() {
    let shared = shared(0.5);
    let (mut egress, mut ends) = rings(48_000, 2, shared.clone()).unwrap();
    for n in 0..64 {
        ends.producer.push(n as f32 / 100.0).unwrap();
    }
    let mut output = vec![0.0_f32; 2 * 32];
    let allocations = allocations_during(|| egress.render(&mut output, Duration::ZERO));
    assert_eq!(allocations, 0, "the real-time path allocated");
    for (frame, pair) in output.chunks(2).enumerate().skip(1) {
        let expected = (frame - 1) as f32 / 100.0 * 0.5;
        assert!((pair[0] - expected).abs() < 1e-6 && pair[0] == pair[1]);
    }
    // Integer devices get the same signal, converted.
    let mut integers = [0_i16; 4];
    egress.render(&mut integers, Duration::ZERO);
    assert!(integers[0] > 0 && integers[0] == integers[1]);
    shared.gain.store(4.0_f32.to_bits(), Ordering::Release);
    ends.producer.push(1.0).unwrap();
    assert!(
        render(&mut egress, 2).iter().all(|value| *value <= 1.0),
        "gain never clips past full scale"
    );
}

#[test]
fn deafen_and_zero_volume_silence_the_output_and_its_reference() {
    let shared = shared(0.0);
    let (mut egress, mut ends) = rings(48_000, 1, shared.clone()).unwrap();
    for _ in 0..20 {
        ends.producer.push(0.5).unwrap();
    }
    assert!(render(&mut egress, 10).iter().all(|value| *value == 0.0));
    let reference: Vec<f32> = std::iter::from_fn(|| ends.references.pop().ok()).collect();
    assert_eq!(
        reference, [0.0; 10],
        "the silence that played, sample for sample"
    );
    shared.gain.store(1.0_f32.to_bits(), Ordering::Release);
    let played = render(&mut egress, 5);
    assert_eq!(&played[1..], &[0.5; 4]);
    assert_eq!(ends.references.slots(), 5, "what played is the reference");
}

#[test]
fn underruns_are_silence_counted_only_while_a_source_plays_and_kept_in_the_reference() {
    let shared = shared(1.0);
    let (mut egress, mut ends) = rings(48_000, 2, shared.clone()).unwrap();
    assert!(render(&mut egress, 8).iter().all(|value| *value == 0.0));
    assert_eq!(
        shared.underruns.load(Ordering::Relaxed),
        0,
        "idle is not starved"
    );
    shared.playing.store(true, Ordering::Release);
    ends.producer.push(0.25).unwrap();
    ends.producer.push(0.25).unwrap();
    let output = render(&mut egress, 8);
    assert_eq!(output[2], 0.25);
    assert!(output[6..].iter().all(|value| *value == 0.0));
    assert_eq!(shared.underruns.load(Ordering::Relaxed), 6);
    // The reference keeps the speaker's timeline: idle, then the two
    // samples, then the gap, each where it played.
    let reference: Vec<f32> = std::iter::from_fn(|| ends.references.pop().ok()).collect();
    assert_eq!(reference.len(), 16);
    assert_eq!(&reference[8..11], &[0.25, 0.25, 0.0]);
    assert_eq!(reference.iter().filter(|value| **value != 0.0).count(), 2);
    assert_eq!(shared.latency_micros.load(Ordering::Relaxed), 12_000);
}

#[test]
fn rings_admit_only_bounded_rates_and_channels() {
    for (rate, channels) in [(0, 2), (7_999, 2), (192_001, 2), (48_000, 0), (48_000, 9)] {
        assert!(rings(rate, channels, shared(1.0)).is_err());
    }
    let (egress, ends) = rings(44_100, 8, shared(1.0)).unwrap();
    assert_eq!(egress.consumer.buffer().capacity(), DEVICE_RING);
    assert_eq!(ends.references.buffer().capacity(), REFERENCE_RING);
}

fn active(shared: &Arc<Shared>) -> (Egress, Active<()>) {
    let (egress, ends) = rings(48_000, 1, shared.clone()).unwrap();
    (
        egress,
        Active {
            _stream: (),
            ends,
            device: None,
            default: None,
        },
    )
}

#[test]
fn a_feed_ahead_of_the_backlog_loses_its_oldest_audio() {
    let shared = shared(1.0);
    let (mut egress, mut active) = active(&shared);
    let (mut producer, consumer) = RingBuffer::new(FEED_RING);
    let mut feed = Some(consumer);
    let queued = 150 * MS;
    for n in 0..queued {
        producer.push(n as f32).unwrap();
    }
    active.fill(
        &shared,
        PlaybackSource::Feed,
        &mut Tone::default(),
        &mut feed,
        &mut Vec::new(),
    );
    let dropped = (queued - TARGET) as u64;
    assert_eq!(shared.overruns.load(Ordering::Relaxed), dropped);
    assert_eq!(active.queued(), TARGET);
    // The newest audio stays: what plays next is right after the drop.
    assert_eq!(egress.consumer.pop().unwrap(), dropped as f32);
}

#[test]
fn sounds_are_mixed_over_the_feed_and_go_once_played_out() {
    let shared = shared(1.0);
    let (mut egress, mut active) = active(&shared);
    let (mut voice, consumer) = RingBuffer::new(FEED_RING);
    let mut feed = Some(consumer);
    let (mut chime, sound) = RingBuffer::new(FEED_RING);
    let mut sounds = vec![sound];
    for _ in 0..TARGET {
        voice.push(0.25).unwrap();
    }
    // A sound shorter than what the feed has: the feed goes on alone after it.
    for _ in 0..10 {
        chime.push(0.5).unwrap();
    }
    drop(chime);
    let mut fill = |feed: &mut Option<Consumer<f32>>, sounds: &mut Vec<Consumer<f32>>| {
        active.fill(
            &shared,
            PlaybackSource::Feed,
            &mut Tone::default(),
            feed,
            sounds,
        )
    };
    fill(&mut feed, &mut sounds);
    let played: Vec<f32> = (0..TARGET)
        .map(|_| egress.consumer.pop().unwrap())
        .collect();
    assert_eq!(played[..10], [0.75; 10]);
    assert!(played[10..].iter().all(|&sample| sample == 0.25));
    fill(&mut feed, &mut sounds);
    assert!(sounds.is_empty(), "an ended sound is let go");
    // A sound alone, without any feed, plays too.
    let (mut chime, sound) = RingBuffer::new(FEED_RING);
    chime.push(0.5).unwrap();
    let mut sounds = vec![sound];
    fill(&mut None, &mut sounds);
    assert_eq!(egress.consumer.pop(), Ok(0.5));
    assert_eq!(sounds.len(), 1, "it may still send more");
}

#[test]
fn a_full_feed_drops_and_counts_what_does_not_fit() {
    let shared = shared(1.0);
    let (producer, _consumer) = RingBuffer::new(4);
    let mut feed = Feed {
        producer,
        overruns: shared.overruns.clone(),
    };
    assert_eq!(feed.push(&[0.1, f32::NAN, 2.0, -0.1, 0.2, 0.3]), 2);
    assert_eq!(shared.overruns.load(Ordering::Relaxed), 2);
    assert!(feed.connected());
    drop(_consumer);
    assert!(!feed.connected());
}

#[test]
fn the_device_ring_is_topped_up_to_the_target_or_two_periods() {
    let shared = shared(1.0);
    let (_egress, mut active) = active(&shared);
    let mut tone = Tone::default();
    active.fill(
        &shared,
        PlaybackSource::Tone,
        &mut tone,
        &mut None,
        &mut Vec::new(),
    );
    assert_eq!(active.queued(), TARGET);
    active.fill(
        &shared,
        PlaybackSource::Tone,
        &mut tone,
        &mut None,
        &mut Vec::new(),
    );
    assert_eq!(active.queued(), TARGET, "a full target is left alone");
    shared.period.store(50 * MS as u32, Ordering::Relaxed);
    active.fill(
        &shared,
        PlaybackSource::Tone,
        &mut tone,
        &mut None,
        &mut Vec::new(),
    );
    assert_eq!(
        active.queued(),
        100 * MS,
        "a long device period gets two periods"
    );
    assert!(active.ends.producer.slots() > 0);
}

#[test]
fn the_chime_is_soft_starts_from_silence_and_repeats_each_second() {
    let mut tone = Tone::default();
    let second: Vec<f32> = (0..PLAYBACK_RATE).map(|_| tone.next()).collect();
    assert_eq!(second[0], 0.0, "it fades in from silence");
    assert!(second.iter().all(|sample| sample.abs() <= Tone::AMPLITUDE));
    let peak = second
        .iter()
        .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
    assert!(peak > Tone::AMPLITUDE * 0.95);
    assert!(second[400 * MS..].iter().all(|sample| *sample == 0.0));
    let again: Vec<f32> = (0..PLAYBACK_RATE).map(|_| tone.next()).collect();
    assert_eq!(second, again);
}

#[test]
fn deafen_and_bad_volumes_map_to_a_bounded_gain() {
    let request = |volume, deafened| PlaybackRequest {
        device: None,
        source: PlaybackSource::Tone,
        volume,
        deafened,
    };
    assert_eq!(gain(&request(0.8, false)), 0.8);
    assert_eq!(gain(&request(0.8, true)), 0.0);
    assert_eq!(gain(&request(9.0, false)), MAX_VOLUME);
    assert_eq!(gain(&request(-1.0, false)), 0.0);
    assert_eq!(gain(&request(f32::NAN, false)), 0.0);
}

#[test]
fn the_echo_reference_reads_whole_frames_and_trims_the_oldest() {
    let (mut producer, mut reference) = EchoReference::pair(16);
    for n in 0..10 {
        producer.push(n as f32).unwrap();
    }
    let mut frame = [0.0; 4];
    assert_eq!(reference.trim(8), 2);
    assert!(reference.read(&mut frame));
    assert_eq!(frame, [2.0, 3.0, 4.0, 5.0]);
    assert!(reference.read(&mut frame));
    let mut big = [0.0; 4];
    assert!(!reference.read(&mut big), "a partial frame waits");
    assert_eq!(reference.available(), 0);
}

/// What a fake open hands the test: the callback's state and the stream's
/// failure flag.
type Opening = (Option<String>, Egress, Arc<StreamSignals>);

struct Fake {
    opened: Sender<Opening>,
    default: Arc<Mutex<Option<String>>>,
    fail: Arc<AtomicBool>,
}

impl OutputStream for () {
    fn play(&self) -> Result<(), MediaError> {
        Ok(())
    }
}

impl Backend for Fake {
    type Stream = ();

    fn open(
        &mut self,
        device: Option<&str>,
        shared: Arc<Shared>,
    ) -> Result<Opened<()>, MediaError> {
        if self.fail.load(Ordering::Acquire) {
            return Err(MediaError("no está".into()));
        }
        let (egress, ends) = rings(48_000, 2, shared)?;
        let default = device
            .is_none()
            .then(|| self.default.lock().unwrap().clone())
            .flatten();
        let _ = self
            .opened
            .send((device.map(str::to_owned), egress, ends.signals.clone()));
        Ok(Opened {
            stream: (),
            ends,
            default,
        })
    }

    fn default_output(&mut self) -> Option<String> {
        self.default.lock().unwrap().clone()
    }
}

struct Harness {
    playback: Playback,
    opened: Receiver<Opening>,
    default: Arc<Mutex<Option<String>>>,
    fail: Arc<AtomicBool>,
}

fn harness() -> Harness {
    let (sender, opened) = mpsc::channel();
    let default = Arc::new(Mutex::new(Some("first".to_owned())));
    let fail = Arc::new(AtomicBool::new(false));
    let playback = Playback::with_backend(Fake {
        opened: sender,
        default: default.clone(),
        fail: fail.clone(),
    })
    .unwrap();
    Harness {
        playback,
        opened,
        default,
        fail,
    }
}

fn wait(mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(start.elapsed() < Duration::from_secs(5), "timed out");
        thread::sleep(Duration::from_millis(2));
    }
}

fn tone(device: Option<&str>, volume: f32) -> PlaybackRequest {
    PlaybackRequest {
        device: device.map(str::to_owned),
        source: PlaybackSource::Tone,
        volume,
        deafened: false,
    }
}

fn next_open(harness: &Harness) -> Opening {
    harness
        .opened
        .recv_timeout(Duration::from_secs(5))
        .expect("the worker opened a stream")
}

#[test]
fn volume_and_deafen_apply_to_the_running_stream_without_reopening_it() {
    let mut harness = harness();
    harness.playback.configure(Some(tone(Some("a"), 1.0)));
    let (device, mut egress, _) = next_open(&harness);
    assert_eq!(device.as_deref(), Some("a"));
    wait(|| harness.playback.snapshot().state == CaptureState::Running);
    wait(|| egress.consumer.slots() >= TARGET);
    let loud = render(&mut egress, 2_000)
        .iter()
        .fold(0.0_f32, |peak, s| peak.max(s.abs()));
    assert!(loud > 0.2, "the chime plays: {loud}");
    harness.playback.configure(Some(tone(Some("a"), 0.5)));
    egress
        .consumer
        .read_chunk(egress.consumer.slots())
        .unwrap()
        .commit_all();
    wait(|| egress.consumer.slots() >= TARGET);
    let soft = render(&mut egress, 2_000)
        .iter()
        .fold(0.0_f32, |peak, s| peak.max(s.abs()));
    assert!((soft - loud * 0.5).abs() < 0.02, "{soft} is half of {loud}");
    let mut deafened = tone(Some("a"), 0.5);
    deafened.deafened = true;
    harness.playback.configure(Some(deafened));
    assert!(render(&mut egress, 500).iter().all(|value| *value == 0.0));
    assert!(harness.opened.try_recv().is_err(), "nothing was reopened");
    harness.playback.configure(None);
    assert_eq!(harness.playback.snapshot(), PlaybackSnapshot::default());
}

#[test]
fn a_new_device_rebuilds_and_a_lost_one_is_reopened() {
    let mut harness = harness();
    harness.playback.configure(Some(tone(Some("a"), 1.0)));
    next_open(&harness);
    harness.playback.configure(Some(tone(Some("b"), 1.0)));
    let (device, _, signals) = next_open(&harness);
    assert_eq!(device.as_deref(), Some("b"));
    wait(|| harness.playback.snapshot().state == CaptureState::Running);
    signals.failed.store(true, Ordering::Release);
    wait(|| harness.playback.snapshot().state == CaptureState::Starting);
    let (device, _, _) = next_open(&harness);
    assert_eq!(
        device.as_deref(),
        Some("b"),
        "the same device, after a pause"
    );
    wait(|| harness.playback.snapshot().state == CaptureState::Running);
}

#[test]
fn following_the_default_moves_with_it() {
    let mut harness = harness();
    harness.playback.configure(Some(tone(None, 1.0)));
    let (device, _, _) = next_open(&harness);
    assert_eq!(device, None);
    *harness.default.lock().unwrap() = Some("second".into());
    let (device, _, _) = next_open(&harness);
    assert_eq!(device, None, "reopened on the new default");
}

#[test]
fn a_missing_device_fails_once_and_a_cleared_request_retries() {
    let mut harness = harness();
    harness.fail.store(true, Ordering::Release);
    harness.playback.configure(Some(tone(Some("gone"), 1.0)));
    wait(|| matches!(harness.playback.snapshot().state, CaptureState::Failed(_)));
    harness.fail.store(false, Ordering::Release);
    thread::sleep(Duration::from_millis(30));
    assert!(harness.opened.try_recv().is_err(), "no loop of reopening");
    harness.playback.configure(None);
    harness.playback.configure(Some(tone(Some("gone"), 1.0)));
    next_open(&harness);
    wait(|| harness.playback.snapshot().state == CaptureState::Running);
}

#[test]
fn the_reference_and_feed_cross_to_the_worker() {
    let mut harness = harness();
    let mut feed = harness.playback.feed();
    let mut reference = harness.playback.echo_reference();
    harness.playback.configure(Some(PlaybackRequest {
        source: PlaybackSource::Feed,
        ..tone(Some("a"), 1.0)
    }));
    let (_, mut egress, _) = next_open(&harness);
    assert_eq!(feed.push(&[0.3; 960]), 0);
    wait(|| egress.consumer.slots() >= 960);
    let played = render(&mut egress, 960);
    assert_eq!(played[2], 0.3);
    wait(|| reference.available() >= 960);
    let mut frame = vec![0.0; 960];
    assert!(reference.read(&mut frame));
    assert!(frame.iter().all(|sample| *sample == 0.3));
    let replaced = harness.playback.feed();
    wait(|| !feed.connected());
    assert!(replaced.connected());
}

#[test]
fn sound_feeds_cross_to_the_worker_and_one_too_many_lets_the_oldest_go() {
    let mut harness = harness();
    let mut feed = harness.playback.feed();
    let mut sounds: Vec<Feed> = (0..=MAX_SOUNDS)
        .map(|_| harness.playback.sound_feed())
        .collect();
    wait(|| !sounds[0].connected());
    assert!(sounds[1..].iter().all(Feed::connected));
    // All queued before the output opens, so its first fill mixes them all.
    assert_eq!(feed.push(&[0.25; 960]), 0);
    for sound in &mut sounds[1..] {
        assert_eq!(sound.push(&[0.125; 960]), 0);
    }
    harness.playback.configure(Some(PlaybackRequest {
        source: PlaybackSource::Feed,
        ..tone(Some("a"), 1.0)
    }));
    let (_, mut egress, _) = next_open(&harness);
    wait(|| egress.consumer.slots() >= 960);
    let played = render(&mut egress, 960);
    assert_eq!(played[2], 0.25 + MAX_SOUNDS as f32 * 0.125);
}
