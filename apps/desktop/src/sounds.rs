//! The call's sounds: what happened since the last frame, and what each
//! sounds like. The built-ins are Discord's public static MP3 assets, with
//! their provenance and checksums in `assets/sounds/discord`; no profile,
//! cache, credential, or session data is read. A folder the person chose can
//! replace any of them, read on a thread of its own ([`Library`]).
//!
//! What changed is read by comparing each frame with the one before: the
//! first look at a call, or at the account's controls, only remembers them.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;

use rune_media::sounds::Found;
use rune_model::{CallPhase, ChannelId, StreamPhase, UserId};
use rune_state::Store;
use rune_ui::{CallSound, FolderReport, Ui};

/// The built-in sound for `sound`.
pub fn default_audio(sound: CallSound) -> &'static [u8] {
    match sound {
        CallSound::Join | CallSound::UserJoin => {
            include_bytes!("../../../assets/sounds/discord/user_join.mp3")
        }
        CallSound::Leave | CallSound::UserLeave => {
            include_bytes!("../../../assets/sounds/discord/user_leave.mp3")
        }
        CallSound::Mute => include_bytes!("../../../assets/sounds/discord/mute.mp3"),
        CallSound::Unmute => include_bytes!("../../../assets/sounds/discord/unmute.mp3"),
        CallSound::Deafen => include_bytes!("../../../assets/sounds/discord/deafen.mp3"),
        CallSound::Undeafen => include_bytes!("../../../assets/sounds/discord/undeafen.mp3"),
        CallSound::StreamStart => {
            include_bytes!("../../../assets/sounds/discord/stream_started.mp3")
        }
        CallSound::StreamStop => {
            include_bytes!("../../../assets/sounds/discord/stream_ended.mp3")
        }
        CallSound::UserStream => {
            include_bytes!("../../../assets/sounds/discord/stream_user_joined.mp3")
        }
        CallSound::Disconnect => include_bytes!("../../../assets/sounds/discord/disconnect.mp3"),
        CallSound::Message => include_bytes!("../../../assets/sounds/discord/message1.mp3"),
        // Private calls.
        CallSound::Ring => include_bytes!("../../../assets/sounds/discord/call_ringing.mp3"),
        CallSound::RingOut => include_bytes!("../../../assets/sounds/discord/call_calling.mp3"),
    }
}

/// The account's call as one frame saw it.
#[derive(Debug, PartialEq)]
struct Call {
    channel: ChannelId,
    connected: bool,
    /// It connected once: coming back after reconnecting is not joining.
    joined: bool,
    failed: bool,
    /// Who else is in it, and who of them streams.
    others: Vec<UserId>,
    streaming: Vec<UserId>,
}

/// What the last frame showed.
#[derive(Debug, Default)]
pub struct Watch {
    call: Option<Call>,
    /// The account's microphone and sound off.
    own: Option<(bool, bool)>,
    /// The account's own stream is on.
    sharing: Option<bool>,
    // Private calls.
    /// The ring sounding, and when it played last.
    ring: Option<(CallSound, std::time::Instant)>,
}

impl Watch {
    /// The sound for what changed since the last frame, if the settings
    /// want it: one's own controls first, then the call, then one's
    /// stream, then the others.
    pub fn step(&mut self, ui: &mut Ui, store: &Store) -> Option<CallSound> {
        let dropped = std::mem::take(&mut ui.call_sounds.dropped);
        let own = (ui.muted, ui.deafened);
        let mine = match self.own.replace(own) {
            Some((_, deafened)) if deafened != own.1 => Some(if own.1 {
                CallSound::Deafen
            } else {
                CallSound::Undeafen
            }),
            Some((muted, _)) if muted != own.0 => Some(if own.0 {
                CallSound::Mute
            } else {
                CallSound::Unmute
            }),
            _ => None,
        };
        let me = store.me();
        let call = store.call().map(|call| {
            let states = store.voice_in(call.channel);
            let mut others: Vec<UserId> = states
                .iter()
                .map(|state| state.user)
                .filter(|user| Some(*user) != me)
                .collect();
            let mut streaming: Vec<UserId> = states
                .iter()
                .filter(|state| state.streaming && Some(state.user) != me)
                .map(|state| state.user)
                .collect();
            others.sort_unstable();
            streaming.sort_unstable();
            let connected = call.phase.is_connected();
            let joined = connected
                || self
                    .call
                    .as_ref()
                    .is_some_and(|was| was.channel == call.channel && was.joined);
            Call {
                channel: call.channel,
                connected,
                joined,
                failed: matches!(call.phase, CallPhase::Failed { .. }),
                others,
                streaming,
            }
        });
        let sharing = store.sharing().is_some_and(|sharing| {
            matches!(sharing.phase, StreamPhase::Live | StreamPhase::Paused)
        });
        let stream = match self.sharing.replace(sharing) {
            Some(before) if before != sharing => Some(if sharing {
                CallSound::StreamStart
            } else {
                CallSound::StreamStop
            }),
            _ => None,
        };
        let before = self.call.take();
        let theirs = match (&before, &call) {
            // Gone, failed, or ended by the service with a reason: dropped.
            (Some(was), now) if !was.failed && now.as_ref().is_none_or(|now| now.failed) => {
                Some(if dropped || now.is_some() {
                    CallSound::Disconnect
                } else {
                    CallSound::Leave
                })
            }
            (Some(was), Some(now)) if was.channel == now.channel => {
                let joined = now.others.iter().any(|user| !was.others.contains(user));
                let left = was.others.iter().any(|user| !now.others.contains(user));
                let streams = now
                    .streaming
                    .iter()
                    .any(|user| !was.streaming.contains(user));
                if now.joined && !was.joined {
                    Some(CallSound::Join)
                } else if joined {
                    Some(CallSound::UserJoin)
                } else if left {
                    Some(CallSound::UserLeave)
                } else if streams {
                    Some(CallSound::UserStream)
                } else {
                    None
                }
            }
            // A first look, or another channel: only its connecting sounds.
            (_, Some(now)) if now.connected => Some(CallSound::Join),
            _ => None,
        };
        self.call = call;
        let call_level = theirs.filter(|sound| {
            matches!(
                sound,
                CallSound::Join | CallSound::Leave | CallSound::Disconnect
            )
        });
        mine.or(call_level)
            .or(stream)
            .or(theirs)
            .filter(|sound| ui.prefs.sound_on(*sound))
    }
}

impl Watch {
    /// The ring to play now, if one sounds (private calls): at once when
    /// it starts, then again each time its period passes, as the settings
    /// want it.
    pub fn ring(&mut self, ui: &Ui, store: &Store, now: std::time::Instant) -> Option<CallSound> {
        let wanted = rune_ui::call_ringing(store).filter(|sound| ui.prefs.sound_on(*sound));
        let due = match (self.ring, wanted) {
            (_, None) => {
                self.ring = None;
                return None;
            }
            (Some((sound, at)), Some(wanted)) if sound == wanted => sound
                .repeats()
                .is_some_and(|period| now.duration_since(at) >= period),
            _ => true,
        };
        let sound = wanted?;
        if due {
            self.ring = Some((sound, now));
        }
        due.then_some(sound)
    }
}

/// What a folder gave, read on its thread: the folder, and its sounds.
type Read = (String, Result<Vec<Found>, String>);

/// RUNE's sounds and the person's own: those of the folder the settings
/// name, read off the interface's thread when it changes or is used again.
#[derive(Default)]
pub struct Library {
    /// The folder of [`Self::own`], or being read.
    folder: Option<String>,
    own: HashMap<CallSound, Arc<[u8]>>,
    reading: Option<mpsc::Receiver<Read>>,
}

impl Library {
    /// Follow the folder the settings name; say what it gave once read.
    pub fn step(&mut self, ui: &mut Ui) {
        if let Some(reading) = &self.reading {
            match reading.try_recv() {
                Ok((folder, read)) => {
                    self.reading = None;
                    if Some(&folder) == self.folder.as_ref() {
                        let report = self.take(folder, read);
                        rune_ui::call_sounds_folder_read(ui, report);
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return,
                Err(mpsc::TryRecvError::Disconnected) => self.reading = None,
            }
        }
        let wanted = ui.prefs.sound_folder.clone();
        // «Usar» again, after adding files: read it again.
        let again = wanted.is_some() && ui.call_sounds.found.is_none();
        if wanted == self.folder && !again {
            return;
        }
        self.folder = wanted.clone();
        self.own.clear();
        let Some(folder) = wanted else {
            return;
        };
        let (sent, reading) = mpsc::channel();
        let names: Vec<&'static str> = CallSound::ALL.iter().map(|sound| sound.key()).collect();
        let spawned = std::thread::Builder::new()
            .name("rune-sounds".into())
            .spawn(move || {
                let read = rune_media::sounds::load(&expand(&folder), &names)
                    .map_err(|error| error.to_string());
                let _ = sent.send((folder, read));
            });
        if spawned.is_ok() {
            self.reading = Some(reading);
        }
    }

    /// What `read` of `folder` gave: kept, and said for the settings.
    fn take(&mut self, folder: String, read: Result<Vec<Found>, String>) -> FolderReport {
        let mut report = FolderReport {
            folder,
            ..FolderReport::default()
        };
        match read {
            Err(error) => report.error = Some(error),
            Ok(found) => {
                for (sound, found) in CallSound::ALL.into_iter().zip(found) {
                    match found {
                        Found::Sound { file, bytes } => {
                            self.own.insert(sound, bytes);
                            report.taken.push((sound, file));
                        }
                        Found::Refused { file, reason } => report.refused.push((file, reason)),
                        Found::Missing => {}
                    }
                }
            }
        }
        report
    }

    /// What `sound` sounds like: the person's, else the built-in sound.
    pub fn bytes(&self, sound: CallSound) -> Arc<[u8]> {
        self.own
            .get(&sound)
            .cloned()
            .unwrap_or_else(|| default_audio(sound).into())
    }
}

/// `~/…` from the home folder.
fn expand(folder: &str) -> PathBuf {
    match folder.strip_prefix("~/") {
        Some(rest) => rune_platform::folders::home()
            .unwrap_or_default()
            .join(rest),
        None => PathBuf::from(folder),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::VoiceState;
    use rune_provider::mock::{MockConfig, MockProvider};
    use rune_provider::Event;
    use std::time::Instant;

    const ROOM: ChannelId = ChannelId(900);

    fn state(user: UserId, streaming: bool) -> VoiceState {
        VoiceState {
            user,
            channel: ROOM,
            muted: false,
            deafened: false,
            speaking: false,
            streaming,
            server_muted: false,
            server_deafened: false,
            camera: false,
            suppressed: false,
            speak_request: None,
        }
    }

    fn present(store: &mut Store, user: UserId, here: bool) {
        let event = if here {
            Event::VoiceChanged(state(user, false))
        } else {
            Event::VoiceLeft {
                user,
                channel: ROOM,
            }
        };
        store.apply(event, Instant::now());
    }

    fn connected(store: &mut Store) {
        store.apply(
            Event::CallPhase {
                channel: ROOM,
                phase: CallPhase::Connected,
                quality: None,
            },
            Instant::now(),
        );
    }

    fn ready() -> (Store, Ui, Watch) {
        let mut store = Store::new();
        let provider = MockProvider::new(MockConfig::default());
        store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
        (store, Ui::new(), Watch::default())
    }

    #[test]
    fn sounds_follow_changes_never_the_first_look_and_respect_the_settings() {
        let (mut store, mut ui, mut watch) = ready();
        store.begin_call(ROOM, false, false);
        // Joining a room with people in it plays nothing for them…
        present(&mut store, UserId(77), true);
        assert_eq!(watch.step(&mut ui, &store), None, "the first look");
        assert_eq!(watch.step(&mut ui, &store), None, "nothing changed");
        // …but the call connecting plays the account's own.
        connected(&mut store);
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::Join));
        present(&mut store, UserId(78), true);
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::UserJoin));
        present(&mut store, UserId(77), false);
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::UserLeave));
        store.apply(Event::VoiceChanged(state(UserId(78), true)), Instant::now());
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::UserStream));
        ui.muted = true;
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::Mute));
        // Deafening silences too: one sound, the deafen one.
        ui.deafened = true;
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::Deafen));
        ui.deafened = false;
        ui.muted = false;
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::Undeafen));
        // The account's own stream.
        let me = store.me().unwrap();
        store.begin_share(ROOM);
        store.apply(
            Event::StreamPhase {
                channel: ROOM,
                owner: me,
                phase: StreamPhase::Live,
            },
            Instant::now(),
        );
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::StreamStart));
        store.stop_share();
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::StreamStop));
        // Each event has its switch.
        ui.prefs.sounds_off = vec![CallSound::UserJoin];
        present(&mut store, UserId(77), true);
        assert_eq!(watch.step(&mut ui, &store), None, "turned off");
        // Hanging up is leaving; the call coming back is joining.
        store.end_call();
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::Leave));
        assert_eq!(watch.step(&mut ui, &store), None);
        ui.prefs.sounds_off = vec![CallSound::Mute];
        ui.muted = true;
        assert_eq!(watch.step(&mut ui, &store), None, "turned off");
    }

    #[test]
    fn a_call_the_service_ends_or_that_fails_sounds_as_a_disconnection_once() {
        let (mut store, mut ui, mut watch) = ready();
        store.begin_call(ROOM, false, false);
        connected(&mut store);
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::Join));
        // Coming back after reconnecting is not joining again.
        store.apply(
            Event::CallPhase {
                channel: ROOM,
                phase: CallPhase::Reconnecting { attempt: 1 },
                quality: None,
            },
            Instant::now(),
        );
        assert_eq!(watch.step(&mut ui, &store), None);
        connected(&mut store);
        assert_eq!(watch.step(&mut ui, &store), None, "the same call");
        // Ended by the service, with why: rune-ui marks it dropped.
        ui.call_sounds.dropped = true;
        store.apply(
            Event::CallEnded {
                channel: ROOM,
                reason: Some("Se cortó".into()),
            },
            Instant::now(),
        );
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::Disconnect));
        assert!(!ui.call_sounds.dropped, "taken");
        // A call that fails, once.
        store.begin_call(ROOM, false, false);
        connected(&mut store);
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::Join));
        store.apply(
            Event::Connection(rune_provider::ConnectionState::Disconnected {
                reason: "red".into(),
            }),
            Instant::now(),
        );
        assert_eq!(watch.step(&mut ui, &store), Some(CallSound::Disconnect));
        assert_eq!(watch.step(&mut ui, &store), None, "once");
    }

    #[test]
    fn a_ring_plays_at_once_then_each_period_and_stops_with_its_call() {
        let (mut store, mut ui, mut watch) = ready();
        let start = Instant::now();
        assert_eq!(watch.ring(&ui, &store, start), None);
        let direct = store.directs()[0].id;
        let me = store.me().unwrap();
        store.apply(
            Event::PrivateCall(rune_model::PrivateCall {
                channel: direct,
                ringing: vec![me],
            }),
            start,
        );
        assert_eq!(watch.ring(&ui, &store, start), Some(CallSound::Ring));
        let soon = start + std::time::Duration::from_secs(1);
        assert_eq!(watch.ring(&ui, &store, soon), None, "still playing");
        let later = start + std::time::Duration::from_millis(2_600);
        assert_eq!(watch.ring(&ui, &store, later), Some(CallSound::Ring));
        // Turned off in the settings, it is not heard.
        ui.prefs.sounds_off = vec![CallSound::Ring];
        let much_later = start + std::time::Duration::from_secs(10);
        assert_eq!(watch.ring(&ui, &store, much_later), None);
        // Declined: it rings no more.
        ui.prefs.sounds_off.clear();
        store.decline_call(direct);
        assert_eq!(watch.ring(&ui, &store, much_later), None);
    }

    #[test]
    fn every_built_in_sound_is_a_clip_rune_can_play() {
        use rune_media::{tap, Clip, ClipStatus};
        for sound in CallSound::ALL {
            let (feed, mut heard) = tap(48_000);
            let clip = Clip::start(default_audio(sound).into(), feed, 0.0).unwrap();
            let deadline = Instant::now() + std::time::Duration::from_secs(5);
            while clip.duration().is_none() && clip.status() == ClipStatus::Open {
                assert!(Instant::now() < deadline, "{sound:?} opens");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            assert!(clip.duration().is_some(), "{sound:?} has a length");
            clip.set_paused(false);
            let mut loudest = 0.0_f32;
            while Instant::now() < deadline && clip.status() == ClipStatus::Open {
                let length = heard.available().min(6_000);
                if length == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                    continue;
                }
                let mut samples = vec![0.0; length];
                assert!(heard.read(&mut samples));
                loudest = loudest.max(rune_media::rms(&samples));
                if loudest > 0.001 {
                    break;
                }
            }
            // Some call sounds start with silence and some rings are longer
            // than RUNE's synthesized clips were. Decode until an audible
            // stretch instead of waiting for every asset to play out.
            assert!(loudest > 0.001, "{sound:?}: {loudest}");
        }
    }

    #[test]
    fn built_in_sounds_are_mp3_assets() {
        for sound in CallSound::ALL {
            assert!(
                default_audio(sound).starts_with(b"ID3"),
                "{sound:?} is an MP3 asset"
            );
        }
    }

    #[test]
    fn a_folder_of_ones_own_replaces_what_it_has_and_says_so() {
        let folder = std::env::temp_dir().join(format!("rune-library-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let mute = default_audio(CallSound::Unmute);
        std::fs::write(folder.join("mute.mp3"), mute).unwrap();
        std::fs::write(folder.join("leave.mp3"), b"no").unwrap();
        let mut ui = Ui::new();
        ui.prefs.sound_folder = Some(folder.to_string_lossy().into_owned());
        let mut library = Library::default();
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while ui.call_sounds.found.is_none() {
            assert!(Instant::now() < deadline, "read");
            library.step(&mut ui);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let found = ui.call_sounds.found.clone().unwrap();
        assert_eq!(found.taken, [(CallSound::Mute, "mute.mp3".to_owned())]);
        assert_eq!(found.refused.len(), 1);
        assert_eq!(*library.bytes(CallSound::Mute), *mute);
        assert_eq!(
            *library.bytes(CallSound::Join),
            *default_audio(CallSound::Join)
        );
        // Taking the folder away brings RUNE's back.
        ui.prefs.sound_folder = None;
        library.step(&mut ui);
        assert_eq!(
            *library.bytes(CallSound::Mute),
            *default_audio(CallSound::Mute)
        );
        let _ = std::fs::remove_dir_all(&folder);
    }
}
