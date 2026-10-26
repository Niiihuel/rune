//! What the account keeps with its provider, as the window shows and
//! changes it (P1): how servers and channels notify when the provider keeps
//! that instead of this machine, the folders of the rail and their menus,
//! the messaging privacy in the settings, and the private note on each
//! person in their profile.
//!
//! Nothing here changes the domain ahead of the provider: a change is asked
//! for, and shows once the provider reports it.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use limn::{
    Component, ControlSize, DropdownOption, Element, Point, Select, SelectVariant, UiAction,
};
use rune_components::channel_menu::{self as menu_parts, grouped, Entry, Item};
use rune_components::folders::{self as folder_parts, FolderForm};
use rune_components::profile::NoteField;
use rune_components::rail::{Placement, RailEntry, RailFolder, RailItem};
use rune_components::user_settings as setting_parts;
use rune_components::{line, Look};
use rune_i18n::{t, tr};
use rune_model::{
    account::{valid_folders, valid_note},
    ChannelId, ContentFilter, FriendSources, MessagingPrivacy, NotificationChange, PrivacyChange,
    Rgb, Server, ServerFolder, ServerId, SpamFilter, UserId,
};
use rune_provider::Command;
use rune_state::{Applied, Permit, Store};

use crate::channel_list::ChannelNotify;
use crate::server_menu::{Notify, NotifyLevel};
use crate::{Intent, Ui, ViewContext};

/// A folder's color until one is chosen: the service's own blue.
const FOLDER_COLOR: Rgb = Rgb(0x58_65_F2);
/// The folders open on the rail, kept for the account between runs.
pub(crate) const OPEN_FOLDERS_SETTING: &str = "open_folders";
const DIALOG_WIDTH: f32 = 440.0;

/// The window's state of what the account keeps.
#[derive(Debug, Default)]
pub struct AccountUi {
    /// The folders open on the rail, by id. On this machine only, kept
    /// for the account ([`OPEN_FOLDERS_SETTING`]).
    pub open_folders: HashSet<u64>,
    /// The rail's menu: on a server or a folder, and its corner.
    pub menu: Option<(RailTarget, (f32, f32))>,
    /// The folder being named and colored.
    pub dialog: Option<FolderDialog>,
    /// Notes being written, by person.
    pub notes: HashMap<UserId, String>,
    /// People whose note was asked for already.
    notes_asked: HashSet<UserId>,
    /// People whose note could not be read: their card says so, instead of
    /// waiting for good, and asks again when it opens next (2026-10-17).
    notes_failed: HashSet<UserId>,
    // Rail and notifications.
    /// A server or a folder being dragged on the rail.
    pub(crate) drag: Option<crate::rail::Drag>,
}

/// What the rail's menu was opened on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RailTarget {
    Server(ServerId),
    Folder(u64),
}

/// A folder being named and colored, as written in its dialog.
#[derive(Clone, Debug, PartialEq)]
pub struct FolderDialog {
    pub folder: u64,
    pub name: String,
    pub color: Rgb,
}

// Notifications.

/// How `server` notifies: as the account keeps it when the provider keeps
/// it, else as set on this machine. Without a level of its own, the
/// server's default for its members, as Serein and the official client.
pub(crate) fn server_notify(ui: &Ui, store: &Store, server: ServerId) -> Notify {
    if !store.synced_notifications() {
        return ui
            .server_menu
            .notify
            .get(&server)
            .copied()
            .unwrap_or_else(|| default_notify(store, server));
    }
    store.notifications(Some(server)).map_or_else(
        || default_notify(store, server),
        |kept| Notify {
            level: kept
                .level
                .unwrap_or_else(|| default_notify(store, server).level),
            muted_until: kept.muted_until,
        },
    )
}

/// How `server` notifies whoever never set it: its own default level.
pub(crate) fn default_notify(store: &Store, server: ServerId) -> Notify {
    let mentions = store
        .server(server)
        .is_some_and(|found| found.options.mentions_only);
    Notify {
        level: if mentions {
            NotifyLevel::Mentions
        } else {
            NotifyLevel::All
        },
        muted_until: None,
    }
}

/// How a channel or category notifies of its own, from the same source.
pub(crate) fn channel_notify(ui: &Ui, store: &Store, channel: ChannelId) -> ChannelNotify {
    if !store.synced_notifications() {
        return ui
            .channel_list
            .notify
            .get(&channel)
            .copied()
            .unwrap_or_default();
    }
    store
        .channel_notifications(channel)
        .map_or_else(ChannelNotify::default, |kept| ChannelNotify {
            level: kept.level,
            muted_until: kept.muted_until,
        })
}

/// Whether muted channels of `server` are left out of its list.
pub(crate) fn hides_muted(ui: &Ui, store: &Store, server: Option<ServerId>) -> bool {
    if !store.synced_notifications() {
        return ui.channel_list.hide_muted;
    }
    server
        .and_then(|server| store.notifications(Some(server)))
        .is_some_and(|kept| kept.hide_muted)
}

/// Ask the provider to change how `server` notifies; when it may not, say why.
pub(crate) fn notify_command(
    ui: &mut Ui,
    store: &Store,
    server: Option<ServerId>,
    change: NotificationChange,
    now: Instant,
) -> Vec<Intent> {
    if let Some(reason) = store.account_abilities().notifications.reason() {
        ui.notify(
            t("No se pudieron cambiar las notificaciones"),
            Some(reason.into()),
            true,
            now,
        );
        return Vec::new();
    }
    vec![Intent::Command(Command::SetNotifications {
        server,
        change,
    })]
}

// Folders.

/// The layout a change starts from: the account's, with the servers it does
/// not name first, each on its own, as the rail shows them.
pub(crate) fn layout(store: &Store) -> Vec<ServerFolder> {
    let kept = store.folders().unwrap_or_default();
    let named: HashSet<ServerId> = kept
        .iter()
        .flat_map(|folder| folder.servers.iter().copied())
        .collect();
    let mut layout: Vec<ServerFolder> = store
        .servers()
        .iter()
        .filter(|server| !named.contains(&server.id))
        .map(|server| ServerFolder::single(server.id))
        .collect();
    layout.extend(kept.iter().cloned());
    layout
}

/// A change to the layout of the rail.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Edit {
    /// One place up (`-1`) or down (`1`), among the entries shown, or among
    /// its folder's servers.
    Move(RailTarget, i8),
    /// Into a folder, or a new one with it alone (`None`).
    Into(ServerId, Option<u64>),
    /// Out of its folder, right after it.
    Out(ServerId),
    /// The folder goes; its servers stay, on their own, where it was.
    Dissolve(u64),
    Customize {
        folder: u64,
        name: Option<String>,
        color: Rgb,
    },
    // Rail and notifications.
    /// Dragged onto another mark, as Serein: a folder before or after its
    /// entry; a server out of where it was and before or after it (in its
    /// folder, when it is in one), or inside it: into its folder, or into a
    /// new one with both.
    Drop(RailTarget, RailTarget, Placement),
}

/// An id no folder of `layout` has.
fn fresh_id(layout: &[ServerFolder]) -> u64 {
    (1..)
        .find(|id| layout.iter().all(|folder| folder.id != Some(*id)))
        .unwrap_or(1)
}

/// `layout` after `edit`; `shown` says which servers are on the rail, so a
/// move skips entries of servers left long ago.
pub(crate) fn edited(
    mut layout: Vec<ServerFolder>,
    edit: Edit,
    shown: &dyn Fn(ServerId) -> bool,
) -> Vec<ServerFolder> {
    let entry_of = |layout: &[ServerFolder], target: RailTarget| {
        layout.iter().position(|folder| match target {
            RailTarget::Folder(id) => folder.id == Some(id),
            RailTarget::Server(server) => folder.servers.contains(&server),
        })
    };
    let visible = |folder: &ServerFolder| folder.servers.iter().any(|server| shown(*server));
    match edit {
        Edit::Move(target, step) => {
            let Some(index) = entry_of(&layout, target) else {
                return layout;
            };
            let inside = matches!(target, RailTarget::Server(_)) && layout[index].id.is_some();
            if let (true, RailTarget::Server(server)) = (inside, target) {
                let servers = &mut layout[index].servers;
                let at = servers
                    .iter()
                    .position(|found| *found == server)
                    .unwrap_or(0);
                let to = at as i64 + i64::from(step);
                if (0..servers.len() as i64).contains(&to) {
                    servers.swap(at, to as usize);
                }
                return layout;
            }
            let neighbor = if step < 0 {
                (0..index).rev().find(|at| visible(&layout[*at]))
            } else {
                (index + 1..layout.len()).find(|at| visible(&layout[*at]))
            };
            if let Some(neighbor) = neighbor {
                layout.swap(index, neighbor);
            }
        }
        Edit::Into(server, folder) => {
            let Some(from) = entry_of(&layout, RailTarget::Server(server)) else {
                return layout;
            };
            let target = folder.and_then(|id| entry_of(&layout, RailTarget::Folder(id)));
            if folder.is_some() && (target.is_none() || target == Some(from)) {
                return layout;
            }
            layout[from].servers.retain(|found| *found != server);
            match target {
                Some(target) => layout[target].servers.push(server),
                None => {
                    let id = fresh_id(&layout);
                    layout.insert(
                        from,
                        ServerFolder {
                            id: Some(id),
                            name: None,
                            color: Some(FOLDER_COLOR),
                            servers: vec![server],
                        },
                    );
                }
            }
        }
        Edit::Out(server) => {
            if let Some(from) = entry_of(&layout, RailTarget::Server(server))
                .filter(|from| layout[*from].id.is_some())
            {
                layout[from].servers.retain(|found| *found != server);
                layout.insert(from + 1, ServerFolder::single(server));
            }
        }
        Edit::Dissolve(id) => {
            if let Some(at) = entry_of(&layout, RailTarget::Folder(id)) {
                let folder = layout.remove(at);
                layout.splice(at..at, folder.servers.into_iter().map(ServerFolder::single));
            }
        }
        Edit::Customize {
            folder,
            name,
            color,
        } => {
            if let Some(at) = entry_of(&layout, RailTarget::Folder(folder)) {
                layout[at].name = name;
                layout[at].color = Some(color);
            }
        }
        Edit::Drop(source, target, placement) => {
            let (Some(from), Some(to)) = (entry_of(&layout, source), entry_of(&layout, target))
            else {
                return layout;
            };
            let after = usize::from(placement == Placement::After);
            match source {
                RailTarget::Folder(_) => {
                    if source == target || from == to || placement == Placement::Inside {
                        return layout;
                    }
                    let moved = layout.remove(from);
                    let to = if from < to { to - 1 } else { to };
                    layout.insert(to + after, moved);
                }
                RailTarget::Server(server) => {
                    if source == target {
                        return layout;
                    }
                    // Emptied entries stay until the end, so `to` holds.
                    layout[from].servers.retain(|found| *found != server);
                    match (placement, target) {
                        (Placement::Inside, _) => {
                            if layout[to].id.is_none() {
                                layout[to].id = Some(fresh_id(&layout));
                                layout[to].color = Some(FOLDER_COLOR);
                            }
                            layout[to].servers.push(server);
                        }
                        (_, RailTarget::Server(beside)) if layout[to].id.is_some() => {
                            let servers = &mut layout[to].servers;
                            let at = servers
                                .iter()
                                .position(|found| *found == beside)
                                .unwrap_or(servers.len());
                            servers.insert((at + after).min(servers.len()), server);
                        }
                        _ => layout.insert(to + after, ServerFolder::single(server)),
                    }
                }
            }
        }
    }
    layout.retain(|folder| !folder.servers.is_empty());
    layout
}

/// Ask for `edit` of the rail; a layout the service would not keep, or a
/// provider that may not, goes nowhere and says why.
pub(crate) fn folders_command(ui: &mut Ui, store: &Store, edit: Edit, now: Instant) -> Vec<Intent> {
    if let Some(reason) = store.account_abilities().folders.reason() {
        ui.notify(
            t("No se pudieron cambiar las carpetas"),
            Some(reason.into()),
            true,
            now,
        );
        return Vec::new();
    }
    let before = layout(store);
    let after = edited(before.clone(), edit, &|server| {
        store.server(server).is_some()
    });
    if after == before || !valid_folders(&after) {
        return Vec::new();
    }
    vec![Intent::Command(Command::SetServerFolders(after))]
}

/// A folder's name as shown: its own, else its servers'.
fn folder_name(folder: &ServerFolder, servers: &[&Server]) -> String {
    folder
        .name
        .clone()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| {
            servers
                .iter()
                .map(|server| server.shown_name())
                .collect::<Vec<_>>()
                .join(", ")
        })
}

/// The rail's entries, each server drawn by `item`, as the account laid
/// them out.
pub(crate) fn rail_entries(
    ui: &Ui,
    store: &Store,
    look: &Look,
    item: &dyn Fn(&Server) -> RailItem,
) -> Vec<RailEntry> {
    store
        .rail()
        .into_iter()
        .map(|entry| match entry {
            rune_state::RailEntry::Server(server) => RailEntry::Server(item(server)),
            rune_state::RailEntry::Folder { folder, servers } => {
                let id = folder.id.unwrap_or_default();
                RailEntry::Folder(RailFolder {
                    id: format!("folder:{id}"),
                    name: folder_name(folder, &servers),
                    color: look.ink(folder.color.unwrap_or(FOLDER_COLOR)),
                    open: ui.account.open_folders.contains(&id),
                    servers: servers.into_iter().map(item).collect(),
                })
            }
        })
        .collect()
}

/// A secondary click on the rail: the menu of a server or a folder, where
/// the provider has folders.
pub(crate) fn secondary_click(
    ui: &mut Ui,
    store: &Store,
    target: &str,
    at: Point,
) -> Option<Vec<Intent>> {
    let number = |prefix: &str| target.strip_prefix(prefix)?.parse::<u64>().ok();
    let found = match (number("server:"), number("folder:")) {
        (Some(server), _) => RailTarget::Server(ServerId(server)),
        (_, Some(folder)) if has_folders(store) => RailTarget::Folder(folder),
        _ => return None,
    };
    ui.account.menu = Some((found, (at.x, at.y)));
    Some(Vec::new())
}

/// Whether the provider keeps folders of servers (its rail's folder group).
fn has_folders(store: &Store) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.server_folders)
}

/// The rail's groups for `server`'s folder: moves, into a folder, out of it.
/// None when the provider has no folders.
fn folder_groups(store: &Store, server: ServerId) -> Vec<Vec<Entry>> {
    if !has_folders(store) {
        return Vec::new();
    }
    let denied = store
        .account_abilities()
        .folders
        .reason()
        .map(str::to_owned);
    let item = |id: String, label: &str| Item::new(id, label).denied(denied.clone());
    let kept = layout(store);
    let key = format!("server:{server}");
    let moves = vec![
        item(format!("rail:up:{key}"), t("Mover arriba")).into(),
        item(format!("rail:down:{key}"), t("Mover abajo")).into(),
    ];
    let inside = kept
        .iter()
        .find(|folder| folder.servers.contains(&server))
        .and_then(|folder| folder.id);
    let mut folders: Vec<Entry> = kept
        .iter()
        .filter(|folder| folder.id.is_some() && folder.id != inside)
        .map(|folder| {
            let servers: Vec<&Server> = folder
                .servers
                .iter()
                .filter_map(|id| store.server(*id))
                .collect();
            item(
                format!("rail:into:{server}:{}", folder.id.unwrap_or_default()),
                &folder_name(folder, &servers),
            )
            .into()
        })
        .collect();
    folders.push(item(format!("rail:into:{server}:new"), t("Nueva carpeta")).into());
    let mut groups = vec![
        moves,
        vec![
            Item::new(format!("rail:submenu:{server}"), t("Mover a una carpeta"))
                .submenu(folders)
                .into(),
        ],
    ];
    if inside.is_some() {
        groups.push(vec![item(
            format!("rail:out:{server}"),
            t("Sacar de la carpeta"),
        )
        .into()]);
    }
    groups
}

/// The items of the rail's menu on `target`.
fn menu_entries(ui: &Ui, store: &Store, target: RailTarget) -> Vec<Entry> {
    match target {
        // The server's own menu, as the channel column's header has it.
        RailTarget::Server(server) => grouped(crate::server_menu::server_groups(
            ui,
            store,
            server,
            folder_groups(store, server),
        )),
        RailTarget::Folder(folder) => {
            let denied = store
                .account_abilities()
                .folders
                .reason()
                .map(str::to_owned);
            let item = |id: String, label: &str| Item::new(id, label).denied(denied.clone());
            let key = format!("folder:{folder}");
            let moves = vec![
                item(format!("rail:up:{key}"), t("Mover arriba")).into(),
                item(format!("rail:down:{key}"), t("Mover abajo")).into(),
            ];
            grouped(vec![
                vec![item(format!("rail:edit:{folder}"), t("Ajustes de carpeta")).into()],
                moves,
                vec![
                    Item::new(format!("rail:dissolve:{folder}"), t("Desarmar carpeta"))
                        .denied(denied.clone())
                        .danger()
                        .into(),
                ],
            ])
        }
    }
}

/// Answer the rail's folders and their menu and dialog, and the notes
/// written in profiles. `None` for actions of other parts.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    match action {
        // Rail and notifications: servers and folders dragged.
        UiAction::DragStarted { .. }
        | UiAction::DragMoved { .. }
        | UiAction::Dropped { .. }
        | UiAction::DragCancelled { .. } => crate::rail::dragged(ui, store, action, now),
        UiAction::Clicked { id } => {
            // Any click closes the menu; its own items are answered below.
            let menu = ui.account.menu.take();
            if let Some(folder) = id
                .strip_prefix("folder:")
                .and_then(|rest| rest.parse::<u64>().ok())
            {
                if !ui.account.open_folders.remove(&folder) {
                    ui.account.open_folders.insert(folder);
                }
                return Some(vec![save_open_folders(ui)]);
            }
            if let Some(rest) = id.strip_prefix("rail:") {
                return Some(rail_clicked(ui, store, rest, now));
            }
            if let Some(rest) = id.strip_prefix(folder_parts::ids::COLOR) {
                if let (Some(dialog), Ok(color)) =
                    (ui.account.dialog.as_mut(), u32::from_str_radix(rest, 16))
                {
                    dialog.color = Rgb(color);
                }
                return Some(Vec::new());
            }
            match id.as_str() {
                folder_parts::ids::CLOSE => {
                    ui.account.dialog = None;
                    Some(Vec::new())
                }
                folder_parts::ids::SAVE => Some(save_folder(ui, store, now)),
                menu_parts::ids::MENU_DISMISS if menu.is_some() => Some(Vec::new()),
                _ => None,
            }
        }
        UiAction::InputChanged { id, value } => {
            if id == folder_parts::ids::NAME {
                if let Some(dialog) = ui.account.dialog.as_mut() {
                    dialog.name = value.clone();
                }
                return Some(Vec::new());
            }
            let user = note_user(id)?;
            ui.account.notes.insert(user, value.clone());
            Some(Vec::new())
        }
        UiAction::Submitted { id, value } => {
            if id == folder_parts::ids::NAME {
                if let Some(dialog) = ui.account.dialog.as_mut() {
                    dialog.name = value.clone();
                }
                return Some(save_folder(ui, store, now));
            }
            let user = note_user(id)?;
            Some(save_note(ui, store, user, value, now))
        }
        _ => None,
    }
}

fn rail_clicked(ui: &mut Ui, store: &Store, rest: &str, now: Instant) -> Vec<Intent> {
    let parse_target = |key: &str| {
        let (kind, id) = key.split_once(':')?;
        let id = id.parse::<u64>().ok()?;
        match kind {
            "server" => Some(RailTarget::Server(ServerId(id))),
            "folder" => Some(RailTarget::Folder(id)),
            _ => None,
        }
    };
    let (verb, key) = rest.split_once(':').unwrap_or((rest, ""));
    let edit = match verb {
        "up" | "down" => {
            parse_target(key).map(|target| Edit::Move(target, if verb == "up" { -1 } else { 1 }))
        }
        "into" => key.split_once(':').and_then(|(server, folder)| {
            let server = ServerId(server.parse().ok()?);
            match folder {
                "new" => Some(Edit::Into(server, None)),
                id => Some(Edit::Into(server, Some(id.parse().ok()?))),
            }
        }),
        "out" => key.parse().ok().map(|server| Edit::Out(ServerId(server))),
        "dissolve" => key.parse().ok().map(Edit::Dissolve),
        "edit" => {
            let Ok(folder) = key.parse::<u64>() else {
                return Vec::new();
            };
            return open_folder_dialog(ui, store, folder);
        }
        _ => None,
    };
    match edit {
        Some(edit) => {
            // A server put in a new folder shows inside it.
            let mut intents = Vec::new();
            if let Edit::Into(server, None) = &edit {
                intents.extend(open_new_folder(ui, store, &edit, *server));
            }
            intents.extend(folders_command(ui, store, edit, now));
            intents
        }
        None => Vec::new(),
    }
}

/// Open the folder `server` lands in after `edit` when `edit` makes it, and
/// keep that it is open.
pub(crate) fn open_new_folder(
    ui: &mut Ui,
    store: &Store,
    edit: &Edit,
    server: ServerId,
) -> Option<Intent> {
    let before = layout(store);
    let fresh = edited(before.clone(), edit.clone(), &|found| {
        store.server(found).is_some()
    });
    let id = fresh
        .iter()
        .find(|folder| folder.servers.contains(&server))
        .and_then(|folder| folder.id)
        .filter(|id| !before.iter().any(|folder| folder.id == Some(*id)))?;
    ui.account.open_folders.insert(id);
    Some(save_open_folders(ui))
}

/// The folders open on the rail, as kept: their ids, smallest first.
pub(crate) fn kept_open_folders(ui: &Ui) -> Option<String> {
    let mut open: Vec<u64> = ui.account.open_folders.iter().copied().collect();
    open.sort_unstable();
    (!open.is_empty()).then(|| {
        open.iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    })
}

/// Take back what [`kept_open_folders`] kept; what cannot be read is left out.
pub(crate) fn restore_open_folders(ui: &mut Ui, kept: Option<String>) {
    ui.account.open_folders = kept
        .unwrap_or_default()
        .split(',')
        .filter_map(|id| id.parse().ok())
        .collect();
}

fn save_open_folders(ui: &Ui) -> Intent {
    Intent::SaveSetting {
        key: OPEN_FOLDERS_SETTING.into(),
        value: kept_open_folders(ui),
    }
}

fn open_folder_dialog(ui: &mut Ui, store: &Store, folder: u64) -> Vec<Intent> {
    let Some(found) = layout(store)
        .into_iter()
        .find(|found| found.id == Some(folder))
    else {
        return Vec::new();
    };
    let name = found.name.clone().unwrap_or_default();
    ui.account.dialog = Some(FolderDialog {
        folder,
        name: name.clone(),
        color: found.color.unwrap_or(FOLDER_COLOR),
    });
    vec![
        Intent::SetInput {
            id: folder_parts::ids::NAME.into(),
            value: name,
        },
        Intent::Focus(folder_parts::ids::NAME.into()),
    ]
}

fn save_folder(ui: &mut Ui, store: &Store, now: Instant) -> Vec<Intent> {
    let Some(dialog) = ui.account.dialog.clone() else {
        return Vec::new();
    };
    let name = dialog.name.trim().to_owned();
    if name.chars().count() > rune_model::account::FOLDER_NAME_MAX {
        return Vec::new();
    }
    ui.account.dialog = None;
    folders_command(
        ui,
        store,
        Edit::Customize {
            folder: dialog.folder,
            name: (!name.is_empty()).then_some(name),
            color: dialog.color,
        },
        now,
    )
}

/// The rail's menu and the folder dialog, over `root`.
pub(crate) fn overlay(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    _context: &ViewContext,
) -> Element {
    let mut root = root;
    if let Some(dialog) = &ui.account.dialog {
        let servers: Vec<&Server> = layout(store)
            .into_iter()
            .find(|found| found.id == Some(dialog.folder))
            .map(|found| {
                found
                    .servers
                    .iter()
                    .filter_map(|id| store.server(*id))
                    .collect()
            })
            .unwrap_or_default();
        let form = FolderForm {
            name: dialog.name.clone(),
            placeholder: folder_name(&ServerFolder::default(), &servers),
            color: dialog.color,
            denied: store
                .account_abilities()
                .folders
                .reason()
                .map(str::to_owned),
        };
        root = folder_parts::folder_dialog(root, &form, DIALOG_WIDTH, look);
    }
    if let Some((target, at)) = ui.account.menu {
        let entries = menu_entries(ui, store, target);
        root = Element::stack(vec![root, menu_parts::context_menu(at, entries, look)]);
    }
    root
}

// Messaging privacy, in the settings.

/// The setting `key` of «Mensajes directos y solicitudes», as its page
/// shows it; `None` for other keys.
/// `open` is the list open in the settings, if any: the explicit filters'
/// lists are named `explicit-{who}`.
pub(crate) fn privacy_setting(
    store: &Store,
    key: &str,
    look: &Look,
    width: f32,
    open: Option<&str>,
) -> Option<Element> {
    let permit = store.account_abilities().privacy;
    let kept = store.messaging_privacy();
    let denied = permit.reason().map(str::to_owned).or_else(|| {
        kept.is_none()
            .then(|| t("Todavía no se leyó la privacidad de tu cuenta.").into())
    });
    let privacy = kept.cloned().unwrap_or_default();
    let denied = denied.as_deref();
    let toggle = |id: &str, label: &str, note: &str, on: bool| {
        setting_parts::toggle(id, label, Some(note), on, denied, look)
    };
    let stacked = |label: &str, note: &str, below: Element| {
        Element::column(vec![
            setting_parts::setting(
                label,
                Some(denied.unwrap_or(note)),
                None,
                denied.is_none(),
                look,
            ),
            below,
            Element::spacer().size(None, Some(12.0)),
        ])
    };
    Some(match key {
        "server-dms" => toggle(
            "us:privacy:server-dms",
            t("Permitir mensajes directos de miembros de servidores"),
            t("Para los servidores a los que entres desde ahora; cada uno se cambia abajo."),
            privacy.server_dms,
        ),
        "message-requests" => toggle(
            "us:privacy:requests",
            t("Filtrar como solicitudes los mensajes de quienes quizás no conozcas"),
            t("Los de miembros de servidores llegan a «Solicitudes de mensajes» en vez de a tus mensajes directos."),
            privacy.request_filter,
        ),
        "spam-filter" => {
            let selected = match privacy.spam {
                SpamFilter::Everyone => "us:privacy:spam=everyone",
                SpamFilter::Off => "us:privacy:spam=off",
                SpamFilter::NonFriends | SpamFilter::Default => "us:privacy:spam=non-friends",
            };
            let choices = setting_parts::choices(
                "us:privacy:spam",
                t("Filtro de spam"),
                &[
                    (
                        "us:privacy:spam=everyone".to_owned(),
                        t("Filtrar todo el spam"),
                        Some(t("También los de tus amigos.")),
                    ),
                    (
                        "us:privacy:spam=non-friends".to_owned(),
                        t("Filtrar los mensajes de personas que no sean tus amigos (recomendado)"),
                        Some(t("Lo que la cuenta hace si nunca se cambió.")),
                    ),
                    ("us:privacy:spam=off".to_owned(), t("No filtrar el spam"), None),
                ],
                selected,
                look,
            );
            stacked(
                t("Filtro de spam"),
                t("Lo que parece spam va a una carpeta aparte en vez de a tus mensajes directos."),
                if denied.is_some() {
                    Element::spacer()
                } else {
                    choices
                },
            )
        }
        "explicit-content" => {
            let rows = [
                ("servers", t("En servidores"), privacy.explicit_servers),
                ("friends", t("Mensajes directos de amigos"), privacy.explicit_friends),
                ("others", t("Mensajes directos de otras personas"), privacy.explicit_others),
            ]
            .into_iter()
            .map(|(who, label, filter)| {
                let name = format!("explicit-{who}");
                // Never set, the service's default: shown in servers and from
                // friends, blocked from anyone else (as the store applies it).
                let effective = match (filter, who) {
                    (ContentFilter::Default, "others") => ContentFilter::Block,
                    (ContentFilter::Default, _) => ContentFilter::Show,
                    (chosen, _) => chosen,
                };
                let filters = [
                    ("show", t("Mostrar"), ContentFilter::Show),
                    ("blur", t("Difuminar"), ContentFilter::Blur),
                    ("block", t("Bloquear"), ContentFilter::Block),
                ];
                let current = filters
                    .iter()
                    .find(|(_, _, value)| *value == effective)
                    .map_or("", |(_, label, _)| *label);
                let choices = filters
                    .into_iter()
                    .map(|(key, label, value)| {
                        DropdownOption::new(format!("us:privacy:explicit-{who}={key}"), label)
                            .selected(effective == value)
                            .enabled(denied.is_none())
                    })
                    .collect();
                let control = Select::new(
                    format!("us:select:{name}"),
                    current,
                    choices,
                    open == Some(name.as_str()),
                    "us:select:dismiss",
                    look.palette(),
                )
                .menu_palette(look.menu_palette())
                .variant(SelectVariant::Field)
                .size(ControlSize::Spacious)
                .accessibility_label(label)
                .enabled(denied.is_none())
                .width(180.0)
                .menu_width(180.0)
                .render();
                setting_parts::setting(label, None, Some(control), denied.is_none(), look)
            })
            .collect();
            Element::column(vec![
                setting_parts::subtitle(t("Filtro de imágenes explícitas"), look),
                line(
                    t("Difumina las imágenes explícitas hasta que decidas verlas."),
                    14.0,
                    look.muted,
                ),
                Element::spacer().size(None, Some(8.0)),
                setting_parts::rows(rows, look),
            ])
        }
        "friend-requests" => {
            let sources = privacy.friend_sources;
            Element::column(vec![
                setting_parts::subtitle(t("Quién puede mandarte solicitudes de amistad"), look),
                setting_parts::rows(
                    vec![
                        toggle(
                            "us:privacy:friends-everyone",
                            t("Todo el mundo"),
                            t("Cualquiera que sepa tu nombre de usuario."),
                            sources.everyone,
                        ),
                        toggle(
                            "us:privacy:friends-fof",
                            t("Amigos de amigos"),
                            t("Quienes tienen un amigo en común contigo."),
                            sources.friends_of_friends,
                        ),
                        toggle(
                            "us:privacy:friends-members",
                            t("Miembros del servidor"),
                            t("Quienes están en un servidor contigo."),
                            sources.server_members,
                        ),
                    ],
                    look,
                ),
            ])
        }
        "server-privacy" => {
            let palette = look.palette();
            let mut rows = vec![Element::row(vec![
                line(t("Servidor"), 12.0, look.muted).flex(1.0),
                line(t("Mensajes directos"), 12.0, look.muted).size(Some(130.0), None),
                line(t("Solicitudes"), 12.0, look.muted).size(Some(90.0), None),
            ])
            .gap(8.0)];
            for server in store.servers() {
                let switch = |id: String, on: bool, label: String| {
                    limn::Switch::new(id, on, palette)
                        .accessibility_label(label)
                        .enabled(denied.is_none())
                        .render()
                };
                rows.push(
                    Element::row(vec![
                        line(server.shown_name(), 15.0, look.text).flex(1.0).centered_y(),
                        Element::row(vec![switch(
                            format!("us:privacy:dms-in:{}", server.id),
                            privacy.dms_in(server.id),
                            tr!("Mensajes directos de miembros de {server}", server = server.shown_name()),
                        )])
                        .size(Some(130.0), None)
                        .centered_y(),
                        Element::row(vec![switch(
                            format!("us:privacy:requests-in:{}", server.id),
                            privacy.requests_in(server.id),
                            tr!("Filtrar solicitudes de miembros de {server}", server = server.shown_name()),
                        )])
                        .size(Some(90.0), None)
                        .centered_y(),
                    ])
                    .gap(8.0)
                    .size(None, Some(36.0)),
                );
            }
            stacked(
                t("Por servidor"),
                t("Si sus miembros pueden escribirte directamente, y si sus mensajes se filtran como solicitudes."),
                Element::column(rows).gap(4.0).size(Some(width.min(520.0)), None),
            )
        }
        _ => return None,
    })
}

/// A click on a privacy setting: `rest` follows `us:privacy:`.
pub(crate) fn privacy_clicked(ui: &mut Ui, store: &Store, rest: &str, now: Instant) -> Vec<Intent> {
    let Some(privacy) = store.messaging_privacy().cloned() else {
        return Vec::new();
    };
    if let Permit::Denied(reason) = store.account_abilities().privacy {
        ui.notify(
            t("No se pudo cambiar la privacidad"),
            Some(reason),
            true,
            now,
        );
        return Vec::new();
    }
    let server = |prefix: &str| {
        rest.strip_prefix(prefix)
            .and_then(|id| id.parse().ok())
            .map(ServerId)
    };
    let filter = |key: &str| match key {
        "show" => Some(ContentFilter::Show),
        "blur" => Some(ContentFilter::Blur),
        "block" => Some(ContentFilter::Block),
        _ => None,
    };
    let sources = privacy.friend_sources;
    let change = if let Some(server) = server("dms-in:") {
        Some(PrivacyChange::ServerDmsIn(server, !privacy.dms_in(server)))
    } else if let Some(server) = server("requests-in:") {
        Some(PrivacyChange::RequestFilterIn(
            server,
            !privacy.requests_in(server),
        ))
    } else if let Some((who, key)) = rest
        .strip_prefix("explicit-")
        .and_then(|rest| rest.split_once('='))
    {
        filter(key).and_then(|filter| match who {
            "servers" => Some(PrivacyChange::ExplicitServers(filter)),
            "friends" => Some(PrivacyChange::ExplicitFriends(filter)),
            "others" => Some(PrivacyChange::ExplicitOthers(filter)),
            _ => None,
        })
    } else {
        match rest {
            "server-dms" => Some(PrivacyChange::ServerDms(!privacy.server_dms)),
            "requests" => Some(PrivacyChange::RequestFilter(!privacy.request_filter)),
            "spam=everyone" => Some(PrivacyChange::Spam(SpamFilter::Everyone)),
            "spam=non-friends" => Some(PrivacyChange::Spam(SpamFilter::NonFriends)),
            "spam=off" => Some(PrivacyChange::Spam(SpamFilter::Off)),
            // «Todo el mundo» takes in the other two; leaving either out leaves it.
            "friends-everyone" => Some(PrivacyChange::FriendSources(if sources.everyone {
                FriendSources {
                    everyone: false,
                    ..sources
                }
            } else {
                FriendSources::default()
            })),
            "friends-fof" => Some(PrivacyChange::FriendSources(FriendSources {
                friends_of_friends: !sources.friends_of_friends,
                everyone: sources.everyone && !sources.friends_of_friends,
                ..sources
            })),
            "friends-members" => Some(PrivacyChange::FriendSources(FriendSources {
                server_members: !sources.server_members,
                everyone: sources.everyone && !sources.server_members,
                ..sources
            })),
            _ => None,
        }
    };
    let Some(change) = change else {
        return Vec::new();
    };
    let mut wanted: MessagingPrivacy = privacy.clone();
    wanted.apply(change);
    if wanted == privacy {
        return Vec::new();
    }
    vec![Intent::Command(Command::SetMessagingPrivacy(change))]
}

// Notes.

fn note_id(user: UserId) -> String {
    format!("pp:note:{user}")
}

fn note_user(id: &str) -> Option<UserId> {
    id.strip_prefix("pp:note:")?.parse().ok().map(UserId)
}

/// The note field of `user`'s profile, where the provider keeps notes;
/// never on the account's own.
pub(crate) fn note_field(ui: &Ui, store: &Store, user: UserId) -> Option<NoteField> {
    let notes = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.notes);
    if !notes || store.me() == Some(user) {
        return None;
    }
    let kept = store.note(user);
    let failed = kept.is_none() && ui.account.notes_failed.contains(&user);
    Some(NoteField {
        id: note_id(user),
        text: ui
            .account
            .notes
            .get(&user)
            .cloned()
            .or_else(|| kept.map(str::to_owned))
            .unwrap_or_default(),
        loading: kept.is_none() && !failed,
        denied: store
            .account_abilities()
            .notes
            .reason()
            .map(str::to_owned)
            .or_else(|| failed.then(|| t("No se pudo cargar la nota.").to_owned())),
    })
}

/// Whether `failure` is the read of a note asked for: the card shows it,
/// and the next time it opens the note is asked for again.
pub(crate) fn note_failed(ui: &mut Ui, store: &Store, failure: &rune_provider::Failure) -> bool {
    let rune_provider::Failure::Note { user } = failure else {
        return false;
    };
    let reading = store.note(*user).is_none() && ui.account.notes_asked.remove(user);
    if reading {
        ui.account.notes_failed.insert(*user);
    }
    reading
}

/// Ask for `user`'s note once, when their profile opens.
pub(crate) fn load_note(ui: &mut Ui, store: &Store, user: UserId) -> Vec<Intent> {
    let wanted = store.account_abilities().notes.allowed()
        && store.me() != Some(user)
        && store.note(user).is_none()
        && ui.account.notes_asked.insert(user);
    if wanted {
        ui.account.notes_failed.remove(&user);
        vec![Intent::Command(Command::LoadNote { user })]
    } else {
        Vec::new()
    }
}

fn save_note(ui: &mut Ui, store: &Store, user: UserId, value: &str, now: Instant) -> Vec<Intent> {
    let note = value.trim().to_owned();
    if let Some(reason) = store.account_abilities().notes.reason() {
        ui.notify(
            t("No se pudo guardar la nota"),
            Some(reason.into()),
            true,
            now,
        );
        return Vec::new();
    }
    if !valid_note(&note) {
        ui.notify(
            t("La nota es muy larga"),
            Some(t("Puede tener hasta 256 caracteres.").into()),
            true,
            now,
        );
        return Vec::new();
    }
    if store.note(user) == Some(note.as_str()) {
        ui.account.notes.remove(&user);
        return Vec::new();
    }
    vec![Intent::Command(Command::SetNote { user, note })]
}

/// A note arrived: the field shows it, unless something else is being written.
pub(crate) fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied) -> Vec<Intent> {
    let Applied::Note(user) = applied else {
        return Vec::new();
    };
    ui.account.notes_failed.remove(user);
    let kept = store.note(*user).unwrap_or_default();
    let writing = ui
        .account
        .notes
        .get(user)
        .is_some_and(|draft| draft.trim() != kept);
    if writing {
        return Vec::new();
    }
    ui.account.notes.remove(user);
    vec![Intent::SetInput {
        id: note_id(*user),
        value: kept.to_owned(),
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(id: u64, servers: &[u64]) -> ServerFolder {
        ServerFolder {
            id: Some(id),
            name: None,
            color: None,
            servers: servers.iter().copied().map(ServerId).collect(),
        }
    }

    fn summary(layout: &[ServerFolder]) -> Vec<(Option<u64>, Vec<u64>)> {
        layout
            .iter()
            .map(|folder| (folder.id, folder.servers.iter().map(|s| s.0).collect()))
            .collect()
    }

    #[test]
    fn folders_are_made_filled_emptied_and_moved_as_the_rail_shows_them() {
        let shown = |server: ServerId| server.0 != 9;
        let start = vec![
            ServerFolder::single(ServerId(1)),
            ServerFolder::single(ServerId(9)),
            ServerFolder::single(ServerId(2)),
            folder(4, &[3]),
        ];
        // A new folder takes the server's place; the left server is skipped.
        let made = edited(start.clone(), Edit::Into(ServerId(2), None), &shown);
        assert_eq!(
            summary(&made),
            [
                (None, vec![1]),
                (None, vec![9]),
                (Some(1), vec![2]),
                (Some(4), vec![3])
            ]
        );
        let up = edited(made, Edit::Move(RailTarget::Folder(1), -1), &shown);
        assert_eq!(summary(&up)[0], (Some(1), vec![2]));
        let joined = edited(up, Edit::Into(ServerId(1), Some(4)), &shown);
        assert_eq!(summary(&joined).last().unwrap(), &(Some(4), vec![3, 1]));
        let inner = edited(
            joined,
            Edit::Move(RailTarget::Server(ServerId(1)), -1),
            &shown,
        );
        assert_eq!(summary(&inner).last().unwrap(), &(Some(4), vec![1, 3]));
        let out = edited(inner, Edit::Out(ServerId(3)), &shown);
        assert_eq!(summary(&out)[2..], [(Some(4), vec![1]), (None, vec![3])]);
        let dissolved = edited(out, Edit::Dissolve(4), &shown);
        assert_eq!(summary(&dissolved)[2..], [(None, vec![1]), (None, vec![3])]);
        // Emptied, a folder goes.
        let emptied = edited(dissolved, Edit::Into(ServerId(2), Some(1)), &shown);
        assert!(valid_folders(&emptied));
        let renamed = edited(
            start,
            Edit::Customize {
                folder: 4,
                name: Some("Juegos".into()),
                color: Rgb(0xFF0000),
            },
            &shown,
        );
        assert_eq!(renamed[3].name.as_deref(), Some("Juegos"));
        assert_eq!(renamed[3].color, Some(Rgb(0xFF0000)));
    }

    #[test]
    fn a_dropped_mark_lands_as_serein_lands_it() {
        let shown = |_: ServerId| true;
        let start = vec![
            ServerFolder::single(ServerId(1)),
            ServerFolder::single(ServerId(2)),
            folder(4, &[3, 5]),
        ];
        let drop = |layout: Vec<ServerFolder>, source, target, placement| {
            summary(&edited(
                layout,
                Edit::Drop(source, target, placement),
                &shown,
            ))
        };
        let server = |id| RailTarget::Server(ServerId(id));
        // Inside one alone: a new folder with both, where it was.
        assert_eq!(
            drop(start.clone(), server(1), server(2), Placement::Inside),
            [(Some(1), vec![2, 1]), (Some(4), vec![3, 5])]
        );
        // Inside a folder or a server of it: at its end.
        assert_eq!(
            drop(
                start.clone(),
                server(1),
                RailTarget::Folder(4),
                Placement::Inside
            ),
            [(None, vec![2]), (Some(4), vec![3, 5, 1])]
        );
        // Beside a server of a folder: in the folder, before or after it.
        assert_eq!(
            drop(start.clone(), server(2), server(5), Placement::Before),
            [(None, vec![1]), (Some(4), vec![3, 2, 5])]
        );
        // Out of a folder, before it.
        assert_eq!(
            drop(
                start.clone(),
                server(3),
                RailTarget::Folder(4),
                Placement::Before
            ),
            [
                (None, vec![1]),
                (None, vec![2]),
                (None, vec![3]),
                (Some(4), vec![5])
            ]
        );
        // Within its folder, after another.
        assert_eq!(
            drop(start.clone(), server(3), server(5), Placement::After),
            [(None, vec![1]), (None, vec![2]), (Some(4), vec![5, 3])]
        );
        // A folder before or after another entry; never inside, nor on itself.
        assert_eq!(
            drop(
                start.clone(),
                RailTarget::Folder(4),
                server(1),
                Placement::Before
            ),
            [(Some(4), vec![3, 5]), (None, vec![1]), (None, vec![2])]
        );
        assert_eq!(
            drop(
                start.clone(),
                RailTarget::Folder(4),
                server(1),
                Placement::After
            ),
            [(None, vec![1]), (Some(4), vec![3, 5]), (None, vec![2])]
        );
        assert_eq!(
            drop(
                start.clone(),
                RailTarget::Folder(4),
                server(1),
                Placement::Inside
            ),
            summary(&start)
        );
        assert_eq!(
            drop(
                start.clone(),
                RailTarget::Folder(4),
                server(3),
                Placement::Before
            ),
            summary(&start)
        );
        assert_eq!(
            drop(start.clone(), server(1), server(1), Placement::Inside),
            summary(&start)
        );
        // After the last entry.
        assert_eq!(
            drop(
                start.clone(),
                server(1),
                RailTarget::Folder(4),
                Placement::After
            ),
            [(None, vec![2]), (Some(4), vec![3, 5]), (None, vec![1])]
        );
    }
}
