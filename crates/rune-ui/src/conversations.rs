//! Conversations outside servers: opening the direct one with someone,
//! making a group with chosen friends, managing a group (its picture, name
//! and people) and closing a direct conversation. A new conversation opens
//! only once the provider names its channel: nothing is made up here, asking
//! again while waiting sends nothing more, and the provider answers a repeat
//! with the same channel. Changes show when the provider reports the channel
//! again. Messages of people the account blocked fold here too, as the list
//! of a conversation shows them. Actions start with `group:` or `blocked:`.

use std::collections::HashSet;
use std::time::Instant;

use limn::{ButtonVariant, Component, Element, Icon, IconButton, UiAction};
use rune_components::channel_menu::{Entry, Item};
use rune_components::group::{self as parts, ids, Choice, GroupForm, GroupPerson, NewGroupForm};
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::{ChannelId, ChannelKind, MessageId, Timestamp, UserId};
use rune_provider::{Command, Created, Failure, ProviderKind, RequestId};
use rune_state::{Applied, Permit, Store, GROUP_OTHERS};

use crate::{open, Intent, Ui};

#[derive(Debug, Default)]
pub struct Conversations {
    /// The direct conversation asked of the provider, until it names it.
    pub(crate) opening: Option<(RequestId, UserId)>,
    group: Option<GroupDraft>,
    confirm: Option<GroupConfirm>,
    /// The dialog that makes a group, while open.
    creating: Option<NewGroup>,
    /// The menu of a conversation of the list, and where it opened.
    menu: Option<(ChannelId, (f32, f32))>,
    /// Runs of blocked messages shown anyway, by their first message.
    pub(crate) blocked_shown: HashSet<MessageId>,
}

#[derive(Debug)]
struct GroupDraft {
    channel: ChannelId,
    name: String,
}

#[derive(Debug, Default)]
struct NewGroup {
    chosen: Vec<UserId>,
    query: String,
    /// The group asked of the provider, until it names it.
    waiting: Option<RequestId>,
}

#[derive(Clone, Copy, Debug)]
enum GroupConfirm {
    Remove(ChannelId, UserId),
    Leave(ChannelId),
    Close(ChannelId),
    Owner(ChannelId, UserId),
}

/// Open the direct conversation with `user`, asking the provider for it when
/// there is none yet.
pub(crate) fn open_direct(
    ui: &mut Ui,
    store: &mut Store,
    user: UserId,
    now: Instant,
) -> Vec<Intent> {
    if let Some(channel) = store.direct_with(user) {
        return open(ui, store, None, channel);
    }
    if let Permit::Denied(reason) = store.open_direct(user) {
        ui.notify(
            t("No se pudo abrir la conversación"),
            Some(reason),
            true,
            now,
        );
        return Vec::new();
    }
    if ui
        .conversations
        .opening
        .is_some_and(|(_, waiting)| waiting == user)
    {
        return Vec::new();
    }
    let request = ui.fresh_request();
    ui.conversations.opening = Some((request, user));
    vec![Intent::Command(Command::OpenDirect { request, user })]
}

/// Answer what the provider reported about conversations.
pub(crate) fn on_applied(ui: &mut Ui, store: &mut Store, applied: &Applied) -> Vec<Intent> {
    match applied {
        Applied::Created {
            request,
            created: Created::Channel(channel),
        } => {
            let made = ui
                .conversations
                .creating
                .as_ref()
                .is_some_and(|new| new.waiting == Some(*request));
            if made {
                ui.conversations.creating = None;
                return if store
                    .channel(*channel)
                    .is_some_and(|found| found.kind == ChannelKind::Group)
                {
                    open(ui, store, None, *channel)
                } else {
                    Vec::new()
                };
            }
            let Some((_, user)) = ui
                .conversations
                .opening
                .filter(|(waiting, _)| waiting == request)
            else {
                return Vec::new();
            };
            ui.conversations.opening = None;
            // Only the conversation the provider reported, and only if it is
            // the one with that person.
            if store.direct_with(user) == Some(*channel) {
                return open(ui, store, None, *channel);
            }
            Vec::new()
        }
        Applied::Failed {
            failure: Failure::Direct { request, .. } | Failure::NewGroup { request },
            ..
        } => {
            if ui
                .conversations
                .opening
                .is_some_and(|(waiting, _)| waiting == *request)
            {
                ui.conversations.opening = None;
            }
            // The dialog stays, to try again; the corner says why.
            if let Some(new) = ui
                .conversations
                .creating
                .as_mut()
                .filter(|new| new.waiting == Some(*request))
            {
                new.waiting = None;
            }
            Vec::new()
        }
        Applied::ChannelRemoved(channel) => {
            if ui
                .conversations
                .group
                .as_ref()
                .is_some_and(|draft| draft.channel == *channel)
            {
                ui.conversations.group = None;
            }
            if matches!(ui.conversations.confirm,
                Some(GroupConfirm::Remove(of, _) | GroupConfirm::Leave(of) | GroupConfirm::Close(of)
                    | GroupConfirm::Owner(of, _))
                    if of == *channel)
            {
                ui.conversations.confirm = None;
            }
            if ui.conversations.menu.is_some_and(|(of, _)| of == *channel) {
                ui.conversations.menu = None;
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// A secondary click on a conversation of the home list opens its menu.
pub(crate) fn secondary_click(
    ui: &mut Ui,
    store: &Store,
    target: &str,
    at: limn::Point,
) -> Option<Vec<Intent>> {
    if ui.server.is_some() {
        return None;
    }
    let channel = ChannelId(target.strip_prefix("channel:")?.parse().ok()?);
    store
        .channel(channel)
        .filter(|found| found.server.is_none())?;
    ui.conversations.menu = Some((channel, (at.x, at.y)));
    Some(Vec::new())
}

/// Choosing anything closes the menu of a conversation of the list.
pub(crate) fn close_menu(ui: &mut Ui) {
    ui.conversations.menu = None;
}

/// Answer the group dialogs' actions; `None` when the action is not one.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let (id, value) = match action {
        UiAction::Clicked { id } => (id, None),
        UiAction::InputChanged { id, value } => (id, Some(value)),
        UiAction::Submitted { id, .. } => (id, None),
        _ => return None,
    };
    if let Some(intents) = new_group(ui, store, id, value, now) {
        return Some(intents);
    }
    if let Some(first) = id
        .strip_prefix("blocked:")
        .and_then(|first| first.parse().ok())
    {
        let shown = &mut ui.conversations.blocked_shown;
        if !shown.remove(&MessageId(first)) {
            shown.insert(MessageId(first));
        }
        return Some(Vec::new());
    }
    match id.as_str() {
        ids::CLOSE => {
            ui.conversations.group = None;
            return Some(Vec::new());
        }
        ids::CONFIRM_CANCEL => {
            ui.conversations.confirm = None;
            return Some(Vec::new());
        }
        ids::CONFIRM => {
            let confirm = ui.conversations.confirm.take();
            return Some(match confirm {
                Some(GroupConfirm::Remove(channel, user))
                    if permit(store, channel, |a| &a.remove) =>
                {
                    vec![Intent::Command(Command::RemoveGroupMember {
                        channel,
                        user,
                    })]
                }
                Some(GroupConfirm::Leave(channel)) if permit(store, channel, |a| &a.leave) => {
                    ui.conversations.group = None;
                    vec![Intent::Command(Command::LeaveGroup { channel })]
                }
                Some(GroupConfirm::Close(channel)) if store.close_direct(channel).allowed() => {
                    vec![Intent::Command(Command::CloseDirect { channel })]
                }
                Some(GroupConfirm::Owner(channel, user))
                    if permit(store, channel, |a| &a.owner) =>
                {
                    vec![Intent::Command(Command::SetGroupOwner { channel, user })]
                }
                _ => Vec::new(),
            });
        }
        _ => {}
    }
    let (channel, rest) = id.strip_prefix("group:")?.split_once(':')?;
    let channel = ChannelId(channel.parse().ok()?);
    let refuse = |ui: &mut Ui, permit: &Permit| {
        if let Permit::Denied(reason) = permit {
            ui.notify(t("No se puede"), Some(reason.clone()), true, now);
        }
        Some(Vec::new())
    };
    // Desktop (P6).
    if rest == "pin" {
        ui.conversations.menu = None;
        return Some(crate::desktop::toggle_pinned(ui, channel));
    }
    // Rail and notifications: silenced until turned back on, as Serein.
    if rest == "mute" {
        ui.conversations.menu = None;
        let until = (!muted(ui, store, channel, crate::server_menu::wall_now()))
            .then_some(rune_model::MUTED_FOREVER);
        let change = rune_model::NotificationChange::ChannelMute(channel, until);
        return Some(crate::channel_list::change_notify(
            ui, store, channel, change, now,
        ));
    }
    if rest == "close" {
        let permit = store.close_direct(channel);
        if !permit.allowed() {
            return refuse(ui, &permit);
        }
        ui.conversations.confirm = Some(GroupConfirm::Close(channel));
        return Some(Vec::new());
    }
    let abilities = store.group_abilities(channel)?;
    let user = |prefix: &str| {
        rest.strip_prefix(prefix)
            .and_then(|user| user.parse().ok())
            .map(UserId)
    };
    if let Some(value) = value {
        if rest == "name" {
            if let Some(draft) = ui.conversations.group.as_mut() {
                draft.name = value.clone();
            }
        }
        return Some(Vec::new());
    }
    if rest == "manage" {
        let name = store
            .channel(channel)
            .map(|found| found.name.clone())
            .unwrap_or_default();
        ui.conversations.group = Some(GroupDraft {
            channel,
            name: name.clone(),
        });
        return Some(vec![Intent::SetInput {
            id: ids::of(channel, "name"),
            value: name,
        }]);
    }
    if rest == "icon" || rest == "icon:remove" {
        if !abilities.icon.allowed() {
            return refuse(ui, &abilities.icon);
        }
        // The picture is chosen and prepared away from the interface.
        return Some(vec![if rest == "icon" {
            Intent::PickGroupIcon { channel }
        } else {
            Intent::Command(Command::SetGroupIcon {
                channel,
                icon: None,
            })
        }]);
    }
    if rest == "rename" || rest == "name" {
        if !abilities.rename.allowed() {
            return refuse(ui, &abilities.rename);
        }
        let name = ui
            .conversations
            .group
            .as_ref()
            .filter(|draft| draft.channel == channel)?
            .name
            .trim()
            .to_owned();
        if store
            .channel(channel)
            .is_some_and(|found| found.name == name)
        {
            return Some(Vec::new());
        }
        return Some(vec![Intent::Command(Command::RenameGroup {
            channel,
            name,
        })]);
    }
    if rest == "leave" {
        if !abilities.leave.allowed() {
            return refuse(ui, &abilities.leave);
        }
        ui.conversations.confirm = Some(GroupConfirm::Leave(channel));
        return Some(Vec::new());
    }
    if let Some(user) = user("add:") {
        if !abilities.add.allowed() {
            return refuse(ui, &abilities.add);
        }
        return Some(vec![Intent::Command(Command::AddGroupMember {
            channel,
            user,
        })]);
    }
    if let Some(user) = user("remove:") {
        if !abilities.remove.allowed() {
            return refuse(ui, &abilities.remove);
        }
        ui.conversations.confirm = Some(GroupConfirm::Remove(channel, user));
        return Some(Vec::new());
    }
    if let Some(user) = user("owner:") {
        if !abilities.owner.allowed() {
            return refuse(ui, &abilities.owner);
        }
        ui.conversations.confirm = Some(GroupConfirm::Owner(channel, user));
        return Some(Vec::new());
    }
    Some(Vec::new())
}

/// The dialog that makes a group: open, search, choose, make, close.
fn new_group(
    ui: &mut Ui,
    store: &mut Store,
    id: &str,
    value: Option<&String>,
    now: Instant,
) -> Option<Vec<Intent>> {
    match id {
        ids::NEW => {
            if let Permit::Denied(reason) = store.create_group() {
                ui.notify(t("No se puede"), Some(reason), true, now);
            } else {
                ui.conversations.creating = Some(NewGroup::default());
            }
            return Some(Vec::new());
        }
        ids::NEW_CLOSE => {
            ui.conversations.creating = None;
            return Some(Vec::new());
        }
        _ => {}
    }
    let new = ui.conversations.creating.as_mut()?;
    if id == ids::NEW_QUERY {
        if let Some(value) = value {
            new.query = value.clone();
        }
        return Some(Vec::new());
    }
    if let Some(user) = id
        .strip_prefix(ids::NEW_PICK)
        .and_then(|user| user.parse().ok())
        .map(UserId)
    {
        if let Some(at) = new.chosen.iter().position(|known| *known == user) {
            new.chosen.remove(at);
        } else if new.chosen.len() < GROUP_OTHERS && store.friends().contains(&user) {
            new.chosen.push(user);
        }
        return Some(Vec::new());
    }
    if id != ids::NEW_CREATE {
        return None;
    }
    // A second press while the provider answers sends nothing more.
    if value.is_some() || new.waiting.is_some() {
        return Some(Vec::new());
    }
    match new.chosen[..] {
        [] => Some(Vec::new()),
        // With one friend, their direct conversation, as the official client does.
        [user] => {
            ui.conversations.creating = None;
            Some(open_direct(ui, store, user, now))
        }
        _ => {
            if let Permit::Denied(reason) = store.create_group() {
                ui.notify(t("No se puede"), Some(reason), true, now);
                return Some(Vec::new());
            }
            let users = new.chosen.clone();
            let request = ui.fresh_request();
            if let Some(new) = ui.conversations.creating.as_mut() {
                new.waiting = Some(request);
            }
            Some(vec![Intent::Command(Command::CreateGroup {
                request,
                users,
            })])
        }
    }
}

fn permit(
    store: &Store,
    channel: ChannelId,
    which: impl Fn(&rune_state::GroupAbilities) -> &Permit,
) -> bool {
    store
        .group_abilities(channel)
        .is_some_and(|abilities| which(&abilities).allowed())
}

/// The button of a group's header that opens its dialog.
pub(crate) fn header_tool(store: &Store, channel: ChannelId, look: &Look) -> Option<Element> {
    store.group_abilities(channel)?;
    Some(
        IconButton::new(ids::of(channel, "manage"), Icon::Users, look.menu_palette())
            .variant(ButtonVariant::Ghost)
            .tooltip(t("Gestionar grupo"))
            .accessibility_label(t("Gestionar grupo"))
            .ink(look.muted)
            .render(),
    )
}

/// The dialogs, menus and questions of conversations, over `root`.
pub(crate) fn overlay(ui: &Ui, store: &Store, root: Element, look: &Look) -> Element {
    let name = |user: UserId| store.display_name(user, None);
    let mut root = match ui.conversations.group.as_ref() {
        Some(draft) => match form(ui, store, draft, look) {
            Some(form) => parts::dialog(root, &form, look),
            None => root,
        },
        None => root,
    };
    if let Some(new) = &ui.conversations.creating {
        root = parts::create_dialog(root, &new_group_form(ui, store, new, look), look);
    }
    if let Some((channel, at)) = ui.conversations.menu {
        let entries = menu(ui, store, channel);
        if !entries.is_empty() {
            root = Element::stack(vec![
                root,
                rune_components::channel_menu::context_menu(at, entries, look),
            ]);
        }
    }
    match ui.conversations.confirm {
        Some(GroupConfirm::Remove(_, user)) => parts::confirm(
            root,
            &tr!("Quitar a «{name}»", name = name(user)),
            t("Deja de ver la conversación del grupo. Podés volver a añadirle si es tu amigo."),
            t("Quitar del grupo"),
            look,
        ),
        Some(GroupConfirm::Leave(channel)) => parts::confirm(
            root,
            &tr!("Abandonar «{group}»", group = store.channel_title(channel)),
            t("Dejás de ver sus mensajes. Para volver, alguien del grupo tiene que añadirte otra vez."),
            t("Abandonar grupo"),
            look,
        ),
        Some(GroupConfirm::Owner(channel, user)) => parts::confirm(
            root,
            &tr!(
                "Pasar «{group}» a «{name}»",
                group = store.channel_title(channel),
                name = name(user)
            ),
            t("Pasa a ser dueño del grupo y vos dejás de serlo: ya no vas a poder quitar personas ni pasarlo de nuevo."),
            t("Pasar propiedad"),
            look,
        ),
        Some(GroupConfirm::Close(channel)) => parts::confirm(
            root,
            &tr!(
                "Cerrar la conversación con «{name}»",
                name = store.channel_title(channel)
            ),
            t("Sale de tu lista, sin borrar nada: vuelve con su historial cuando te escriban o la abras otra vez."),
            t("Cerrar conversación"),
            look,
        ),
        None => root,
    }
}

/// What the menu of a conversation of the list offers.
fn menu(ui: &Ui, store: &Store, channel: ChannelId) -> Vec<Entry> {
    let item = |action: &str, label: &str, icon: Icon, permit: Permit, danger: bool| {
        let mut item = Item::new(ids::of(channel, action), label);
        item.icon = Some(icon);
        item.denied = permit.reason().map(str::to_owned);
        item.danger = danger;
        Entry::Item(item)
    };
    // Desktop (P6): pinned on top of the list, on this device.
    let pin = item(
        "pin",
        if ui.prefs.pinned.contains(&channel) {
            t("Desfijar de la lista")
        } else {
            t("Fijar en la lista")
        },
        Icon::Pin,
        Permit::Allowed,
        false,
    );
    // Rail and notifications: whoever keeps the notifications may change it.
    let silenced = muted(ui, store, channel, crate::server_menu::wall_now());
    let mute = item(
        "mute",
        if silenced {
            t("Desactivar silencio")
        } else {
            t("Silenciar conversación")
        },
        if silenced {
            Icon::Bell
        } else {
            Icon::BellSlash
        },
        if store.synced_notifications() {
            store.account_abilities().notifications
        } else {
            Permit::Allowed
        },
        false,
    );
    let mut entries = match store.channel(channel).map(|found| found.kind) {
        Some(ChannelKind::Group) => store.group_abilities(channel).map_or_else(Vec::new, |can| {
            vec![
                item(
                    "manage",
                    t("Gestionar grupo"),
                    Icon::Users,
                    Permit::Allowed,
                    false,
                ),
                Entry::Separator,
                item(
                    "leave",
                    t("Abandonar grupo"),
                    Icon::ArrowLeft,
                    can.leave,
                    true,
                ),
            ]
        }),
        Some(ChannelKind::Direct) => vec![item(
            "close",
            t("Cerrar conversación"),
            Icon::Close,
            store.close_direct(channel),
            false,
        )],
        _ => return Vec::new(),
    };
    if entries.is_empty() {
        return vec![pin, mute];
    }
    entries.splice(0..0, [pin, mute, Entry::Separator]);
    entries
}

/// Whether the direct or group conversation `channel` is muted at `wall`.
pub(crate) fn muted(ui: &Ui, store: &Store, channel: ChannelId, wall: Timestamp) -> bool {
    crate::account::channel_notify(ui, store, channel).muted(wall)
}

/// What the home's mark says: whether a direct or group conversation that
/// is not muted waits, and how many do.
pub(crate) fn directs_unread(ui: &Ui, store: &Store, wall: Timestamp) -> (bool, u32) {
    let waiting = store
        .directs()
        .into_iter()
        .filter(|channel| store.read(channel.id).unread() && !muted(ui, store, channel.id, wall))
        .count() as u32;
    (waiting > 0, waiting)
}

fn new_group_form(ui: &Ui, store: &Store, new: &NewGroup, look: &Look) -> NewGroupForm {
    let query = new.query.trim().to_lowercase();
    let friends: Vec<Choice> = store
        .friends()
        .into_iter()
        .filter_map(|user| {
            let account = store.user(user);
            let name = store.display_name(user, None);
            let handle = account.map_or_else(String::new, |account| account.name.clone());
            let matches = query.is_empty()
                || name.to_lowercase().contains(&query)
                || handle.to_lowercase().contains(&query);
            matches.then(|| Choice {
                user: user.0,
                name,
                handle,
                image: account.and_then(|account| ui.picture(account.picture())),
                accent: account.map_or(look.faint, |account| look.ink(account.accent)),
                chosen: new.chosen.contains(&user),
            })
        })
        .collect();
    let note = match store.friends_list() {
        Permit::Denied(reason) => Some(reason),
        Permit::Allowed if friends.is_empty() && !query.is_empty() => {
            Some(t("Ningún amigo coincide.").into())
        }
        Permit::Allowed if friends.is_empty() => Some(t("Todavía no tenés amigos.").into()),
        Permit::Allowed => None,
    };
    NewGroupForm {
        query: new.query.clone(),
        friends,
        chosen: new.chosen.len(),
        most: GROUP_OTHERS,
        note,
        denied: if new.waiting.is_some() {
            Some(t("Esperando al proveedor…").into())
        } else if new.chosen.len() > 1 {
            store.create_group().reason().map(str::to_owned)
        } else {
            None
        },
    }
}

fn form(ui: &Ui, store: &Store, draft: &GroupDraft, look: &Look) -> Option<GroupForm> {
    let channel = store.channel(draft.channel)?;
    let abilities = store.group_abilities(draft.channel)?;
    let person = |user: UserId, actions: Vec<(String, String, Option<String>)>| {
        let account = store.user(user);
        GroupPerson {
            name: store.display_name(user, None),
            handle: account.map_or_else(String::new, |account| account.name.clone()),
            image: account.and_then(|account| ui.picture(account.picture())),
            accent: account.map_or(look.faint, |account| look.ink(account.accent)),
            owner: channel.owner == Some(user),
            actions,
        }
    };
    let members = store
        .direct_members(draft.channel)
        .into_iter()
        .map(|user| {
            let mut actions = Vec::new();
            if Some(user) != store.me() {
                actions.push((
                    ids::of(draft.channel, &format!("remove:{user}")),
                    t("Quitar").to_owned(),
                    abilities.remove.reason().map(str::to_owned),
                ));
                // Only who owns it is offered to hand it over.
                if channel.owner.is_some() && channel.owner == store.me() {
                    actions.push((
                        ids::of(draft.channel, &format!("owner:{user}")),
                        t("Hacer dueño").to_owned(),
                        abilities.owner.reason().map(str::to_owned),
                    ));
                }
            }
            person(user, actions)
        })
        .collect();
    let candidates: Vec<GroupPerson> = store
        .group_candidates(draft.channel)
        .into_iter()
        .map(|user| {
            person(
                user,
                vec![(
                    ids::of(draft.channel, &format!("add:{user}")),
                    t("Añadir").to_owned(),
                    abilities.add.reason().map(str::to_owned),
                )],
            )
        })
        .collect();
    let candidates_note = match store.friends_list() {
        Permit::Denied(reason) => Some(reason),
        Permit::Allowed if candidates.is_empty() => {
            Some(t("Todos tus amigos ya están en el grupo.").to_owned())
        }
        Permit::Allowed => None,
    };
    let simulated = store
        .provider
        .as_ref()
        .is_some_and(|info| info.kind == ProviderKind::Simulated);
    Some(GroupForm {
        channel: draft.channel.0,
        title: store.channel_title(draft.channel),
        name: draft.name.clone(),
        rename_denied: abilities.rename.reason().map(str::to_owned),
        can_save: draft.name.trim() != channel.name,
        members,
        candidates,
        candidates_note,
        leave_denied: abilities.leave.reason().map(str::to_owned),
        icon: ui.image(&channel.icon),
        accent: look.accent,
        has_icon: channel.icon.is_some(),
        icon_denied: abilities.icon.reason().map(str::to_owned),
        icon_note: simulated
            .then(|| t("Simulación local: la imagen se acepta, pero no se dibuja.").to_owned()),
    })
}
