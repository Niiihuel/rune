//! Local capture shared by the microphone test and the call's speaking marker,
//! and the output for the settings' chime, hearing oneself and a call. With a
//! provider that carries audio, a connected call's media runs on its own
//! thread ([`CallMedia`]) between the provider's link and these devices.
//! Device ownership, codec and DSP live in rune-media.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rune_i18n::{t, tr};
use rune_media::call::{CallControls, CallMedia, Incoming};
use rune_media::mixer::STREAM_SOUND;
use rune_media::{
    Capture, CaptureRequest, CaptureSnapshot, CaptureState, Clip, ClipStatus, Feed, Playback,
    PlaybackRequest, PlaybackSnapshot, PlaybackSource, PushToTalk, VoiceActivity,
};
use rune_model::{ChannelId, UserId};
use rune_provider::voice::VoiceLink;
use rune_state::Store;
use rune_ui::{Played, Ui, VoiceState};

use rune_ui::CallSound;

use crate::sounds::{Library, Watch};

/// The microphone queue a call reads: 120 ms.
const TRANSMIT_QUEUE: usize = 5_760;
/// A voice message's queue into a call: half a second, past its lead.
const CLIP_QUEUE: usize = 24_000;

/// The simulation's voice message: four seconds of a tone RUNE synthesized,
/// encoded as Ogg Opus. No recording of anyone.
pub const SIMULATED_VOICE: &[u8] = include_bytes!("../../../assets/voice-sample.ogg");
/// The simulation's audio file: three seconds of a synthesized tone, MP3.
pub const SIMULATED_AUDIO: &[u8] = include_bytes!("../../../assets/audio-sample.mp3");
/// The simulation's video: four seconds of a generated test pattern and a
/// tone, H264 and AAC in MP4. No recording of anyone or anything.
pub const SIMULATED_VIDEO: &[u8] = include_bytes!("../../../assets/video-sample.mp4");
/// The same video as WebM: VP9 and Opus.
pub const SIMULATED_WEBM: &[u8] = include_bytes!("../../../assets/video-sample.webm");

/// A clip's playback and the call it is mixed into, or `None` on the output
/// alone: a voice message owns the output's feed, a sound plays over it.
type Playing = (Clip, Option<ChannelId>);

pub struct Audio {
    capture: Option<Capture>,
    playback: Option<Playback>,
    /// The capture holds this playback's echo reference.
    linked: bool,
    /// The capture's monitor feeds this playback.
    monitored: bool,
    /// The provider's audio path, when it carries audio at all.
    link: Option<Arc<Mutex<VoiceLink>>>,
    /// The media of the connected call, and the microphone feed it waits
    /// for until a capture exists.
    call: Option<(ChannelId, CallMedia, Option<Feed>)>,
    /// People marked speaking from the call's audio.
    heard: Vec<UserId>,
    sampled: Instant,
    voice: VoiceActivity,
    push_to_talk: PushToTalk,
    /// The push-to-talk key is held, in the window or through the desktop.
    pub held: bool,
    /// The push-to-mute key is held (P6): nothing goes out meanwhile.
    pub muted_held: bool,
    speaking_user: Option<UserId>,
    /// A failing device is retried by an explicit test, device change or new call.
    failed: Option<(Option<ChannelId>, Option<String>)>,
    active: bool,
    /// Asks the interface to repaint: the call's speaking marks wake it.
    wake: Arc<dyn Fn() + Send + Sync>,
    /// The voice message, audio or video file: its request, its file, kept
    /// to play again without fetching it, and its playback.
    message: Option<(u64, Arc<[u8]>, Option<Playing>)>,
    /// What the call's sounds compare each frame with.
    watch: Watch,
    /// A call sound waiting for the output (and whether «Probar» asked for
    /// it), and those playing, each into the call it was mixed into or,
    /// with `None`, over the output's feed.
    sound: Option<(CallSound, bool)>,
    sounds: Vec<Playing>,
    /// RUNE's sounds, and the person's own folder's.
    library: Library,
    /// Whether the microphone was open at the last frame it was captured,
    /// for the push-to-talk sounds: none until one frame has been seen.
    ptt_open: Option<bool>,
    /// The microphone (speaker) is the default standing in for a chosen one
    /// that is missing: said once when that changes.
    input_fell_back: bool,
    output_fell_back: bool,
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            capture: None,
            playback: None,
            linked: false,
            monitored: false,
            link: None,
            call: None,
            heard: Vec::new(),
            sampled: Instant::now(),
            voice: VoiceActivity::default(),
            push_to_talk: PushToTalk::default(),
            held: false,
            muted_held: false,
            speaking_user: None,
            failed: None,
            active: false,
            wake: Arc::new(|| {}),
            message: None,
            watch: Watch::default(),
            sound: None,
            sounds: Vec::new(),
            library: Library::default(),
            ptt_open: None,
            input_fell_back: false,
            output_fell_back: false,
        }
    }
}

impl Audio {
    /// With the provider's audio path: a connected call carries sound. `wake`
    /// asks the interface for a repaint when the call's speaking marks change.
    pub fn with_link(link: Option<VoiceLink>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self {
            link: link.map(|link| Arc::new(Mutex::new(link))),
            wake,
            ..Self::default()
        }
    }

    pub fn retry(&mut self) {
        // A healthy call already owns the same capture. Starting its meter
        // must not interrupt it or reset the adaptive noise filter.
        self.failed = None;
    }

    /// When the interface looks at the audio again. A remote speaker wakes it
    /// by itself (see `wake`); what needs a look is the meter, a playing sound
    /// or voice message (50 ms), and the local speaking mark, which follows
    /// the microphone while it can be heard in a call (100 ms). Otherwise
    /// nothing polls.
    pub fn next_poll(&self, ui: &Ui, store: &Store) -> Option<Duration> {
        if !self.active {
            return None;
        }
        let voice = ui
            .voice_player
            .as_ref()
            .is_some_and(|player| player.state == VoiceState::Playing);
        if ui.audio.testing || voice || self.sounding() {
            Some(Duration::from_millis(50))
        } else if audible_call(ui, store).is_some() {
            Some(Duration::from_millis(100))
        } else {
            None
        }
    }

    /// Play `sound` once, as the call's sounds play.
    pub fn play(&mut self, sound: CallSound) {
        self.sound = Some((sound, false));
    }

    /// The file of the clip `request`, fetched; it plays as the
    /// interface's player says.
    pub fn load_voice(&mut self, request: u64, file: Arc<[u8]>) {
        self.stop_voice();
        self.message = Some((request, file, None));
    }

    /// The newest picture of the video playing, for its surface.
    #[cfg(target_os = "linux")]
    pub fn take_picture(&self) -> Option<rune_media::video::Frame> {
        let (_, _, Some((clip, _))) = self.message.as_ref()? else {
            return None;
        };
        clip.take_picture()
    }

    fn stop_voice(&mut self) {
        if let Some((_, _, Some(_))) = self.message.take() {
            // Its feed went with it: the monitor may take the output again.
            self.monitored = false;
        }
    }

    /// Play, pause, seek or end the voice message the interface shows.
    fn step_voice(&mut self, ui: &mut Ui) {
        let speed = rune_ui::voice_speed(ui);
        let deafened = ui.deafened;
        let Some(player) = ui.voice_player.as_mut() else {
            self.stop_voice();
            return;
        };
        if self
            .message
            .as_ref()
            .is_some_and(|(request, ..)| *request != player.request)
        {
            self.stop_voice();
        }
        // Into the connected call's sound, never its microphone, or alone.
        let sink = self.call.as_ref().map(|(channel, ..)| *channel);
        let running = self
            .playback
            .as_ref()
            .is_some_and(|playback| playback.snapshot().state == CaptureState::Running);
        let Some((_, ogg, clip)) = self.message.as_mut() else {
            return;
        };
        if clip.as_ref().is_some_and(|(_, into)| *into != sink) {
            // A call started or ended under it: it goes on where it was.
            *clip = None;
            self.monitored = false;
        }
        if player.state == VoiceState::Playing && clip.is_none() {
            let from = player.seek.take().unwrap_or(player.position);
            let started = match (&self.call, &self.playback) {
                (Some((_, media, _)), _) => {
                    let (feed, tap) = rune_media::tap(CLIP_QUEUE);
                    media.play_alongside(tap);
                    Some(Clip::start(ogg.clone(), feed, from))
                }
                (None, Some(playback)) => Some(Clip::start(ogg.clone(), playback.feed(), from)),
                (None, None) => None,
            };
            match started {
                Some(Ok(started)) => {
                    *clip = Some((started, sink));
                    // Held by the clip: the monitor waits until it ends.
                    self.monitored |= sink.is_none();
                }
                Some(Err(error)) => player.state = VoiceState::Failed(error.to_string()),
                None => {}
            }
        }
        let Some((started, _)) = clip.as_ref() else {
            if let Some(seek) = player.seek.take() {
                player.position = seek;
            }
            return;
        };
        if let Some(seek) = player.seek.take() {
            started.seek(seek);
        }
        // Only voice messages change speed; files keep their own.
        started.set_speed(if player.kind == Played::Voice {
            speed
        } else {
            1.0
        });
        started.set_volume(clip_volume(
            player.kind,
            player.volume,
            deafened && sink.is_none(),
        ));
        if let Some(duration) = started.duration() {
            player.duration = Some(duration);
        }
        // Held until the output runs, so its first moments are not lost.
        started.set_paused(!(player.state == VoiceState::Playing && running));
        match started.status() {
            ClipStatus::Open => player.position = started.position(),
            ClipStatus::Ended => {
                player.state = VoiceState::Paused;
                player.position = 0.0;
                *clip = None;
                self.monitored = false;
            }
            ClipStatus::Failed(error) => {
                player.state = VoiceState::Failed(error);
                *clip = None;
                self.monitored = false;
            }
        }
    }

    pub fn step(&mut self, ui: &mut Ui, store: &mut Store, now: Instant) {
        self.library.step(ui);
        if let Some(sound) = self.watch.step(ui, store) {
            self.sound = Some((sound, false));
        }
        // Private calls: the ring, again and again while it rings.
        if let Some(sound) = self.watch.ring(ui, store, now) {
            self.sound = Some((sound, false));
        }
        if let Some(sound) = rune_ui::call_sounds_take_preview(ui) {
            self.sound = Some((sound, true));
        }
        self.step_output(ui, store, now);
        self.step_voice(ui);
        let request = self.request(ui, store);
        self.active |= request.is_some();
        if request.is_some() && self.capture.is_none() {
            match Capture::new() {
                Ok(capture) => {
                    self.capture = Some(capture);
                    self.linked = false;
                    self.monitored = false;
                    // The call's microphone went with the old capture: restart it.
                    if self
                        .call
                        .as_ref()
                        .is_some_and(|(_, _, pending)| pending.is_none())
                    {
                        self.call = None;
                    }
                }
                Err(error) => {
                    self.failed(ui, store, error.to_string(), now);
                    return;
                }
            }
        }
        self.step_call(ui, store, now);
        // After the call's media is steered: a call that just ended sounds alone.
        self.step_sounds(ui.prefs.sound_volume, ui.deafened);
        if let (false, Some(capture), Some(playback)) = (self.linked, &self.capture, &self.playback)
        {
            capture.set_echo_reference(playback.echo_reference());
            self.linked = true;
        }
        // A call owns the output's one feed while it lasts.
        if let (false, None, Some(capture), Some(playback)) =
            (self.monitored, &self.call, &self.capture, &self.playback)
        {
            capture.set_monitor(playback.feed());
            self.monitored = true;
        }
        let snapshot = if let Some(capture) = &mut self.capture {
            capture.configure(request);
            capture.set_input_volume(ui.prefs.input_volume);
            Some(capture.snapshot())
        } else {
            None
        };
        if let Some(snapshot) = snapshot {
            self.apply(ui, store, &snapshot, now);
        } else {
            ui.audio.starting = false;
            ui.audio.system_echo = None;
            self.clear_speaking(ui, store);
        }
    }

    /// Whether a call sound plays, or waits to.
    fn sounding(&self) -> bool {
        self.sound.is_some() || !self.sounds.is_empty()
    }

    /// Start the waiting call sound, into the connected call's sound or
    /// alone on the output, at `volume`, and let go of those that ended.
    /// Deafened only deafening and undeafening sound, and what «Probar»
    /// asked for.
    fn step_sounds(&mut self, volume: f32, deafened: bool) {
        let running = self
            .playback
            .as_ref()
            .is_some_and(|playback| playback.snapshot().state == CaptureState::Running);
        let wanted = self
            .sound
            .take()
            .filter(|(sound, tried)| !deafened || *tried || sound.heard_deafened());
        if let Some((sound, _)) = wanted {
            let ogg: Arc<[u8]> = self.library.bytes(sound);
            let started = match (&self.call, &self.playback) {
                (Some((channel, media, _)), _) => {
                    let (feed, tap) = rune_media::tap(CLIP_QUEUE);
                    media.play_sound(tap);
                    Clip::start(ogg, feed, 0.0)
                        .ok()
                        .map(|clip| (clip, Some(*channel)))
                }
                // Mixed over what the output's feed plays: a voice message,
                // or oneself while testing the microphone.
                (None, Some(playback)) => Clip::start(ogg, playback.sound_feed(), 0.0)
                    .ok()
                    .map(|clip| (clip, None)),
                (None, None) => None,
            };
            if let Some((clip, _)) = &started {
                clip.set_volume(volume);
            }
            self.sounds.extend(started);
        }
        let sink = self.call.as_ref().map(|(channel, ..)| *channel);
        self.sounds.retain(|(clip, into)| {
            // Held until the output runs, so its start is not lost.
            clip.set_paused(!running);
            clip.status() == ClipStatus::Open && (into.is_none() || *into == sink)
        });
    }

    /// Start, steer or stop the connected call's media.
    fn step_call(&mut self, ui: &mut Ui, store: &mut Store, now: Instant) {
        let connected = self
            .link
            .as_ref()
            .and(store.call())
            .filter(|call| call.phase.is_connected())
            .map(|call| call.channel);
        if self.call.as_ref().map(|(channel, ..)| *channel) != connected {
            self.end_call(store);
        }
        if let (Some(channel), None, Some(link), Some(playback)) =
            (connected, &self.call, &self.link, &self.playback)
        {
            let (microphone, tap) = rune_media::tap(TRANSMIT_QUEUE);
            let (outgoing, incoming) = (link.clone(), link.clone());
            let wake = self.wake.clone();
            let started = CallMedia::start(
                tap,
                playback.feed(),
                move |packet| {
                    if let Ok(link) = outgoing.try_lock() {
                        // Full: the network side is behind; drop it as it would.
                        let _ = link.outgoing.try_send(packet);
                    }
                },
                move || {
                    let mut link = incoming.try_lock().ok()?;
                    let packet = link.incoming.try_recv().ok()?;
                    Some(Incoming {
                        key: packet.user.0 | if packet.stream { STREAM_SOUND } else { 0 },
                        sequence: packet.sequence,
                        packet: packet.opus,
                    })
                },
                move || wake(),
            );
            match started {
                Ok(media) => {
                    self.call = Some((channel, media, Some(microphone)));
                    self.monitored = false;
                }
                Err(error) => {
                    ui.notify(
                        t("No se pudo usar el audio de la llamada"),
                        Some(error.to_string()),
                        true,
                        now,
                    );
                    return;
                }
            }
        }
        let Some((_, media, pending)) = self.call.as_mut() else {
            return;
        };
        if let Some(capture) = &self.capture {
            if let Some(feed) = pending.take() {
                capture.set_transmit(feed);
            }
        }
        // The same thresholds as the speaking mark, which follow the room
        // with automatic sensitivity.
        ui.prefs.tune(&mut self.voice);
        media.set_controls(CallControls {
            open: audible_call(ui, store).is_some() && ui.audio.transmitting && !ui.deafened,
            voice_activity: !(ui.prefs.push_to_talk || store.push_to_talk_required()),
            activity_on: self.voice.on,
            activity_off: self.voice.off,
            deafened: ui.deafened,
            // Only this device's choices: nothing of it goes to the provider.
            choices: ui
                .prefs
                .voice_volumes
                .iter()
                .map(|(user, (volume, muted))| (*user, *volume, *muted))
                .collect(),
        });
        let status = media.status();
        if let Some(reason) = status.failed {
            self.end_call(store);
            ui.notify(
                t("Se cortó el audio de la llamada"),
                Some(reason),
                true,
                now,
            );
            return;
        }
        // A stream's sound marks nobody speaking.
        for (key, speaking) in status
            .speaking
            .into_iter()
            .filter(|(key, _)| key & STREAM_SOUND == 0)
        {
            let user = UserId(key);
            store.set_speaking(user, speaking && !ui.deafened);
            if !self.heard.contains(&user) {
                self.heard.push(user);
            }
        }
    }

    /// The call's media stops; the devices stay for whatever else uses them.
    fn end_call(&mut self, store: &mut Store) {
        if self.call.take().is_some() {
            // Its feed went with it: the monitor takes the output again.
            self.monitored = false;
        }
        for user in self.heard.drain(..) {
            store.set_speaking(user, false);
        }
    }

    /// Open, change or stop the output, and say when it cannot be used.
    fn step_output(&mut self, ui: &mut Ui, store: &Store, now: Instant) {
        let in_call =
            self.link.is_some() && store.call().is_some_and(|call| call.phase.is_connected());
        let request = output_request(ui, in_call, self.sounding());
        self.active = request.is_some();
        if request.is_some() && self.playback.is_none() {
            match Playback::new() {
                Ok(playback) => {
                    self.playback = Some(playback);
                    self.linked = false;
                    self.monitored = false;
                    // A call's feed belonged to the output that went.
                    self.call = None;
                }
                Err(error) => {
                    self.output_failed(ui, error.to_string(), now);
                    return;
                }
            }
        }
        let Some(playback) = &mut self.playback else {
            ui.audio.output_starting = false;
            return;
        };
        playback.configure(request);
        let snapshot = playback.snapshot();
        self.apply_output(ui, &snapshot, now);
    }

    fn apply_output(&mut self, ui: &mut Ui, snapshot: &PlaybackSnapshot, now: Instant) {
        ui.audio.output_starting =
            ui.audio.loopback && matches!(snapshot.state, CaptureState::Starting);
        if snapshot.state == CaptureState::Running && snapshot.fallback != self.output_fell_back {
            self.output_fell_back = snapshot.fallback;
            announce_switch(ui, false, snapshot.fallback, now);
        }
        if let CaptureState::Failed(error) = &snapshot.state {
            self.output_failed(ui, error.clone(), now);
        }
    }

    fn output_failed(&mut self, ui: &mut Ui, error: String, now: Instant) {
        ui.audio.loopback = false;
        ui.audio.output_starting = false;
        self.active = false;
        // A new output starts clean, also after its worker ended.
        self.playback = None;
        self.call = None;
        ui.notify(
            t("No se pudo usar la salida de audio"),
            Some(error),
            true,
            now,
        );
    }

    fn request(&mut self, ui: &Ui, store: &Store) -> Option<CaptureRequest> {
        let call = audible_call(ui, store);
        if call.is_some() {
            self.speaking_user = store.me();
        }
        let identity = (call, ui.audio.input.clone());
        if self.failed.as_ref() != Some(&identity) {
            self.failed = None;
        }
        let processing = ui.prefs.effective_processing();
        (ui.audio.testing || (call.is_some() && self.failed.is_none())).then(|| CaptureRequest {
            device: ui.audio.input.clone(),
            mode: processing.suppression,
            muted: ui.muted,
            echo_cancellation: processing.echo_cancellation,
            automatic_gain: processing.automatic_gain,
            // Deafened, not oneself either: a sound playing over the feed
            // has the output undeafened under it.
            monitor: ui.audio.loopback && !ui.deafened,
            // A channel that asks for push-to-talk goes by the key, not the gate.
            speech_gate: ui.prefs.speech_gate() && !store.push_to_talk_required(),
            // What the PC plays is a reference for the canceller only.
            system_echo: ui.prefs.system_echo,
            output: ui.audio.output.clone(),
        })
    }

    fn apply(&mut self, ui: &mut Ui, store: &mut Store, snapshot: &CaptureSnapshot, now: Instant) {
        if snapshot.state == CaptureState::Idle {
            self.input_fell_back = false;
        } else if snapshot.state == CaptureState::Running
            && snapshot.fallback != self.input_fell_back
        {
            self.input_fell_back = snapshot.fallback;
            announce_switch(ui, true, snapshot.fallback, now);
        }
        ui.audio.starting = ui.audio.testing && matches!(snapshot.state, CaptureState::Starting);
        // Said once the canceller has heard something: before that, it
        // has not chosen its reference yet.
        let wanted = ui.prefs.system_echo && ui.prefs.effective_processing().echo_cancellation;
        ui.audio.system_echo =
            (wanted && snapshot.state == CaptureState::Running && snapshot.processed_frames > 0)
                .then_some(snapshot.system_echo);
        match &snapshot.state {
            CaptureState::Failed(error) => {
                self.ptt_open = None;
                self.failed(ui, store, error.clone(), now);
            }
            CaptureState::Running => {
                // Voice activity always lets the microphone through.
                let transmitting = (!(ui.prefs.push_to_talk || store.push_to_talk_required()) || {
                    self.push_to_talk.release_delay = ui.prefs.release_delay();
                    self.push_to_talk.update(self.held, now)
                }) && !self.muted_held;
                ui.audio.transmitting = transmitting;
                if let Some(sound) = ptt_sound(
                    self.ptt_open.replace(transmitting),
                    transmitting,
                    ui.prefs.push_to_talk,
                    audible_call(ui, store).is_some(),
                ) {
                    self.sound = Some((sound, false));
                }
                if ui.audio.testing && now.duration_since(self.sampled) >= Duration::from_millis(80)
                {
                    self.sampled = now;
                    push_level(&mut ui.audio.raw_levels, snapshot.raw_level);
                    push_level(&mut ui.audio.levels, snapshot.processed_level);
                }
                // Heard also while only testing: automatic sensitivity learns
                // the room, and the settings show where speech starts.
                ui.prefs.tune(&mut self.voice);
                let heard = self.voice.update(snapshot.processed_level, now);
                ui.audio.threshold = Some(self.voice.on);
                if audible_call(ui, store).is_some() {
                    let speaking = heard && transmitting;
                    if let Some(me) = store.me() {
                        store.set_speaking(me, speaking);
                    }
                } else {
                    self.voice.reset();
                    self.clear_marker(store);
                }
            }
            CaptureState::Starting | CaptureState::Idle => self.clear_speaking(ui, store),
        }
    }

    fn failed(&mut self, ui: &mut Ui, store: &mut Store, error: String, now: Instant) {
        self.failed = Some((audible_call(ui, store), ui.audio.input.clone()));
        self.active = false;
        ui.audio.testing = false;
        ui.audio.starting = false;
        ui.audio.levels.clear();
        ui.audio.raw_levels.clear();
        self.clear_speaking(ui, store);
        // Dropping this handle only signals the worker. A retry also recovers
        // from a terminated worker, instead of sending it more configurations.
        self.capture = None;
        ui.notify(t("No se pudo usar el micrófono"), Some(error), true, now);
    }

    /// Capture stopped: nothing is heard or let through any more.
    fn clear_speaking(&mut self, ui: &mut Ui, store: &mut Store) {
        self.voice.reset();
        self.push_to_talk.reset();
        ui.audio.transmitting = false;
        self.ptt_open = None;
        self.clear_marker(store);
    }

    fn clear_marker(&mut self, store: &mut Store) {
        if let Some(me) = self.speaking_user.take() {
            store.set_speaking(me, false);
        }
    }
}

/// Says which device is in use now, by name, after a fallback or a return.
/// Nothing is said while the device list is not loaded.
fn announce_switch(ui: &mut Ui, input: bool, fallback: bool, now: Instant) {
    let Some(Ok(devices)) = &ui.audio.devices else {
        return;
    };
    let (list, chosen) = if input {
        (&devices.inputs, &ui.audio.input)
    } else {
        (&devices.outputs, &ui.audio.output)
    };
    // While the default stands in, it is the one in use; else the chosen one.
    let device = if fallback {
        list.iter().find(|device| device.default)
    } else {
        list.iter()
            .find(|device| Some(&device.id) == chosen.as_ref())
    };
    let Some(name) = device.map(|device| device.name.clone()) else {
        return;
    };
    let title = if input {
        tr!("Entrada cambiada a {name}", name)
    } else {
        tr!("Salida cambiada a {name}", name)
    };
    ui.notify(title, None, false, now);
}

/// The push-to-talk sound for the microphone opening or closing (`was` is
/// what the last frame saw): only in push-to-talk, in a call one is heard
/// in, and never on the first look.
fn ptt_sound(was: Option<bool>, open: bool, push_to_talk: bool, heard: bool) -> Option<CallSound> {
    if !push_to_talk || !heard || was.is_none_or(|was| was == open) {
        return None;
    }
    Some(if open {
        CallSound::PttStart
    } else {
        CallSound::PttStop
    })
}

fn audible_call(ui: &Ui, store: &Store) -> Option<ChannelId> {
    store
        .call()
        .filter(|call| call.phase.is_connected() && !call.muted && !ui.muted)
        // Held by the server, or no permission to speak: nothing goes out.
        .filter(|_| store.mic_held().is_none())
        .map(|call| call.channel)
}

/// What the output plays: the microphone while it is tested, a connected
/// call that carries audio, a voice message, or a call sound (also the
/// settings' sound test), which is heard even deafened: the deafen sound is
/// one.
fn output_request(ui: &Ui, in_call: bool, sound: bool) -> Option<PlaybackRequest> {
    let voice = ui
        .voice_player
        .as_ref()
        .is_some_and(|player| player.state == VoiceState::Playing);
    if !(in_call || (ui.audio.loopback && ui.audio.testing) || voice || sound) {
        return None;
    }
    Some(PlaybackRequest {
        device: ui.audio.output.clone(),
        source: PlaybackSource::Feed,
        volume: ui.prefs.output_volume,
        deafened: ui.deafened && !sound,
    })
}

/// A clip's gain: a voice message at full, a file at its `volume`. Deafened
/// outside a call, none, also while a sound has the output undeafened under it.
fn clip_volume(kind: Played, volume: f32, deafened_alone: bool) -> f32 {
    if deafened_alone {
        0.0
    } else if kind == Played::Voice {
        1.0
    } else {
        volume
    }
}

fn push_level(levels: &mut Vec<f32>, level: f32) {
    levels.push(level);
    let excess = levels.len().saturating_sub(24);
    levels.drain(..excess);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_media::ProcessingMode;
    use rune_model::{CallPhase, VoiceState};
    use rune_provider::{
        mock::{MockConfig, MockProvider},
        Event,
    };

    fn call() -> (Ui, Store, Instant) {
        let now = Instant::now();
        let mut store = Store::new();
        let provider = MockProvider::new(MockConfig::default());
        store.apply(Event::Ready(Box::new(provider.snapshot())), now);
        let channel = ChannelId(900);
        store.begin_call(channel, false, false);
        store.apply(
            Event::VoiceChanged(VoiceState {
                user: store.me().unwrap(),
                channel,
                muted: false,
                deafened: false,
                speaking: true,
                streaming: false,
                server_muted: false,
                server_deafened: false,
                camera: false,
                suppressed: false,
                speak_request: None,
            }),
            now,
        );
        store.apply(
            Event::CallPhase {
                channel,
                phase: CallPhase::Connected,
                quality: None,
            },
            now,
        );
        (Ui::new(), store, now)
    }

    #[test]
    fn a_server_mute_keeps_the_call_microphone_closed() {
        let (ui, mut store, now) = call();
        assert!(audible_call(&ui, &store).is_some());
        let held = VoiceState {
            server_muted: true,
            ..store.my_voice().unwrap().clone()
        };
        store.apply(Event::VoiceChanged(held), now);
        assert_eq!(audible_call(&ui, &store), None);
    }

    #[test]
    fn failure_clears_speaking_and_does_not_retry_on_every_frame() {
        let (mut ui, mut store, now) = call();
        ui.audio.testing = true;
        let mut audio = Audio::default();
        assert!(audio.request(&ui, &store).is_some());
        assert!(audio.voice.update(1.0, now));
        audio.apply(
            &mut ui,
            &mut store,
            &CaptureSnapshot {
                state: CaptureState::Failed("Dispositivo desconectado".into()),
                ..Default::default()
            },
            now,
        );
        assert!(!store.my_voice().unwrap().speaking);
        assert!(!audio.voice.speaking());
        assert!(!ui.audio.testing);
        for _ in 0..3 {
            assert!(audio.request(&ui, &store).is_none());
        }
        ui.audio.input = Some("another-device".into());
        assert_eq!(audio.request(&ui, &store).unwrap().device, ui.audio.input);
    }

    #[test]
    fn test_and_call_share_one_request_and_stop_independently() {
        let (mut ui, mut store, _) = call();
        let mut audio = Audio::default();
        let calling = audio.request(&ui, &store).unwrap();
        ui.audio.testing = true;
        assert_eq!(audio.request(&ui, &store), Some(calling.clone()));
        store.end_call();
        assert_eq!(audio.request(&ui, &store), Some(calling));
        ui.audio.testing = false;
        assert!(audio.request(&ui, &store).is_none());
    }

    #[test]
    fn mute_and_selection_change_the_shared_capture_in_the_same_frame() {
        let (mut ui, store, _) = call();
        let mut audio = Audio::default();
        ui.audio.input = Some("first".into());
        assert_eq!(
            audio.request(&ui, &store).unwrap().device.as_deref(),
            Some("first")
        );
        ui.audio.input = Some("second".into());
        ui.prefs.input_profile = rune_ui::InputProfile::Custom;
        ui.prefs.processing.suppression = ProcessingMode::WebRtc;
        let request = audio.request(&ui, &store).unwrap();
        assert_eq!(request.device.as_deref(), Some("second"));
        assert_eq!(request.mode, ProcessingMode::WebRtc);
        // A profile sets all of it; one's own values wait underneath.
        ui.prefs.input_profile = rune_ui::InputProfile::VoiceIsolation;
        let request = audio.request(&ui, &store).unwrap();
        assert_eq!(request.mode, ProcessingMode::RnNoise);
        assert!(request.echo_cancellation && request.automatic_gain);
        ui.prefs.input_profile = rune_ui::InputProfile::Studio;
        let request = audio.request(&ui, &store).unwrap();
        assert_eq!(request.mode, ProcessingMode::Off);
        assert!(!request.echo_cancellation && !request.automatic_gain);
        ui.muted = true;
        assert!(
            audio.request(&ui, &store).is_none(),
            "muted call releases capture"
        );
        ui.audio.testing = true;
        assert!(
            audio.request(&ui, &store).unwrap().muted,
            "test cannot bypass mute"
        );
    }

    #[test]
    fn push_to_talk_gates_the_speaking_mark_and_a_released_key_stops_after_its_delay() {
        let (mut ui, mut store, now) = call();
        ui.prefs.push_to_talk = true;
        ui.prefs.push_to_talk_release = 100;
        let mut audio = Audio::default();
        audio.request(&ui, &store);
        let loud = CaptureSnapshot {
            state: CaptureState::Running,
            processed_level: 0.8,
            ..Default::default()
        };
        let at = |millis: u64| now + Duration::from_millis(millis);
        let speaking = |store: &Store| store.my_voice().unwrap().speaking;
        audio.apply(&mut ui, &mut store, &loud, at(0));
        assert!(
            !speaking(&store),
            "a voice without the key is not let through"
        );
        assert!(!ui.audio.transmitting);
        audio.held = true;
        audio.apply(&mut ui, &mut store, &loud, at(10));
        assert!(speaking(&store) && ui.audio.transmitting);
        // The host reads a lost focus or an ended desktop session as released.
        audio.held = false;
        audio.apply(&mut ui, &mut store, &loud, at(60));
        assert!(speaking(&store), "the release delay keeps the word's end");
        audio.apply(&mut ui, &mut store, &loud, at(160));
        assert!(!speaking(&store) && !ui.audio.transmitting);
        audio.held = true;
        audio.apply(&mut ui, &mut store, &loud, at(200));
        assert!(ui.audio.transmitting);
        audio.apply(&mut ui, &mut store, &CaptureSnapshot::default(), at(210));
        assert!(
            !ui.audio.transmitting,
            "a stopped capture lets nothing through, without the delay"
        );
    }

    #[test]
    fn a_call_that_carries_audio_plays_its_feed_and_deafen_silences_it() {
        let (mut ui, store, _) = call();
        let request = output_request(&ui, true, false).unwrap();
        assert_eq!(request.source, PlaybackSource::Feed);
        ui.deafened = true;
        assert!(output_request(&ui, true, false).unwrap().deafened);
        // A simulated provider hands no link: its call never opens the output.
        let audio = Audio::with_link(None, Arc::new(|| {}));
        assert!(audio.link.is_none() && store.call().is_some());
        assert!(audio.call.is_none());
    }

    #[test]
    fn the_sound_test_follows_the_chosen_output_and_its_volume() {
        let (mut ui, _, _) = call();
        assert_eq!(
            output_request(&ui, false, false),
            None,
            "nothing plays by itself"
        );
        ui.audio.output = Some("speaker".into());
        ui.prefs.output_volume = 0.4;
        let request = output_request(&ui, false, true).unwrap();
        assert_eq!(request.device.as_deref(), Some("speaker"));
        assert_eq!(request.source, PlaybackSource::Feed);
        assert_eq!(request.volume, 0.4);
    }

    #[test]
    fn push_to_talk_sounds_once_per_opening_and_closing_in_a_heard_call() {
        // The first look only remembers; then each change plays once.
        assert_eq!(ptt_sound(None, true, true, true), None);
        assert_eq!(
            ptt_sound(Some(false), true, true, true),
            Some(CallSound::PttStart)
        );
        assert_eq!(ptt_sound(Some(true), true, true, true), None, "still open");
        assert_eq!(
            ptt_sound(Some(true), false, true, true),
            Some(CallSound::PttStop)
        );
        // Not in voice activity, self-muted (not heard), or outside a call.
        assert_eq!(ptt_sound(Some(false), true, false, true), None);
        assert_eq!(ptt_sound(Some(false), true, true, false), None);
    }

    #[test]
    fn a_call_sound_opens_the_output_and_is_heard_deafened() {
        let (mut ui, _, _) = call();
        ui.deafened = true;
        assert_eq!(output_request(&ui, false, false), None);
        let request = output_request(&ui, false, true).unwrap();
        assert_eq!(request.source, PlaybackSource::Feed);
        assert!(!request.deafened, "the deafen sound is heard");
        assert!(output_request(&ui, true, false).unwrap().deafened);
    }

    #[test]
    fn hearing_oneself_needs_the_microphone_test_and_feeds_its_monitor() {
        let (mut ui, store, _) = call();
        let mut audio = Audio::default();
        // One's own processing, starting from nothing on.
        ui.prefs.input_profile = rune_ui::InputProfile::Custom;
        ui.audio.loopback = true;
        assert_eq!(output_request(&ui, false, false), None);
        ui.audio.testing = true;
        assert_eq!(
            output_request(&ui, false, false).unwrap().source,
            PlaybackSource::Feed
        );
        let capture = audio.request(&ui, &store).unwrap();
        assert!(capture.monitor);
        assert!(!capture.echo_cancellation, "off until enabled");
        ui.prefs.processing.echo_cancellation = true;
        assert!(audio.request(&ui, &store).unwrap().echo_cancellation);
        // The PC's sound is cancelled too by default, on the output chosen.
        ui.audio.output = Some("headset".into());
        let capture = audio.request(&ui, &store).unwrap();
        assert!(capture.system_echo);
        assert_eq!(capture.output.as_deref(), Some("headset"));
        ui.prefs.system_echo = false;
        assert!(!audio.request(&ui, &store).unwrap().system_echo);
        // Deafened, oneself is not heard either, also while the deafen
        // sound has the output undeafened.
        ui.deafened = true;
        assert!(!output_request(&ui, false, true).unwrap().deafened);
        assert!(!audio.request(&ui, &store).unwrap().monitor);
    }

    #[test]
    fn deafened_outside_a_call_a_clip_is_silent_under_a_sound() {
        assert_eq!(clip_volume(Played::Voice, 0.3, false), 1.0);
        assert_eq!(clip_volume(Played::Audio, 0.3, false), 0.3);
        assert_eq!(clip_volume(Played::Voice, 0.3, true), 0.0);
        assert_eq!(clip_volume(Played::Video, 0.3, true), 0.0);
    }

    #[test]
    fn an_unusable_output_stops_both_tests_and_says_so() {
        let (mut ui, _, now) = call();
        let mut audio = Audio::default();
        ui.audio.testing = true;
        ui.audio.loopback = true;
        ui.audio.output_starting = true;
        audio.apply_output(
            &mut ui,
            &PlaybackSnapshot {
                state: CaptureState::Failed("Salida desconectada".into()),
                ..Default::default()
            },
            now,
        );
        assert!(!ui.audio.loopback && !ui.audio.output_starting);
        assert!(ui.audio.testing, "the microphone test goes on");
        assert_eq!(
            output_request(&ui, false, false),
            None,
            "no reopening on every frame"
        );
        assert_eq!(ui.notices.len(), 1);
    }

    #[test]
    fn idle_capture_does_not_overwrite_provider_speaking() {
        let (mut ui, mut store, now) = call();
        store.end_call();
        let mut audio = Audio::default();
        audio.step(&mut ui, &mut store, now);
        assert!(store.my_voice().unwrap().speaking);
        assert_eq!(audio.next_poll(&ui, &store), None);
    }

    #[test]
    fn audio_is_polled_only_for_what_the_interface_shows() {
        // No device opens under test, so the capture is taken as running.
        let (mut ui, store, _) = call();
        let audio = Audio {
            active: true,
            ..Audio::default()
        };
        assert_eq!(
            audio.next_poll(&ui, &store),
            Some(Duration::from_millis(100)),
            "the local speaking mark in an unmuted call"
        );
        ui.muted = true;
        assert_eq!(audio.next_poll(&ui, &store), None, "muted: nothing to mark");
        ui.audio.testing = true;
        assert_eq!(
            audio.next_poll(&ui, &store),
            Some(Duration::from_millis(50)),
            "a meter shown"
        );
    }

    #[test]
    fn processed_audio_drives_speaking_and_both_histories_stay_bounded() {
        let (mut ui, mut store, now) = call();
        ui.audio.testing = true;
        let mut audio = Audio::default();
        // By default RNNoise's gate decides, in the capture: the call and
        // the mark take whatever it let through.
        let gated = audio.request(&ui, &store).unwrap();
        assert!(gated.speech_gate && gated.mode == ProcessingMode::RnNoise);
        // Without RNNoise, automatic sensitivity follows the room.
        ui.prefs.input_profile = rune_ui::InputProfile::Studio;
        assert!(!audio.request(&ui, &store).unwrap().speech_gate);
        let quiet = CaptureSnapshot {
            state: CaptureState::Running,
            raw_level: 0.9,
            processed_level: 0.1,
            ..Default::default()
        };
        for frame in 1..=30 {
            audio.apply(
                &mut ui,
                &mut store,
                &quiet,
                now + Duration::from_millis(frame * 100),
            );
        }
        assert!(
            !store.my_voice().unwrap().speaking,
            "raw noise is not the speaking input"
        );
        assert_eq!(ui.audio.levels, vec![0.1; 24]);
        assert_eq!(ui.audio.raw_levels, vec![0.9; 24]);
        // Automatic sensitivity sets speech 15 dB over the room it heard,
        // and the settings are told.
        let threshold = ui.audio.threshold.unwrap();
        assert!((threshold - 0.1 - VoiceActivity::MARGIN).abs() < 1e-6);
        let loud = CaptureSnapshot {
            processed_level: 0.8,
            ..quiet
        };
        audio.request(&ui, &store);
        audio.apply(&mut ui, &mut store, &loud, now + Duration::from_secs(4));
        assert!(store.my_voice().unwrap().speaking);
        ui.muted = true;
        audio.request(&ui, &store);
        audio.apply(&mut ui, &mut store, &loud, now + Duration::from_secs(5));
        assert!(
            !store.my_voice().unwrap().speaking,
            "mute wins over a previous frame"
        );
        assert!(!audio.voice.speaking());
    }
}
