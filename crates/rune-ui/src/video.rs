//! The account's screen and camera: which it asked to show, and how far the
//! host got with them. The host owns the sources and reads this each frame;
//! the interface only says what is wanted, and lets go when the call ends.
//! The screen goes to the call when the provider carries it: a quality is
//! picked first ([`share`]), the system's dialog chooses the source, and
//! only then is the provider asked for the stream ([`screen_ready`]); the
//! store follows how far it got, and the host says what it sends. The camera goes to the call when the
//! provider carries it and the account may ([`camera_sending`]); otherwise
//! it stays on this machine.
//!
//! Also the others' streams the account watches, up to [`MAX_STREAMS`] at
//! once: the store says whose and how far each got; the host decodes them
//! and says here what each shows. And the others' cameras on the stage:
//! [`cameras`] says whose the host should decode, it says here what each
//! shows.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rune_components::voice::{ids, Preview, ScreenChoice, Thumbnail, VideoControls, NO_VIDEO};
use rune_i18n::{t, tr};
use rune_model::{CallPhase, ChannelId, ServerId, StreamPhase, StreamQuality, UserId};
use rune_provider::{
    video::{MAX_CAMERAS, MAX_STREAMS},
    Command,
};
use rune_state::Store;

use crate::{Intent, Ui};

/// The account's screen and camera.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Video {
    /// The screen, shared in the call.
    pub screen: Share,
    /// The camera, on in the call.
    pub camera: Share,
    /// «Probar video» of the settings: the camera, outside any call.
    pub testing: bool,
    /// The camera chosen; `None` is the system's.
    pub device: Option<String>,
    /// The cameras the system showed the last time it gave access, by id
    /// and name.
    pub cameras: Vec<(String, String)>,
    /// What the host shows of each stream watched.
    pub streams: Vec<(UserId, Live)>,
    /// What the host shows of the cameras [`cameras`] asks for.
    pub watched: Vec<(UserId, Live)>,
    /// How the screen is sent; kept between shares.
    pub quality: StreamQuality,
    /// When the picture of each stream in the column was last asked for.
    pub previews: HashMap<UserId, Instant>,
    /// The stream watched shown alone, the window full screen.
    pub focus: Option<UserId>,
    /// The streams watched shown in a window of their own as well.
    pub windows: Vec<UserId>,
    /// The quality picker is open.
    pub picker: bool,
    /// The shared screen goes without the applications' sound.
    pub silent: bool,
    // RUNE's own screen picker (windows-medios).
    /// The screens and windows the host listed for the picker, by id, name
    /// and whether each is a window: only where the system has no picker of
    /// its own (Windows); empty elsewhere.
    pub sources: Vec<(String, String, bool)>,
    /// The one of them chosen to share; the main screen until one is.
    pub source: Option<String>,
}

/// One source: whether it is wanted, and what the host has of it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Share {
    pub on: bool,
    pub live: Live,
    /// What actually goes to the call, while it does: frames a second and
    /// kilobits a second.
    pub sent: Option<(u32, u32)>,
    /// Who encodes it, by name (`VA-API`, `NVENC`, `OpenH264`), while it goes.
    pub encoder: Option<&'static str>,
}

/// What the host has of a source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Live {
    #[default]
    Off,
    /// The system's dialog is open, or the first frame is on its way.
    Waiting,
    /// A frame of this size, in pixels, is on its surface.
    Showing(u32, u32),
}

impl Live {
    pub fn frame(self) -> Option<(u32, u32)> {
        match self {
            Self::Showing(width, height) => Some((width, height)),
            _ => None,
        }
    }
}

impl Video {
    /// Whether the host should hold the screen, the camera, given the call.
    pub fn wanted(&self, store: &Store) -> (bool, bool) {
        let call = in_call(store);
        (
            call && self.screen.on,
            self.testing || (call && self.camera.on),
        )
    }
}

/// Whose cameras the host should decode: those on the tiles of the stage of
/// the account's call, while it is connected there and no stream takes the
/// stage, at most [`MAX_CAMERAS`] in the stage's order.
pub fn cameras(ui: &Ui, store: &Store) -> Vec<UserId> {
    let (Some(call), Some(me)) = (store.call(), store.me()) else {
        return Vec::new();
    };
    let channel = call.channel;
    let shown = call.phase.is_connected()
        && ui.voice.stage == Some(channel)
        && store
            .watching()
            .iter()
            .all(|watching| watching.channel != channel)
        && store
            .abilities(channel)
            .is_some_and(|abilities| abilities.watch_camera.allowed());
    if !shown {
        return Vec::new();
    }
    store
        .voice_in(channel)
        .into_iter()
        .filter(|state| state.camera && state.user != me)
        .map(|state| state.user)
        .take(MAX_CAMERAS)
        .collect()
}

/// A stream's picture is asked for again this often while it shows.
const PREVIEW_REFRESH: Duration = Duration::from_secs(60);
/// Pictures asked for at once.
const PREVIEW_ASKS: usize = 4;

/// The streams the column shows: of others, in the voice channels of the
/// server open, not watched, when the provider has their pictures.
fn previewed(ui: &Ui, store: &Store) -> Vec<(ServerId, rune_model::VoiceState)> {
    let offered = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.stream_previews);
    let (Some(server), true) = (ui.server, offered) else {
        return Vec::new();
    };
    let me = store.me();
    let watched = store.watching();
    store
        .channel_tree(server)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .filter(|channel| channel.kind == rune_model::ChannelKind::Voice)
        .flat_map(|channel| store.voice_in(channel.id))
        .filter(|state| state.streaming && Some(state.user) != me)
        .filter(|state| watched.iter().all(|watching| watching.owner != state.user))
        .map(|state| (server, state.clone()))
        .collect()
}

/// The pictures of the streams the column shows, asked for at most every
/// [`PREVIEW_REFRESH`] and [`PREVIEW_ASKS`] at once; the host runs these
/// each frame.
pub fn stream_previews(ui: &mut Ui, store: &Store, now: Instant) -> Vec<Intent> {
    let shown = previewed(ui, store);
    let asked = &mut ui.video.previews;
    asked.retain(|user, _| shown.iter().any(|(_, state)| state.user == *user));
    let due: Vec<_> = shown
        .into_iter()
        .filter(|(_, state)| {
            asked
                .get(&state.user)
                .is_none_or(|at| now.duration_since(*at) >= PREVIEW_REFRESH)
        })
        .take(PREVIEW_ASKS)
        .collect();
    due.into_iter()
        .map(|(server, state)| {
            asked.insert(state.user, now);
            Intent::Command(Command::LoadStreamPreview {
                server,
                channel: state.channel,
                owner: state.user,
            })
        })
        .collect()
}

/// The images of the pictures the column shows, for the host to load.
pub(crate) fn preview_images(ui: &Ui, store: &Store) -> Vec<rune_model::RemoteImage> {
    previewed(ui, store)
        .into_iter()
        .filter_map(|(_, state)| {
            store
                .stream_preview(state.channel, state.user)
                .flatten()
                .cloned()
        })
        .collect()
}

/// The picture of `state`'s stream under its row in the column, once it
/// arrived; made up, and said so, in the simulation.
pub(crate) fn thumbnail(
    ui: &Ui,
    store: &Store,
    state: &rune_model::VoiceState,
) -> Option<Thumbnail> {
    if !previewed(ui, store)
        .iter()
        .any(|(_, shown)| shown.user == state.user)
    {
        return None;
    }
    match store.stream_preview(state.channel, state.user)? {
        Some(image) => ui.images.get(image).cloned().map(Thumbnail::Image),
        None => crate::voice::simulated(store).then_some(Thumbnail::Simulated),
    }
}

/// Why the screen and camera cannot be had on this system, if they cannot.
pub(crate) fn unsupported() -> Option<&'static str> {
    (!cfg!(any(target_os = "linux", windows))).then(|| t(NO_VIDEO))
}

/// The account is in a call it can share in.
pub(crate) fn in_call(store: &Store) -> bool {
    store.me().is_some()
        && store
            .call()
            .is_some_and(|call| !matches!(call.phase, CallPhase::Failed { .. }))
}

/// The screen goes to the call: the provider carries it and the account may.
fn can_share(store: &Store) -> bool {
    store
        .call()
        .and_then(|call| store.abilities(call.channel))
        .is_some_and(|abilities| abilities.share_screen.allowed())
}

/// The camera goes to the call: the provider carries it and the account may.
fn can_send_camera(store: &Store) -> bool {
    store
        .call()
        .and_then(|call| store.abilities(call.channel))
        .is_some_and(|abilities| abilities.send_video.allowed())
}

/// The camera is on and goes to the account's call: the host encodes it.
pub fn camera_sending(ui: &Ui, store: &Store) -> bool {
    ui.video.camera.on && in_call(store) && can_send_camera(store)
}

/// The provider is told the camera is on or off in the call, when it
/// carries cameras at all.
fn tell_camera(store: &Store, on: bool) -> Vec<Intent> {
    let carried = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.video);
    match store.call() {
        Some(call) if carried && (!on || can_send_camera(store)) => {
            vec![Intent::Command(Command::SetCamera {
                channel: call.channel,
                on,
            })]
        }
        _ => Vec::new(),
    }
}

/// What the camera's preview says: what goes to the call, or that it
/// stays here.
pub(crate) fn camera_note(ui: &Ui, store: &Store) -> String {
    if !camera_sending(ui, store) {
        return note(store);
    }
    let connected = store.call().is_some_and(|call| call.phase.is_connected());
    let phase = match (connected, ui.video.camera.sent) {
        (false, _) => t("Conectando…").to_owned(),
        (true, Some((fps, kilobits))) => {
            tr!(
                "En la llamada · {fps} fps · {kilobits} kbit/s",
                fps,
                kilobits
            )
        }
        (true, None) => t("En la llamada").to_owned(),
    };
    let phase = with_encoder(phase, ui.video.camera.encoder);
    if crate::voice::simulated(store) {
        tr!("{phase} · simulada", phase)
    } else {
        phase
    }
}

/// How the shared screen is sent, while it is: its quality.
pub fn sending(ui: &Ui, store: &Store) -> Option<StreamQuality> {
    store
        .sharing()
        .filter(|sharing| ui.video.screen.on && !matches!(sharing.phase, StreamPhase::Ended(_)))
        .map(|_| ui.video.quality)
}

/// The applications' sound goes along with the shared screen: the host
/// reads it while the stream is live, not paused.
pub fn sound(ui: &Ui, store: &Store) -> bool {
    !ui.video.silent
        && sending(ui, store).is_some()
        && store
            .sharing()
            .is_some_and(|sharing| sharing.phase != StreamPhase::Paused)
}

/// The system remembered the screen chosen: kept with the preferences, so
/// the next share does not ask again.
pub fn screen_chosen(ui: &mut Ui, token: String) -> Vec<Intent> {
    let fresh = crate::settings::restore_token(&token)
        && ui.prefs.screen_restore.as_deref() != Some(token.as_str());
    if !fresh {
        return Vec::new();
    }
    ui.prefs.screen_restore = Some(token.clone());
    vec![Intent::SaveSetting {
        key: "screen_restore".into(),
        value: Some(token),
    }]
}

/// «Elegir otra pantalla» in the picker, which stays open: the system's
/// picker asks again next time.
pub(crate) fn pick_screen_again(ui: &mut Ui) -> Vec<Intent> {
    ui.video.picker = true;
    ui.prefs
        .screen_restore
        .take()
        .map(|_| Intent::SaveSetting {
            key: "screen_restore".into(),
            value: None,
        })
        .into_iter()
        .collect()
}

/// The host listed what RUNE's own picker offers: the choice stays if it
/// is still there, else the first (the main screen) is chosen.
pub fn sources_listed(ui: &mut Ui, listed: Vec<(String, String, bool)>) {
    let kept = ui
        .video
        .source
        .as_ref()
        .is_some_and(|chosen| listed.iter().any(|(id, ..)| id == chosen));
    if !kept {
        ui.video.source = listed.first().map(|(id, ..)| id.clone());
    }
    ui.video.sources = listed;
}

/// A screen or a window chosen in RUNE's own picker, which stays open.
pub(crate) fn choose_source(ui: &mut Ui, id: &str) -> Vec<Intent> {
    if ui.video.sources.iter().any(|(listed, ..)| listed == id) {
        ui.video.source = Some(id.to_owned());
    }
    ui.video.picker = true;
    Vec::new()
}

/// «Con sonido» in the picker, which stays open.
pub(crate) fn toggle_sound(ui: &mut Ui) -> Vec<Intent> {
    ui.video.silent = !ui.video.silent;
    ui.video.picker = true;
    Vec::new()
}

/// The applications' sound could not be read: the screen goes on without
/// it, and says why.
pub fn sound_failed(ui: &mut Ui, reason: String, now: Instant) {
    ui.video.silent = true;
    ui.notify(t("Se comparte sin sonido"), Some(reason), true, now);
}

/// What the screen's preview says: how far its stream got and what goes
/// out, or that it stays here.
fn screen_note(ui: &Ui, store: &Store) -> String {
    let Some(sharing) = store.sharing().filter(|_| ui.video.screen.on) else {
        return note(store);
    };
    let phase = match (&sharing.phase, ui.video.screen.sent) {
        (StreamPhase::Connecting, _) => t("Conectando…").to_owned(),
        (StreamPhase::Live, Some((fps, kilobits))) => {
            tr!("En vivo · {fps} fps · {kilobits} kbit/s", fps, kilobits)
        }
        (StreamPhase::Live, None) => t("En vivo").to_owned(),
        (StreamPhase::Paused, _) => t("En pausa").to_owned(),
        (StreamPhase::Reconnecting, _) => t("Reconectando…").to_owned(),
        (StreamPhase::Ended(_), _) => t("Terminó").to_owned(),
    };
    let phase = match sharing.phase {
        StreamPhase::Live => with_encoder(phase, ui.video.screen.encoder),
        _ => phase,
    };
    if crate::voice::simulated(store) {
        tr!("{phase} · simulada", phase)
    } else {
        phase
    }
}

/// `phase` and who encodes, a name in any language.
fn with_encoder(phase: String, encoder: Option<&str>) -> String {
    match encoder {
        Some(encoder) => format!("{phase} · {encoder}"),
        None => phase,
    }
}

/// What the hints and the previews add: that nothing leaves this machine.
fn note(store: &Store) -> String {
    let reason = store
        .call()
        .and_then(|call| store.abilities(call.channel))
        .and_then(|abilities| abilities.send_video.reason().map(str::to_owned))
        .unwrap_or_else(|| t(rune_state::NO_VIDEO_SENDING).to_owned());
    tr!("Solo vos lo ves. {reason}.", reason)
}

/// The call's camera and screen buttons.
pub(crate) fn controls(ui: &Ui, store: &Store) -> VideoControls {
    VideoControls {
        screen: ui.video.screen.on,
        camera: ui.video.camera.on,
        denied: unsupported().map(str::to_owned),
        note: note(store),
        screen_note: can_share(store)
            .then(|| t("Elegí la calidad; se transmite a la llamada.").to_owned()),
        camera_note: can_send_camera(store).then(|| t("Se transmite a la llamada.").to_owned()),
        picker: ui.video.picker.then_some(ui.video.quality),
        sound: !ui.video.silent,
        pick_again: ui.prefs.screen_restore.is_some(),
        sources: ui
            .video
            .sources
            .iter()
            .map(|(id, name, window)| ScreenChoice {
                id: id.clone(),
                name: name.clone(),
                window: *window,
                chosen: ui.video.source.as_deref() == Some(id.as_str()),
            })
            .collect(),
    }
}

/// The previews on the stage, the screen first.
pub(crate) fn previews(ui: &Ui, store: &Store) -> Vec<Preview> {
    let mut previews = Vec::new();
    if ui.video.screen.on {
        previews.push(Preview {
            surface: ids::SCREEN_SURFACE.into(),
            title: t("Tu pantalla").into(),
            waiting: t("Esperando al sistema…").into(),
            frame: ui.video.screen.live.frame(),
            stop: (ids::STOP_SCREEN.into(), t("Dejar de compartir")),
            note: screen_note(ui, store),
            actions: pause_action(store).into_iter().collect(),
        });
    }
    if ui.video.camera.on {
        previews.push(Preview {
            surface: ids::CAMERA_SURFACE.into(),
            title: t("Tu cámara").into(),
            waiting: t("Esperando al sistema…").into(),
            frame: ui.video.camera.live.frame(),
            stop: (ids::STOP_CAMERA.into(), t("Apagar la cámara")),
            note: camera_note(ui, store),
            actions: Vec::new(),
        });
    }
    previews
}

/// «Pausar» while the shared screen is live, «Reanudar» while paused.
fn pause_action(store: &Store) -> Option<(String, &'static str)> {
    match store.sharing()?.phase {
        StreamPhase::Live => Some((ids::PAUSE.into(), t("Pausar"))),
        StreamPhase::Paused => Some((ids::RESUME.into(), t("Reanudar"))),
        _ => None,
    }
}

/// «Pausar», «Reanudar»: the shared screen stops going out, or goes on.
/// The provider answers with the stream's phase.
pub(crate) fn pause(store: &Store, paused: bool) -> Vec<Intent> {
    store
        .sharing()
        .filter(|sharing| !matches!(sharing.phase, StreamPhase::Ended(_)))
        .map(|sharing| {
            Intent::Command(Command::PauseStream {
                channel: sharing.channel,
                paused,
            })
        })
        .into_iter()
        .collect()
}

/// The streams watched in `channel`, as the stage shows them.
pub(crate) fn streams(ui: &Ui, store: &Store, channel: ChannelId) -> Vec<Preview> {
    store
        .watching()
        .iter()
        .filter(|watching| watching.channel == channel)
        .map(|watching| stream(ui, store, watching))
        .collect()
}

/// A stream watched, as the stage shows it.
fn stream(ui: &Ui, store: &Store, watching: &rune_state::Watching) -> Preview {
    let server = store
        .channel(watching.channel)
        .and_then(|found| found.server);
    let (note, waiting) = match &watching.phase {
        StreamPhase::Connecting => (t("Conectando…"), t("Conectando con la transmisión…").into()),
        StreamPhase::Live => (t("En vivo"), t("Esperando video…").into()),
        StreamPhase::Paused => (t("En pausa"), t("La transmisión está en pausa").into()),
        StreamPhase::Reconnecting => (
            t("Reconectando…"),
            t("Reconectando con la transmisión…").into(),
        ),
        StreamPhase::Ended(reason) => (
            t("Terminó"),
            reason
                .clone()
                .unwrap_or_else(|| t("La transmisión terminó").into()),
        ),
    };
    let note = if crate::voice::simulated(store) {
        tr!("{note} · simulada", note)
    } else {
        note.to_owned()
    };
    let ended = matches!(watching.phase, StreamPhase::Ended(_));
    let owner = watching.owner;
    let mut actions = Vec::new();
    if !ended {
        actions.push((ids::fullscreen(owner.0), t("Pantalla completa")));
        if !ui.video.windows.contains(&owner) {
            actions.push((ids::window(owner.0), t("Ventana aparte")));
        }
    }
    let shown = ui
        .video
        .streams
        .iter()
        .find(|(owner, _)| *owner == watching.owner)
        .and_then(|(_, live)| live.frame());
    Preview {
        surface: ids::stream_surface(watching.owner.0),
        title: tr!(
            "Pantalla de {name}",
            name = store.display_name(watching.owner, server)
        ),
        frame: shown.filter(|_| !ended),
        waiting,
        stop: (ids::stop_watching(watching.owner.0), t("Dejar de ver")),
        note,
        actions,
    }
}

/// The stream watched shown alone, if it still is: it fills the window,
/// with the way out of full screen in place of the way to stop it.
pub(crate) fn focused_view(ui: &Ui, store: &Store, size: limn::Size) -> Option<limn::Element> {
    let watching = store
        .watching()
        .iter()
        .find(|watching| Some(watching.owner) == ui.video.focus)?;
    let shown = Preview {
        stop: (ids::FULLSCREEN_EXIT.into(), t("Salir de pantalla completa")),
        actions: Vec::new(),
        ..stream(ui, store, watching)
    };
    Some(rune_components::voice::preview(&shown, size, &ui.look()))
}

/// The window should be full screen: a stream watched is shown alone.
pub fn fullscreen(ui: &Ui, store: &Store) -> bool {
    ui.video.focus.is_some_and(|owner| {
        store
            .watching()
            .iter()
            .any(|watching| watching.owner == owner)
    })
}

/// «Pantalla completa»: `owner`'s stream alone, the window full screen.
pub(crate) fn focus(ui: &mut Ui, owner: UserId) -> Vec<Intent> {
    ui.video.focus = Some(owner);
    Vec::new()
}

/// «Salir de pantalla completa», or Escape: back to the stage.
pub(crate) fn unfocus(ui: &mut Ui) -> Vec<Intent> {
    ui.video.focus = None;
    Vec::new()
}

/// Escape leaves a stream shown alone.
pub(crate) fn intercept(ui: &mut Ui, event: &limn::UiEvent) -> Option<Vec<Intent>> {
    ui.video.focus?;
    match event {
        limn::UiEvent::Key {
            key: limn::Key::Escape,
            ..
        } => Some(unfocus(ui)),
        _ => None,
    }
}

/// «Ventana aparte»: `owner`'s stream in a window of its own as well.
pub(crate) fn pop_out(ui: &mut Ui, owner: UserId) -> Vec<Intent> {
    if !ui.video.windows.contains(&owner) && ui.video.windows.len() < MAX_STREAMS {
        ui.video.windows.push(owner);
    }
    Vec::new()
}

/// The streams to show in a window of their own, with each window's title.
pub fn windows(ui: &Ui, store: &Store) -> Vec<(UserId, String)> {
    ui.video
        .windows
        .iter()
        .filter_map(|owner| {
            let watching = store
                .watching()
                .iter()
                .find(|watching| watching.owner == *owner)?;
            let server = store
                .channel(watching.channel)
                .and_then(|found| found.server);
            let title = tr!(
                "Pantalla de {name} — RUNE",
                name = store.display_name(*owner, server)
            );
            Some((*owner, title))
        })
        .collect()
}

/// What the window of `owner`'s stream shows at `size`, while watched.
pub fn window_view(
    ui: &Ui,
    store: &Store,
    owner: UserId,
    size: limn::Size,
) -> Option<limn::Element> {
    let watching = store
        .watching()
        .iter()
        .find(|watching| watching.owner == owner)?;
    let shown = Preview {
        surface: ids::window_surface(owner.0),
        ..stream(ui, store, watching)
    };
    Some(rune_components::voice::stream_window(
        &shown,
        size,
        &ui.look(),
    ))
}

/// The window of `owner`'s stream was closed: it stays on the stage.
pub fn window_closed(ui: &mut Ui, owner: UserId) {
    ui.video.windows.retain(|shown| *shown != owner);
}

/// «Ver transmisión»: `owner`'s stream in the account's call, besides the
/// others; past [`MAX_STREAMS`] the one watched longest goes.
pub(crate) fn watch(ui: &mut Ui, store: &mut Store, owner: UserId, now: Instant) -> Vec<Intent> {
    let Some(channel) = store.call().map(|call| call.channel) else {
        return Vec::new();
    };
    let denied = store
        .abilities(channel)
        .map(|abilities| abilities.watch_stream)
        .and_then(|permit| permit.reason().map(str::to_owned));
    if let Some(reason) = denied {
        ui.notify(t("No se puede ver la transmisión"), Some(reason), true, now);
        return Vec::new();
    }
    // Watched again, it starts over.
    let mut intents = stop_watching(ui, store, owner);
    let watched = store.watching();
    if watched.len() >= MAX_STREAMS {
        let oldest = watched[0].owner;
        intents.extend(stop_watching(ui, store, oldest));
    }
    store.begin_watch(channel, owner);
    ui.video.streams.push((owner, Live::Waiting));
    intents.push(Intent::Command(Command::WatchStream { channel, owner }));
    intents
}

/// «Dejar de ver»: `owner`'s stream goes, ended or not.
pub(crate) fn stop_watching(ui: &mut Ui, store: &mut Store, owner: UserId) -> Vec<Intent> {
    let Some(watching) = store
        .watching()
        .iter()
        .find(|watching| watching.owner == owner)
        .cloned()
    else {
        return Vec::new();
    };
    store.stop_watch(owner);
    ui.video.streams.retain(|(shown, _)| *shown != owner);
    ui.video.windows.retain(|shown| *shown != owner);
    if ui.video.focus == Some(owner) {
        ui.video.focus = None;
    }
    if matches!(watching.phase, StreamPhase::Ended(_)) {
        return Vec::new();
    }
    vec![Intent::Command(Command::StopWatching {
        channel: watching.channel,
        owner: watching.owner,
    })]
}

/// The camera or screen button: on, or off again. The screen, when it can
/// go to the call, opens the quality picker first (or closes it, `picker`
/// being whether it was open).
pub(crate) fn toggle(
    ui: &mut Ui,
    store: &mut Store,
    screen: bool,
    picker: bool,
    now: Instant,
) -> Vec<Intent> {
    let on = if screen {
        ui.video.screen.on
    } else {
        ui.video.camera.on
    };
    if on {
        return stop(ui, store, screen);
    }
    if let Some(reason) = unsupported() {
        ui.notify(
            t("No se puede usar el video"),
            Some(reason.into()),
            true,
            now,
        );
    } else if screen && can_share(store) {
        ui.video.picker = !picker;
        ui.voice.stage = store.call().map(|call| call.channel);
    } else if in_call(store) {
        let share = if screen {
            &mut ui.video.screen
        } else {
            &mut ui.video.camera
        };
        share.on = true;
        share.live = Live::Waiting;
        if !screen {
            return tell_camera(store, true);
        }
    }
    Vec::new()
}

/// A size or a rate chosen in the quality picker, which stays open.
pub(crate) fn choose(ui: &mut Ui, height: Option<u32>, fps: Option<u32>) -> Vec<Intent> {
    let quality = &mut ui.video.quality;
    *quality = StreamQuality {
        height: height.unwrap_or(quality.height),
        fps: fps.unwrap_or(quality.fps),
    };
    if !quality.valid() {
        *quality = StreamQuality::default();
    }
    ui.video.picker = true;
    Vec::new()
}

/// «Compartir pantalla» in the picker: the system's dialog opens. The
/// provider is asked for the stream only once the person chose what to
/// share there ([`screen_ready`]), as the official client does: a stream
/// that fails then never closes the dialog under them.
pub(crate) fn share(ui: &mut Ui, store: &mut Store, now: Instant) -> Vec<Intent> {
    let Some(channel) = store.call().map(|call| call.channel) else {
        return Vec::new();
    };
    let denied = store
        .abilities(channel)
        .and_then(|abilities| abilities.share_screen.reason().map(str::to_owned));
    if let Some(reason) = denied {
        ui.notify(
            t("No se puede compartir la pantalla"),
            Some(reason),
            true,
            now,
        );
        return Vec::new();
    }
    ui.video.screen = Share {
        on: true,
        live: Live::Waiting,
        ..Share::default()
    };
    // A stream that ended before is not this one.
    if store
        .sharing()
        .is_some_and(|sharing| matches!(sharing.phase, StreamPhase::Ended(_)))
    {
        store.stop_share();
    }
    Vec::new()
}

/// How the screen is captured while it is to go to the call: at the
/// quality chosen from the system's dialog on, so the source the person
/// chose is the one sent once the stream exists.
pub fn capture_quality(ui: &Ui, store: &Store) -> Option<StreamQuality> {
    (ui.video.screen.on && in_call(store) && can_share(store)).then_some(ui.video.quality)
}

/// The host says whether the screen's source runs: the person chose it in
/// the system's dialog and its frames arrive. Then, once, the provider is
/// asked for the stream at the quality chosen.
pub fn screen_ready(ui: &Ui, store: &mut Store, running: bool) -> Vec<Intent> {
    let channel = store.call().map(|call| call.channel);
    let (Some(channel), true, Some(quality), None) = (
        channel,
        running,
        capture_quality(ui, store),
        store.sharing(),
    ) else {
        return Vec::new();
    };
    store.begin_share(channel);
    vec![Intent::Command(Command::StartStream { channel, quality })]
}

/// The provider is told the shared screen stops, unless it ended already.
fn stop_sharing(store: &mut Store) -> Vec<Intent> {
    let Some(sharing) = store.sharing().cloned() else {
        return Vec::new();
    };
    store.stop_share();
    if matches!(sharing.phase, StreamPhase::Ended(_)) {
        return Vec::new();
    }
    vec![Intent::Command(Command::StopStream {
        channel: sharing.channel,
    })]
}

/// «Dejar de compartir», «Apagar la cámara».
pub(crate) fn stop(ui: &mut Ui, store: &mut Store, screen: bool) -> Vec<Intent> {
    if screen {
        ui.video.screen = Share::default();
        stop_sharing(store)
    } else {
        stop_camera(ui, store)
    }
}

/// The camera goes off, and the call is told if it was on in it.
fn stop_camera(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let was = std::mem::take(&mut ui.video.camera).on;
    if was {
        tell_camera(store, false)
    } else {
        Vec::new()
    }
}

/// The account's stream ended at the service: the screen stops, with why.
pub(crate) fn sharing_changed(ui: &mut Ui, store: &Store, now: Instant) {
    let ended = store.sharing().and_then(|sharing| match &sharing.phase {
        StreamPhase::Ended(reason) => Some(reason.clone()),
        _ => None,
    });
    if let Some(reason) = ended.filter(|_| ui.video.screen.on) {
        ui.video.screen = Share::default();
        if let Some(reason) = reason {
            ui.notify(
                t("Se dejó de compartir la pantalla"),
                Some(reason),
                true,
                now,
            );
        }
    }
}

/// The call is over: what was shared in it stops.
pub(crate) fn call_over(ui: &mut Ui) {
    ui.video.picker = false;
    ui.video.screen = Share::default();
    ui.video.camera = Share::default();
    ui.video.streams.clear();
    ui.video.watched.clear();
    ui.video.focus = None;
    ui.video.windows.clear();
}

/// The host lost a source: the system ended it, it went away, or it could
/// not be had. Says why, unless RUNE itself stopped it. A shared screen's
/// stream stops with it.
pub fn ended(
    ui: &mut Ui,
    store: &mut Store,
    screen: bool,
    reason: Option<String>,
    now: Instant,
) -> Vec<Intent> {
    let intents = if screen {
        ui.video.screen = Share::default();
        stop_sharing(store)
    } else {
        ui.video.testing = false;
        stop_camera(ui, store)
    };
    if let Some(reason) = reason {
        let title = if screen {
            t("Se dejó de compartir la pantalla")
        } else {
            t("Se apagó la cámara")
        };
        ui.notify(title, Some(reason), true, now);
    }
    intents
}
