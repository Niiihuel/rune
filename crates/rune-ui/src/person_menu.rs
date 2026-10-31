//! The person menu: what the account can do with someone, opened from a voice
//! row or tile, a member-list row, a message's author or a mention (2026-10-25:
//! Discord's menu in every place). The groups come from the modules that own
//! their permissions; this file says which of them each place shows.

use std::time::Instant;

use limn::{Element, Point, UiAction};
use rune_components::profile::ids as profile_ids;
use rune_components::voice::{self as parts, ids, MenuEntry};
use rune_components::Look;
use rune_i18n::t;
use rune_model::{ChannelId, MessageId, RelationshipKind, ServerId, UserId};
use rune_provider::Created;
use rune_provider::{Command, RequestId};
use rune_state::{Applied, Store};

use crate::server_settings::advanced::{invite_channels, invite_link};
use crate::voice::simulated;
use crate::{conversations, private_calls, Intent, Ui};

/// How long an invitation sent from a person's menu lasts: a week, as the
/// invitations' dialog offers by default. It never runs out of uses.
pub(crate) const INVITE_AGE_SECS: u32 = 7 * 24 * 60 * 60;

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

/// An invitation to a server sent from a person's menu. Its link goes to
/// their conversation once both are there: the link, from the provider, and
/// the conversation, which is opened for it when there is none yet.
#[derive(Clone, Debug, PartialEq)]
pub struct PersonInvite {
    pub user: UserId,
    pub request: RequestId,
    pub link: Option<String>,
    pub channel: Option<ChannelId>,
    /// The request that opens their conversation, while it is asked for.
    pub opening: Option<RequestId>,
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
    let mine = |action: &str| format!("pm:{}:{action}", user.0);
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
    // A call is rung in their conversation, which must exist already.
    let callable = other
        && store.direct_with(user).is_some_and(|channel| {
            private_calls::callable(store, channel)
                && store
                    .abilities(channel)
                    .is_some_and(|abilities| abilities.ring.allowed())
        });
    if callable {
        first.push(item(mine("call"), t("Llamar"), None));
    }
    if other && store.account_abilities().notes.allowed() {
        first.push(item(
            format!("{}{}", profile_ids::FULL_NOTE, user.0),
            t("Añadir nota"),
            None,
        ));
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
        let mut controls = vec![
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
        ];
        // Their camera, hidden for the account: offered while it is on or hidden.
        let hidden = ui.prefs.video_hidden(user.0);
        if hidden || store.voice_state_of(user).is_some_and(|state| state.camera) {
            controls.push(item(mine("video"), t("Desactivar video"), Some(hidden)));
        }
        groups.push(controls);
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
        // Ignoring (or not) sits right before blocking, as the official menu has it.
        people.extend(crate::ignore::menu(store, user));
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
        // Invite them to a server they are not in, where the account may invite.
        let servers: Vec<MenuEntry> = store
            .servers()
            .iter()
            .filter(|found| {
                store.member_of(found.id, user).is_none()
                    && store
                        .server_abilities(found.id)
                        .is_some_and(|abilities| abilities.create_invites.allowed())
                    && !invite_channels(store, found.id).is_empty()
            })
            .map(|found| {
                item(
                    format!("pm:{}:invite:{}", user.0, found.id.0),
                    &found.name,
                    None,
                )
            })
            .collect();
        if !servers.is_empty() {
            groups.push(vec![MenuEntry::Submenu(
                t("Invitar al servidor").into(),
                servers,
            )]);
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

/// The person menu's own items: a call, a camera hidden, and an invitation to
/// a server. `None` for the other ids.
pub(crate) fn action(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    let (user, what) = id.strip_prefix("pm:")?.split_once(':')?;
    let user = UserId(user.parse().ok()?);
    Some(match what {
        "call" => {
            let channel = store.direct_with(user)?;
            private_calls::start(ui, store, channel, now)
        }
        "video" => {
            let hidden = !ui.prefs.video_hidden(user.0);
            vec![ui.prefs.set_video_hidden(user.0, hidden)]
        }
        _ => {
            let server = ServerId(what.strip_prefix("invite:")?.parse().ok()?);
            invite(ui, store, user, server, now)
        }
    })
}

/// Make an invitation to `server` for `user`; its link goes to their
/// conversation once both are there (see [`progress`]).
fn invite(
    ui: &mut Ui,
    store: &mut Store,
    user: UserId,
    server: ServerId,
    now: Instant,
) -> Vec<Intent> {
    let Some(abilities) = store.server_abilities(server) else {
        return Vec::new();
    };
    if let Some(reason) = abilities.create_invites.reason() {
        ui.notify(
            t("No se creó la invitación"),
            Some(reason.to_owned()),
            true,
            now,
        );
        return Vec::new();
    }
    let Some(channel) = invite_channels(store, server).first().copied() else {
        return Vec::new();
    };
    let request = ui.fresh_request();
    ui.person_invite = Some(PersonInvite {
        user,
        request,
        link: None,
        channel: None,
        opening: None,
    });
    let mut intents = vec![Intent::Command(Command::CreateInvite {
        request,
        server,
        channel,
        max_age_secs: INVITE_AGE_SECS,
        max_uses: 0,
        temporary: false,
    })];
    intents.extend(progress(ui, store, now));
    // Their conversation is opened for the link, unless it is there or refused.
    if ui
        .person_invite
        .as_ref()
        .is_some_and(|invite| invite.channel.is_none())
    {
        intents.extend(conversations::open_direct(ui, store, user, now));
        let opening = ui
            .conversations
            .opening
            .filter(|(_, who)| *who == user)
            .map(|(request, _)| request);
        match ui.person_invite.as_mut() {
            Some(invite) if opening.is_some() => invite.opening = opening,
            _ => ui.person_invite = None,
        }
    }
    intents
}

/// Send the link of the invitation in `ui` to its person, once both the link
/// and their conversation are there; the invitation is then done.
fn progress(ui: &mut Ui, store: &Store, now: Instant) -> Vec<Intent> {
    let Some(invite) = ui.person_invite.as_mut() else {
        return Vec::new();
    };
    if invite.channel.is_none() {
        invite.channel = store.direct_with(invite.user);
    }
    match (invite.link.clone(), invite.channel) {
        (Some(content), Some(channel)) => {
            ui.person_invite = None;
            ui.notify(t("Invitación enviada"), None, false, now);
            vec![Intent::Send {
                channel,
                content,
                reply_to: None,
                mention_reply: false,
            }]
        }
        _ => Vec::new(),
    }
}

/// What the provider answered for an invitation from a person's menu: its
/// link, and the conversation opened for it.
pub(crate) fn on_applied(
    ui: &mut Ui,
    store: &Store,
    applied: &Applied,
    now: Instant,
) -> Vec<Intent> {
    let Some(invite) = ui.person_invite.as_mut() else {
        return Vec::new();
    };
    match applied {
        Applied::InviteCreated { request, code, .. } if *request == invite.request => {
            invite.link = Some(invite_link(store, code));
            // Nothing opens their conversation: the link has nowhere to go.
            if invite.channel.is_none() && invite.opening.is_none() {
                ui.person_invite = None;
                return Vec::new();
            }
        }
        Applied::Created {
            request,
            created: Created::Channel(channel),
        } if invite.opening == Some(*request) => invite.channel = Some(*channel),
        _ => return Vec::new(),
    }
    progress(ui, store, now)
}
