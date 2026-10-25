//! The person menu: what the account can do with someone, opened from a voice
//! row or tile, a member-list row, a message's author or a mention (2026-10-25:
//! Discord's menu in every place). The groups come from the modules that own
//! their permissions; this file says which of them each place shows.

use limn::{Element, Point, UiAction};
use rune_components::profile::ids as profile_ids;
use rune_components::voice::{self as parts, ids, MenuEntry};
use rune_components::Look;
use rune_i18n::t;
use rune_model::{ChannelId, MessageId, RelationshipKind, ServerId, UserId};
use rune_state::Store;

use crate::voice::simulated;
use crate::{Intent, Ui};

/// Where a person's menu was opened, and so which of its groups show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    /// A row of a voice channel in the column.
    VoiceRow { channel: ChannelId },
    /// A tile of a voice channel's stage.
    Tile { channel: ChannelId },
    /// A row of the member list of `server`.
    MemberList { server: ServerId },
    /// The author of a message in `channel`, in a server.
    Author { channel: ChannelId },
    /// The author of a message in a direct conversation, `channel`.
    Direct { channel: ChannelId },
}

/// A person's menu opened by a secondary click outside a voice channel: on
/// whom, where, and the corner it opened at.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PersonMenu {
    pub user: UserId,
    pub place: Place,
    pub at: (f32, f32),
}

/// Whether choosing `id` keeps the menu open: a role changes right away and
/// the menu stays for the next, as the official client's does.
pub(crate) fn keeps_open(id: &str) -> bool {
    id.starts_with("ss:member-of:") && id.contains(":role:")
}

/// The entries of `user`'s menu in `place`.
pub(crate) fn entries(ui: &Ui, store: &Store, user: UserId, place: Place) -> Vec<MenuEntry> {
    let item = |id: String, label: &str, checked: Option<bool>| {
        MenuEntry::Item(id, label.to_owned(), checked)
    };
    let channel = match place {
        Place::MemberList { .. } => None,
        Place::VoiceRow { channel }
        | Place::Tile { channel }
        | Place::Author { channel }
        | Place::Direct { channel } => Some(channel),
    };
    let server = match place {
        Place::MemberList { server } => Some(server),
        _ => channel
            .and_then(|channel| store.channel(channel))
            .and_then(|found| found.server),
    };
    let voice = matches!(place, Place::VoiceRow { .. } | Place::Tile { .. });
    let other = store.me() != Some(user);
    let social = store.social(user);
    let person = |action: &str| format!("friend:{}:{action}", user.0);
    let mut groups: Vec<Vec<MenuEntry>> = Vec::new();

    // Their profile: the voice's card there, else their full profile.
    let profile = if voice {
        format!("{}{}", ids::PROFILE, user.0)
    } else {
        format!("{}{}", profile_ids::FULL, user.0)
    };
    let mut first = vec![item(profile, t("Perfil"), None)];
    let writable = ui
        .channel()
        .and_then(|channel| store.abilities(channel))
        .is_some_and(|abilities| abilities.write.allowed());
    if writable && !matches!(place, Place::Tile { .. }) {
        first.push(item(
            format!("{}{}", ids::MENTION, user.0),
            t("Mención"),
            None,
        ));
    }
    if other && social.message.allowed() {
        first.push(item(person("message"), t("Mensaje"), None));
    }
    groups.push(first);

    // Their stream, as the official client's menus offer it (voice only).
    if voice {
        let streaming = store.voice_state_of(user).is_some_and(|state| {
            state.streaming
                && store
                    .call()
                    .is_some_and(|call| call.channel == state.channel)
        });
        if !other && ui.video.screen.on {
            // Discord's «Dejar de transmitir»; that key is a call sound's here.
            groups.push(vec![item(
                ids::STOP_SCREEN.into(),
                t("Dejar de compartir"),
                None,
            )]);
        } else if store
            .watching()
            .iter()
            .any(|watching| watching.owner == user)
        {
            groups.push(vec![item(
                ids::stop_watching(user.0),
                t("Dejar de ver"),
                None,
            )]);
        } else if other && streaming {
            groups.push(vec![item(ids::watch(user.0), t("Ver transmisión"), None)]);
        }
    }

    // How loud they play for the account, and whether they are silenced (voice only).
    if voice && other {
        let note = || simulated(store).then(|| t("Simulado: la llamada no lleva audio").to_owned());
        let (volume, silenced) = ui.prefs.voice_volume(user.0);
        groups.push(vec![
            MenuEntry::Volume {
                user: user.0,
                label: t("Volumen del usuario"),
                volume,
                note: note(),
            },
            item(
                ids::local_mute(user.0),
                t("Silenciar para mí"),
                Some(silenced),
            ),
        ]);
        // The sound of their stream watched has a volume of its own.
        if store
            .watching()
            .iter()
            .any(|watching| watching.owner == user)
        {
            let sound = user.0 | rune_media::mixer::STREAM_SOUND;
            let (volume, silenced) = ui.prefs.voice_volume(sound);
            groups.push(vec![
                MenuEntry::Volume {
                    user: sound,
                    label: t("Volumen de la transmisión"),
                    volume,
                    note: note(),
                },
                item(
                    ids::local_mute(sound),
                    t("Silenciar la transmisión"),
                    Some(silenced),
                ),
            ]);
        }
    }

    // A stage's moderation, in its channel.
    if let (true, Some(channel)) = (voice, channel) {
        let stage = crate::stage::menu_group(store, channel, user);
        if !stage.is_empty() {
            groups.push(stage);
        }
    }

    if other {
        let requests = store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.friend_requests);
        let mut people = Vec::new();
        match store.relationship(user).map(|known| known.kind) {
            Some(RelationshipKind::Friend) if social.remove.allowed() => {
                people.push(MenuEntry::Danger(
                    person("remove"),
                    t("Eliminar amigo").into(),
                ));
            }
            None if requests => people.push(item(person("add"), t("Añadir amigo"), None)),
            _ => {}
        }
        if store.relationship(user).map(|known| known.kind) == Some(RelationshipKind::Blocked) {
            if social.unblock.allowed() {
                people.push(item(person("unblock"), t("Desbloquear"), None));
            }
        } else if social.block.allowed() {
            people.push(MenuEntry::Danger(person("block"), t("Bloquear").into()));
        }
        if !people.is_empty() {
            groups.push(people);
        }
    }

    // Voice moderation, where they are in voice; the server's own next: nickname,
    // roles, timeout, kick and ban, only those the account may use.
    let moderated = if voice {
        channel
    } else {
        store.voice_state_of(user).map(|state| state.channel)
    };
    if let Some(channel) = moderated {
        let moderation = crate::message_actions::voice_menu_group(store, channel, user);
        if !moderation.is_empty() {
            groups.push(moderation);
        }
    }
    if let Some(server) = server {
        let members = crate::server_settings::member_menu_group(store, server, user);
        if !members.is_empty() {
            groups.push(members);
        }
    }

    // The account's own: its call's controls (in a call) and its profile.
    if !other {
        let mut own = Vec::new();
        if voice || store.call().is_some() {
            own.push(item(ids::MENU_MUTE.into(), t("Silenciar"), Some(ui.muted)));
            own.push(item(
                ids::MENU_DEAFEN.into(),
                t("Ensordecer"),
                Some(ui.deafened),
            ));
        }
        let editable = store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.edit_profile);
        if editable {
            own.push(item(profile_ids::EDIT.into(), t("Editar perfil"), None));
        }
        if !own.is_empty() {
            groups.push(own);
        }
    }
    if ui.prefs.developer {
        groups.push(vec![item(
            format!("{}{}", ids::COPY_ID, user.0),
            t("Copiar ID del usuario"),
            None,
        )]);
    }
    let mut entries = Vec::new();
    for group in groups {
        if !entries.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        entries.extend(group);
    }
    entries
}

/// A secondary click on a person outside a voice channel opens their menu
/// there: on their row in the member list, on a message's author, or on a
/// mention of them. `None` when it was not on one.
pub(crate) fn secondary_click(
    ui: &mut Ui,
    store: &Store,
    target: &str,
    at: Point,
) -> Option<Vec<Intent>> {
    let number = |prefix: &str| {
        target
            .strip_prefix(prefix)?
            .split(':')
            .next()?
            .parse::<u64>()
            .ok()
    };
    let (user, place) = if let Some(user) = number("member:") {
        let place = match ui.server {
            Some(server) => Place::MemberList { server },
            None => Place::Direct {
                channel: ui.channel()?,
            },
        };
        (UserId(user), place)
    } else if let Some(message) = number("pp:name:").or_else(|| number("pp:avatar:")) {
        let channel = ui.channel()?;
        let user = store.timeline(channel)?.get(MessageId(message))?.author;
        let server = store.channel(channel).and_then(|found| found.server);
        let place = match server {
            Some(_) => Place::Author { channel },
            None => Place::Direct { channel },
        };
        (user, place)
    } else {
        return None;
    };
    // Opening it closes the other menus, as the voice's does.
    ui.voice.menu = None;
    ui.voice.card = None;
    ui.voice.more = false;
    ui.channel_list.menu = None;
    ui.server_menu.open = false;
    ui.person_menu = Some(PersonMenu {
        user,
        place,
        at: (at.x, at.y),
    });
    Some(Vec::new())
}

/// The person menu opened by a secondary click, over everything.
pub(crate) fn overlays(ui: &Ui, store: &Store, root: Element, look: &Look) -> Element {
    let Some(menu) = ui.person_menu else {
        return root;
    };
    // A member list's menu goes once its server is left or the person is.
    if let Place::MemberList { server } = menu.place {
        if ui.server != Some(server) || store.member_of(server, menu.user).is_none() {
            return root;
        }
    }
    Element::stack(vec![
        root,
        parts::context_menu(menu.at, entries(ui, store, menu.user, menu.place), look),
    ])
}

/// Clicks choose an entry of the menu, which closes it unless it keeps open.
pub(crate) fn update(ui: &mut Ui, action: &UiAction) {
    if let UiAction::Clicked { id } = action {
        if !keeps_open(id) {
            ui.person_menu = None;
        }
    }
}
