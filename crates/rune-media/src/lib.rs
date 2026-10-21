//! Local audio devices, bounded capture, speech preprocessing and output.
//!
//! The capture and playback controllers own their audio streams off the UI
//! thread. Their real-time callbacks only convert samples and use bounded SPSC
//! queues. Samples are never persisted. They leave this machine only through
//! a call ([`call::CallMedia`]), Opus-encoded, while the host opens its gate;
//! what plays is the test tone, the microphone's own loopback, a call or a
//! voice message ([`Clip`]).

use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait};

/// The media of one call: gate, Opus, jitter and mix, off the UI thread.
pub mod call;
mod capture;
mod clip;
/// Opus at 48 kHz mono in 20 ms frames.
pub mod codec;
/// A bounded reorder buffer for one remote stream.
pub mod jitter;
/// What the system plays, as an echo reference only.
mod loopback;
/// Remote streams summed into one frame, with per-person volume.
pub mod mixer;
/// The pictures of a Matroska or WebM file: how they are coded.
mod mkv;
/// The pictures of an MP4 file: where each is and how it is coded.
pub mod mp4;
mod playback;
mod processing;
/// The person's own call sounds, from a folder they chose.
pub mod sounds;
mod stretch;
/// Local screen and camera preview through the desktop portal; on Windows
/// through RUNE's own picker, Windows.Graphics.Capture and Media
/// Foundation (windows-medios).
#[cfg(any(target_os = "linux", windows))]
pub mod video;

pub use capture::{Capture, CaptureRequest, CaptureSnapshot, CaptureState};
pub use clip::{Clip, ClipStatus};
pub use playback::{
    tap, EchoReference, Feed, Playback, PlaybackRequest, PlaybackSnapshot, PlaybackSource, Tap,
    MAX_VOLUME, PLAYBACK_RATE,
};
pub use processing::{
    CaptureProcessor, ProcessingMetrics, ProcessingMode, MAX_INPUT_VOLUME, REFERENCE_FRAME,
    RNNOISE_RATE,
};

/// The quietest level the meter shows, in dBFS: anything below reads as silence.
pub const FLOOR_DBFS: f32 = -60.0;

/// An audio device of this machine.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceInfo {
    /// Stable across runs where the platform allows; what to keep in settings.
    pub id: String,
    /// What the platform calls it.
    pub name: String,
    /// The one the system uses unless told otherwise.
    pub default: bool,
}

/// The audio devices of this machine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Devices {
    /// The audio system they were found through, such as PipeWire or ALSA.
    pub backend: String,
    pub inputs: Vec<DeviceInfo>,
    pub outputs: Vec<DeviceInfo>,
}

/// Something went wrong with a device.
#[derive(Debug)]
pub struct MediaError(pub String);

impl std::fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for MediaError {}

impl From<cpal::Error> for MediaError {
    fn from(error: cpal::Error) -> Self {
        Self(error.to_string())
    }
}

/// List the microphones and speakers. It asks the audio system, which can
/// take a moment: call it off the interface's thread.
pub fn devices() -> Result<Devices, MediaError> {
    let host = cpal::default_host();
    let id = |device: &cpal::Device| {
        device
            .id()
            .map(|id| id.to_string())
            .unwrap_or_else(|_| device.to_string())
    };
    let default_input = host.default_input_device().map(|device| id(&device));
    let default_output = host.default_output_device().map(|device| id(&device));
    let describe = |device: cpal::Device, default: &Option<String>| {
        let id = id(&device);
        DeviceInfo {
            default: default.as_deref() == Some(id.as_str()),
            name: device.to_string(),
            id,
        }
    };
    Ok(Devices {
        backend: format!("{:?}", host.id()),
        inputs: host
            .input_devices()?
            .filter(|device| !output_monitor(&host, device))
            .map(|device| describe(device, &default_input))
            .collect(),
        outputs: host
            .output_devices()?
            .map(|device| describe(device, &default_output))
            .collect(),
    })
}

/// Whether `device`, an input of `host`, records what an output plays.
fn output_monitor(host: &cpal::Host, device: &cpal::Device) -> bool {
    let direction = device.description().map(|it| it.direction());
    records_output(&format!("{:?}", host.id()), direction.unwrap_or_default())
}

/// cpal lists each PipeWire sink (and its own `default_sink`) as a `Duplex`
/// device whose input is the sink's monitor: what this machine plays, never
/// a microphone. Other backends' duplex devices are real ones.
/// shortcut: a genuine `Audio/Duplex` PipeWire node is hidden too, as cpal
/// shows it the same way; upgrade if cpal exposes the node's media class.
fn records_output(backend: &str, direction: cpal::DeviceDirection) -> bool {
    backend == "PipeWire" && direction == cpal::DeviceDirection::Duplex
}

/// The root mean square of `samples`, from 0 to 1 for samples in [-1, 1].
pub fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
}

/// A level as dBFS, never below [`FLOOR_DBFS`].
pub fn dbfs(level: f32) -> f32 {
    if level <= 0.0 || !level.is_finite() {
        FLOOR_DBFS
    } else {
        (20.0 * level.log10()).max(FLOOR_DBFS)
    }
}

/// Where a level in dBFS stands on a meter, from 0 (the floor) to 1 (full scale).
pub fn meter_position(dbfs: f32) -> f32 {
    if dbfs.is_finite() {
        ((dbfs - FLOOR_DBFS) / -FLOOR_DBFS).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// The meter position of the second quietest quarter of `frame`: speech
/// fills all four, a click one, so this stays low for a click.
pub fn sustained_level(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return 0.0;
    }
    let quarter = frame.len().div_ceil(4);
    let mut quietest = [f32::MAX; 2];
    for part in frame.chunks(quarter).take(4) {
        let level = rms(part);
        if level < quietest[0] {
            quietest = [level, quietest[0]];
        } else if level < quietest[1] {
            quietest[1] = level;
        }
    }
    meter_position(dbfs(if quietest[1] == f32::MAX {
        quietest[0]
    } else {
        quietest[1]
    }))
}

/// Whether someone is speaking, from the levels of their microphone: on
/// above one level, off only after staying under a lower one for a while, so
/// the mark does not flicker between words.
#[derive(Clone, Debug)]
pub struct VoiceActivity {
    /// Meter position that starts speech.
    pub on: f32,
    /// Meter position under which silence counts.
    pub off: f32,
    /// Silence needed to stop.
    pub hold: Duration,
    /// Set `on` and `off` from the room instead: `on` stands
    /// [`Self::MARGIN`] over the quietest level heard lately, within
    /// [`Self::LOWEST`] and [`Self::HIGHEST`], so a voice passes and a fan
    /// or a hum does not, whatever the microphone.
    pub automatic: bool,
    speaking: bool,
    quiet_since: Option<std::time::Instant>,
    /// The room's level, as a meter position, and when it was last heard.
    floor: f32,
    heard: Option<std::time::Instant>,
}

impl Default for VoiceActivity {
    /// About -27 dBFS to start and -33 dBFS to count as quiet, held for 350 ms:
    /// a voice at a normal distance, not the room.
    fn default() -> Self {
        Self {
            on: 0.55,
            off: 0.45,
            hold: Duration::from_millis(350),
            automatic: false,
            speaking: false,
            quiet_since: None,
            // Where `on` starts at its default.
            floor: 0.55 - Self::MARGIN,
            heard: None,
        }
    }
}

impl VoiceActivity {
    /// How far over the room speech starts when automatic: 15 dB.
    pub const MARGIN: f32 = 0.25;
    /// With the speech gate deciding ([`CaptureProcessor::set_speech_gate`]),
    /// what it let through counts as speech: from −55 dBFS, quiet under −58.
    pub const GATED_ON: f32 = 0.08;
    pub const GATED_OFF: f32 = 0.03;
    /// The automatic threshold never goes under -42 dBFS…
    pub const LOWEST: f32 = 0.3;
    /// …nor over -15 dBFS.
    pub const HIGHEST: f32 = 0.75;
    /// How fast the room's level may rise, per second: 1.2 dB, so a long
    /// sentence barely moves it while a quieter moment brings it down at once.
    const RISE: f32 = 0.02;

    /// Feed the meter position heard at `now`; returns whether the person speaks.
    pub fn update(&mut self, level: f32, now: std::time::Instant) -> bool {
        self.update_sustained(level, level, now)
    }

    /// [`Self::update`], where speech starts only on `sustained`, the level
    /// of a frame's second quietest quarter ([`sustained_level`]): a click
    /// (a key, a tap on the desk) fills one quarter and does not start it.
    /// What keeps it going is the whole frame's level.
    pub fn update_sustained(
        &mut self,
        level: f32,
        sustained: f32,
        now: std::time::Instant,
    ) -> bool {
        let clean = |level: f32| {
            if level.is_finite() {
                level.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let (level, sustained) = (clean(level), clean(sustained));
        if self.automatic {
            self.follow(level, now);
        }
        let on = if self.on.is_finite() {
            self.on.clamp(0.0, 1.0)
        } else {
            0.55
        };
        let off = if self.off.is_finite() {
            self.off.clamp(0.0, on)
        } else {
            on
        };
        // A zero threshold must not turn digital silence into speech.
        if (sustained > 0.0 && sustained >= on) || (self.speaking && level > 0.0 && level >= on) {
            self.speaking = true;
            self.quiet_since = None;
        } else if self.speaking && (level == 0.0 || level < off) {
            let since = *self.quiet_since.get_or_insert(now);
            if now.saturating_duration_since(since) >= self.hold {
                self.speaking = false;
                self.quiet_since = None;
            }
        } else if level >= off {
            self.quiet_since = None;
        }
        self.speaking
    }

    /// Track the quietest recent level and set the thresholds over it.
    fn follow(&mut self, level: f32, now: std::time::Instant) {
        let elapsed = self
            .heard
            .map_or(0.0, |heard| {
                now.saturating_duration_since(heard).as_secs_f32()
            })
            .min(1.0);
        self.heard = Some(now);
        self.floor = if level < self.floor {
            level
        } else {
            (self.floor + Self::RISE * elapsed).min(level)
        };
        let usual = Self::default();
        self.on = (self.floor + Self::MARGIN).clamp(Self::LOWEST, Self::HIGHEST);
        self.off = self.on - (usual.on - usual.off);
    }

    /// Forget held activity when capture stops, changes devices or is muted.
    /// What automatic sensitivity learned of the room stays.
    pub fn reset(&mut self) {
        self.speaking = false;
        self.quiet_since = None;
    }

    pub fn speaking(&self) -> bool {
        self.speaking
    }
}

/// Whether push-to-talk lets the microphone through: while its key is held,
/// and for `release_delay` after, so the end of a word is not cut. A key whose
/// release was lost (focus moved, the desktop's session ended) reads as
/// released, since the caller passes what it knows is held right now.
#[derive(Clone, Debug, Default)]
pub struct PushToTalk {
    pub release_delay: Duration,
    released: Option<std::time::Instant>,
    transmitting: bool,
}

impl PushToTalk {
    /// Feed whether the key is held at `now`; returns whether to transmit.
    pub fn update(&mut self, held: bool, now: std::time::Instant) -> bool {
        if held {
            self.transmitting = true;
            self.released = None;
        } else if self.transmitting {
            let since = *self.released.get_or_insert(now);
            if now.saturating_duration_since(since) >= self.release_delay {
                self.reset();
            }
        }
        self.transmitting
    }

    /// Stop at once, without the delay: capture stopped or was muted.
    pub fn reset(&mut self) {
        self.transmitting = false;
        self.released = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_pipewire_sinks_count_as_output_monitors() {
        use cpal::DeviceDirection::{Duplex, Input, Output, Unknown};
        // The name `devices` reports, which the rule compares against.
        #[cfg(target_os = "linux")]
        assert_eq!(format!("{:?}", cpal::HostId::PipeWire), "PipeWire");
        // `sink_default` and `alsa_output.*`, as cpal lists them as inputs.
        assert!(records_output("PipeWire", Duplex));
        for direction in [Input, Output, Unknown] {
            assert!(!records_output("PipeWire", direction), "{direction:?}");
        }
        // An ALSA `hw:` card or a WASAPI/CoreAudio duplex device is a real one.
        for backend in ["Alsa", "Wasapi", "CoreAudio"] {
            assert!(!records_output(backend, Duplex), "{backend}");
        }
    }

    #[test]
    fn push_to_talk_holds_for_its_release_delay_and_a_new_press_keeps_it() {
        let start = std::time::Instant::now();
        let at = |millis: u64| start + Duration::from_millis(millis);
        let mut ptt = PushToTalk {
            release_delay: Duration::from_millis(100),
            ..Default::default()
        };
        assert!(!ptt.update(false, at(0)), "nothing held, nothing sent");
        assert!(ptt.update(true, at(10)));
        assert!(ptt.update(false, at(20)), "the delay keeps the word's end");
        assert!(ptt.update(true, at(90)), "pressing again inside the delay");
        assert!(ptt.update(false, at(100)));
        assert!(ptt.update(false, at(199)));
        assert!(!ptt.update(false, at(200)), "the delay passed");
        assert!(ptt.update(true, at(300)));
        ptt.reset();
        assert!(!ptt.transmitting, "a reset skips the delay");
        let mut instant = PushToTalk::default();
        assert!(instant.update(true, at(0)));
        assert!(!instant.update(false, at(0)), "no delay releases at once");
    }

    #[test]
    fn speech_starts_at_once_and_ends_after_a_held_silence() {
        let start = std::time::Instant::now();
        let at = |millis: u64| start + Duration::from_millis(millis);
        let mut voice = VoiceActivity::default();
        assert!(!voice.update(0.2, at(0)), "the room is not speech");
        assert!(voice.update(0.7, at(10)), "a word starts it");
        assert!(voice.update(0.5, at(20)), "between the two levels it holds");
        assert!(
            voice.update(0.1, at(30)),
            "a short pause between words keeps it"
        );
        assert!(voice.update(0.1, at(200)));
        assert!(
            voice.update(0.6, at(250)),
            "speaking again resets the pause"
        );
        assert!(voice.update(0.1, at(300)));
        assert!(!voice.update(0.1, at(700)), "a held silence ends it");
    }

    #[test]
    fn automatic_sensitivity_sets_its_threshold_over_the_room() {
        let start = std::time::Instant::now();
        let at = |millis: u64| start + Duration::from_millis(millis);
        let mut voice = VoiceActivity {
            automatic: true,
            ..Default::default()
        };
        // A quiet room brings it down at once, never under its lowest.
        assert!(!voice.update(0.0, at(0)));
        assert_eq!(voice.on, VoiceActivity::LOWEST);
        assert!(voice.off < voice.on);
        assert!(voice.update(0.35, at(100)), "a soft voice passes");
        // A long sentence only raises the room slowly.
        for second in 1..=5 {
            voice.update(0.6, at(100 + second * 1_000));
        }
        assert!(voice.on < 0.45, "{}", voice.on);
        voice.reset();
        assert!(!voice.speaking() && voice.on < 0.45, "reset keeps the room");
        // A fan at -30 dBFS that never stops: speech must stand 15 dB over it.
        for second in 6..=40 {
            voice.update(0.5, at(second * 1_000));
        }
        assert!((voice.on - 0.75).abs() < 1e-6, "{}", voice.on);
        assert!(
            !voice.update(0.7, at(40_100)),
            "a little over the fan is the fan"
        );
        assert!(voice.update(0.8, at(40_200)));
    }

    #[test]
    fn a_full_scale_sine_reads_three_decibels_under_full_scale() {
        let sine: Vec<f32> = (0..4_800)
            .map(|n| (n as f32 * std::f32::consts::TAU * 440.0 / 48_000.0).sin())
            .collect();
        assert!((dbfs(rms(&sine)) + 3.01).abs() < 0.05);
        assert!((meter_position(dbfs(rms(&sine))) - 0.95).abs() < 0.01);
    }

    #[test]
    fn silence_and_nonsense_read_as_the_floor() {
        assert_eq!(dbfs(rms(&[])), FLOOR_DBFS);
        assert_eq!(dbfs(rms(&[0.0; 64])), FLOOR_DBFS);
        assert_eq!(dbfs(f32::NAN), FLOOR_DBFS);
        assert_eq!(meter_position(FLOOR_DBFS), 0.0);
        assert_eq!(meter_position(6.0), 1.0, "clipping stays on the meter");
        assert_eq!(meter_position(f32::NAN), 0.0);
    }

    #[test]
    fn zero_threshold_never_classifies_silence_or_invalid_values_as_speech() {
        let now = std::time::Instant::now();
        let mut voice = VoiceActivity {
            on: 0.0,
            off: 0.0,
            ..Default::default()
        };
        for level in [0.0, f32::NAN, f32::INFINITY, -1.0] {
            assert!(!voice.update(level, now));
        }
        assert!(voice.update(0.2, now));
        assert!(voice.update(0.0, now));
        assert!(!voice.update(0.0, now + voice.hold));
        assert!(voice.update(0.2, now));
        voice.reset();
        assert!(!voice.speaking());
        assert!(!voice.update(0.0, now));
    }
}
