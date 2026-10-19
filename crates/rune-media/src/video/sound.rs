//! The sound of the account's shared screen: what the other applications
//! play, read from PipeWire on a thread of its own (`rune-screen-sound`).
//! Never a call's voices, which the call would hear back: not RUNE's own
//! playback, not a stream whose role is a call's (`Communication`, `Phone`)
//! and not another voice client's (Discord's and the like, [`VOICE_CLIENTS`]).
//! Who plays each stream is asked of its PipeWire client (the node's own
//! properties do not say), and a stream waits unheard until it answers.
//! Each application's playback is read by a capture stream of its own (at
//! most [`MAX_SOURCES`]), summed every 20 ms into 48 kHz stereo ([`Mix`])
//! and encoded to Opus for music, handed to `send`. Silence sends nothing.
//!
//! No capture stream falls back to another node when its application's
//! goes or is not found: a microphone never ends up in the screen's sound.
//! What is mixed and what is left out, and why, is kept for the share's
//! diagnostic line ([`ScreenSound::heard`]): applications' names and
//! roles only, never what they play. Dropping the [`ScreenSound`] stops
//! the thread within a tick.
//!
//! On Windows (windows-medios) WASAPI's process loopback reads everything
//! the computer plays except RUNE's own processes ([`windows`]): the same
//! mix, encoder and handing on, without telling the applications apart.

#[cfg(target_os = "linux")]
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
#[cfg(target_os = "linux")]
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread::{self, Thread};
use std::time::Duration;

#[cfg(target_os = "linux")]
use pipewire as pw;
#[cfg(target_os = "linux")]
use pw::spa;
use rune_i18n::{es, t};

// Windows (windows-medios).
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows::work;

use super::{lock, Shared, VideoState};

/// Applications read at once; past this, newer ones are not heard.
pub const MAX_SOURCES: usize = 32;
/// One 20 ms frame of stereo at 48 kHz, in samples.
const FRAME: usize = 960 * 2;
/// An application starts being mixed with two frames buffered, and keeps at
/// most ten: older samples go.
const PRIMED: usize = 2 * FRAME;
const MAX_BUFFERED: usize = 10 * FRAME;
const TICK: Duration = Duration::from_millis(20);
/// Music, not speech: a game or a video sounds as it is.
const BITRATE: i32 = 128_000;
const UNAVAILABLE: &str = es("No se pudo leer el sonido de las aplicaciones.");

/// Programs whose playback is a call's, by a word of their name or binary
/// (lowercase): Discord's clients and its modified ones (the official
/// client's voice engine plays as `WEBRTC VoiceEngine`), and other calls.
/// Their voices are the call's: the screen's sound would bring them back.
pub const VOICE_CLIENTS: &[&str] = &[
    "discord",
    "webrtc voiceengine",
    "vesktop",
    "vencord",
    "webcord",
    "armcord",
    "legcord",
    "dorion",
    "serein",
    "zoom",
    "teams",
    "skype",
    "mumble",
    "teamspeak",
    "ts3client",
    "jitsi",
    "webex",
];

/// Why a playing stream is or is not in the screen's sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    Mixed,
    /// RUNE's own playback: the call would hear itself.
    Own,
    /// Its role is a call's.
    Communication,
    /// A voice client's ([`VOICE_CLIENTS`]).
    VoiceClient,
    /// Past [`MAX_SOURCES`], or PipeWire refused to read it.
    Unread,
}

impl Verdict {
    fn reason(self) -> &'static str {
        match self {
            Self::Mixed => "mixed",
            Self::Own => "own",
            Self::Communication => "communication",
            Self::VoiceClient => "voice client",
            Self::Unread => "not read",
        }
    }
}

/// One playing stream as the diagnostic line shows it: its application's
/// name and its role (or class), never what it plays.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heard {
    pub name: String,
    pub kind: String,
    pub verdict: Verdict,
}

/// Who plays a stream, as its PipeWire client says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(windows, allow(dead_code))]
pub(crate) struct Player {
    pid: Option<u32>,
    binary: String,
    name: String,
}

#[cfg_attr(windows, allow(dead_code))]
impl Player {
    /// From a client's properties: the process it says it is, or the one
    /// its socket belongs to.
    fn of(props: &dyn Fn(&str) -> Option<String>) -> Self {
        let pid = |key| props(key).and_then(|pid| pid.parse().ok());
        Self {
            pid: pid("application.process.id").or_else(|| pid("pipewire.sec.pid")),
            binary: props("application.process.binary").unwrap_or_default(),
            name: props("application.name").unwrap_or_default(),
        }
    }
}

/// Whether the stream with the properties `node`, played by `player`, goes
/// in the screen's sound of the process `own`.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn verdict(node: &dyn Fn(&str) -> Option<String>, player: &Player, own: u32) -> Verdict {
    if player.pid == Some(own) {
        return Verdict::Own;
    }
    let role = node("media.role").unwrap_or_default().to_lowercase();
    if role == "communication" || role == "phone" {
        return Verdict::Communication;
    }
    let names = [
        node("application.name"),
        node("node.name"),
        Some(player.binary.clone()),
        Some(player.name.clone()),
    ];
    let voice = names.into_iter().flatten().any(|name| {
        let name = name.to_lowercase();
        VOICE_CLIENTS.iter().any(|client| name.contains(client))
    });
    if voice {
        Verdict::VoiceClient
    } else {
        Verdict::Mixed
    }
}

/// `heard` for the diagnostic line: `mixing A (Music); left out B (own), C (voice client)`.
pub fn summary(heard: &[Heard]) -> String {
    let list = |mixed: bool| {
        heard
            .iter()
            .filter(|heard| (heard.verdict == Verdict::Mixed) == mixed)
            .map(|heard| match mixed {
                true => format!("{} ({})", heard.name, heard.kind),
                false => format!(
                    "{} ({}, {})",
                    heard.name,
                    heard.kind,
                    heard.verdict.reason()
                ),
            })
            .collect::<Vec<_>>()
    };
    let (mixed, out) = (list(true), list(false));
    let join = |names: Vec<String>| {
        if names.is_empty() {
            "nothing".to_owned()
        } else {
            names.join(", ")
        }
    };
    format!("mixing {}; left out {}", join(mixed), join(out))
}

/// A name for the log: printable, at most 40 characters.
#[cfg_attr(windows, allow(dead_code))]
fn shown(name: &str) -> String {
    let name: String = name.chars().filter(|c| !c.is_control()).take(40).collect();
    if name.is_empty() {
        "?".to_owned()
    } else {
        name
    }
}

/// The applications' samples, interleaved stereo, summed a frame at a time.
#[derive(Default)]
pub(crate) struct Mix {
    sources: HashMap<u32, (VecDeque<f32>, bool)>,
}

impl Mix {
    /// Samples of the application `source`; a full buffer drops its oldest.
    pub(crate) fn push(&mut self, source: u32, samples: &[f32]) {
        let Some((buffered, _)) = self.sources.get_mut(&source) else {
            return;
        };
        buffered.extend(samples.iter().map(
            |sample| {
                if sample.is_finite() {
                    *sample
                } else {
                    0.0
                }
            },
        ));
        let over = buffered.len().saturating_sub(MAX_BUFFERED);
        buffered.drain(..over);
    }

    /// Whether `source` could be added: at most [`MAX_SOURCES`].
    pub(crate) fn add(&mut self, source: u32) -> bool {
        if self.sources.len() >= MAX_SOURCES && !self.sources.contains_key(&source) {
            return false;
        }
        self.sources.entry(source).or_default();
        true
    }

    #[cfg_attr(windows, allow(dead_code))]
    pub(crate) fn remove(&mut self, source: u32) {
        self.sources.remove(&source);
    }

    /// The next 20 ms of all the applications buffered enough, summed and
    /// clipped; `None` when nobody played anything.
    pub(crate) fn frame(&mut self) -> Option<Vec<f32>> {
        let mut mixed = vec![0.0f32; FRAME];
        let mut heard = false;
        for (buffered, primed) in self.sources.values_mut() {
            *primed = (*primed || buffered.len() >= PRIMED) && buffered.len() >= FRAME;
            if !*primed {
                continue;
            }
            for (out, sample) in mixed.iter_mut().zip(buffered.drain(..FRAME)) {
                *out += sample;
                heard |= sample != 0.0;
            }
        }
        heard.then(|| {
            mixed
                .iter_mut()
                .for_each(|sample| *sample = sample.clamp(-1.0, 1.0));
            mixed
        })
    }
}

/// The screen's sound being read and encoded. Creating it starts the
/// thread; dropping it stops the thread and releases PipeWire.
pub struct ScreenSound {
    shared: Arc<Shared>,
    heard: Arc<Mutex<Vec<(u32, Heard)>>>,
    worker: Option<Thread>,
}

impl ScreenSound {
    /// `send` takes each Opus packet; `false` when it has no room (dropped).
    pub fn start(send: impl FnMut(Vec<u8>) -> bool + Send + 'static) -> Self {
        let shared = Arc::new(Shared::new());
        let heard = Arc::new(Mutex::new(Vec::new()));
        let (worker, told) = (shared.clone(), heard.clone());
        let spawned = thread::Builder::new()
            .name("rune-screen-sound".into())
            .spawn(move || {
                let ended = work(&worker, told, send).err();
                worker.end(ended);
            });
        let worker = match spawned {
            Ok(handle) => Some(handle.thread().clone()),
            Err(_) => {
                shared.end(Some(t(UNAVAILABLE).into()));
                None
            }
        };
        Self {
            shared,
            heard,
            worker,
        }
    }

    /// `Running` once something was heard; `Ended` with why if it failed.
    pub fn state(&self) -> VideoState {
        lock(&self.shared.state).clone()
    }

    /// The applications playing now: those mixed, and those left out and why.
    pub fn heard(&self) -> Vec<Heard> {
        lock(&self.heard)
            .iter()
            .map(|(_, heard)| heard.clone())
            .collect()
    }
}

impl Drop for ScreenSound {
    fn drop(&mut self) {
        self.shared.end(None);
        if let Some(worker) = &self.worker {
            worker.unpark();
        }
    }
}

/// The Opus encoder of the screen's sound: music, at [`BITRATE`].
fn encoder() -> Result<opus::Encoder, String> {
    let mut encoder = opus::Encoder::new(48_000, opus::Channels::Stereo, opus::Application::Audio)
        .map_err(|_| t(UNAVAILABLE).to_owned())?;
    encoder
        .set_bitrate(opus::Bitrate::Bits(BITRATE))
        .map_err(|_| t(UNAVAILABLE).to_owned())?;
    Ok(encoder)
}

/// Encode 20 ms of the mix and hand the packet to `send`: the sound runs
/// from its first packet. No room in `send` drops it, as the network would.
fn send_frame(
    shared: &Shared,
    encoder: &mut opus::Encoder,
    frame: &[f32],
    send: &mut impl FnMut(Vec<u8>) -> bool,
) -> Result<(), String> {
    let mut packet = [0u8; 1275];
    let length = encoder
        .encode_float(frame, &mut packet)
        .map_err(|_| t(UNAVAILABLE).to_owned())?;
    let mut state = lock(&shared.state);
    if *state == VideoState::Starting {
        *state = VideoState::Running;
    }
    drop(state);
    let _ = send(packet[..length].to_vec());
    Ok(())
}

/// One application's capture stream, kept while it plays.
#[cfg(target_os = "linux")]
struct Source {
    _stream: pw::stream::StreamRc,
    _listener: pw::stream::StreamListener<()>,
}

/// A playing stream whose client has not said who it is yet.
#[cfg(target_os = "linux")]
struct Waiting {
    client: u32,
    serial: String,
    props: HashMap<&'static str, String>,
}

/// What the registry's callbacks share on the thread.
#[cfg(target_os = "linux")]
struct Registry {
    core: pw::core::CoreRc,
    mix: Rc<RefCell<Mix>>,
    sources: RefCell<HashMap<u32, Source>>,
    /// The clients, bound to ask who they are (the listener dropped first),
    /// and what they said.
    clients: RefCell<HashMap<u32, (pw::client::ClientListener, pw::client::Client)>>,
    players: RefCell<HashMap<u32, Player>>,
    waiting: RefCell<HashMap<u32, Waiting>>,
    heard: Arc<Mutex<Vec<(u32, Heard)>>>,
    own: u32,
}

/// The node properties a verdict and the diagnostic line read.
#[cfg(target_os = "linux")]
const NODE_KEYS: [&str; 4] = ["media.role", "media.class", "application.name", "node.name"];

#[cfg(target_os = "linux")]
impl Registry {
    /// Decide on the stream `id` now that its client said who plays it.
    fn decide(&self, id: u32, stream: Waiting, player: &Player) {
        let props = |key: &str| stream.props.get(key).cloned();
        let mut verdict = verdict(&props, player, self.own);
        if verdict == Verdict::Mixed {
            let read = self.mix.borrow_mut().add(id)
                && match capture(&self.core, &stream.serial, id, &self.mix) {
                    Some(source) => {
                        self.sources.borrow_mut().insert(id, source);
                        true
                    }
                    None => {
                        self.mix.borrow_mut().remove(id);
                        false
                    }
                };
            if !read {
                verdict = Verdict::Unread;
            }
        }
        let name = props("application.name")
            .or_else(|| (!player.name.is_empty()).then(|| player.name.clone()))
            .unwrap_or_default();
        let kind = props("media.role")
            .or_else(|| props("media.class"))
            .unwrap_or_default();
        lock(&self.heard).push((
            id,
            Heard {
                name: shown(&name),
                kind: shown(&kind),
                verdict,
            },
        ));
    }

    fn forget(&self, id: u32) {
        self.mix.borrow_mut().remove(id);
        self.sources.borrow_mut().remove(&id);
        self.waiting.borrow_mut().remove(&id);
        self.clients.borrow_mut().remove(&id);
        self.players.borrow_mut().remove(&id);
        lock(&self.heard).retain(|(heard, _)| *heard != id);
    }
}

#[cfg(target_os = "linux")]
fn work(
    shared: &Arc<Shared>,
    heard: Arc<Mutex<Vec<(u32, Heard)>>>,
    send: impl FnMut(Vec<u8>) -> bool + 'static,
) -> Result<(), String> {
    let unavailable = |_| t(UNAVAILABLE).to_owned();
    let encoder = encoder()?;
    pw::init();
    let mainloop = pw::main_loop::MainLoopRc::new(None).map_err(unavailable)?;
    let context = pw::context::ContextRc::new(&mainloop, None).map_err(unavailable)?;
    let core = context.connect_rc(None).map_err(unavailable)?;
    let mix: Rc<RefCell<Mix>> = Rc::default();
    let shared_registry = Rc::new(Registry {
        core: core.clone(),
        mix: mix.clone(),
        sources: RefCell::default(),
        clients: RefCell::default(),
        players: RefCell::default(),
        waiting: RefCell::default(),
        heard,
        own: std::process::id(),
    });
    let registry = core.get_registry_rc().map_err(unavailable)?;
    let _registry = registry
        .add_listener_local()
        .global({
            let (state, registry) = (shared_registry.clone(), registry.clone());
            move |global| {
                let Some(props) = global.props else { return };
                match global.type_ {
                    pw::types::ObjectType::Client => {
                        let Ok(client) = registry.bind::<pw::client::Client, _>(global) else {
                            return;
                        };
                        let id = global.id;
                        let listener = client
                            .add_listener_local()
                            .info({
                                let state = state.clone();
                                move |info| {
                                    let Some(props) = info.props() else { return };
                                    let player =
                                        Player::of(&|key| props.get(key).map(str::to_owned));
                                    let ready: Vec<(u32, Waiting)> = {
                                        let mut waiting = state.waiting.borrow_mut();
                                        let ids: Vec<u32> = waiting
                                            .iter()
                                            .filter(|(_, stream)| stream.client == id)
                                            .map(|(node, _)| *node)
                                            .collect();
                                        ids.into_iter()
                                            .filter_map(|node| Some((node, waiting.remove(&node)?)))
                                            .collect()
                                    };
                                    for (node, stream) in ready {
                                        state.decide(node, stream, &player);
                                    }
                                    state.players.borrow_mut().insert(id, player);
                                }
                            })
                            .register();
                        state.clients.borrow_mut().insert(id, (listener, client));
                    }
                    pw::types::ObjectType::Node
                        if props.get("media.class") == Some("Stream/Output/Audio") =>
                    {
                        let client = props.get("client.id").and_then(|id| id.parse().ok());
                        let (Some(client), Some(serial)) = (client, props.get("object.serial"))
                        else {
                            return;
                        };
                        let stream = Waiting {
                            client,
                            serial: serial.to_owned(),
                            props: NODE_KEYS
                                .iter()
                                .filter_map(|key| Some((*key, props.get(key)?.to_owned())))
                                .collect(),
                        };
                        let player = state.players.borrow().get(&client).cloned();
                        match player {
                            Some(player) => state.decide(global.id, stream, &player),
                            // Unheard until its client says who it is.
                            None => {
                                state.waiting.borrow_mut().insert(global.id, stream);
                            }
                        }
                    }
                    _ => {}
                }
            }
        })
        .global_remove({
            let state = shared_registry.clone();
            move |id| state.forget(id)
        })
        .register();
    let failure: Rc<RefCell<Option<String>>> = Rc::default();
    let (encoder, send) = (RefCell::new(encoder), RefCell::new(send));
    let timer = mainloop.loop_().add_timer({
        let (mainloop, shared, failure) = (mainloop.clone(), shared.clone(), failure.clone());
        move |_| {
            if shared.stopped() {
                return mainloop.quit();
            }
            let Some(frame) = mix.borrow_mut().frame() else {
                return;
            };
            let sent = send_frame(
                &shared,
                &mut encoder.borrow_mut(),
                &frame,
                &mut *send.borrow_mut(),
            );
            if let Err(reason) = sent {
                failure.borrow_mut().get_or_insert(reason);
                mainloop.quit();
            }
        }
    });
    let _ = timer.update_timer(Some(TICK), Some(TICK));
    mainloop.run();
    shared_registry.sources.borrow_mut().clear();
    shared_registry.clients.borrow_mut().clear();
    let failure = failure.borrow_mut().take();
    match failure {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

/// A capture stream of the application stream node `id` (`serial`), its
/// samples into `mix`; `None` when PipeWire refuses it.
#[cfg(target_os = "linux")]
fn capture(
    core: &pw::core::CoreRc,
    serial: &str,
    id: u32,
    mix: &Rc<RefCell<Mix>>,
) -> Option<Source> {
    let properties = pw::properties::properties! {
        *pw::keys::MEDIA_TYPE => "Audio",
        *pw::keys::MEDIA_CATEGORY => "Capture",
        *pw::keys::MEDIA_ROLE => "Screen",
        *pw::keys::TARGET_OBJECT => serial,
        // Never another node: not the default source, not a microphone.
        "node.dont-fallback" => "true",
        "node.dont-reconnect" => "true",
    };
    let stream = pw::stream::StreamRc::new(core.clone(), "rune-screen-sound", properties).ok()?;
    let listener = stream
        .add_local_listener_with_user_data(())
        .process({
            let mix = mix.clone();
            move |stream, _| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let Some(data) = buffer.datas_mut().first_mut() else {
                    return;
                };
                let chunk = data.chunk();
                let (offset, size) = (chunk.offset() as usize, chunk.size() as usize);
                let Some(bytes) = data
                    .data()
                    .and_then(|bytes| bytes.get(offset..offset + size))
                else {
                    return;
                };
                let samples: Vec<f32> = bytes
                    .chunks_exact(4)
                    .map(|sample| f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]))
                    .collect();
                mix.borrow_mut().push(id, &samples);
            }
        })
        .register()
        .ok()?;
    let mut format = spa::param::audio::AudioInfoRaw::new();
    format.set_format(spa::param::audio::AudioFormat::F32LE);
    format.set_rate(48_000);
    format.set_channels(2);
    let object = spa::pod::Object {
        type_: spa::utils::SpaTypes::ObjectParamFormat.as_raw(),
        id: spa::param::ParamType::EnumFormat.as_raw(),
        properties: format.into(),
    };
    let bytes = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(object),
    )
    .ok()?
    .0
    .into_inner();
    let mut params = [spa::pod::Pod::from_bytes(&bytes)?];
    stream
        .connect(
            spa::utils::Direction::Input,
            None,
            pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut params,
        )
        .ok()?;
    Some(Source {
        _stream: stream,
        _listener: listener,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props<'a>(pairs: &'a [(&str, &str)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn a_calls_voices_never_go_in_the_screens_sound() {
        let own = 4242;
        let rune = Player::of(&props(&[
            ("application.process.id", "4242"),
            ("application.name", "rune"),
        ]));
        // RUNE's own playback (the call's voices, its sounds), by its client's pid.
        let speaker = props(&[("media.class", "Stream/Output/Audio")]);
        assert_eq!(verdict(&speaker, &rune, own), Verdict::Own);
        // A browser's video is heard; the same browser in a call is not.
        let firefox = Player::of(&props(&[
            ("application.process.id", "77"),
            ("application.process.binary", "firefox"),
            ("application.name", "Firefox"),
        ]));
        let video = props(&[("media.role", "Movie"), ("application.name", "Firefox")]);
        assert_eq!(verdict(&video, &firefox, own), Verdict::Mixed);
        for role in ["Communication", "phone"] {
            let pairs = [("media.role", role), ("application.name", "Firefox")];
            let call = props(&pairs);
            assert_eq!(
                verdict(&call, &firefox, own),
                Verdict::Communication,
                "{role}"
            );
        }
        // The official client and others like it, by name or by binary, even
        // when the node says nothing of a call (its pulse socket's pid is not its own).
        let official = Player::of(&props(&[
            ("pipewire.sec.pid", "900"),
            ("application.process.binary", "Discord"),
            ("application.name", "Chromium"),
        ]));
        assert_eq!(official.pid, Some(900));
        assert_eq!(verdict(&speaker, &official, own), Verdict::VoiceClient);
        let engine = props(&[("application.name", "WEBRTC VoiceEngine")]);
        assert_eq!(
            verdict(&engine, &Player::default(), own),
            Verdict::VoiceClient
        );
        let vesktop = Player::of(&props(&[("application.process.binary", "vesktop")]));
        assert_eq!(verdict(&speaker, &vesktop, own), Verdict::VoiceClient);
        let canary = props(&[("node.name", "DiscordCanary")]);
        assert_eq!(verdict(&canary, &firefox, own), Verdict::VoiceClient);
        // A game is heard.
        let game = Player::of(&props(&[("application.process.binary", "celeste")]));
        assert_eq!(verdict(&speaker, &game, own), Verdict::Mixed);
    }

    #[test]
    fn the_line_names_applications_and_why_they_are_left_out_never_what_they_play() {
        let heard = [
            Heard {
                name: "Firefox".into(),
                kind: "Movie".into(),
                verdict: Verdict::Mixed,
            },
            Heard {
                name: "rune".into(),
                kind: "Stream/Output/Audio".into(),
                verdict: Verdict::Own,
            },
            Heard {
                name: "WEBRTC VoiceEngine".into(),
                kind: "Communication".into(),
                verdict: Verdict::Communication,
            },
        ];
        assert_eq!(
            summary(&heard),
            "mixing Firefox (Movie); left out rune (Stream/Output/Audio, own), \
             WEBRTC VoiceEngine (Communication, communication)"
        );
        assert_eq!(summary(&[]), "mixing nothing; left out nothing");
        assert_eq!(shown("a\u{7}b"), "ab");
        assert_eq!(shown(""), "?");
        assert_eq!(shown(&"x".repeat(90)).len(), 40);
    }

    #[test]
    fn applications_are_summed_once_buffered_and_silence_sends_nothing() {
        let mut mix = Mix::default();
        assert!(mix.add(1) && mix.add(2));
        assert_eq!(mix.frame(), None, "nothing played");
        // One frame is not enough to start; two are, and both are heard.
        mix.push(1, &[0.25; FRAME]);
        assert_eq!(mix.frame(), None);
        mix.push(1, &[0.25; FRAME]);
        mix.push(2, &[0.5; PRIMED]);
        for _ in 0..2 {
            let frame = mix.frame().unwrap();
            assert_eq!(frame.len(), FRAME);
            assert!(frame.iter().all(|sample| (*sample - 0.75).abs() < 1e-6));
        }
        // Ran dry: each waits to be buffered again.
        assert_eq!(mix.frame(), None);
        mix.push(1, &[0.1; FRAME]);
        assert_eq!(mix.frame(), None);
        // Too loud is clipped; what is not a number is silence.
        mix.push(1, &[1.0; FRAME]);
        mix.push(2, &[0.5; PRIMED]);
        let frame = mix.frame().unwrap();
        assert!(frame.iter().all(|sample| (*sample - 0.6).abs() < 1e-6));
        let frame = mix.frame().unwrap();
        assert!(frame.iter().all(|sample| *sample == 1.0));
        mix.push(1, &[f32::NAN; PRIMED]);
        assert_eq!(mix.frame(), None, "NaN is silence");
        // Buffered at most so much; past it the oldest goes.
        mix.push(2, &vec![0.2; MAX_BUFFERED * 2]);
        assert_eq!(mix.sources[&2].0.len(), MAX_BUFFERED);
        // At most so many applications; one gone takes its samples along.
        for source in 3..100 {
            mix.add(source);
        }
        assert_eq!(mix.sources.len(), MAX_SOURCES);
        assert!(!mix.add(500));
        mix.remove(2);
        assert!(mix.add(500));
        mix.push(77, &[0.3; PRIMED]);
        assert!(!mix.sources.contains_key(&77), "never added");
    }
}
