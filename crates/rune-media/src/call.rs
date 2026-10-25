//! The media of one call, on its own thread, every 20 ms: the processed
//! microphone through its gate to Opus and out, and what arrives from the
//! others through the [`Mixer`] to the output.
//!
//! It knows no service: packets leave through `send` and arrive from
//! `receive`, both supplied by the host. It opens no device either: the
//! microphone comes from a [`Tap`] the capture fills and the sound goes to a
//! [`Feed`] the output plays, with a voice message mixed in when the host
//! hands one over ([`CallMedia::play_alongside`]), and the call's own sounds
//! ([`CallMedia::play_sound`]).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use rune_i18n::t;

use crate::codec::{Encoder, FRAME};
use crate::jitter::JitterStats;
use crate::mixer::Mixer;
use crate::{dbfs, meter_position, rms, Feed, MediaError, Tap, VoiceActivity};

const TICK: Duration = Duration::from_millis(20);
/// Packets taken from the network per tick, at most: four per source.
const MAX_INCOMING: usize = 256;
/// Microphone audio kept waiting beyond this is old: speech goes out late
/// rather than never, but never more than 60 ms late.
const MIC_BACKLOG: usize = 3 * FRAME;
/// A voice message runs 60 ms ahead of its clock; more waiting than this
/// means the call fell behind it, and the oldest goes.
const CLIP_BACKLOG: usize = 10 * FRAME;
/// Call sounds playing at once, at most; a new one past this drops the oldest.
const MAX_SOUNDS: usize = 4;

/// What the host decides about the call's media.
#[derive(Clone, Debug, PartialEq)]
pub struct CallControls {
    /// The microphone may go out: connected, not muted, and the
    /// push-to-talk key held when it is in use.
    pub open: bool,
    /// Send only while the person speaks; off with push-to-talk, where
    /// everything held goes out.
    pub voice_activity: bool,
    /// [`VoiceActivity`] thresholds, as meter positions.
    pub activity_on: f32,
    pub activity_off: f32,
    /// Nothing is heard; what arrives is dropped.
    pub deafened: bool,
    /// The listener's volume (0–2) and mute for each person, by key.
    pub choices: Vec<(u64, f32, bool)>,
}

impl Default for CallControls {
    fn default() -> Self {
        let activity = VoiceActivity::default();
        Self {
            open: false,
            voice_activity: true,
            activity_on: activity.on,
            activity_off: activity.off,
            deafened: false,
            choices: Vec::new(),
        }
    }
}

/// A packet from someone in the call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Incoming {
    /// Who sent it, in the host's terms.
    pub key: u64,
    pub sequence: u16,
    pub packet: Vec<u8>,
}

/// What the call's media is doing now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CallStatus {
    /// Each remote stream and whether it is heard speaking.
    pub speaking: Vec<(u64, bool)>,
    /// The microphone is going out.
    pub sending: bool,
    pub received: JitterStats,
    /// Microphone samples dropped as too old.
    pub mic_dropped: u64,
    /// The engine stopped: the encoder or the host's link failed.
    pub failed: Option<String>,
}

/// What the interface last saw of the status: the speaking marks and failure.
#[derive(Default)]
struct Shown {
    speaking: Vec<(u64, bool)>,
    failed: bool,
}

impl Shown {
    /// Whether `status` differs from what was seen, and remembers it. Counters
    /// such as the jitter statistics change every tick and do not count.
    fn changed(&mut self, status: &CallStatus) -> bool {
        let failed = status.failed.is_some();
        if self.speaking == status.speaking && self.failed == failed {
            return false;
        }
        self.speaking.clone_from(&status.speaking);
        self.failed = failed;
        true
    }
}

struct Shared {
    controls: Mutex<CallControls>,
    /// A voice message to mix in, until the engine takes it.
    clip: Mutex<Option<Tap>>,
    /// Call sounds to mix in, until the engine takes them.
    sounds: Mutex<Vec<Tap>>,
    status: Mutex<CallStatus>,
    shutdown: AtomicBool,
}

/// Handle to a call's media thread. Dropping it stops the thread, which
/// then releases the tap, the feed and both link ends.
pub struct CallMedia {
    shared: Arc<Shared>,
    worker: Thread,
}

type Send = Box<dyn FnMut(Vec<u8>) + std::marker::Send>;
type Receive = Box<dyn FnMut() -> Option<Incoming> + std::marker::Send>;

impl CallMedia {
    /// `wake` is called when [`CallStatus`] changes what the interface shows
    /// (the speaking marks or the failure), so it repaints instead of polling.
    pub fn start(
        microphone: Tap,
        output: Feed,
        send: impl FnMut(Vec<u8>) + std::marker::Send + 'static,
        receive: impl FnMut() -> Option<Incoming> + std::marker::Send + 'static,
        wake: impl Fn() + std::marker::Send + 'static,
    ) -> Result<Self, MediaError> {
        let mut engine = Engine::new(microphone, output, Box::new(send), Box::new(receive))?;
        let shared = Arc::new(Shared {
            controls: Mutex::new(CallControls::default()),
            clip: Mutex::new(None),
            sounds: Mutex::new(Vec::new()),
            status: Mutex::new(CallStatus::default()),
            shutdown: AtomicBool::new(false),
        });
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("rune-call".into())
            .spawn(move || {
                let mut deadline = Instant::now();
                let mut shown = Shown::default();
                while !worker_shared.shutdown.load(Ordering::Acquire) {
                    if let Ok(controls) = worker_shared.controls.try_lock() {
                        engine.configure(&controls);
                    }
                    if let Some(clip) = worker_shared
                        .clip
                        .try_lock()
                        .ok()
                        .and_then(|mut c| c.take())
                    {
                        engine.clip = Some(clip);
                    }
                    if let Ok(mut sounds) = worker_shared.sounds.try_lock() {
                        engine.sounds.append(&mut sounds);
                        let excess = engine.sounds.len().saturating_sub(MAX_SOUNDS);
                        engine.sounds.drain(..excess);
                    }
                    let status = engine.tick(Instant::now());
                    let failed = status.failed.is_some();
                    let changed = shown.changed(&status);
                    if let Ok(mut current) = worker_shared.status.lock() {
                        *current = status;
                    }
                    if changed {
                        wake();
                    }
                    if failed {
                        break;
                    }
                    deadline += TICK;
                    let now = Instant::now();
                    if deadline < now {
                        // Asleep too long: skip the missed ticks.
                        deadline = now;
                    }
                    // Never a tick before its time: one early takes a packet
                    // before its turn and the speaker runs dry (2026-10-21:
                    // a sound handed over woke it early). `sleep` waits on a
                    // high-resolution timer on Windows, where parking rounds
                    // up to the system's 15.6 ms tick.
                    loop {
                        let now = Instant::now();
                        if now >= deadline || worker_shared.shutdown.load(Ordering::Acquire) {
                            break;
                        }
                        thread::sleep(deadline - now);
                    }
                }
            })
            .map_err(|_| MediaError(t("No se pudo iniciar el audio de la llamada.").into()))?;
        Ok(Self {
            shared,
            worker: worker.thread().clone(),
        })
    }

    pub fn set_controls(&self, controls: CallControls) {
        let mut current = self
            .shared
            .controls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *current = controls;
    }

    /// Play what `clip` receives along with the call, replacing the last
    /// one; it goes once its feed is dropped. It is never sent.
    pub fn play_alongside(&self, clip: Tap) {
        *self
            .shared
            .clip
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(clip);
        self.worker.unpark();
    }

    /// Play what `sound` receives along with the call and any other sound,
    /// heard even while deafened: the deafen sound is one. It is never sent.
    pub fn play_sound(&self, sound: Tap) {
        self.shared
            .sounds
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(sound);
        self.worker.unpark();
    }

    pub fn status(&self) -> CallStatus {
        self.shared
            .status
            .lock()
            .map(|status| status.clone())
            .unwrap_or_default()
    }
}

impl Drop for CallMedia {
    fn drop(&mut self) {
        self.shared.shutdown.store(true, Ordering::Release);
        self.worker.unpark();
    }
}

struct Engine {
    encoder: Encoder,
    mixer: Mixer,
    microphone: Tap,
    output: Feed,
    clip: Option<Tap>,
    sounds: Vec<Tap>,
    send: Send,
    receive: Receive,
    voice: VoiceActivity,
    controls: CallControls,
    mic_dropped: u64,
}

impl Engine {
    fn new(
        microphone: Tap,
        output: Feed,
        send: Send,
        receive: Receive,
    ) -> Result<Self, MediaError> {
        Ok(Self {
            encoder: Encoder::new()?,
            mixer: Mixer::default(),
            microphone,
            output,
            clip: None,
            sounds: Vec::new(),
            send,
            receive,
            voice: VoiceActivity::default(),
            controls: CallControls::default(),
            mic_dropped: 0,
        })
    }

    fn configure(&mut self, controls: &CallControls) {
        if *controls == self.controls {
            return;
        }
        // A choice the host no longer lists went back to the usual.
        for &(key, ..) in &self.controls.choices {
            if !controls.choices.iter().any(|choice| choice.0 == key) {
                self.mixer.set_choice(key, 1.0, false);
            }
        }
        for &(key, volume, muted) in &controls.choices {
            self.mixer.set_choice(key, volume, muted);
        }
        self.voice.on = controls.activity_on;
        self.voice.off = controls.activity_off;
        self.controls = controls.clone();
    }

    fn tick(&mut self, now: Instant) -> CallStatus {
        let mut failed = None;
        let deafened = self.controls.deafened;
        for _ in 0..MAX_INCOMING {
            let Some(incoming) = (self.receive)() else {
                break;
            };
            if !deafened {
                self.mixer
                    .push(incoming.key, incoming.sequence, incoming.packet);
            }
        }
        // Read while deafened too, so it keeps its pace.
        let mut clip = [0.0; FRAME];
        if let Some(tap) = &mut self.clip {
            tap.trim(CLIP_BACKLOG);
            if !tap.read(&mut clip) && tap.abandoned() {
                self.clip = None;
            }
        }
        let mut sound = [0.0; FRAME];
        let mut sounding = false;
        self.sounds.retain_mut(|tap| {
            let mut frame = [0.0; FRAME];
            tap.trim(CLIP_BACKLOG);
            if !tap.read(&mut frame) {
                return !tap.abandoned();
            }
            sounding = true;
            for (sum, sample) in sound.iter_mut().zip(frame) {
                *sum += sample;
            }
            true
        });
        if deafened {
            self.mixer.clear();
            if sounding {
                self.output.push(&sound);
            }
        } else {
            let mut frame = [0.0; FRAME];
            self.mixer.mix(&mut frame);
            for ((sum, sample), sound) in frame.iter_mut().zip(clip).zip(sound) {
                *sum += sample + sound;
            }
            // Silence too: the output keeps its pace between words.
            self.output.push(&frame);
        }
        self.mic_dropped += self.microphone.trim(MIC_BACKLOG) as u64;
        let mut sending = false;
        let mut frame = [0.0; FRAME];
        // Two frames when one waits beyond the tick: the clocks drift.
        for _ in 0..2 {
            if self.microphone.available() < FRAME || !self.microphone.read(&mut frame) {
                break;
            }
            if !self.controls.open {
                self.voice.reset();
                continue;
            }
            // A click (a key, a tap on the desk) does not open it.
            let level = meter_position(dbfs(rms(&frame)));
            let speaking = self
                .voice
                .update_sustained(level, crate::sustained_level(&frame), now);
            if self.controls.voice_activity && !speaking {
                continue;
            }
            match self.encoder.encode(&frame) {
                Ok(packet) => {
                    (self.send)(packet);
                    sending = true;
                }
                Err(error) => failed = Some(error.to_string()),
            }
            if self.microphone.available() < 2 * FRAME {
                break;
            }
        }
        CallStatus {
            speaking: self.mixer.speaking().collect(),
            sending,
            received: self.mixer.stats(),
            mic_dropped: self.mic_dropped,
            failed,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codec::Decoder;
    use std::sync::mpsc;

    fn engine() -> (
        Engine,
        Feed,
        rtrb::Consumer<f32>,
        mpsc::Receiver<Vec<u8>>,
        mpsc::SyncSender<Incoming>,
    ) {
        let (mic, tap) = crate::tap(48_000);
        let (output, heard) = Feed::pair(48_000);
        let (sent, outgoing) = mpsc::sync_channel(64);
        let (incoming, received) = mpsc::sync_channel(64);
        let engine = Engine::new(
            tap,
            output,
            Box::new(move |packet| {
                let _ = sent.try_send(packet);
            }),
            Box::new(move || received.try_recv().ok()),
        )
        .unwrap();
        (engine, mic, heard, outgoing, incoming)
    }

    fn speech(mic: &mut Feed, frames: usize) {
        for frame in 0..frames {
            let pcm: Vec<f32> = (0..FRAME)
                .map(|n| 0.5 * (((frame * FRAME + n) as f32) * 0.05).sin())
                .collect();
            mic.push(&pcm);
        }
    }

    #[test]
    fn the_microphone_goes_out_only_through_its_gate() {
        let (mut engine, mut mic, _heard, outgoing, _) = engine();
        let now = Instant::now();
        speech(&mut mic, 2);
        assert!(!engine.tick(now).sending, "closed: nothing leaves");
        assert!(outgoing.try_recv().is_err());
        engine.configure(&CallControls {
            open: true,
            ..CallControls::default()
        });
        speech(&mut mic, 1);
        assert!(engine.tick(now).sending, "a voice opens voice activity");
        let packet = outgoing.try_recv().unwrap();
        let mut decoded = vec![0.0; crate::codec::MAX_PACKET_SAMPLES];
        assert_eq!(
            Decoder::new()
                .unwrap()
                .decode(&packet, &mut decoded)
                .unwrap(),
            FRAME
        );
        mic.push(&[0.0; FRAME]);
        assert!(
            engine.tick(now + Duration::from_secs(1)).sending,
            "held between words"
        );
        mic.push(&[0.0; FRAME]);
        assert!(
            !engine.tick(now + Duration::from_secs(2)).sending,
            "silence stops it"
        );
        // Push-to-talk sends whatever is held, silence included.
        engine.configure(&CallControls {
            open: true,
            voice_activity: false,
            ..CallControls::default()
        });
        mic.push(&[0.0; FRAME]);
        assert!(engine.tick(now).sending);
        // A stalled reader never lets old speech pile up.
        speech(&mut mic, 50);
        engine.tick(now);
        assert!(engine.microphone.available() <= MIC_BACKLOG);
        assert!(engine.mic_dropped > 0);
    }

    #[test]
    fn remote_speech_is_played_and_deafen_drops_it() {
        let (mut engine, _mic, mut heard, _, incoming) = engine();
        let mut encoder = Encoder::new().unwrap();
        let now = Instant::now();
        let mut played = 0.0f32;
        for sequence in 0..10u16 {
            let pcm: [f32; FRAME] =
                std::array::from_fn(|n| 0.5 * ((n as f32) * 0.05 + f32::from(sequence)).sin());
            incoming
                .try_send(Incoming {
                    key: 7,
                    sequence,
                    packet: encoder.encode(&pcm).unwrap(),
                })
                .unwrap();
            let status = engine.tick(now);
            while let Ok(sample) = heard.pop() {
                played = played.max(sample.abs());
            }
            if sequence > 3 {
                assert_eq!(status.speaking, vec![(7, true)]);
            }
        }
        assert!(played > 0.2, "heard: {played}");
        engine.configure(&CallControls {
            deafened: true,
            ..CallControls::default()
        });
        incoming
            .try_send(Incoming {
                key: 7,
                sequence: 10,
                packet: vec![0xF8, 0xFF, 0xFE],
            })
            .unwrap();
        let status = engine.tick(now);
        assert!(status.speaking.is_empty(), "deafened: no streams kept");
        assert_eq!(heard.slots(), 0, "and nothing played");
    }

    #[test]
    fn a_voice_message_plays_along_never_goes_out_and_leaves_on_its_own() {
        let (mut engine, mut mic, mut heard, outgoing, _) = engine();
        engine.configure(&CallControls {
            open: true,
            voice_activity: false,
            ..CallControls::default()
        });
        let (mut clip, tap) = crate::tap(48_000);
        engine.clip = Some(tap);
        clip.push(&[0.25; 2 * FRAME]);
        mic.push(&[0.0; FRAME]);
        let now = Instant::now();
        assert!(engine.tick(now).sending);
        let mut frame = [0.0; FRAME];
        for sample in &mut frame {
            *sample = heard.pop().unwrap();
        }
        assert!(frame.iter().all(|sample| (sample - 0.25).abs() < 1e-6));
        let mut sent = vec![0.0; crate::codec::MAX_PACKET_SAMPLES];
        let length = Decoder::new()
            .unwrap()
            .decode(&outgoing.try_recv().unwrap(), &mut sent)
            .unwrap();
        assert!(
            rms(&sent[..length]) < 0.01,
            "only the silent microphone left"
        );
        drop(clip);
        engine.tick(now);
        assert!(engine.clip.is_some(), "what is queued still plays");
        engine.tick(now);
        assert!(engine.clip.is_none(), "then it goes, and the call goes on");
        assert_eq!(heard.slots(), 2 * FRAME);
    }

    #[test]
    fn a_choice_the_host_drops_goes_back_to_the_usual() {
        let (mut engine, ..) = engine();
        engine.configure(&CallControls {
            choices: vec![(7, 0.5, true)],
            ..CallControls::default()
        });
        assert_eq!(engine.mixer.choice(7), (0.5, true));
        engine.configure(&CallControls::default());
        assert_eq!(engine.mixer.choice(7), (1.0, false));
    }

    #[test]
    fn a_call_sound_is_heard_even_deafened_and_leaves_on_its_own() {
        let (mut engine, _mic, mut heard, _, _) = engine();
        engine.configure(&CallControls {
            deafened: true,
            ..CallControls::default()
        });
        let (mut sound, tap) = crate::tap(48_000);
        engine.sounds.push(tap);
        sound.push(&[0.25; FRAME]);
        let now = Instant::now();
        engine.tick(now);
        assert_eq!(heard.slots(), FRAME, "the sound plays though deafened");
        while let Ok(sample) = heard.pop() {
            assert!((sample - 0.25).abs() < 1e-6);
        }
        drop(sound);
        engine.tick(now);
        assert!(engine.sounds.is_empty(), "it goes once played");
        assert_eq!(heard.slots(), 0, "and deafened, nothing else is heard");
    }

    #[test]
    fn dropping_the_handle_stops_the_thread_and_releases_the_output() {
        let (mic, tap) = crate::tap(4_800);
        let (output, heard) = Feed::pair(4_800);
        let media = CallMedia::start(tap, output, |_| {}, || None, || {}).unwrap();
        media.set_controls(CallControls::default());
        drop(media);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !heard.is_abandoned() || mic.connected() {
            assert!(Instant::now() < deadline, "the thread let go of both ends");
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn the_interface_wakes_only_when_its_speaking_marks_or_failure_change() {
        let mut shown = Shown::default();
        let mut status = CallStatus::default();
        assert!(
            !shown.changed(&status),
            "nothing shown yet, nothing changed"
        );
        status.received.received += 1;
        assert!(!shown.changed(&status), "counters alone do not wake");
        status.speaking = vec![(7, true)];
        assert!(shown.changed(&status), "a speaking mark wakes once");
        assert!(!shown.changed(&status), "the same marks do not");
        status.speaking = vec![(7, false)];
        assert!(shown.changed(&status));
        status.failed = Some("cut".into());
        assert!(shown.changed(&status), "a failure wakes");
        assert!(!shown.changed(&status));
    }
}
