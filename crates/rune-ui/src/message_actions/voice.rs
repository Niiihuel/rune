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

/// The voice moderation items of `user`'s menu in the voice channel `channel`,
/// split where the menu places them. Only those the account may do now: as in
/// Discord, an action it cannot take (offline, no other room) is absent, not
/// greyed out.
#[derive(Debug, Default)]
pub(crate) struct VoiceMenu {
    /// Server mute, server deafen and disconnect.
    pub moderation: Vec<MenuEntry>,
    /// «Mover a», with the rooms the user may be moved to.
    pub move_to: Option<MenuEntry>,
}

pub(crate) fn menu_group(store: &Store, channel: ChannelId, user: UserId) -> VoiceMenu {
    let Some(server) = store.channel(channel).and_then(|found| found.server) else {
        return VoiceMenu::default();
    };
    let (Some(can), Some(granted)) = (
        store.voice_moderation(server, user),
        store.server_abilities(server),
    ) else {
        return VoiceMenu::default();
    };
    let state = store
        .voice_in(can.channel)
        .into_iter()
        .find(|state| state.user == user);
    let mut moderation = Vec::new();
    let mut move_to = None;
    if granted.mute_members.allowed() && can.mute.allowed() {
        let id = format!("{PREFIX}mute:{user}");
        // Discord marks the server mute in danger too.
        moderation.push(MenuEntry::DangerChecked(
            id,
            t("Silenciar servidor").into(),
            state.is_some_and(|state| state.server_muted),
        ));
    }
    if granted.deafen_members.allowed() && can.deafen.allowed() {
        let id = format!("{PREFIX}deafen:{user}");
        moderation.push(MenuEntry::Item(
            id,
            t("Ensordecer servidor").into(),
            Some(state.is_some_and(|state| state.server_deafened)),
        ));
    }
    if granted.move_members.allowed() && can.move_to.allowed() {
        move_to = Some(MenuEntry::Submenu(
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
    // Disconnecting takes the move permission, as `voice_moderation` has it.
    if granted.move_members.allowed() && can.disconnect.allowed() {
        let id = format!("{PREFIX}disconnect:{user}");
        moderation.push(MenuEntry::Danger(id, t("Desconectar").into()));
    }
    VoiceMenu {
        moderation,
        move_to,
    }
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
        ui.notify(
            t("No puedes moderar su voz"),
            Some(reason.into()),
            true,
            now,
        );
        return Some(Vec::new());
    }
    Some(vec![Intent::Command(Command::ModerateVoice {
        server,
        user,
        change,
    })])
}
