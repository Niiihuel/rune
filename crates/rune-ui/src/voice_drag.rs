//! People dragged between voice channels (admin-canales-y-voz, 2026-10-18):
//! as in Serein, someone's row under a voice channel of the column is picked
//! up and let go over another voice channel, which moves them there as
//! «Mover a» of their menu does, with the same permits
//! (`Store::voice_moderation`). While someone is carried, each voice channel
//! and the people under it take the drop, as the rail's zones do; where they
//! would go is ringed, and where they may not go is dimmed.

use limn::{Element, UiAction};
use rune_components::sidebar::DragMark;
use rune_model::{ChannelId, ServerId, UserId};
use rune_provider::{Command, VoiceModeration};
use rune_state::Store;

use crate::{Intent, Ui};

/// What a drop zone of a voice channel starts with: `vz:drop:{channel}:{part}`.
const ZONE: &str = "vz:drop:";

/// Someone being carried, and the voice channel they would go to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Drag {
    pub user: UserId,
    pub over: Option<ChannelId>,
}

/// Whom a participant's row stands for, from its action.
fn carried(id: &str) -> Option<UserId> {
    id.strip_prefix("vz:user:")?.parse().ok().map(UserId)
}

/// The voice channel a drop zone stands for.
fn zone_channel(id: &str) -> Option<ChannelId> {
    let rest = id.strip_prefix(ZONE)?;
    rest.split(':').next()?.parse().ok().map(ChannelId)
}

/// The server `user` is heard in and the voice channels the account may
/// move them to, when it may move them at all.
fn rooms(store: &Store, user: UserId) -> Option<(ServerId, Vec<ChannelId>)> {
    let state = store.voice_state_of(user)?;
    let server = store.channel(state.channel)?.server?;
    let can = store.voice_moderation(server, user)?;
    can.move_to.allowed().then_some((server, can.rooms))
}

/// Whether `user`'s row can be picked up: the account may move them.
pub(crate) fn movable(store: &Store, user: UserId) -> bool {
    // Most accounts may move nobody: that is asked first, before the rooms.
    let may = store
        .voice_state_of(user)
        .and_then(|state| store.channel(state.channel))
        .and_then(|channel| store.server_abilities(channel.server?))
        .is_some_and(|abilities| abilities.move_members.allowed());
    may && rooms(store, user).is_some()
}

/// How the row of the voice channel `channel` shows while someone is
/// carried: ringed where they would go, dimmed where they may not go.
/// `None` while nobody is.
pub(crate) fn mark(ui: &Ui, store: &Store, channel: ChannelId) -> Option<DragMark> {
    let drag = ui.voice_drag?;
    Some(if drag.over == Some(channel) {
        DragMark::Inside
    } else if rooms(store, drag.user).is_some_and(|(_, rooms)| rooms.contains(&channel)) {
        DragMark::None
    } else {
        DragMark::Dim
    })
}

/// `row`, of the voice channel `channel` or of someone under it (`part`,
/// unique in it), with the zone that takes a drop while someone is carried.
pub(crate) fn zone(ui: &Ui, row: Element, channel: ChannelId, part: &str) -> Element {
    if ui.voice_drag.is_none() {
        return row;
    }
    Element::stack(vec![
        row,
        Element::clickable(format!("{ZONE}{channel}:{part}"), Element::spacer()).drop_target(),
    ])
}

/// Answer a drag of someone's row; `None` for other drags and actions.
pub(crate) fn update(ui: &mut Ui, store: &Store, action: &UiAction) -> Option<Vec<Intent>> {
    match action {
        UiAction::DragStarted { id, .. } => {
            let user = carried(id)?;
            ui.voice_drag = movable(store, user).then_some(Drag { user, over: None });
            ui.voice.menu = None;
            Some(Vec::new())
        }
        UiAction::DragMoved { id, over, .. } => {
            let user = carried(id)?;
            if let Some(drag) = ui.voice_drag.as_mut().filter(|drag| drag.user == user) {
                drag.over = over.as_deref().and_then(zone_channel).filter(|channel| {
                    rooms(store, user).is_some_and(|(_, rooms)| rooms.contains(channel))
                });
            }
            Some(Vec::new())
        }
        UiAction::Dropped { id, target, .. } => {
            let user = carried(id)?;
            let drag = ui.voice_drag.take().filter(|drag| drag.user == user);
            let room = drag.and(target.as_deref()).and_then(zone_channel);
            // Asked again on letting go: the permits may have changed meanwhile.
            let Some(((server, rooms), room)) = rooms(store, user).zip(room) else {
                return Some(Vec::new());
            };
            if !rooms.contains(&room) {
                return Some(Vec::new());
            }
            Some(vec![Intent::Command(Command::ModerateVoice {
                server,
                user,
                change: VoiceModeration::Move(room),
            })])
        }
        UiAction::DragCancelled { id } => {
            carried(id)?;
            ui.voice_drag = None;
            Some(Vec::new())
        }
        _ => None,
    }
}
