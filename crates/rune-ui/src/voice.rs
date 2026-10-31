//! Voice: the account's call where the window shows it. The panel above the
//! account's own, with its connection's details; the people under each
//! voice channel in the column; the call's stage where the conversation goes,
//! opened by clicking the channel of the call (the first click joins); and
//! the menus and cards of the people in it. Its actions start with `vz:`,
//! and a voice channel's row reports `voice:{id}`.
//!
//! The stage covers the conversation without closing it: hanging up, or
//! opening any conversation, shows the one that was open again.

use std::time::{Duration, Instant};

use limn::{Color, Element, Point, Size, UiAction};
use rune_components::voice::{self as parts, ids, Details, MenuEntry, Panel, Participant, Stage};
use rune_components::Look;
use rune_i18n::t;
use rune_model::{CallPhase, CallQuality, ChannelId, ServerId, Timestamp, UserId};
use rune_provider::{Command, VoiceSupport};
use rune_state::{Applied, Store};

use crate::person_menu::Place;
use crate::{editor_id, profile, settings, video, Intent, SettingsPage, Ui, ViewContext};

/// How many measures of the connection its details keep.
const SAMPLES: usize = 60;

/// What is open of the voice, and what the call measured.
#[derive(Debug, Default)]
pub struct Voice {
    /// The voice channel whose stage covers the conversation.
    pub stage: Option<ChannelId>,
    /// The connection's details are open over the panel.
    pub details: bool,
    /// «Más opciones» of the stage's bar is open.
    pub more: bool,
    /// The tiles under a stream focused are hidden; kept for the session.
    pub tiles_hidden: bool,
    /// A person's menu.
    pub menu: Option<Menu>,
    /// A person's card: who, in which channel, and whether from their tile
    /// (else from their row in the column).
    pub card: Option<(UserId, ChannelId, bool)>,
    /// The round trips of this call, as they arrived.
    pub samples: Vec<(Instant, CallQuality)>,
    /// The stage's channel's chat shows beside it, in place of the
    /// conversation that was open, kept here to go back to.
    pub chat: Option<Option<ChannelId>>,
}

/// The chat shows beside the stage from this width of the conversation's
/// place, as in Serein; the stage keeps the rest.
const CHAT_FROM: f32 = 720.0;
/// The narrowest the chat beside the stage gets.
const CHAT_MIN: f32 = 320.0;
/// What the stage keeps however wide the chat is dragged.
const STAGE_KEEPS: f32 = 400.0;

/// A person's menu: on whom, where they are, and where it opened.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Menu {
    pub user: UserId,
    pub channel: ChannelId,
    /// The corner where a secondary click opened it; `None` from the «⋯»
    /// of their tile.
    pub at: Option<(f32, f32)>,
    /// Opened on their row in the column, which offers to mention them.
    pub row: bool,
}

/// The channel the column marks as open: the stage's while it shows.
pub(crate) fn selected(ui: &Ui) -> Option<ChannelId> {
    ui.voice.stage.or(ui.channel())
}

pub(crate) fn simulated(store: &Store) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.voice == VoiceSupport::Simulated)
}

/// How long until the call's clock changes, for the host to paint then.
pub fn call_tick(store: &Store, now: Instant) -> Option<Duration> {
    let duration = store.call_duration(now)?;
    Some(Duration::from_millis(
        1_000 - u64::from(duration.subsec_millis()),
    ))
}

// Voice protocol (voz-protocolo).
/// How long someone has been in the voice room `channel`, at `wall`, as its
/// row in the column reads it; `None` for a stage, an empty room, or a
/// provider that does not tell.
pub(crate) fn voice_since(
    store: &Store,
    channel: &rune_model::Channel,
    wall: Timestamp,
) -> Option<String> {
    // The official client keeps them for voice rooms only.
    if channel.kind != rune_model::ChannelKind::Voice {
        return None;
    }
    let since = store.voice_started(channel.id)?;
    Some(parts::on_for(Duration::from_millis(
        (wall.0 - since.0).max(0) as u64,
    )))
}

/// Tell the provider which server's channels show, once that changes: it
/// may keep since when their voice rooms are on current.
pub(crate) fn watch_server(ui: &mut Ui) -> Vec<Intent> {
    if ui.server == ui.server_watch {
        return Vec::new();
    }
    ui.server_watch = ui.server;
    vec![Intent::Command(Command::WatchServer(ui.server))]
}

/// How long until a voice room's time in the column changes, for the host
/// to paint then.
pub fn voice_since_tick(store: &Store, wall: Timestamp) -> Option<Duration> {
    store
        .voice_started_all()
        .map(|since| Duration::from_millis((60_000 - (wall.0 - since.0).rem_euclid(60_000)) as u64))
        .min()
}

/// Join the voice channel `channel`, showing it at once as joining, with the
/// microphone and sound as the account has them.
pub(crate) fn join(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    now: Instant,
) -> Vec<Intent> {
    let abilities = store.abilities(channel);
    let permit = abilities.as_ref().map(|abilities| &abilities.join_voice);
    if let Some(reason) = permit.and_then(|permit| permit.reason()) {
        ui.notify(
            t("No se puede entrar a la voz"),
            Some(reason.to_owned()),
            true,
            now,
        );
        return Vec::new();
    }
    let connected = store
        .call()
        .is_some_and(|call| call.channel == channel && call.phase.is_connected());
    if connected {
        return Vec::new();
    }
    ui.voice.samples.clear();
    ui.voice.details = false;
    if let Some(abilities) = &abilities {
        if let Some(reason) = abilities.speak.reason() {
            // Listening only: the microphone stays closed in this channel.
            ui.muted = true;
            ui.notify(
                t("Entras sólo para escuchar"),
                Some(reason.into()),
                false,
                now,
            );
        } else if !abilities.voice_activity.allowed() && !ui.prefs.push_to_talk {
            ui.notify(
                t("Este canal exige pulsar para hablar"),
                Some(t("Tu micrófono se abre sólo mientras mantienes la tecla").into()),
                false,
                now,
            );
        }
    }
    store.begin_call(channel, ui.muted, ui.deafened);
    vec![Intent::Command(Command::JoinVoice {
        channel,
        muted: ui.muted,
        deafened: ui.deafened,
    })]
}

/// A voice channel's row was clicked: the channel of the call opens its
/// stage; another joins it (and the stage follows, if it was open).
fn channel_clicked(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    now: Instant,
) -> Vec<Intent> {
    let here = store.call().is_some_and(|call| {
        call.channel == channel && !matches!(call.phase, CallPhase::Failed { .. })
    });
    if here {
        ui.voice.stage = Some(channel);
        ui.voice.card = None;
        return Vec::new();
    }
    let intents = join(ui, store, channel, now);
    if ui.voice.stage.is_some() && !intents.is_empty() {
        ui.voice.stage = Some(channel);
    }
    intents
}

/// Run `reopen`, a conversation the window opens on its own (once ready
/// again after a reconnection, or when the open one is gone), keeping the
/// call's stage in front: only the person opening one takes it away.
pub(crate) fn keeping_stage(
    ui: &mut Ui,
    reopen: impl FnOnce(&mut Ui) -> Vec<Intent>,
) -> Vec<Intent> {
    let stage = ui.voice.stage;
    let intents = reopen(ui);
    ui.voice.stage = stage;
    intents
}

/// Hang up, and show again the conversation the stage covered.
fn leave(ui: &mut Ui, store: &mut Store) -> Vec<Intent> {
    store.end_call();
    video::call_over(ui);
    let mut intents = vec![Intent::Command(Command::LeaveVoice)];
    intents.extend(hide_chat(ui, store));
    ui.voice.stage = None;
    ui.voice.details = false;
    ui.voice.samples.clear();
    if let Some(channel) = ui.channel() {
        intents.push(Intent::Focus(editor_id(channel)));
    }
    intents
}

/// Why the stage's channel shows no chat beside it in `width`, if it does not.
fn chat_denied(store: &Store, channel: ChannelId, width: f32) -> Option<String> {
    let abilities = store.abilities(channel)?;
    match abilities.read_history.reason() {
        Some(reason) => Some(reason.to_owned()),
        None => (width < CHAT_FROM)
            .then(|| t("Agranda la ventana para ver el chat al lado de la llamada").to_owned()),
    }
}

/// How wide the chat beside the stage is, out of `width`, while it shows:
/// as wide as it was dragged, leaving the stage its room.
pub(crate) fn chat_width(ui: &Ui, store: &Store, width: f32) -> Option<f32> {
    let stage = ui.voice.stage?;
    (ui.voice.chat.is_some()
        && ui.channel() == Some(stage)
        && chat_denied(store, stage, width).is_none())
    .then(|| {
        ui.prefs
            .call_chat_width
            .min(width - STAGE_KEEPS)
            .max(CHAT_MIN)
    })
}

/// Show the stage's channel's chat beside it, or hide it and go back to the
/// conversation it replaced.
fn toggle_chat(ui: &mut Ui, store: &mut Store) -> Vec<Intent> {
    if ui.voice.chat.is_some() {
        return keeping_stage(ui, |ui| hide_chat(ui, store));
    }
    let Some(stage) = ui.voice.stage else {
        return Vec::new();
    };
    if store
        .abilities(stage)
        .is_none_or(|abilities| !abilities.read_history.allowed())
    {
        return Vec::new();
    }
    let covered = ui.channel();
    let server = store.channel(stage).and_then(|found| found.server);
    let intents = keeping_stage(ui, |ui| crate::open(ui, store, server, stage));
    ui.voice.chat = Some(covered);
    intents
}

/// Put back the conversation the chat replaced, if it is showing.
fn hide_chat(ui: &mut Ui, store: &mut Store) -> Vec<Intent> {
    let Some(covered) = ui.voice.chat.take() else {
        return Vec::new();
    };
    match covered.filter(|channel| store.channel(*channel).is_some_and(Store::visible)) {
        Some(channel) => {
            let server = store.channel(channel).and_then(|found| found.server);
            crate::open(ui, store, server, channel)
        }
        None => {
            ui.channels.remove(&ui.server);
            Vec::new()
        }
    }
}

/// Write `user`'s mention at the end of the open conversation's composer,
/// and show it.
fn mention(ui: &mut Ui, store: &Store, user: UserId) -> Vec<Intent> {
    let (Some(channel), Some(handle)) =
        (ui.channel(), store.user(user).map(|user| user.name.clone()))
    else {
        return Vec::new();
    };
    let draft = ui.drafts.get(&channel).cloned().unwrap_or_default();
    let value = if draft.is_empty() || draft.ends_with(' ') {
        format!("{draft}@{handle} ")
    } else {
        format!("{draft} @{handle} ")
    };
    ui.set_draft(channel, Some(value.clone()));
    ui.voice.stage = None;
    vec![
        Intent::SetInput {
            id: editor_id(channel),
            value,
        },
        Intent::Focus(editor_id(channel)),
    ]
}

fn number(id: &str, prefix: &str) -> Option<u64> {
    id.strip_prefix(prefix)?.split(':').next()?.parse().ok()
}

/// The channel `user` is in, as shown: their voice state, or the account's
/// call before the provider confirms it. The account's call first, so that
/// a direct call, in no server, is found too (2026-10-21: its people had no
/// menu).
fn channel_of(store: &Store, user: UserId) -> Option<ChannelId> {
    let call = store.call().map(|call| call.channel).filter(|channel| {
        store.me() == Some(user)
            || store
                .voice_in(*channel)
                .iter()
                .any(|state| state.user == user)
    });
    call.or_else(|| {
        store
            .servers()
            .iter()
            .flat_map(|server| store.channel_tree(server.id))
            .flat_map(|(_, channels)| channels)
            .find(|channel| {
                store
                    .voice_in(channel.id)
                    .iter()
                    .any(|state| state.user == user)
            })
            .map(|channel| channel.id)
    })
}

/// Answer the voice's actions. `None` for actions of other parts.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    if let UiAction::SliderChanged {
        id,
        fraction,
        committed,
    } = action
    {
        let user = number(id, "vz:volume:")?;
        // Whole percents, so 100 % is easy to land on.
        let volume = (fraction.clamp(0.0, 1.0) * parts::MAX_VOLUME * 100.0).round() / 100.0;
        let muted = ui.prefs.voice_volume(user).1;
        let save = ui.prefs.set_voice_volume(user, volume, muted);
        return Some(if *committed { vec![save] } else { Vec::new() });
    }
    let UiAction::Clicked { id } = action else {
        return None;
    };
    let id = id.as_str();
    // Choosing anything closes a person's menu and the bar's options, items
    // of other parts included; a role toggled in it keeps it open for the
    // next, as the official client's does.
    let role = id.starts_with("ss:member-of:") && id.contains(":role:");
    let menu = if role {
        ui.voice.menu
    } else {
        ui.voice.menu.take()
    };
    let more = std::mem::take(&mut ui.voice.more);
    let picker = std::mem::take(&mut ui.video.picker);
    if let Some(height) = number(id, parts::ids::QUALITY) {
        return Some(video::choose(ui, u32::try_from(height).ok(), None));
    }
    if let Some(fps) = number(id, parts::ids::FPS) {
        return Some(video::choose(ui, None, u32::try_from(fps).ok()));
    }
    if let Some(channel) = number(id, "voice:") {
        return Some(channel_clicked(ui, store, ChannelId(channel), now));
    }
    if let Some(user) = number(id, parts::ids::PROFILE) {
        let on_tile = menu.is_some_and(|menu| !menu.row);
        ui.voice.card =
            channel_of(store, UserId(user)).map(|channel| (UserId(user), channel, on_tile));
        return Some(open_card(ui, store));
    }
    if let Some(user) = number(id, parts::ids::MENTION) {
        return Some(mention(ui, store, UserId(user)));
    }
    if let Some(user) = number(id, "vz:mute:") {
        let (volume, muted) = ui.prefs.voice_volume(user);
        return Some(vec![ui.prefs.set_voice_volume(user, volume, !muted)]);
    }
    if let Some(user) = number(id, parts::ids::COPY_ID) {
        ui.notify(t("ID copiado"), None, false, now);
        return Some(vec![Intent::Copy(user.to_string())]);
    }
    for (prefix, on_tile) in [("vz:user:", false), ("vz:tile:", true)] {
        if let Some(user) = number(id, prefix) {
            let user = UserId(user);
            let open = ui
                .voice
                .card
                .is_some_and(|(shown, _, tile)| shown == user && tile == on_tile);
            ui.voice.card = (!open)
                .then(|| channel_of(store, user).map(|channel| (user, channel, on_tile)))
                .flatten();
            return Some(open_card(ui, store));
        }
    }
    if let Some(user) = number(id, "vz:watch:") {
        return Some(video::watch(ui, store, UserId(user), now));
    }
    if let Some(user) = number(id, "vz:live:watch:") {
        return Some(crate::voice_live::watch(ui, store, UserId(user), now));
    }
    if let Some(user) = number(id, "vz:fullscreen:") {
        return Some(video::focus(ui, UserId(user)));
    }
    if let Some(user) = number(id, "vz:window:") {
        return Some(video::pop_out(ui, UserId(user)));
    }
    if let Some(user) = number(id, ids::STOP_WATCHING).or_else(|| number(id, ids::CLOSE_STREAM)) {
        return Some(video::stop_watching(ui, store, UserId(user)));
    }
    // «Reintentar» on a stream that failed: watched again, from the start.
    if let Some(user) = id
        .strip_prefix("stream:")
        .and_then(|rest| rest.strip_suffix(":retry"))
        .and_then(|user| user.parse().ok())
    {
        return Some(video::watch(ui, store, UserId(user), now));
    }
    // RUNE's own picker (windows-medios).
    if let Some(source) = id.strip_prefix(ids::SOURCE) {
        return Some(video::choose_source(ui, source));
    }
    if let Some(user) = number(id, "vz:tile-more:") {
        let user = UserId(user);
        let open = menu.is_some_and(|menu| menu.user == user && menu.at.is_none());
        ui.voice.card = None;
        ui.voice.menu = (!open)
            .then(|| {
                channel_of(store, user).map(|channel| Menu {
                    user,
                    channel,
                    at: None,
                    row: false,
                })
            })
            .flatten();
        return Some(Vec::new());
    }
    Some(match id {
        ids::MUTE | ids::MENU_MUTE => crate::clicked(ui, store, "me:mute", now),
        ids::DEAFEN | ids::MENU_DEAFEN => crate::clicked(ui, store, "me:deafen", now),
        ids::LEAVE => leave(ui, store),
        ids::SCREEN => video::toggle(ui, store, true, picker, now),
        ids::CAMERA => video::toggle(ui, store, false, false, now),
        ids::SHARE => video::share(ui, store, now),
        ids::SHARE_DISMISS => Vec::new(),
        ids::SOUND => video::toggle_sound(ui),
        ids::PICK_SCREEN => video::pick_screen_again(ui),
        ids::STOP_SCREEN => video::stop(ui, store, true),
        ids::OWN_STREAM_STOP => video::stop(ui, store, true),
        ids::OWN_STREAM => video::focus_own(ui),
        ids::STOP_CAMERA => video::stop(ui, store, false),
        ids::PAUSE => video::pause(store, true),
        ids::FULLSCREEN_EXIT => video::unfocus(ui),
        ids::RESUME => video::pause(store, false),
        ids::RETRY => match store.call().map(|call| call.channel) {
            Some(channel) => join(ui, store, channel, now),
            None => Vec::new(),
        },
        // A voice channel's stage, else the open conversation's call.
        ids::JOIN => match ui.voice.stage.or_else(|| {
            ui.channel()
                .filter(|channel| crate::private_calls::callable(store, *channel))
        }) {
            Some(channel) => join(ui, store, channel, now),
            None => Vec::new(),
        },
        ids::DETAILS | ids::DETAILS_TITLE => {
            ui.voice.details = !ui.voice.details;
            Vec::new()
        }
        ids::DETAILS_DISMISS => {
            ui.voice.details = false;
            Vec::new()
        }
        ids::MORE => {
            ui.voice.more = !more;
            Vec::new()
        }
        ids::VOICE_SETTINGS => settings::open_settings(ui, store, SettingsPage::Voice),
        ids::CHAT => toggle_chat(ui, store),
        parts::ids::TILES => {
            ui.voice.tiles_hidden = !ui.voice.tiles_hidden;
            Vec::new()
        }
        rune_components::private_call::ids::EXPAND => {
            crate::private_calls::toggle_expanded(ui, store)
        }
        ids::CARD_DISMISS => {
            ui.voice.card = None;
            Vec::new()
        }
        ids::MENU_DISMISS | ids::MORE_DISMISS => Vec::new(),
        _ => return None,
    })
}

/// A secondary click on someone in a voice channel opens their menu there:
/// on their row, their tile (their camera too) or a control on the tile.
/// `None` when it was not on one.
pub(crate) fn secondary_click(
    ui: &mut Ui,
    store: &Store,
    target: &str,
    at: Point,
) -> Option<Vec<Intent>> {
    let on_tile = || {
        ["vz:tile:", "vz:tile-more:", "vz:watch:"]
            .into_iter()
            .find_map(|prefix| number(target, prefix))
    };
    let (user, row) = match (number(target, "vz:user:"), on_tile()) {
        (Some(user), _) => (UserId(user), true),
        (_, Some(user)) => (UserId(user), false),
        _ => return None,
    };
    let channel = channel_of(store, user)?;
    ui.voice.menu = Some(Menu {
        user,
        channel,
        at: Some((at.x, at.y)),
        row,
    });
    ui.voice.card = None;
    ui.voice.more = false;
    ui.channel_list.menu = None;
    ui.server_menu.open = false;
    ui.person_menu = None;
    Some(Vec::new())
}

/// Answer what an event changed: keep the connection's measures, and let go
/// of what is no longer there.
pub(crate) fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied, now: Instant) {
    match applied {
        Applied::Call => {
            if let (Some(stage), Some(call)) = (ui.voice.stage, store.call()) {
                let still_here = store
                    .me()
                    .is_some_and(|me| store.voice_in(stage).iter().any(|state| state.user == me));
                if stage != call.channel && !still_here {
                    // The provider ended the old call and reported its new
                    // channel after an administrative move. Keep the stage
                    // with the call RUNE now follows.
                    ui.voice.stage = Some(call.channel);
                    ui.voice.chat = None;
                }
            }
            if let Some(quality) = store
                .call()
                .filter(|call| call.phase.is_connected())
                .and_then(|call| call.quality)
            {
                ui.voice.samples.push((now, quality));
                if ui.voice.samples.len() > SAMPLES {
                    ui.voice.samples.remove(0);
                }
            }
        }
        // The stage stays, inviting to join again.
        Applied::CallEnded { .. } => {
            ui.voice.details = false;
            ui.voice.samples.clear();
            video::call_over(ui);
        }
        // The provider said why (a notice); the camera goes off here too.
        Applied::Failed {
            failure: rune_provider::Failure::Camera { .. },
            ..
        } => ui.video.camera = video::Share::default(),
        _ => {}
    }
    video::sharing_changed(ui, store, now);
    // A failed call, or none, shares nothing; trying again starts with it off.
    if !video::in_call(store) {
        video::call_over(ui);
    }
    if ui.voice.stage.is_some_and(|stage| {
        store
            .channel(stage)
            .is_none_or(|found| !Store::visible(found))
    }) {
        ui.voice.stage = None;
        ui.voice.chat = None;
    }
    // A card or a menu goes with the person it is on.
    let here = |user: UserId, channel: ChannelId| {
        store
            .voice_in(channel)
            .iter()
            .any(|state| state.user == user)
            || (store.me() == Some(user)
                && store.call().is_some_and(|call| call.channel == channel))
    };
    if ui
        .voice
        .card
        .is_some_and(|(user, channel, _)| !here(user, channel))
    {
        ui.voice.card = None;
    }
    if ui
        .voice
        .menu
        .is_some_and(|menu| !here(menu.user, menu.channel))
    {
        ui.voice.menu = None;
    }
}

/// The color of `user`'s tile: their avatar's, as Discord paints it, else
/// their own accent.
pub(crate) fn tile_accent(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
    look: &Look,
) -> Color {
    match ui.picture_color(store.avatar_in(server, user)) {
        Some(color) => look.ink(color),
        None => store
            .user(user)
            .map_or(look.faint, |account| look.ink(account.accent)),
    }
}

/// `user` as a participant of a call in `server`'s channel (or outside
/// servers), with nothing marked of their voice.
pub(crate) fn participant(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
    look: &Look,
) -> Participant {
    Participant {
        user: user.0,
        name: store.display_name(user, server),
        accent: tile_accent(ui, store, server, user, look),
        muted: false,
        deafened: false,
        speaking: false,
        server_muted: false,
        server_deafened: false,
        silenced: store.me() != Some(user) && ui.prefs.voice_volume(user.0).1,
        streaming: false,
        own: false,
        camera: ui
            .video
            .watched
            .iter()
            .find(|(shown, _)| *shown == user)
            .and_then(|(_, live)| live.frame())
            .filter(|_| !ui.prefs.video_hidden(user.0)),
        camera_on: false,
        thumbnail: None,
        image: ui.picture(store.avatar_in(server, user)),
        decoration: ui.picture(
            crate::identity::identity(ui, store, server, user)
                .decoration
                .as_ref(),
        ),
        movable: false,
        carried: false,
    }
}

/// The people in `channel` as shown, the account's call included before the
/// provider confirms it.
fn people(ui: &Ui, store: &Store, channel: ChannelId, look: &Look) -> Vec<Participant> {
    let server = store.channel(channel).and_then(|found| found.server);
    let me = store.me();
    let person = |user: UserId, muted: bool, deafened: bool, speaking: bool| Participant {
        muted,
        deafened,
        speaking,
        ..participant(ui, store, server, user, look)
    };
    let mut people: Vec<Participant> = store
        .voice_in(channel)
        .into_iter()
        .map(|state| Participant {
            server_muted: state.server_muted,
            server_deafened: state.server_deafened,
            // The account's own is live too, as Discord shows it; there is
            // nothing of it to watch (`own`).
            streaming: state.streaming,
            own: Some(state.user) == me,
            camera_on: state.camera,
            thumbnail: video::thumbnail(ui, store, state),
            ..person(state.user, state.muted, state.deafened, state.speaking)
        })
        .collect();
    let calling = store
        .call()
        .filter(|call| call.channel == channel && !matches!(call.phase, CallPhase::Failed { .. }));
    if let (Some(me), Some(_)) = (me, calling) {
        if !people.iter().any(|person| person.user == me.0) {
            people.push(person(me, ui.muted, ui.deafened, false));
        }
    }
    crate::stage::mark(store, channel, &mut people);
    people
}

/// Ask for what the card just opened shows beyond the account: the
/// profile in the channel's server and the note, as the member list's card
/// does (2026-10-17: nothing was asked, and the note waited for good).
fn open_card(ui: &mut Ui, store: &mut Store) -> Vec<Intent> {
    let Some((user, channel, _)) = ui.voice.card else {
        return Vec::new();
    };
    let server = store.channel(channel).and_then(|found| found.server);
    profile::request_profile(ui, store, user, server)
}

/// The card of `user` as seen in `server`.
fn card(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    user: UserId,
    look: &Look,
) -> Option<Element> {
    profile::popout_card(ui, store, server, user, look)
        .map(|found| rune_components::profile::card(&found, look))
}

/// The people under the voice channel `channel` of `server` in the column.
pub(crate) fn participant_rows(
    ui: &Ui,
    store: &Store,
    server: ServerId,
    channel: ChannelId,
    look: &Look,
) -> Vec<Element> {
    people(ui, store, channel, look)
        .into_iter()
        .map(|person| {
            let user = UserId(person.user);
            let open = ui.voice.card == Some((user, channel, false));
            let card = open
                .then(|| card(ui, store, Some(server), user, look))
                .flatten();
            // Only rows are dragged to another voice channel, not tiles.
            let person = Participant {
                movable: crate::voice_drag::movable(store, user),
                carried: ui.voice_drag.is_some_and(|drag| drag.user == user),
                ..person
            };
            let live = (person.streaming && !person.own)
                .then(|| crate::voice_live::card(ui, store, channel, user, &person.name))
                .flatten();
            let row = parts::participant(&person, card, live.as_ref(), look);
            crate::voice_drag::zone(ui, row, channel, &person.user.to_string())
        })
        .collect()
}

/// The entries of the menu on `menu.user`, in the voice's place it opened in.
fn menu_entries(ui: &Ui, store: &Store, menu: Menu) -> Vec<MenuEntry> {
    let place = if menu.row {
        Place::VoiceRow {
            channel: menu.channel,
        }
    } else {
        Place::Tile {
            channel: menu.channel,
        }
    };
    crate::person_menu::entries(ui, store, menu.user, place)
}

/// The voice card of the account's box, while there is a call.
pub(crate) fn panel(
    ui: &Ui,
    store: &Store,
    look: &Look,
    now: Instant,
    wall: Timestamp,
) -> Option<Element> {
    let call = store.call()?;
    let server = store
        .channel(call.channel)
        .and_then(|channel| channel.server)
        .and_then(|server| store.server(server))
        .map(|server| server.shown_name().to_owned());
    let details = ui.voice.details.then(|| {
        let samples = ui
            .voice
            .samples
            .iter()
            .map(|(when, quality)| {
                let ago = now.saturating_duration_since(*when).as_millis() as i64;
                let at = rune_components::time::clock(Timestamp(wall.0 - ago));
                (at, quality.rtt_ms)
            })
            .collect();
        // Desktop (P6): the call's encryption codes, in a tab of their own.
        let privacy = crate::desktop::privacy(ui, store, look)
            .map(|privacy| (privacy, crate::desktop::privacy_tab(ui)));
        parts::details_tabbed(
            &Details {
                samples,
                loss_permille: ui
                    .voice
                    .samples
                    .last()
                    .map(|(_, quality)| quality.loss_permille),
                duration: store
                    .call_duration(now)
                    .filter(|_| call.phase.is_connected()),
                simulated: simulated(store),
            },
            privacy,
            look,
        )
    });
    let box_ = parts::panel(
        &Panel {
            channel: store.channel_title(call.channel),
            server,
            phase: call.phase.clone(),
            quality: call.quality,
            simulated: simulated(store),
            video: video::controls(ui, store),
            live: video::sending(ui, store).is_some() && ui.voice.stage != Some(call.channel),
        },
        details,
        look,
    );
    Some(box_)
}

/// The stage of the voice channel it shows, where the conversation goes, in
/// `width` with its chat when that shows beside it.
pub(crate) fn stage_view(
    ui: &Ui,
    store: &Store,
    context: &ViewContext,
    look: &Look,
    width: f32,
) -> Option<Element> {
    let channel = ui.voice.stage?;
    let room = width;
    let width = width - chat_width(ui, store, room).unwrap_or(0.0);
    let chat = match chat_denied(store, channel, room) {
        Some(reason) => Err(reason),
        None => Ok(ui.voice.chat.is_some() && ui.channel() == Some(channel)),
    };
    let size = Size::new(width, context.window.height);
    // A conversation's call, expanded, keeps what it adds: who it rings,
    // «Volver a llamar», «Reducir».
    let private = crate::private_calls::callable(store, channel)
        .then(|| crate::private_calls::private_stage(ui, store, channel, look))
        .flatten();
    stage_of(ui, store, channel, size, chat, private, look, context.now)
}

/// The stage of `channel` in `size`: a voice channel's, with whether its
/// chat shows beside it or why it cannot, or a conversation's call, with
/// what that adds (`private`); at `now`, for a stream at rest.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stage_of(
    ui: &Ui,
    store: &Store,
    channel: ChannelId,
    size: Size,
    chat: Result<bool, String>,
    private: Option<rune_components::private_call::PrivateStage>,
    look: &Look,
    now: Instant,
) -> Option<Element> {
    let found = store.channel(channel)?;
    let phase = store
        .call()
        .filter(|call| call.channel == channel)
        .map(|call| call.phase.clone());
    let mut people = people(ui, store, channel, look);
    // The account first.
    if let Some(me) = store.me() {
        people.sort_by_key(|person| person.user != me.0);
    }
    let card = ui
        .voice
        .card
        .filter(|(_, shown, tile)| *shown == channel && *tile)
        .and_then(|(user, _, _)| Some((user.0, card(ui, store, found.server, user, look)?)));
    let tile_menu = ui
        .voice
        .menu
        .filter(|menu| menu.at.is_none() && menu.channel == channel)
        .map(|menu| (menu.user.0, menu_entries(ui, store, menu)));
    // The window's own buttons (barra-e-icono): a channel's stage ends at
    // the window's right unless its chat shows beside it; a call's stage is
    // under its conversation's header.
    let end = if private.is_none() && chat != Ok(true) {
        crate::title_bar::room(ui)
    } else {
        0.0
    };
    Some(parts::stage(
        Stage {
            channel: if private.is_some() {
                store.channel_title(channel)
            } else {
                found.name.clone()
            },
            phase,
            people,
            muted: ui.muted,
            deafened: ui.deafened,
            simulated: simulated(store),
            join_denied: store
                .abilities(channel)
                .and_then(|abilities| abilities.join_voice.reason().map(str::to_owned)),
            chat,
            size,
            card,
            tile_menu,
            more_open: ui.voice.more,
            video: video::controls(ui, store),
            previews: if store.call().is_some_and(|call| call.channel == channel) {
                video::previews(ui, store)
            } else {
                Vec::new()
            },
            watch: match store
                .abilities(channel)
                .and_then(|abilities| abilities.watch_stream.reason().map(str::to_owned))
            {
                Some(reason) => Err(reason),
                None => Ok(()),
            },
            watching: video::streams(ui, store, channel, now),
            tiles_hidden: ui.voice.tiles_hidden,
            private,
            quick: crate::quick_voice::view(ui, store, look, true),
            camera_panel: crate::quick_voice::camera_view(ui, store, look),
            end,
        },
        look,
    ))
}

/// A person's menu opened by a secondary click, over everything.
pub(crate) fn overlays(ui: &Ui, store: &Store, root: Element, look: &Look) -> Element {
    let Some(menu) = ui.voice.menu else {
        return root;
    };
    let Some(at) = menu.at else {
        return root;
    };
    Element::stack(vec![
        root,
        parts::context_menu(at, menu_entries(ui, store, menu), look),
    ])
}
