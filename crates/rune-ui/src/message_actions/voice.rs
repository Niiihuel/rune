//! Voice moderation from someone's menu in a voice channel: mute or deafen
//! them in the server, move them to another voice channel, or disconnect
//! them, as the account's permissions and rank allow.

use std::time::Instant;

use limn::UiAction;
use rune_components::voice::MenuEntry;
use rune_i18n::t;
use rune_model::{ChannelId, UserId};
use rune_provider::{Command, VoiceModeration};
use rune_state::Store;

use crate::{Intent, Ui};

const PREFIX: &str = "vz:mod:";

/// The moderation items of `user`'s menu in the voice channel `channel`:
/// only those the account may use.
pub(crate) fn menu_group(store: &Store, channel: ChannelId, user: UserId) -> Vec<MenuEntry> {
    let Some(can) = store
        .channel(channel)
        .and_then(|found| found.server)
        .and_then(|server| store.voice_moderation(server, user))
    else {
        return Vec::new();
    };
    let state = store
        .voice_in(can.channel)
        .into_iter()
        .find(|state| state.user == user);
    let mut items = Vec::new();
    if can.mute.allowed() {
        items.push(MenuEntry::Item(
            format!("{PREFIX}mute:{user}"),
            t("Silenciar en el servidor").into(),
            Some(state.is_some_and(|state| state.server_muted)),
        ));
    }
    if can.deafen.allowed() {
        items.push(MenuEntry::Item(
            format!("{PREFIX}deafen:{user}"),
            t("Ensordecer en el servidor").into(),
            Some(state.is_some_and(|state| state.server_deafened)),
        ));
    }
    if can.move_to.allowed() {
        items.push(MenuEntry::Submenu(
            t("Mover a").into(),
            can.rooms
                .iter()
                .map(|room| {
                    MenuEntry::Item(
                        format!("{PREFIX}move:{user}:{room}"),
                        store.channel_title(*room),
                        None,
                    )
                })
                .collect(),
        ));
    }
    if can.disconnect.allowed() {
        items.push(MenuEntry::Danger(
            format!("{PREFIX}disconnect:{user}"),
            t("Desconectar").into(),
        ));
    }
    items
}

pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    let rest = id.strip_prefix(PREFIX)?;
    ui.voice.menu = None;
    let mut parts = rest.split(':');
    let (what, user) = (parts.next()?, UserId(parts.next()?.parse().ok()?));
    let state = store.voice_state_of(user)?.clone();
    let server = store.channel(state.channel)?.server?;
    let can = store.voice_moderation(server, user)?;
    let (permit, change) = match what {
        "mute" => (&can.mute, VoiceModeration::Mute(!state.server_muted)),
        "deafen" => (&can.deafen, VoiceModeration::Deafen(!state.server_deafened)),
        "move" => {
            let room = ChannelId(parts.next()?.parse().ok()?);
            if !can.rooms.contains(&room) {
                return Some(Vec::new());
            }
            (&can.move_to, VoiceModeration::Move(room))
        }
        "disconnect" => (&can.disconnect, VoiceModeration::Disconnect),
        _ => return Some(Vec::new()),
    };
    if let Some(reason) = permit.reason() {
        ui.notify(t("No podés moderar su voz"), Some(reason.into()), true, now);
        return Some(Vec::new());
    }
    Some(vec![Intent::Command(Command::ModerateVoice {
        server,
        user,
        change,
    })])
}
