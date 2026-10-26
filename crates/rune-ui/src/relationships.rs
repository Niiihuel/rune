//! The friends page of home: its tab and search, the form that sends a
//! request by handle, the menu of a person and the question before removing
//! or blocking them. Its actions start with `friends:` or `friend:{user}:`.
//! Each is checked again against `Store::social` before it becomes a
//! command, and nothing changes here before the provider reports it.

use std::sync::Arc;
use std::time::Instant;

use limn::{Component, Element, Icon, ScrollArea, UiAction};
use rune_components::friends::{
    self as parts, ids, AddFriend, FriendAction, FriendRow, FriendsTab,
};
use rune_components::look::status_label;
use rune_components::{gap, Look};
use rune_i18n::{t, tr};
use rune_model::{Relationship, RelationshipKind, UserId};
use rune_provider::{Command, Failure, ProviderKind};
use rune_state::{Applied, Permit, Store};

use crate::{Intent, Ui};

/// The page's own state.
#[derive(Debug, Default)]
pub struct Friends {
    /// Home shows this page instead of a conversation.
    pub open: bool,
    pub tab: FriendsTab,
    pub query: String,
    pub username: String,
    /// The handle a request left for, until it is answered.
    sending: Option<String>,
    note: Option<(String, bool)>,
    /// The person whose "more" menu is open.
    menu: Option<UserId>,
    confirm: Option<Confirm>,
    /// The friend being given a private name, and the name written.
    nickname: Option<(UserId, String)>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Confirm {
    Remove(UserId),
    Block(UserId),
}

/// Show the page at home, over whatever conversation was open there.
pub(crate) fn show(ui: &mut Ui) -> Vec<Intent> {
    let mut intents = crate::search::dismiss(ui);
    intents.extend(crate::context::cancel(ui));
    ui.server = None;
    ui.voice.stage = None;
    ui.attachment_viewer = None;
    ui.friends.open = true;
    ui.friends.menu = None;
    intents
}

/// Whether the page is what home shows now.
pub(crate) fn visible(ui: &Ui) -> bool {
    ui.server.is_none() && ui.friends.open
}

/// Answer the page's actions; `None` when the action is not one of them.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    match action {
        UiAction::InputChanged { id, value } if id == ids::QUERY => {
            ui.friends.query = value.clone();
            Some(Vec::new())
        }
        UiAction::InputChanged { id, value } if id == ids::USERNAME => {
            ui.friends.username = value.clone();
            ui.friends.note = None;
            Some(Vec::new())
        }
        UiAction::Submitted { id, .. } if id == ids::USERNAME => Some(send_request(ui, store)),
        UiAction::InputChanged { id, value } if id == ids::NICKNAME => {
            if let Some((_, name)) = ui.friends.nickname.as_mut() {
                *name = value.clone();
            }
            Some(Vec::new())
        }
        UiAction::Submitted { id, .. } if id == ids::NICKNAME => {
            Some(save_nickname(ui, store, false))
        }
        UiAction::Clicked { id } => clicked(ui, store, id, now),
        _ => None,
    }
}

fn clicked(ui: &mut Ui, store: &mut Store, id: &str, now: Instant) -> Option<Vec<Intent>> {
    if id == ids::HOME {
        return Some(show(ui));
    }
    if let Some(tab) = id.strip_prefix(ids::TAB).and_then(FriendsTab::from_key) {
        ui.friends.tab = tab;
        ui.friends.menu = None;
        return Some(Vec::new());
    }
    match id {
        ids::SEND => return Some(send_request(ui, store)),
        ids::NICKNAME_SAVE => return Some(save_nickname(ui, store, false)),
        ids::NICKNAME_CLEAR => return Some(save_nickname(ui, store, true)),
        ids::NICKNAME_CLOSE => {
            ui.friends.nickname = None;
            return Some(Vec::new());
        }
        ids::MENU_DISMISS => {
            ui.friends.menu = None;
            return Some(Vec::new());
        }
        ids::CONFIRM_CANCEL => {
            ui.friends.confirm = None;
            return Some(Vec::new());
        }
        ids::CONFIRM => {
            let social = |user| store.social(user);
            return Some(match ui.friends.confirm.take() {
                Some(Confirm::Remove(user)) if social(user).remove.allowed() => {
                    command(Command::RemoveRelationship { user })
                }
                Some(Confirm::Block(user)) if social(user).block.allowed() => {
                    command(Command::BlockUser { user })
                }
                _ => Vec::new(),
            });
        }
        _ => {}
    }
    let (user, action) = id.strip_prefix("friend:")?.split_once(':')?;
    let user = UserId(user.parse().ok()?);
    let social = store.social(user);
    ui.friends.menu = None;
    // Also reached from someone's menu in a voice channel.
    ui.voice.menu = None;
    let allowed = |permit: &Permit, ui: &mut Ui| {
        if let Permit::Denied(reason) = permit {
            ui.notify(t("No se puede"), Some(reason.clone()), true, now);
        }
        permit.allowed()
    };
    Some(match action {
        "message" => crate::conversations::open_direct(ui, store, user, now),
        "add" if store.relationship(user).is_none() => store
            .user(user)
            .map(|found| {
                command(Command::SendFriendRequest {
                    username: found.name.clone(),
                })
            })
            .unwrap_or_default(),
        "more" => {
            ui.friends.menu = Some(user);
            Vec::new()
        }
        "accept" if allowed(&social.accept, ui) => command(Command::AcceptFriendRequest { user }),
        // Ignoring or cancelling a request takes nothing away: no question.
        "ignore" | "cancel" if allowed(&social.remove, ui) => {
            command(Command::RemoveRelationship { user })
        }
        "unblock" if allowed(&social.unblock, ui) => command(Command::RemoveRelationship { user }),
        "remove" if allowed(&social.remove, ui) => {
            ui.friends.confirm = Some(Confirm::Remove(user));
            Vec::new()
        }
        "block" if allowed(&social.block, ui) => {
            ui.friends.confirm = Some(Confirm::Block(user));
            Vec::new()
        }
        "nickname" if allowed(&social.nickname, ui) => {
            let name = store
                .relationship(user)
                .and_then(|known| known.nickname.clone())
                .unwrap_or_default();
            ui.friends.nickname = Some((user, name.clone()));
            vec![
                Intent::SetInput {
                    id: ids::NICKNAME.into(),
                    value: name,
                },
                Intent::Focus(ids::NICKNAME.into()),
            ]
        }
        _ => Vec::new(),
    })
}

/// Send the private name written for the friend of the dialog, or take it
/// away; the list shows it once the provider reports it.
fn save_nickname(ui: &mut Ui, store: &Store, clear: bool) -> Vec<Intent> {
    let Some((user, name)) = ui.friends.nickname.take() else {
        return Vec::new();
    };
    let name = name.trim();
    let nickname = (!clear && !name.is_empty()).then(|| name.to_owned());
    let current = store
        .relationship(user)
        .and_then(|known| known.nickname.clone());
    if nickname == current || !store.social(user).nickname.allowed() {
        return Vec::new();
    }
    command(Command::SetFriendNickname { user, nickname })
}

fn command(command: Command) -> Vec<Intent> {
    vec![Intent::Command(command)]
}

fn send_request(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let username = ui
        .friends
        .username
        .trim()
        .trim_start_matches('@')
        .to_owned();
    if username.is_empty() || ui.friends.sending.is_some() || !store.add_friend().allowed() {
        return Vec::new();
    }
    ui.friends.note = None;
    ui.friends.sending = Some(username.clone());
    command(Command::SendFriendRequest { username })
}

/// The form says its own failure; the corner does not repeat it.
pub(crate) fn shows_failure(ui: &Ui, failure: &Failure) -> bool {
    matches!(failure, Failure::Relationship { user: None }) && ui.friends.sending.is_some()
}

/// Answer what the provider reported about relationships.
pub(crate) fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied) {
    match applied {
        Applied::Relationship(user) => {
            let Some(relationship) = store.relationship(*user) else {
                if ui.friends.menu == Some(*user) {
                    ui.friends.menu = None;
                }
                return;
            };
            let Some(account) = store.user(*user) else {
                return;
            };
            let sent_to = ui
                .friends
                .sending
                .as_ref()
                .is_some_and(|handle| account.name.eq_ignore_ascii_case(handle));
            if sent_to {
                ui.friends.sending = None;
                ui.friends.username.clear();
                let name = account.shown_name();
                ui.friends.note = Some((
                    match relationship.kind {
                        RelationshipKind::Friend => tr!("Ahora eres amigo de {name}", name),
                        _ => tr!("Solicitud de amistad enviada a {name}", name),
                    },
                    false,
                ));
            }
        }
        Applied::Failed {
            failure: Failure::Relationship { user: None },
            reason,
            ..
        } if ui.friends.sending.is_some() => {
            ui.friends.sending = None;
            ui.friends.note = Some((reason.clone(), true));
        }
        Applied::Ready => ui.friends.sending = None,
        _ => {}
    }
}

/// One row of the page's list.
enum ListRow {
    Section(String, usize),
    Person(Box<FriendRow>),
}

/// The page, in the main area at home.
pub(crate) fn view(ui: &Ui, store: &Store, look: &Look) -> Element {
    let pending = store.pending_requests();
    let tab = ui.friends.tab;
    let body = if tab == FriendsTab::Add {
        let simulated = store
            .provider
            .as_ref()
            .is_some_and(|info| info.kind == ProviderKind::Simulated);
        Element::column(vec![
            parts::add_friend(
                &AddFriend {
                    username: ui.friends.username.clone(),
                    note: ui.friends.note.clone(),
                    denied: store.add_friend().reason().map(str::to_owned),
                    sending: ui.friends.sending.is_some(),
                    simulated,
                },
                look,
            ),
            Element::spacer().flex(1.0),
        ])
        .padding(20.0)
    } else {
        list(ui, store, tab, look)
    };
    Element::column(vec![
        parts::header(tab, pending, look),
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border),
        body.flex(1.0),
    ])
}

fn list(ui: &Ui, store: &Store, tab: FriendsTab, look: &Look) -> Element {
    let presence = store.friends_presence();
    let relationships = match (store.friends_list(), store.relationships()) {
        (Permit::Allowed, Some(list)) => list,
        (permit, _) => {
            return parts::empty(
                t("Amigos no disponibles"),
                permit.reason().unwrap_or_default(),
                look,
            )
        }
    };
    if tab == FriendsTab::Online && !presence {
        return parts::empty(
            t("Sin estado en línea"),
            t("Esta conexión no informa quién está en línea. Mira «Todos»."),
            look,
        );
    }
    let query = ui.friends.query.trim().to_lowercase();
    let matches = |known: &&Relationship| {
        let user = store.user(known.user);
        query.is_empty()
            || [
                user.map(|user| user.name.as_str()),
                user.and_then(|user| user.display_name.as_deref()),
                known.nickname.as_deref(),
            ]
            .into_iter()
            .flatten()
            .any(|name| name.to_lowercase().contains(&query))
    };
    let of_kind = |kinds: &[RelationshipKind], online: bool| -> Vec<&Relationship> {
        relationships
            .iter()
            .copied()
            .filter(|known| kinds.contains(&known.kind) && !known.spam)
            .filter(|known| !online || store.presence(known.user).status.is_around())
            .filter(matches)
            .collect()
    };
    let spam: Vec<&Relationship> = relationships
        .iter()
        .copied()
        .filter(|known| known.spam && known.kind == RelationshipKind::IncomingRequest)
        .filter(matches)
        .collect();
    let sections: Vec<(&str, Vec<&Relationship>)> = match tab {
        FriendsTab::Online => vec![(t("En línea"), of_kind(&[RelationshipKind::Friend], true))],
        FriendsTab::All => vec![(
            t("Todos los amigos"),
            of_kind(&[RelationshipKind::Friend], false),
        )],
        FriendsTab::Pending => vec![
            (
                t("Recibidas"),
                of_kind(&[RelationshipKind::IncomingRequest], false),
            ),
            (
                t("Enviadas"),
                of_kind(&[RelationshipKind::OutgoingRequest], false),
            ),
            // The service's spam folder: apart, and not counted as waiting.
            (t("Posible spam"), spam),
        ],
        FriendsTab::Blocked | FriendsTab::Add => {
            vec![(
                t("Bloqueados"),
                of_kind(&[RelationshipKind::Blocked], false),
            )]
        }
    };
    let search = Element::row(vec![
        gap(20.0, 0.0),
        parts::search(&ui.friends.query, look).flex(1.0),
        gap(20.0, 0.0),
    ]);
    if sections.iter().all(|(_, people)| people.is_empty()) {
        let (title, detail) = if !query.is_empty() {
            (t("Nadie coincide"), t("Prueba con otro nombre."))
        } else {
            match tab {
                FriendsTab::Online => (
                    t("Nadie está en línea"),
                    t("Ninguno de tus amigos está conectado ahora."),
                ),
                FriendsTab::Pending => (
                    t("No hay solicitudes pendientes"),
                    t("Las solicitudes que recibas o envíes aparecen aquí."),
                ),
                FriendsTab::Blocked => (
                    t("No bloqueaste a nadie"),
                    t("Las personas que bloquees aparecen aquí."),
                ),
                _ => (
                    t("Todavía no tienes amigos"),
                    t("Añade a alguien con su nombre de usuario."),
                ),
            }
        };
        return Element::column(vec![
            gap(0.0, 16.0),
            search,
            parts::empty(title, detail, look).flex(1.0),
        ]);
    }
    let mut rows = Vec::new();
    for (title, people) in sections {
        if people.is_empty() {
            continue;
        }
        rows.push((
            u64::MAX - rows.len() as u64,
            ListRow::Section(title.into(), people.len()),
        ));
        for known in people {
            rows.push((
                known.user.0,
                ListRow::Person(Box::new(row_of(ui, store, known, presence, look))),
            ));
        }
    }
    let rows = Arc::new(rows);
    let (keys, look) = (rows.clone(), *look);
    let list = ScrollArea::measured_list(
        "friends:list",
        rows.len(),
        parts::ROW,
        move |index| keys[index].0,
        move |index| {
            let row = match &rows[index].1 {
                ListRow::Section(title, count) => parts::section(title, *count, &look),
                ListRow::Person(row) => parts::row(row, &look),
            };
            Element::row(vec![gap(20.0, 0.0), row.flex(1.0), gap(20.0, 0.0)])
        },
    )
    .appearance(look.scrollbar(true))
    .render()
    .flex(1.0);
    Element::column(vec![gap(0.0, 16.0), search, list])
}

fn row_of(ui: &Ui, store: &Store, known: &Relationship, presence: bool, look: &Look) -> FriendRow {
    let user = known.user;
    let account = store.user(user);
    let status = store.presence(user);
    let social = store.social(user);
    let action =
        |action: &str, label: &str, icon: Icon, permit: &Permit, danger: bool| FriendAction {
            id: ids::person(user, action),
            label: label.into(),
            icon,
            denied: permit.reason().map(str::to_owned),
            danger,
        };
    let block = action("block", t("Bloquear"), Icon::Lock, &social.block, true);
    // Their custom status's server emoji, before its words (2026-10-18).
    let (shown, detail_emoji) = if presence && known.kind == RelationshipKind::Friend {
        crate::identity::status_line(ui, &status)
    } else {
        (None, None)
    };
    let (detail, actions, more) = match known.kind {
        RelationshipKind::Friend => (
            if presence {
                // An emoji alone says enough: no status word after it.
                shown.unwrap_or_else(|| {
                    if detail_emoji.is_some() {
                        String::new()
                    } else {
                        status_label(status.status).to_owned()
                    }
                })
            } else {
                t("Amistad").to_owned()
            },
            vec![action(
                "message",
                t("Mensaje"),
                Icon::Chat,
                &social.message,
                false,
            )],
            vec![
                action(
                    "nickname",
                    t("Cambiar apodo"),
                    Icon::Edit,
                    &social.nickname,
                    false,
                ),
                action(
                    "remove",
                    t("Eliminar amigo"),
                    Icon::Trash,
                    &social.remove,
                    true,
                ),
                block,
            ],
        ),
        RelationshipKind::IncomingRequest => (
            if known.spam {
                t("Solicitud marcada como posible spam").to_owned()
            } else {
                t("Solicitud recibida").to_owned()
            },
            vec![
                action("accept", t("Aceptar"), Icon::Check, &social.accept, false),
                action("ignore", t("Ignorar"), Icon::Close, &social.remove, false),
            ],
            vec![block],
        ),
        RelationshipKind::OutgoingRequest => (
            t("Solicitud enviada").to_owned(),
            vec![action(
                "cancel",
                t("Cancelar"),
                Icon::Close,
                &social.remove,
                false,
            )],
            vec![block],
        ),
        RelationshipKind::Blocked => (
            t("Bloqueado").to_owned(),
            vec![action(
                "unblock",
                t("Desbloquear"),
                Icon::Close,
                &social.unblock,
                false,
            )],
            Vec::new(),
        ),
    };
    FriendRow {
        name: known
            .nickname
            .clone()
            .or_else(|| account.map(|account| account.shown_name().to_owned()))
            .unwrap_or_else(|| t("Usuario desconocido").into()),
        handle: account.map_or_else(|| user.to_string(), |account| account.name.clone()),
        image: account.and_then(|account| ui.picture(account.picture())),
        accent: account.map_or(look.faint, |account| look.ink(account.accent)),
        status: (presence && known.kind == RelationshipKind::Friend)
            .then(|| look.status(status.status)),
        detail,
        detail_emoji,
        actions,
        more,
        more_id: ids::person(user, "more"),
        menu_open: ui.friends.menu == Some(user),
    }
}

/// The question before removing or blocking, and the nickname dialog, over `root`.
pub(crate) fn overlay(ui: &Ui, store: &Store, root: Element, look: &Look) -> Element {
    let root = match &ui.friends.nickname {
        Some((user, name)) => parts::nickname_dialog(
            root,
            &store.user(*user).map_or_else(
                || t("esta persona").to_owned(),
                |user| user.shown_name().to_owned(),
            ),
            name,
            look,
        ),
        None => root,
    };
    let Some(confirm) = ui.friends.confirm else {
        return root;
    };
    let name = |user: UserId| {
        store.user(user).map_or_else(
            || t("esta persona").to_owned(),
            |user| user.shown_name().to_owned(),
        )
    };
    match confirm {
        Confirm::Remove(user) => parts::confirm(
            root,
            &tr!("Eliminar a «{name}»", name = name(user)),
            &tr!(
                "¿Seguro que quieres eliminar a {name} de tus amigos? Vas a poder enviarle otra solicitud.",
                name = name(user)
            ),
            t("Eliminar amigo"),
            look,
        ),
        Confirm::Block(user) => parts::confirm(
            root,
            &tr!("Bloquear a «{name}»", name = name(user)),
            t("Termina cualquier amistad o solicitud con esta persona, y no vas a poder escribirle ni reaccionar en su conversación directa hasta que la desbloquees."),
            t("Bloquear"),
            look,
        ),
    }
}
