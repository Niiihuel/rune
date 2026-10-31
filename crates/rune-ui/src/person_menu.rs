//! The person menu: what the account can do with someone, opened from a voice
//! row or tile, a member-list row, a message's author or a mention (2026-10-25:
//! Discord's menu in every place). The groups come from the modules that own
//! their permissions; this file says which of them each place shows, and in
//! the official client's order.

use std::time::Instant;

use limn::{Element, Point, UiAction};
use rune_components::profile::ids as profile_ids;
use rune_components::voice::{self as parts, ids, MenuEntry};
use rune_components::Look;
use rune_i18n::t;
use rune_model::{ChannelId, MessageId, RelationshipKind, Server, ServerId, UserId};
use rune_provider::Created;
use rune_provider::{Command, Failure, RequestId};
use rune_state::{Applied, RailEntry, Store};

use crate::server_settings::advanced::{invite_channels, invite_link};
use crate::voice::simulated;
use crate::{conversations, private_calls, Intent, Ui};

/// How long an invitation sent from a person's menu lasts: a day, as the
/// official client's. It is good for one use.
pub(crate) const INVITE_AGE_SECS: u32 = 24 * 60 * 60;

/// The setting that asks before a call from a person's menu, unless the
/// account chose «No volver a preguntar». Kept with the account.
pub(crate) const ASK_SETTING: &str = "ask_before_call";

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

/// A call from a person's menu while the question before it is open: with
/// whom, and whether «No volver a preguntar» is ticked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PersonCall {
    pub user: UserId,
    pub dont_ask_again: bool,
}

/// Whether choosing `id` keeps the menu open: a role changes right away and
/// the menu stays for the next, as the official client's does.
pub(crate) fn keeps_open(id: &str) -> bool {
    // A submenu's own row only opens it; the menu stays for its choices.
    (id.starts_with("ss:member-of:") && id.contains(":role:")) || id.starts_with("vz:submenu:")
}

/// The entries of `user`'s menu in `place`, in the official client's order:
/// the stage, their profile and messages, their volume, the controls and the
/// social items, the roles and the moves, the moderation, and the developer's
/// copy of their ID.
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
    let kind = store.relationship(user).map(|known| known.kind);
    let person = |action: &str| format!("friend:{}:{action}", user.0);
    let mine = |action: &str| format!("pm:{}:{action}", user.0);
    // Their moderation in the voice channel and the server's own entries, placed
    // in their groups below.
    let moderated = if voice {
        channel
    } else {
        store.voice_state_of(user).map(|state| state.channel)
    };
    let voice_menu = moderated
        .map(|channel| crate::message_actions::voice_menu_group(store, channel, user))
        .unwrap_or_default();
    let member_menu = server
        .map(|server| crate::server_settings::member_menu_group(store, server, user))
        .unwrap_or_default();
    let mut groups: Vec<Vec<MenuEntry>> = Vec::new();

    // A stage's moderation, in its channel.
    if let (true, Some(channel)) = (voice, channel) {
        groups.push(crate::stage::menu_group(store, channel, user));
    }

    // Their profile (the full one, in every place) and what the account does
    // with them: a mention, a message, a call, a note, a nickname, their stream.
    let mut talk = vec![item(
        format!("{}{}", profile_ids::FULL, user.0),
        t("Perfil"),
        None,
    )];
    let writable = ui
        .channel()
        .and_then(|channel| store.abilities(channel))
        .is_some_and(|abilities| abilities.write.allowed());
    if writable && !matches!(place, Place::Tile { .. }) {
        talk.push(item(
            format!("{}{}", ids::MENTION, user.0),
            t("Mención"),
            None,
        ));
    }
    if other && social.message.allowed() {
        talk.push(item(person("message"), t("Enviar mensaje"), None));
    }
    if other && can_call(store, user) {
        // A conversation that refuses the ring says why, on the item.
        let refused = store
            .direct_with(user)
            .and_then(|channel| store.abilities(channel))
            .and_then(|abilities| abilities.ring.reason().map(str::to_owned));
        talk.push(match refused {
            Some(reason) => {
                MenuEntry::Disabled(mine("call"), t("Iniciar una llamada").into(), reason)
            }
            None => item(mine("call"), t("Iniciar una llamada"), None),
        });
    }
    if other && store.account_abilities().notes.allowed() {
        let label = if store.note(user).is_some_and(|note| !note.is_empty()) {
            t("Editar nota")
        } else {
            t("Añadir nota")
        };
        talk.push(MenuEntry::Described {
            id: format!("{}{}", profile_ids::FULL_NOTE, user.0),
            label: label.into(),
            description: t("Solo tú puedes ver esto").into(),
        });
    }
    // A friend's nickname for them, which only the account sees.
    if other && kind == Some(RelationshipKind::Friend) && social.nickname.allowed() {
        let label = if store
            .relationship(user)
            .is_some_and(|known| known.nickname.is_some())
        {
            t("Cambiar apodo de amigo")
        } else {
            t("Añadir apodo de amigo")
        };
        talk.push(item(person("nickname"), label, None));
    }
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
            talk.push(item(ids::STOP_SCREEN.into(), t("Dejar de compartir"), None));
        } else if store
            .watching()
            .iter()
            .any(|watching| watching.owner == user)
        {
            talk.push(item(ids::stop_watching(user.0), t("Dejar de ver"), None));
        } else if other && streaming {
            talk.push(item(ids::watch(user.0), t("Mirar transmisión"), None));
        }
    }
    groups.push(talk);

    // How loud they play for the account, and the sound of their stream when
    // it is watched (voice only).
    if voice && other {
        let note = || simulated(store).then(|| t("Simulado: la llamada no lleva audio").to_owned());
        groups.push(vec![MenuEntry::Volume {
            user: user.0,
            label: t("Volumen de usuario"),
            volume: ui.prefs.voice_volume(user.0).0,
            note: note(),
        }]);
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
                    label: t("Volumen de transmisión"),
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

    // The call's controls, their nickname, and the social items: an invitation
    // to a server, their friendship, and blocking.
    let mut controls = Vec::new();
    if voice && other {
        let silenced = ui.prefs.voice_volume(user.0).1;
        controls.push(item(
            ids::local_mute(user.0),
            t("Silenciar"),
            Some(silenced),
        ));
        // Offered in every voice call whose provider shows cameras, not only
        // while theirs is on.
        if store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.watch_cameras)
        {
            controls.push(item(
                mine("video"),
                t("Desactivar video"),
                Some(ui.prefs.video_hidden(user.0)),
            ));
        }
    }
    if !other && (voice || store.call().is_some()) {
        controls.push(item(ids::MENU_MUTE.into(), t("Silenciar"), Some(ui.muted)));
        controls.push(item(
            ids::MENU_DEAFEN.into(),
            t("Ensordecer"),
            Some(ui.deafened),
        ));
    }
    controls.extend(member_menu.nickname);
    // [insertion point: «Apps ›», the application commands for this person, right
    // after «Cambiar apodo» and before «Invitar al servidor ›»]
    if other {
        let invites: Vec<MenuEntry> = rail_servers(store)
            .into_iter()
            .filter(|found| Some(found.id) != server)
            .filter(|found| {
                store
                    .server_abilities(found.id)
                    .is_some_and(|abilities| abilities.create_invites.allowed())
                    && !invite_channels(store, found.id).is_empty()
            })
            .map(|found| {
                let id = format!("pm:{}:invite:{}", user.0, found.id.0);
                if ui.person_invited.contains(&(user, found.id)) {
                    MenuEntry::Inert(id, t("Invitación enviada").into())
                } else {
                    item(id, &found.name, None)
                }
            })
            .collect();
        if !invites.is_empty() {
            controls.push(MenuEntry::Submenu(t("Invitar al servidor").into(), invites));
        }
        let requests = store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.friend_requests);
        controls.extend(match kind {
            Some(RelationshipKind::Friend) if social.remove.allowed() => {
                Some(item(person("remove"), t("Eliminar amigo"), None))
            }
            None if requests => Some(item(person("add"), t("Añadir amigos"), None)),
            Some(RelationshipKind::IncomingRequest) if social.accept.allowed() => Some(item(
                person("accept"),
                t("Aceptar solicitud de amistad"),
                None,
            )),
            Some(RelationshipKind::OutgoingRequest) => Some(MenuEntry::Inert(
                person("sent"),
                t("Solicitud de amistad enviada").into(),
            )),
            _ => None,
        });
    }
    if other {
        // Ignoring (or not) sits right before blocking, as the official menu has it.
        controls.extend(crate::ignore::menu(store, user));
        if kind == Some(RelationshipKind::Blocked) {
            if social.unblock.allowed() {
                controls.push(item(person("unblock"), t("Desbloquear"), None));
            }
        } else if social.block.allowed() {
            controls.push(MenuEntry::Danger(person("block"), t("Bloquear").into()));
        }
    }
    if !other
        && store
            .provider
            .as_ref()
            .is_some_and(|info| info.capabilities.edit_profile)
    {
        controls.push(item(profile_ids::EDIT.into(), t("Editar perfil"), None));
    }
    groups.push(controls);

    // Their roles, and where they may be moved in the voice.
    let mut roles_moves: Vec<MenuEntry> = Vec::new();
    roles_moves.extend(member_menu.roles);
    roles_moves.extend(voice_menu.move_to);
    groups.push(roles_moves);

    // The moderation: in the voice, and on the server, only what the account may use.
    let mut moderation = voice_menu.moderation;
    moderation.extend(member_menu.moderation);
    groups.push(moderation);

    if ui.prefs.developer {
        groups.push(vec![item(
            format!("{}{}", ids::COPY_ID, user.0),
            t("Copiar ID del usuario"),
            None,
        )]);
    }
    let mut entries = Vec::new();
    for group in groups.into_iter().filter(|group| !group.is_empty()) {
        if !entries.is_empty() {
            entries.push(MenuEntry::Separator);
        }
        entries.extend(group);
    }
    entries
}

/// Whether the account can call `user`: they are not a bot, the provider has
/// private calls, and the account is not in a call with them already.
fn can_call(store: &Store, user: UserId) -> bool {
    let in_call_with_them = store
        .direct_with(user)
        .is_some_and(|channel| store.call().is_some_and(|call| call.channel == channel));
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.private_calls)
        && !store.user(user).is_some_and(|account| account.bot)
        && !in_call_with_them
}

/// The servers in the rail's order, folders flattened: the order in which the
/// invitations of a person's menu are offered.
fn rail_servers(store: &Store) -> Vec<&Server> {
    store
        .rail()
        .into_iter()
        .flat_map(|entry| match entry {
            RailEntry::Server(server) => vec![server],
            RailEntry::Folder { servers, .. } => servers,
        })
        .collect()
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

/// The person menu opened by a secondary click, and the question before a
/// call from one, over everything.
/// `room` is the window's height: a long menu grows with it, as Discord's.
pub(crate) fn overlays(ui: &Ui, store: &Store, root: Element, room: f32, look: &Look) -> Element {
    let mut root = root;
    if let Some(menu) = ui.person_menu {
        // A member list's menu goes once its server is left or the person is.
        let gone = match menu.place {
            Place::MemberList { server } => {
                ui.server != Some(server) || store.member_of(server, menu.user).is_none()
            }
            _ => false,
        };
        if !gone {
            root = Element::stack(vec![
                root,
                parts::context_menu(
                    menu.at,
                    entries(ui, store, menu.user, menu.place),
                    room,
                    look,
                ),
            ]);
        }
    }
    match ui.person_call {
        Some(call) => parts::call_question(root, call.dont_ask_again, look),
        None => root,
    }
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
/// a server; and the question before a call. `None` for the other ids.
pub(crate) fn action(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    match id.as_str() {
        ids::PERSON_CALL => return Some(confirm_call(ui, store, now)),
        ids::PERSON_CALL_CANCEL => {
            ui.person_call = None;
            return Some(Vec::new());
        }
        ids::PERSON_CALL_FORGET => {
            if let Some(call) = ui.person_call.as_mut() {
                call.dont_ask_again = !call.dont_ask_again;
            }
            return Some(Vec::new());
        }
        _ => {}
    }
    let (user, what) = id.strip_prefix("pm:")?.split_once(':')?;
    let user = UserId(user.parse().ok()?);
    Some(match what {
        "call" => call(ui, store, user, now),
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

/// A call from a person's menu: asked about first, unless the account chose
/// «No volver a preguntar».
fn call(ui: &mut Ui, store: &mut Store, user: UserId, now: Instant) -> Vec<Intent> {
    if ui.prefs.ask_before_call {
        ui.person_call = Some(PersonCall {
            user,
            dont_ask_again: false,
        });
        return Vec::new();
    }
    start_call(ui, store, user, now)
}

/// The question's «Iniciar llamada»: a «No volver a preguntar» ticked is kept,
/// and then the call starts.
fn confirm_call(ui: &mut Ui, store: &mut Store, now: Instant) -> Vec<Intent> {
    let Some(PersonCall {
        user,
        dont_ask_again,
    }) = ui.person_call.take()
    else {
        return Vec::new();
    };
    let mut intents = Vec::new();
    if dont_ask_again {
        intents.push(ui.prefs.set_ask_before_call(false));
    }
    intents.extend(start_call(ui, store, user, now));
    intents
}

/// Start the call with `user`. Their direct conversation is opened first when
/// there is none, and the call starts once it is there (see [`on_applied`]).
fn start_call(ui: &mut Ui, store: &mut Store, user: UserId, now: Instant) -> Vec<Intent> {
    if let Some(channel) = store.direct_with(user) {
        return private_calls::start(ui, store, channel, now);
    }
    let intents = conversations::open_direct(ui, store, user, now);
    ui.person_calling = ui
        .conversations
        .opening
        .filter(|(_, waiting)| *waiting == user)
        .map(|(request, _)| (user, request));
    intents
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
    ui.person_invited.insert((user, server));
    let mut intents = vec![Intent::Command(Command::CreateInvite {
        request,
        server,
        channel,
        max_age_secs: INVITE_AGE_SECS,
        max_uses: 1,
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

/// What the provider answered for a person's menu: the conversation a call
/// waited for, and the link of an invitation with its conversation.
pub(crate) fn on_applied(
    ui: &mut Ui,
    store: &mut Store,
    applied: &Applied,
    now: Instant,
) -> Vec<Intent> {
    let mut intents = calling_on_applied(ui, store, applied, now);
    intents.extend(inviting_on_applied(ui, store, applied, now));
    intents
}

/// A call waiting for its conversation starts once the provider opened it; a
/// conversation refused drops the call.
fn calling_on_applied(
    ui: &mut Ui,
    store: &mut Store,
    applied: &Applied,
    now: Instant,
) -> Vec<Intent> {
    let Some((_, waiting)) = ui.person_calling else {
        return Vec::new();
    };
    match applied {
        Applied::Created {
            request,
            created: Created::Channel(channel),
        } if *request == waiting => {
            ui.person_calling = None;
            private_calls::start(ui, store, *channel, now)
        }
        Applied::Failed {
            failure: Failure::Direct { request, .. },
            ..
        } if *request == waiting => {
            ui.person_calling = None;
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// The link of an invitation from a person's menu, and its conversation.
fn inviting_on_applied(ui: &mut Ui, store: &Store, applied: &Applied, now: Instant) -> Vec<Intent> {
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
