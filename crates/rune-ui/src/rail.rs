//! The rail beyond its marks (2026-10-18): what its hint says of each
//! server's voice, as the official client's (REA in docs/DISCORD.md), and its
//! servers and folders dragged to reorder them or to make and fill folders,
//! as in Serein. The menus do the same.

use std::time::Instant;

use limn::UiAction;
use rune_components::rail::{self, Placement, RailDrag, RailFace, RailVoice};
use rune_components::Look;
use rune_model::{ChannelKind, Server, ServerId, Timestamp, UserId};
use rune_state::{ServerVoice, Store};

use crate::account::{Edit, RailTarget};
use crate::{Intent, Ui};

/// A server or a folder being dragged, and where it would land now.
#[derive(Clone, Debug, PartialEq)]
pub struct Drag {
    pub source: RailTarget,
    pub over: Option<(RailTarget, Placement)>,
}

/// The mark of the rail an id names.
fn target(id: &str) -> Option<RailTarget> {
    let (kind, number) = id.split_once(':')?;
    let number = number.parse().ok()?;
    match kind {
        "server" => Some(RailTarget::Server(ServerId(number))),
        "folder" => Some(RailTarget::Folder(number)),
        _ => None,
    }
}

fn key(target: RailTarget) -> String {
    match target {
        RailTarget::Server(server) => format!("server:{server}"),
        RailTarget::Folder(folder) => format!("folder:{folder}"),
    }
}

/// Where a drop zone lands `source`; `None` on itself, or inside for a folder.
fn landing(source: RailTarget, zone: &str) -> Option<(RailTarget, Placement)> {
    let (mark, placement) = rail::drop_zone(zone)?;
    let found = target(mark)?;
    let folder = matches!(source, RailTarget::Folder(_));
    (found != source && !(folder && placement == Placement::Inside)).then_some((found, placement))
}

/// What the rail draws of a drag: whether its marks can be dragged, the one
/// that is, and where it would land.
pub(crate) fn drag_marks(ui: &Ui, store: &Store) -> RailDrag {
    let drag = ui.account.drag.as_ref();
    RailDrag {
        enabled: store.account_abilities().folders.allowed(),
        source: drag.map(|drag| key(drag.source)),
        inside: drag.is_some_and(|drag| matches!(drag.source, RailTarget::Server(_))),
        over: drag
            .and_then(|drag| drag.over)
            .map(|(over, placement)| (key(over), placement)),
    }
}

/// Answer a drag of a server or a folder of the rail; `None` for others.
pub(crate) fn dragged(
    ui: &mut Ui,
    store: &Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    match action {
        UiAction::DragStarted { id, .. } => {
            let source = target(id)?;
            ui.account.menu = None;
            ui.account.drag = drag_marks(ui, store)
                .enabled
                .then_some(Drag { source, over: None });
            Some(Vec::new())
        }
        UiAction::DragMoved { id, over, .. } => {
            let source = target(id)?;
            if let Some(drag) = ui.account.drag.as_mut().filter(|d| d.source == source) {
                drag.over = over.as_deref().and_then(|zone| landing(source, zone));
            }
            Some(Vec::new())
        }
        UiAction::Dropped {
            id, target: zone, ..
        } => {
            let source = target(id)?;
            let drag = ui.account.drag.take().filter(|d| d.source == source);
            let Some((landed, placement)) = drag
                .and(zone.as_deref())
                .and_then(|zone| landing(source, zone))
            else {
                return Some(Vec::new());
            };
            let edit = Edit::Drop(source, landed, placement);
            let mut intents = Vec::new();
            // A server dropped on one alone makes a folder: it shows open.
            if let RailTarget::Server(server) = source {
                intents.extend(crate::account::open_new_folder(ui, store, &edit, server));
            }
            intents.extend(crate::account::folders_command(ui, store, edit, now));
            Some(intents)
        }
        UiAction::DragCancelled { id } => {
            target(id)?;
            ui.account.drag = None;
            Some(Vec::new())
        }
        _ => None,
    }
}

/// What `server`'s hint says of its voice, from who is there (`voice`), as
/// the official client: speakers of its stages and how many listen, then
/// who is in its voice channels, who streams, and who uses an app there;
/// nobody from its idle (AFK) channel, from a channel the account muted or
/// does not see, nor whom it blocked.
pub(crate) fn voice(
    ui: &Ui,
    store: &Store,
    server: &Server,
    voice: Option<&ServerVoice<'_>>,
    look: &Look,
    wall: Timestamp,
) -> RailVoice {
    let face = |user: UserId| RailFace {
        name: store.display_name(user, Some(server.id)),
        image: ui.picture(store.avatar_in(Some(server.id), user)),
        color: store
            .user(user)
            .map_or(look.accent, |found| look.ink(found.accent)),
    };
    let afk = server.options.afk_channel;
    // As the official client's `isChannelMuted`: the channel's own silence,
    // not its category's.
    let shown_channel = |channel: &rune_model::Channel| {
        Some(channel.id) != afk
            && Store::visible(channel)
            && !crate::account::channel_notify(ui, store, channel.id).muted(wall)
    };
    // Apps go by the voice rows' rule, so the speaker lights as the
    // official's (`getGuildMediaState`); and only whoever is still there.
    let mut in_app: Vec<UserId> = Vec::new();
    for activity in store.voice_activities(server.id) {
        if !store.channel(activity.channel).is_some_and(shown_channel) {
            continue;
        }
        let there = store.voice_in(activity.channel);
        for user in &activity.people {
            if !in_app.contains(user)
                && !store.blocked(*user)
                && there.iter().any(|state| state.user == *user)
            {
                in_app.push(*user);
            }
        }
    }
    let mut shown = RailVoice {
        own: voice.is_some_and(|voice| voice.own),
        activities: in_app.iter().map(|user| face(*user)).collect(),
        ..RailVoice::default()
    };
    for state in voice.map(|voice| voice.people.as_slice()).unwrap_or(&[]) {
        let Some(channel) = store.channel(state.channel) else {
            continue;
        };
        if Some(channel.id) == afk || !Store::visible(channel) || store.blocked(state.user) {
            continue;
        }
        if channel.kind == ChannelKind::Stage {
            if state.suppressed {
                shown.listeners += 1;
            } else {
                shown.stage.push(face(state.user));
            }
            continue;
        }
        if !shown_channel(channel) || in_app.contains(&state.user) {
            continue;
        }
        if state.streaming {
            shown.streaming.push(face(state.user));
        } else {
            shown.people.push(face(state.user));
        }
    }
    shown
}
