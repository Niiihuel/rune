//! The column of a server's channels and the tools of the conversation's
//! header: the rows and their states, the context menus of a channel, a
//! category and the list (mark read, mute, notification level, move,
//! duplicate, delete, copy the ID), dragging channels and categories to
//! reorder them, the notification settings of each channel and category
//! (kept on this machine, like the servers'), the bell, the pinned messages,
//! the topic, and the line that says the slow mode is on.
//!
//! Nothing here changes the domain ahead of the provider: a move, a copy, a
//! deletion or a pin is asked for, and shows once the provider reports it.
//! The channel settings belong to another window: the gear and «Editar
//! canal» only ask for them, with `cs:open:{channel}`.

use std::collections::HashMap;
use std::time::Instant;

use limn::{Component, Element, Modal, Point, UiAction};
use rune_components::channel_menu::{
    self as parts, grouped, ids, Doomed, DuplicateForm, Entry, Item, PinCard, PinsContent,
};
use rune_components::server_menu::{ids as menu_ids, NotifyLevel, DIALOG_WIDTH, MUTE_DURATIONS};
use rune_components::sidebar::{self, CategoryRow, ChannelRow, DragMark};
use rune_components::Look;
use rune_i18n::{es, t, tr};
use rune_model::{Channel, ChannelId, ChannelKind, MessageId, Permissions, ServerId, Timestamp};
use rune_provider::{ChannelEdit, Command, Created, Failure, RequestId};
use rune_state::{Applied, Permit, Store};

use crate::server_menu::{self, FOREVER};
use crate::server_settings::rest;
use crate::{open, Intent, Ui, ViewContext};

/// The settings this part keeps between runs.
pub(crate) const NOTIFY_SETTING: &str = "channel_notifications";
pub(crate) const HIDE_MUTED_SETTING: &str = "hide_muted_channels";
pub(crate) const COLLAPSED_SETTING: &str = "collapsed_categories";

/// What the mute submenu offers, beside the durations of the server's.
pub(crate) const MUTE_LABELS: [&str; 6] = [
    es("Durante 15 minutos"),
    es("Durante 1 hora"),
    es("Durante 3 horas"),
    es("Durante 8 horas"),
    es("Durante 24 horas"),
    es("Hasta que lo vuelvas a activar"),
];

/// How a channel or a category marks what is new, as the account set it on
/// this machine: its own level, or `None` to take the one above it (the
/// category's, then the server's), and until when it is muted.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChannelNotify {
    pub level: Option<NotifyLevel>,
    /// Muted until then; [`FOREVER`] until turned back on.
    pub muted_until: Option<Timestamp>,
}

impl ChannelNotify {
    pub fn muted(&self, now: Timestamp) -> bool {
        self.muted_until.is_some_and(|until| until > now)
    }
}

/// What a context menu was opened on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Target {
    Channel(ChannelId),
    Category(ChannelId),
    /// The list itself, off any channel.
    List(ServerId),
}

/// A dialog of the list.
#[derive(Clone, Debug, PartialEq)]
pub enum ListDialog {
    /// A copy of `channel`, named as written.
    Duplicate { channel: ChannelId, name: String },
    /// The question before deleting a channel or a category.
    Delete(ChannelId),
    /// The question before moving `channel` into `category`, at `index`
    /// among the siblings the account sees there: whether it takes the
    /// category's permissions. The moves are worked out on the answer, so
    /// what others moved meanwhile stays where they put it.
    Sync {
        channel: ChannelId,
        category: ChannelId,
        index: usize,
    },
}

/// A channel or a category being dragged, and the row it would land on.
#[derive(Clone, Debug, PartialEq)]
pub struct Drag {
    pub source: ChannelId,
    /// The action of the row under the pointer, when it may land there.
    pub over: Option<String>,
}

#[derive(Debug)]
struct PinsLoad {
    request: RequestId,
    phase: PinsPhase,
}

#[derive(Debug)]
enum PinsPhase {
    Loading,
    Ready,
    Failed(String),
}

/// The state of the list, its menus and the header's tools.
#[derive(Debug, Default)]
pub struct ChannelList {
    /// The context menu open, on what, and its corner in the window.
    pub menu: Option<(Target, (f32, f32))>,
    /// The channel whose bell's menu is open.
    pub bell: Option<ChannelId>,
    /// The channel whose pinned messages are shown.
    pub pins: Option<ChannelId>,
    pins_load: Option<PinsLoad>,
    /// The channel whose whole topic is shown.
    pub topic: Option<ChannelId>,
    pub dialog: Option<ListDialog>,
    pub notify: HashMap<ChannelId, ChannelNotify>,
    /// Muted channels are left out of the list.
    pub hide_muted: bool,
    pub drag: Option<Drag>,
    /// Copies asked for, by request, and the channel each copies.
    duplicates: HashMap<RequestId, ChannelId>,
    // Threads, forums and stages (2026-10-18).
    /// The channel whose narrow header has its «más» menu open.
    // shortcut: a window widened with it open keeps it set, unseen, until
    // the next click (the Ui does not know the width); a resize hook would fix it.
    pub more: Option<ChannelId>,
}

/// The time on this machine's clock, for when a mute ends.
fn wall_now() -> Timestamp {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Timestamp(elapsed.as_millis() as i64)
}

fn number(id: &str, prefix: &str) -> Option<u64> {
    id.strip_prefix(prefix)?.parse().ok()
}

/// The channel or category a row of the list stands for, from its action.
fn row_of(id: &str) -> Option<ChannelId> {
    ["channel:", "voice:", "category:"]
        .iter()
        .find_map(|prefix| number(id, prefix))
        .map(ChannelId)
}

fn level_label(level: NotifyLevel) -> &'static str {
    match level {
        NotifyLevel::All => t("Todos los mensajes"),
        NotifyLevel::Mentions => t("Solo @menciones"),
        NotifyLevel::Nothing => t("Nada"),
    }
}

/// The level `channel` takes when it has none of its own: its category's,
/// else its server's.
fn inherited(ui: &Ui, store: &Store, channel: &Channel) -> NotifyLevel {
    let category = channel
        .parent
        .filter(|_| channel.kind != ChannelKind::Category)
        .and_then(|parent| crate::account::channel_notify(ui, store, parent).level);
    let server = channel
        .server
        .map(|server| crate::account::server_notify(ui, store, server).level)
        .unwrap_or_default();
    category.unwrap_or(server)
}

/// The level `channel` marks with, and whether it is muted (itself or its
/// category), at `now`. As the account keeps it when the provider does.
pub(crate) fn effective(
    ui: &Ui,
    store: &Store,
    channel: &Channel,
    now: Timestamp,
) -> (NotifyLevel, bool) {
    let own = crate::account::channel_notify(ui, store, channel.id);
    let category_muted = channel
        .parent
        .is_some_and(|parent| crate::account::channel_notify(ui, store, parent).muted(now));
    // A thread the account follows and silenced, as its provider keeps it
    // (2026-10-18); leaving it ends the silence.
    let thread_muted = channel
        .thread
        .as_ref()
        .is_some_and(|thread| channel.is_thread() && thread.joined && thread.muted(now));
    (
        own.level.unwrap_or_else(|| inherited(ui, store, channel)),
        own.muted(now) || category_muted || thread_muted,
    )
}

/// What the list marks on `channel` at `now`: whether it is unread, its
/// mentions, and whether it is muted. Like a server, a muted channel marks
/// only its mentions, and one that marks nothing marks nothing.
pub(crate) fn marks(
    ui: &Ui,
    store: &Store,
    channel: &Channel,
    now: Timestamp,
) -> (bool, u32, bool) {
    let read = store.read(channel.id);
    let (level, muted) = effective(ui, store, channel, now);
    match level {
        NotifyLevel::Nothing => (false, 0, muted),
        _ if muted => (false, read.mentions, true),
        NotifyLevel::Mentions => (false, read.mentions, false),
        NotifyLevel::All => (read.unread(), read.mentions, false),
    }
}

/// Whether anything of `server` is marked unread, and its mentions, after
/// the settings of each channel: what the rail marks before the server's own.
pub(crate) fn server_unread(
    ui: &Ui,
    store: &Store,
    server: ServerId,
    now: Timestamp,
) -> (bool, u32) {
    store
        .channel_tree(server)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .filter(|channel| channel.kind.has_messages())
        .map(|channel| marks(ui, store, channel, now))
        .fold((false, 0), |(unread, mentions), (one, more, _)| {
            (unread || one, mentions.saturating_add(more))
        })
}

/// Whether the account may make and delete channels in `server`.
fn manages(store: &Store, server: Option<ServerId>) -> bool {
    server
        .and_then(|server| store.server_abilities(server))
        .is_some_and(|abilities| abilities.manage_channels.allowed())
}

/// Whether the account may move `server`'s channels, all at once.
fn reorders(store: &Store, server: Option<ServerId>) -> bool {
    server
        .and_then(|server| store.server_abilities(server))
        .is_some_and(|abilities| abilities.reorder_channels.allowed())
}

/// The rows of `server`'s channels, under their categories.
pub(crate) fn rows(
    ui: &Ui,
    store: &Store,
    server: ServerId,
    look: &Look,
    now: Timestamp,
) -> Vec<Element> {
    let selected = crate::voice::selected(ui);
    let manage = manages(store, Some(server));
    let reorder = reorders(store, Some(server));
    let drag = ui.channel_list.drag.as_ref();
    let line = drag.and_then(|drag| insertion(store, server, drag));
    let mark = |id: &str| match drag {
        None => DragMark::None,
        Some(_) if line.as_ref().is_some_and(|(row, _)| row == id) => {
            line.as_ref().map_or(DragMark::None, |(_, mark)| *mark)
        }
        Some(drag) if row_of(id) == Some(drag.source) || !accepts(store, drag.source, id) => {
            DragMark::Dim
        }
        Some(_) => DragMark::None,
    };
    let mut rows = Vec::new();
    // Scheduled events (eventos-programados): their entry, over everything.
    rows.extend(crate::events::entry(store, server, look));
    // Desktop (P6): the favorites first, under their heading, and the
    // channels the account cannot see when asked to show them.
    let (favorites, tree) = crate::desktop::channel_groups(ui, store, server);
    let groups = (!favorites.is_empty())
        .then_some((None, favorites))
        .into_iter()
        .map(|group| (true, group))
        .chain(tree.into_iter().map(|group| (false, group)));
    for (favorite, (category, channels)) in groups {
        if favorite {
            rows.push(sidebar::heading(t("Favoritos"), look));
        }
        let collapsed = category.is_some_and(|category| ui.collapsed.contains(&category.id));
        if let Some(category) = category {
            let id = format!("category:{}", category.id);
            rows.push(sidebar::category(
                CategoryRow {
                    drag: mark(&id),
                    id,
                    name: category.name.clone(),
                    collapsed,
                    create: manage.then(|| format!("newchan:{server}:{}", category.id)),
                    movable: reorder,
                },
                look,
            ));
        }
        for channel in channels {
            if !Store::visible(channel) {
                if !collapsed {
                    rows.push(crate::desktop::hidden_row(channel, look));
                }
                continue;
            }
            let (unread, mentions, muted) = marks(ui, store, channel, now);
            let is_selected = selected == Some(channel.id);
            // A folded category still shows the channel open and those with
            // something new; muted ones go when asked.
            if !is_selected
                && ((collapsed && !unread && mentions == 0)
                    || (muted && crate::account::hides_muted(ui, store, Some(server))))
            {
                continue;
            }
            let voice = channel.kind.is_voice();
            let id = format!("{}:{}", if voice { "voice" } else { "channel" }, channel.id);
            let edit = store
                .abilities(channel.id)
                .is_some_and(|abilities| abilities.manage.allowed());
            let forum = channel.kind == ChannelKind::Forum;
            // Someone carried to another voice channel marks the voice ones.
            let carried = voice
                .then(|| crate::voice_drag::mark(ui, store, channel.id))
                .flatten();
            let row = sidebar::channel(
                ChannelRow {
                    drag: carried.unwrap_or_else(|| mark(&id)),
                    id,
                    name: channel.name.clone(),
                    voice,
                    glyph: glyph(channel.kind),
                    selected: is_selected,
                    unread: !voice && unread,
                    mentions,
                    read_only: !voice && !forum && !channel.permissions.contains(Permissions::SEND),
                    muted,
                    settings: edit.then(|| format!("cs:open:{}", channel.id)),
                    movable: reorder && !favorite,
                    private: Store::private(channel),
                    voice_since: crate::voice::voice_since(store, channel, now),
                },
                look,
            );
            rows.push(if voice {
                crate::voice_drag::zone(ui, row, channel.id, "channel")
            } else {
                row
            });
            if voice {
                rows.extend(crate::voice::participant_rows(
                    ui, store, server, channel.id, look,
                ));
            }
            // A forum's posts are listed where it opens, not here.
            if !forum {
                rows.extend(thread_rows(ui, store, channel, collapsed, look, now));
            }
        }
    }
    rows
}

/// The glyph of a kind of channel, when it is not `#` or a loudspeaker.
pub(crate) fn glyph(kind: ChannelKind) -> Option<limn::Icon> {
    match kind {
        ChannelKind::Forum => Some(limn::Icon::Chat),
        ChannelKind::Announcement => Some(limn::Icon::Custom(rune_components::forum::MEGAPHONE)),
        ChannelKind::Thread => Some(limn::Icon::Unread),
        _ => None,
    }
}

/// The active threads of `channel` the account follows, and the one open,
/// hung under its row; the rest are in its list of threads. Like channels,
/// a folded category keeps only the one open and those with something new.
fn thread_rows(
    ui: &Ui,
    store: &Store,
    channel: &Channel,
    collapsed: bool,
    look: &Look,
    now: Timestamp,
) -> Vec<Element> {
    let selected = crate::voice::selected(ui);
    let shown: Vec<_> = store
        .threads(channel.id, false)
        .into_iter()
        .filter_map(|thread| {
            let (unread, mentions, muted) = marks(ui, store, thread, now);
            let is_selected = selected == Some(thread.id);
            let followed = thread.thread.as_ref().is_some_and(|info| info.joined);
            let hidden = !followed
                || (collapsed && !unread && mentions == 0)
                || (muted && crate::account::hides_muted(ui, store, channel.server));
            (is_selected || !hidden).then_some(ChannelRow {
                id: format!("thread:{}", thread.id),
                name: thread.name.clone(),
                selected: is_selected,
                unread,
                mentions,
                muted,
                ..ChannelRow::default()
            })
        })
        .collect();
    let last = shown.len().saturating_sub(1);
    shown
        .into_iter()
        .enumerate()
        .map(|(index, row)| sidebar::thread(row, index == last, look))
        .collect()
}

/// Whether `source` may land on the row `target`: a channel among channels
/// of its kind (text and voice keep apart) or on a category, which takes it;
/// a category only among categories. Always in the same server.
fn accepts(store: &Store, source: ChannelId, target: &str) -> bool {
    let (Some(source), Some(target)) = (
        store.channel(source),
        row_of(target).and_then(|target| store.channel(target)),
    ) else {
        return false;
    };
    if source.id == target.id || source.server != target.server {
        return false;
    }
    match (source.kind, target.kind) {
        (ChannelKind::Category, ChannelKind::Category) => true,
        (ChannelKind::Category, _) => false,
        (_, ChannelKind::Category) => true,
        (one, other) => one.is_voice() == other.is_voice(),
    }
}

/// Where `source` lands when dropped on `target`: its category (`None` for
/// none) and its place among its siblings there, counted without it. On a
/// channel it takes that channel's place, after it when coming from above;
/// on a category, the first place of its kind inside it.
fn destination(
    store: &Store,
    source: ChannelId,
    target: ChannelId,
) -> Option<(Option<ChannelId>, usize)> {
    let dragged = store.channel(source)?;
    let landing = store.channel(target)?;
    let tree = store.channel_tree(dragged.server?);
    let before = |order: &[ChannelId]| {
        let from = order.iter().position(|id| *id == source)?;
        let to = order.iter().position(|id| *id == target)?;
        let others: Vec<ChannelId> = order.iter().copied().filter(|id| *id != source).collect();
        let at = others.iter().position(|id| *id == target)?;
        Some(if from < to { at + 1 } else { at })
    };
    if dragged.kind == ChannelKind::Category {
        let order: Vec<ChannelId> = tree
            .iter()
            .filter_map(|(category, _)| category.map(|category| category.id))
            .collect();
        return Some((None, before(&order)?));
    }
    if landing.kind == ChannelKind::Category {
        let (_, inside) = tree
            .iter()
            .find(|(category, _)| category.map(|category| category.id) == Some(target))?;
        let inside: Vec<&&Channel> = inside
            .iter()
            .filter(|channel| channel.id != source)
            .collect();
        let at = if dragged.kind == ChannelKind::Voice {
            inside
                .iter()
                .position(|channel| channel.kind == ChannelKind::Voice)
                .unwrap_or(inside.len())
        } else {
            0
        };
        return Some((Some(target), at));
    }
    // The order of every channel, so that crossing a category reads right.
    let flat: Vec<ChannelId> = tree
        .iter()
        .flat_map(|(_, channels)| channels.iter().map(|channel| channel.id))
        .collect();
    let (category, siblings) = tree
        .iter()
        .find(|(_, channels)| channels.iter().any(|channel| channel.id == target))?;
    let from = flat.iter().position(|id| *id == source)?;
    let to = flat.iter().position(|id| *id == target)?;
    let others: Vec<ChannelId> = siblings
        .iter()
        .map(|channel| channel.id)
        .filter(|id| *id != source)
        .collect();
    let at = others.iter().position(|id| *id == target)?;
    Some((
        category.map(|category| category.id),
        if from < to { at + 1 } else { at },
    ))
}

/// Where the line of a drag in course goes: on which row, above or below.
fn insertion(store: &Store, server: ServerId, drag: &Drag) -> Option<(String, DragMark)> {
    let over = drag.over.as_deref()?;
    let target = row_of(over)?;
    let dragged = store.channel(drag.source)?;
    let landing = store.channel(target)?;
    let tree = store.channel_tree(server);
    if dragged.kind != ChannelKind::Category {
        if landing.kind == ChannelKind::Category {
            return Some((over.to_owned(), DragMark::Below));
        }
        let flat: Vec<ChannelId> = tree
            .iter()
            .flat_map(|(_, channels)| channels.iter().map(|channel| channel.id))
            .collect();
        let from = flat.iter().position(|id| *id == drag.source)?;
        let to = flat.iter().position(|id| *id == target)?;
        let mark = if from < to {
            DragMark::Below
        } else {
            DragMark::Above
        };
        return Some((over.to_owned(), mark));
    }
    let order: Vec<ChannelId> = tree
        .iter()
        .filter_map(|(category, _)| category.map(|category| category.id))
        .collect();
    let from = order.iter().position(|id| *id == drag.source)?;
    let to = order.iter().position(|id| *id == target)?;
    if from > to {
        return Some((over.to_owned(), DragMark::Above));
    }
    // After a category is before the next one, or at the very end.
    match order.get(to + 1) {
        Some(next) => Some((format!("category:{next}"), DragMark::Above)),
        None => Some((over.to_owned(), DragMark::Below)),
    }
}

/// Ask for `channel` to be put in `category` at `index` among the siblings
/// the account sees there, unless it is already there: every channel that
/// changes place, at once. Into another category, it asks first whether
/// the channel takes that category's permissions, when it may.
fn move_to(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    category: Option<ChannelId>,
    index: usize,
) -> Vec<Intent> {
    let Some((server, moves)) = moves_to(store, channel, category, index) else {
        return Vec::new();
    };
    let asks = store
        .channel(channel)
        .and_then(|found| category.filter(|category| offers_sync(store, found, Some(*category))));
    if let Some(category) = asks {
        ui.channel_list.dialog = Some(ListDialog::Sync {
            channel,
            category,
            index,
        });
        return Vec::new();
    }
    vec![Intent::Command(Command::ReorderChannels { server, moves })]
}

/// The server and the moves that put `channel` in `category` at `index`
/// among the siblings the account sees there, as the store has them now;
/// `None` when nothing changes or the account may not reorder them.
fn moves_to(
    store: &Store,
    channel: ChannelId,
    category: Option<ChannelId>,
    index: usize,
) -> Option<(ServerId, Vec<rune_model::ChannelMove>)> {
    let found = store.channel(channel)?;
    let server = found
        .server
        .filter(|server| reorders(store, Some(*server)))?;
    let current = rest::layout(store, server);
    let mut slots = current.clone();
    let from = slots.iter().position(|slot| slot.channel == channel)?;
    let moving = slots.remove(from);
    let parent = category.filter(|_| !moving.category);
    let at = slots
        .iter()
        .enumerate()
        .filter(|(_, slot)| {
            slot.category == moving.category && (moving.category || slot.parent == parent)
        })
        .nth(index)
        .map_or(slots.len(), |(at, _)| at);
    slots.insert(at, rest::Slot { parent, ..moving });
    let slots = rest::normalize(slots);
    if slots == current {
        return None;
    }
    let moves = rest::layout_moves(store, server, &slots);
    (!moves.is_empty()).then_some((server, moves))
}

/// Whether moving `channel` into `category` first asks whether it takes the
/// category's permissions, as the official client asks (REA): when the
/// account may change permissions in both, the channel does not have the
/// category's yet, and it had its old category's (or none).
fn offers_sync(store: &Store, channel: &Channel, category: Option<ChannelId>) -> bool {
    let Some(category) = category
        .filter(|category| channel.parent != Some(*category))
        .and_then(|category| store.channel(category))
    else {
        return false;
    };
    let may = |id: ChannelId| {
        store
            .abilities(id)
            .is_some_and(|abilities| abilities.manage_permissions.allowed())
    };
    let synced = |with: &Channel| crate::channel_settings::sync_commands(channel, with).is_empty();
    channel.kind != ChannelKind::Category
        && may(channel.id)
        && may(category.id)
        && !synced(category)
        && channel
            .parent
            .is_none_or(|old| store.channel(old).is_some_and(synced))
}

/// The answer to the question before moving into another category: move
/// taking its permissions, move keeping its own, or nothing.
fn answer_sync(ui: &mut Ui, store: &Store, sync: Option<bool>) -> Vec<Intent> {
    let Some(ListDialog::Sync {
        channel,
        category,
        index,
    }) = ui.channel_list.dialog.take()
    else {
        return Vec::new();
    };
    let Some(sync) = sync else {
        return Vec::new();
    };
    let Some((server, mut moves)) = moves_to(store, channel, Some(category), index) else {
        return Vec::new();
    };
    for step in moves.iter_mut().filter(|step| step.channel == channel) {
        step.lock_permissions = sync && step.parent.is_some();
    }
    vec![Intent::Command(Command::ReorderChannels { server, moves })]
}

/// A secondary click on the control `target`, at `at` in the window: open
/// the menu of the channel, category or list there. `None` when it is not a
/// part of the list, for the tree's own menus to answer.
pub fn secondary_click(ui: &mut Ui, store: &Store, target: &str, at: Point) -> Option<Vec<Intent>> {
    let server = ui.server?;
    let of_server = |channel: u64| {
        let channel = ChannelId(channel);
        store
            .channel(channel)
            .filter(|found| found.server == Some(server))
            .map(|found| (channel, found.kind == ChannelKind::Category))
    };
    let found = if target == "channels" {
        Target::List(server)
    } else if let Some(rest) = target.strip_prefix("newchan:") {
        let category = rest.split_once(':')?.1.parse().ok()?;
        Target::Category(of_server(category)?.0)
    } else {
        let id = ["channel:", "voice:", "category:", "cs:open:"]
            .iter()
            .find_map(|prefix| number(target, prefix))?;
        match of_server(id)? {
            (category, true) => Target::Category(category),
            (channel, false) => Target::Channel(channel),
        }
    };
    ui.channel_list.menu = Some((found, (at.x, at.y)));
    ui.channel_list.bell = None;
    ui.server_menu.open = false;
    Some(Vec::new())
}

/// Answer what the list, its menus and dialogs and the header's tools
/// report. `None` for actions of other parts.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    match action {
        UiAction::DragStarted { id, .. } => {
            let source = row_of(id)?;
            let server = store.channel(source).and_then(|found| found.server);
            ui.channel_list.drag = reorders(store, server).then_some(Drag { source, over: None });
            ui.channel_list.menu = None;
            Some(Vec::new())
        }
        UiAction::DragMoved { id, over, .. } => {
            row_of(id)?;
            if let Some(drag) = &mut ui.channel_list.drag {
                let source = drag.source;
                drag.over = over.clone().filter(|over| accepts(store, source, over));
            }
            Some(Vec::new())
        }
        UiAction::Dropped { id, target, .. } => {
            row_of(id)?;
            let Some(drag) = ui.channel_list.drag.take() else {
                return Some(Vec::new());
            };
            let landing = target
                .as_deref()
                .filter(|target| accepts(store, drag.source, target))
                .and_then(row_of);
            Some(
                landing
                    .and_then(|landing| destination(store, drag.source, landing))
                    .map(|(category, index)| move_to(ui, store, drag.source, category, index))
                    .unwrap_or_default(),
            )
        }
        UiAction::DragCancelled { id } => {
            row_of(id)?;
            ui.channel_list.drag = None;
            Some(Vec::new())
        }
        UiAction::Clicked { id } => clicked(ui, store, id, now),
        UiAction::InputChanged { id, value } if id == ids::DUPLICATE_NAME => {
            if let Some(ListDialog::Duplicate { name, .. }) = &mut ui.channel_list.dialog {
                *name = value.clone();
            }
            Some(Vec::new())
        }
        UiAction::Submitted { id, value } if id == ids::DUPLICATE_NAME => {
            if let Some(ListDialog::Duplicate { name, .. }) = &mut ui.channel_list.dialog {
                *name = value.clone();
            }
            Some(duplicate(ui, store))
        }
        _ => None,
    }
}

fn clicked(ui: &mut Ui, store: &mut Store, id: &str, now: Instant) -> Option<Vec<Intent>> {
    // An item that only opens a submenu keeps the menu open.
    if id.starts_with(ids::SUBMENU) {
        return Some(Vec::new());
    }
    // Choosing anything closes the menus, items of other parts included.
    ui.channel_list.menu = None;
    if id != ids::BELL {
        ui.channel_list.bell = None;
    }
    let channel = ui.channel();
    let save = |key: &str, value: Option<String>| {
        vec![Intent::SaveSetting {
            key: key.into(),
            value,
        }]
    };
    match id {
        ids::BELL => {
            ui.channel_list.bell = match ui.channel_list.bell {
                Some(_) => None,
                None => channel,
            };
            return Some(Vec::new());
        }
        // Any other click closed it already: see `crate::answer_action`.
        ids::MORE => {
            ui.channel_list.more = match ui.channel_list.more {
                Some(_) => None,
                None => channel,
            };
            return Some(Vec::new());
        }
        ids::TOPIC => {
            ui.channel_list.topic = channel;
            return Some(Vec::new());
        }
        ids::TOPIC_CLOSE => {
            ui.channel_list.topic = None;
            return Some(Vec::new());
        }
        ids::PINS => {
            if ui.channel_list.pins.is_some() && ui.channel_list.pins == channel {
                return Some(close_pins(ui));
            }
            let Some(channel) = channel else {
                return Some(Vec::new());
            };
            return Some(load_pins(ui, store, channel));
        }
        ids::PINS_RETRY => {
            let Some(channel) = channel.filter(|channel| ui.channel_list.pins == Some(*channel))
            else {
                return Some(Vec::new());
            };
            if !ui
                .channel_list
                .pins_load
                .as_ref()
                .is_some_and(|load| matches!(load.phase, PinsPhase::Failed(_)))
            {
                return Some(Vec::new());
            }
            return Some(load_pins(ui, store, channel));
        }
        ids::PINS_DISMISS => {
            return Some(close_pins(ui));
        }
        ids::DUPLICATE_SUBMIT => return Some(duplicate(ui, store)),
        ids::DUPLICATE_CLOSE | ids::DELETE_CANCEL => {
            ui.channel_list.dialog = None;
            return Some(Vec::new());
        }
        ids::SYNC_CONFIRM => return Some(answer_sync(ui, store, Some(true))),
        ids::SYNC_KEEP => return Some(answer_sync(ui, store, Some(false))),
        ids::SYNC_CANCEL => return Some(answer_sync(ui, store, None)),
        ids::DELETE_CONFIRM => {
            return Some(match ui.channel_list.dialog.take() {
                Some(ListDialog::Delete(channel)) => {
                    vec![Intent::Command(Command::DeleteChannel { channel })]
                }
                _ => Vec::new(),
            });
        }
        "cl:hide-muted" if store.synced_notifications() => {
            let server = ui.server;
            let hide = !crate::account::hides_muted(ui, store, server);
            return Some(crate::account::notify_command(
                ui,
                store,
                server,
                rune_model::NotificationChange::HideMuted(hide),
                now,
            ));
        }
        "cl:hide-muted" => {
            ui.channel_list.hide_muted = !ui.channel_list.hide_muted;
            return Some(save(
                HIDE_MUTED_SETTING,
                Some(ui.channel_list.hide_muted.to_string()),
            ));
        }
        _ => {}
    }
    if id.starts_with(ids::MENU_DISMISS)
        || id.starts_with(ids::BELL_DISMISS)
        || id.starts_with(ids::MORE_DISMISS)
    {
        return Some(Vec::new());
    }
    if let Some(category) = number(id, "category:").or_else(|| number(id, "cl:collapse:")) {
        let category = ChannelId(category);
        if !ui.collapsed.remove(&category) {
            ui.collapsed.insert(category);
        }
        return Some(save(COLLAPSED_SETTING, kept_collapsed(ui)));
    }
    if let Some(server) = number(id, "cl:collapse-all:") {
        for (category, _) in store.channel_tree(ServerId(server)) {
            if let Some(category) = category {
                ui.collapsed.insert(category.id);
            }
        }
        return Some(save(COLLAPSED_SETTING, kept_collapsed(ui)));
    }
    if let Some(message) = number(id, ids::PIN_JUMP) {
        let channel = channel?;
        if !listed_pin(ui, store, channel, MessageId(message)) {
            return Some(Vec::new());
        }
        let mut intents = close_pins(ui);
        intents.extend(jump(ui, store, channel, MessageId(message), now));
        return Some(intents);
    }
    if let Some(message) = number(id, ids::PIN_REMOVE) {
        let channel = channel?;
        if !listed_pin(ui, store, channel, MessageId(message)) {
            return Some(Vec::new());
        }
        return Some(pin(ui, store, channel, MessageId(message), false, now));
    }
    if let Some(rest) = id.strip_prefix("msg:") {
        let (message, what) = rest.split_once(':')?;
        let pinned = match what {
            "pin" => true,
            "unpin" => false,
            _ => return None,
        };
        let message = MessageId(message.parse().ok()?);
        return Some(pin(ui, store, channel?, message, pinned, now));
    }
    let rest = id.strip_prefix("cl:")?;
    let (what, rest) = rest.split_once(':')?;
    let (target, argument) = rest.split_once(':').unwrap_or((rest, ""));
    let target = ChannelId(target.parse().ok()?);
    Some(match what {
        // Desktop (P6).
        "favorite" => {
            ui.channel_list.menu = None;
            crate::desktop::toggle_favorite(ui, target)
        }
        "read" => mark_read(store, target),
        "mute" => {
            let (_, _, length) = MUTE_DURATIONS.iter().find(|(key, _, _)| *key == argument)?;
            let until = length.map_or(FOREVER, |length| Timestamp(wall_now().0 + length));
            change_notify(
                ui,
                store,
                target,
                rune_model::NotificationChange::ChannelMute(target, Some(until)),
                now,
            )
        }
        "unmute" => change_notify(
            ui,
            store,
            target,
            rune_model::NotificationChange::ChannelMute(target, None),
            now,
        ),
        "notify" => {
            let level = match argument {
                "inherit" => None,
                key => Some(NotifyLevel::from_key(key)?),
            };
            change_notify(
                ui,
                store,
                target,
                rune_model::NotificationChange::ChannelLevel(target, level),
                now,
            )
        }
        "move" => move_by_menu(ui, store, target, argument),
        "duplicate" => {
            let found = store.channel(target)?;
            if let Some(reason) = denied_channels(store, found.server) {
                ui.notify(t("No se puede duplicar el canal"), Some(reason), true, now);
                return Some(Vec::new());
            }
            let name = found.name.clone();
            ui.channel_list.dialog = Some(ListDialog::Duplicate {
                channel: target,
                name: name.clone(),
            });
            vec![
                Intent::SetInput {
                    id: ids::DUPLICATE_NAME.into(),
                    value: name,
                },
                Intent::Focus(ids::DUPLICATE_NAME.into()),
            ]
        }
        "create" => {
            let found = store.channel(target)?.clone();
            let server = found.server?;
            let open = match found.parent {
                Some(parent) => format!("newchan:{server}:{parent}"),
                None => format!("newchan:{server}"),
            };
            let mut intents = server_menu::clicked(ui, store, &open, now);
            if found.kind == ChannelKind::Voice {
                intents.extend(server_menu::clicked(
                    ui,
                    store,
                    menu_ids::CHANNEL_VOICE,
                    now,
                ));
            }
            intents
        }
        "delete" => {
            let permit = store.abilities(target).map(|abilities| abilities.manage);
            match permit.as_ref().and_then(Permit::reason) {
                Some(reason) => ui.notify(
                    t("No se puede eliminar"),
                    Some(reason.to_owned()),
                    true,
                    now,
                ),
                None if permit.is_some() => {
                    ui.channel_list.dialog = Some(ListDialog::Delete(target))
                }
                None => {}
            }
            Vec::new()
        }
        "copy-id" => {
            ui.notify(t("ID copiado"), None, false, now);
            vec![Intent::Copy(target.0.to_string())]
        }
        _ => return None,
    })
}

/// Why the account cannot make channels in `server`, if it cannot.
fn denied_channels(store: &Store, server: Option<ServerId>) -> Option<String> {
    let abilities = store.server_abilities(server?)?;
    abilities.manage_channels.reason().map(str::to_owned)
}

/// Why the account cannot move `server`'s channels, if it cannot.
fn denied_order(store: &Store, server: Option<ServerId>) -> Option<String> {
    let abilities = store.server_abilities(server?)?;
    abilities.reorder_channels.reason().map(str::to_owned)
}

/// Mark `channel` read, or every channel of a category.
fn mark_read(store: &mut Store, target: ChannelId) -> Vec<Intent> {
    let Some(found) = store.channel(target) else {
        return Vec::new();
    };
    let channels: Vec<ChannelId> = match (found.kind, found.server) {
        (ChannelKind::Category, Some(server)) => store
            .channel_tree(server)
            .into_iter()
            .find(|(category, _)| category.map(|category| category.id) == Some(target))
            .map(|(_, channels)| channels.iter().map(|channel| channel.id).collect())
            .unwrap_or_default(),
        _ => vec![target],
    };
    channels
        .into_iter()
        .filter_map(|channel| store.mark_read_command(channel).map(Intent::Command))
        .collect()
}

/// «Mover a…»: `to:{category}` (or `to:none`), `first` or `last`.
fn move_by_menu(ui: &mut Ui, store: &Store, channel: ChannelId, how: &str) -> Vec<Intent> {
    let Some(found) = store.channel(channel) else {
        return Vec::new();
    };
    let Some(server) = found.server else {
        return Vec::new();
    };
    let tree = store.channel_tree(server);
    if found.kind == ChannelKind::Category {
        let count = tree
            .iter()
            .filter(|(category, _)| category.is_some())
            .count();
        return match how {
            "first" => move_to(ui, store, channel, None, 0),
            "last" => move_to(ui, store, channel, None, count.saturating_sub(1)),
            _ => Vec::new(),
        };
    }
    if let Some(category) = how.strip_prefix("to:") {
        let category = match category {
            "none" => None,
            id => match id.parse() {
                Ok(id) => Some(ChannelId(id)),
                Err(_) => return Vec::new(),
            },
        };
        let count = tree
            .iter()
            .find(|(slot, _)| slot.map(|slot| slot.id) == category)
            .map_or(0, |(_, channels)| channels.len());
        return move_to(ui, store, channel, category, count);
    }
    // Its category as listed: none for one whose category it cannot see.
    let Some((current, siblings)) = tree
        .iter()
        .find(|(_, channels)| channels.iter().any(|sibling| sibling.id == channel))
    else {
        return Vec::new();
    };
    let (current, count) = (current.map(|category| category.id), siblings.len());
    match how {
        "first" => move_to(ui, store, channel, current, 0),
        "last" => move_to(ui, store, channel, current, count.saturating_sub(1)),
        _ => Vec::new(),
    }
}

/// Change the notification settings of `channel` (or a category, or a
/// direct or group conversation): ask the provider when it keeps them, else
/// keep them on this machine.
pub(crate) fn change_notify(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    change: rune_model::NotificationChange,
    now: Instant,
) -> Vec<Intent> {
    if store.synced_notifications() {
        let server = store.channel(channel).and_then(|found| found.server);
        return crate::account::notify_command(ui, store, server, change, now);
    }
    let notify = ui.channel_list.notify.entry(channel).or_default();
    match change {
        rune_model::NotificationChange::ChannelMute(_, until) => notify.muted_until = until,
        rune_model::NotificationChange::ChannelLevel(_, level) => notify.level = level,
        _ => {}
    }
    if *notify == ChannelNotify::default() {
        ui.channel_list.notify.remove(&channel);
    }
    vec![Intent::SaveSetting {
        key: NOTIFY_SETTING.into(),
        value: kept_notify(ui),
    }]
}

/// Make the copy the duplicate dialog describes.
fn duplicate(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let Some(ListDialog::Duplicate { channel, name }) = &ui.channel_list.dialog else {
        return Vec::new();
    };
    let Some(original) = store.channel(*channel) else {
        return Vec::new();
    };
    let Some(server) = original.server else {
        return Vec::new();
    };
    let form = DuplicateForm {
        original: original.name.clone(),
        voice: original.kind == ChannelKind::Voice,
        category: None,
        name: name.clone(),
    };
    let name = form.final_name();
    if name.is_empty() {
        return Vec::new();
    }
    let (source, kind, parent) = (original.id, original.kind, original.parent);
    ui.channel_list.dialog = None;
    let request = ui.fresh_request();
    ui.channel_list.duplicates.insert(request, source);
    vec![Intent::Command(Command::CreateChannel {
        request,
        server,
        kind,
        name,
        parent,
        private: false,
    })]
}

pub(crate) fn close_pins(ui: &mut Ui) -> Vec<Intent> {
    let cancel = ui
        .channel_list
        .pins_load
        .take()
        .filter(|load| matches!(load.phase, PinsPhase::Loading))
        .map(|load| Intent::Command(Command::Cancel(load.request)));
    ui.channel_list.pins = None;
    cancel.into_iter().collect()
}

fn load_pins(ui: &mut Ui, store: &Store, channel: ChannelId) -> Vec<Intent> {
    if !store
        .abilities(channel)
        .is_some_and(|abilities| abilities.view_pins.allowed())
    {
        return close_pins(ui);
    }
    let mut intents = close_pins(ui);
    let request = ui.fresh_request();
    ui.channel_list.pins = Some(channel);
    ui.channel_list.pins_load = Some(PinsLoad {
        request,
        phase: PinsPhase::Loading,
    });
    intents.push(Intent::Command(Command::LoadPins { request, channel }));
    intents
}

fn listed_pin(ui: &Ui, store: &Store, channel: ChannelId, message: MessageId) -> bool {
    ui.channel_list.pins == Some(channel)
        && ui
            .channel_list
            .pins_load
            .as_ref()
            .is_some_and(|load| matches!(load.phase, PinsPhase::Ready))
        && store
            .abilities(channel)
            .is_some_and(|abilities| abilities.view_pins.allowed())
        && store
            .pins(channel)
            .is_some_and(|pins| pins.iter().any(|pin| pin.id == message))
}

/// Pin `message` of `channel`, or take it out, if the account may.
fn pin(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    message: MessageId,
    pinned: bool,
    now: Instant,
) -> Vec<Intent> {
    let Some(abilities) = store.abilities(channel) else {
        return Vec::new();
    };
    if store
        .timeline(channel)
        .and_then(|timeline| timeline.get(message))
        .is_none()
        && !store
            .pins(channel)
            .is_some_and(|pins| pins.iter().any(|pin| pin.id == message))
    {
        return Vec::new();
    }
    if let Some(reason) = abilities.pin.reason() {
        ui.notify(
            t("No se pueden fijar mensajes"),
            Some(reason.to_owned()),
            true,
            now,
        );
        return Vec::new();
    }
    vec![Intent::Command(if pinned {
        Command::PinMessage { channel, message }
    } else {
        Command::UnpinMessage { channel, message }
    })]
}

/// Reveal a retained message or ask for one bounded provider context.
fn jump(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    message: MessageId,
    _now: Instant,
) -> Vec<Intent> {
    crate::open_message(
        ui,
        store,
        channel,
        message,
        rune_model::HistorySource::Provider,
    )
}

/// Answer what an event of the provider changed: a copy made gets the
/// original's permissions and settings and opens; a page that arrives may
/// hold a pinned message being gone to; a channel that goes closes what was
/// open on it.
pub(crate) fn on_applied(
    ui: &mut Ui,
    store: &mut Store,
    applied: &Applied,
    _now: Instant,
) -> Vec<Intent> {
    let mut cancelled = Vec::new();
    if matches!(applied, Applied::Ready)
        || ui.channel_list.pins.is_some_and(|channel| {
            !store
                .abilities(channel)
                .is_some_and(|abilities| abilities.view_pins.allowed())
        })
    {
        cancelled = close_pins(ui);
    }
    let intents = match applied {
        Applied::Pins { channel, request } if ui.channel_list.pins == Some(*channel) => {
            if let Some(load) =
                ui.channel_list.pins_load.as_mut().filter(|load| {
                    load.request == *request && matches!(load.phase, PinsPhase::Loading)
                })
            {
                load.phase = PinsPhase::Ready;
            }
            Vec::new()
        }
        Applied::Failed {
            failure: Failure::Pins { channel, request },
            reason,
            ..
        } if ui.channel_list.pins == Some(*channel) => {
            if let Some(load) =
                ui.channel_list.pins_load.as_mut().filter(|load| {
                    load.request == *request && matches!(load.phase, PinsPhase::Loading)
                })
            {
                load.phase = PinsPhase::Failed(reason.clone());
            }
            Vec::new()
        }
        Applied::Created {
            request,
            created: Created::Channel(made),
        } => {
            let Some(source) = ui.channel_list.duplicates.remove(request) else {
                return cancelled;
            };
            let (Some(original), Some(copy)) = (store.channel(source), store.channel(*made)) else {
                return cancelled;
            };
            let (made, server, opens) = (*made, copy.server, copy.kind.has_messages());
            let mut intents: Vec<Intent> = original
                .overwrites
                .iter()
                .map(|overwrite| {
                    Intent::Command(Command::SetOverwrite {
                        channel: made,
                        overwrite: *overwrite,
                    })
                })
                .collect();
            let edit = ChannelEdit {
                topic: original.topic.clone().map(Some),
                slowmode_secs: (original.slowmode_secs > 0).then_some(original.slowmode_secs),
                nsfw: original.nsfw.then_some(true),
                ..ChannelEdit::default()
            };
            if edit != ChannelEdit::default() {
                intents.push(Intent::Command(Command::EditChannel {
                    channel: made,
                    edit,
                }));
            }
            if opens {
                intents.extend(open(ui, store, server, made));
            }
            intents
        }
        Applied::ChannelRemoved(gone) => {
            let list = &mut ui.channel_list;
            let on_it = |target: Target| match target {
                Target::Channel(channel) | Target::Category(channel) => channel == *gone,
                Target::List(_) => false,
            };
            if list.menu.is_some_and(|(target, _)| on_it(target)) {
                list.menu = None;
            }
            // Without the category it was going into, nothing is asked.
            if matches!(
                list.dialog,
                Some(ListDialog::Duplicate { channel, .. } | ListDialog::Delete(channel)) if channel == *gone
            ) || matches!(
                list.dialog,
                Some(ListDialog::Sync { channel, category, .. }) if channel == *gone || category == *gone
            ) {
                list.dialog = None;
            }
            if list.drag.as_ref().is_some_and(|drag| drag.source == *gone) {
                list.drag = None;
            }
            Vec::new()
        }
        _ => Vec::new(),
    };
    cancelled.extend(intents);
    cancelled
}

/// The mute item of `channel`'s menus: its durations, or, while it is muted,
/// turning it back on and until when it lasts.
fn mute_item(ui: &Ui, store: &Store, channel: &Channel, now: Timestamp) -> Item {
    let category = channel.kind == ChannelKind::Category;
    let notify = crate::account::channel_notify(ui, store, channel.id);
    if notify.muted(now) {
        let until = match notify.muted_until.filter(|until| *until != FOREVER) {
            Some(until) => tr!(
                "Silenciado hasta: {until}",
                until = rune_components::time::stamp(until, now)
            ),
            None => t("Silenciado hasta reactivarlo").into(),
        };
        return Item::new(
            format!("cl:unmute:{}", channel.id),
            if category {
                t("Desactivar silencio de la categoría")
            } else {
                t("Desactivar silencio del canal")
            },
        )
        .detail(until);
    }
    Item::new(
        format!("{}mute:{}", ids::SUBMENU, channel.id),
        if category {
            t("Silenciar categoría")
        } else {
            t("Silenciar canal")
        },
    )
    .submenu(
        MUTE_DURATIONS
            .iter()
            .zip(MUTE_LABELS)
            .map(|((key, _, _), label)| {
                Item::new(format!("cl:mute:{}:{key}", channel.id), t(label)).into()
            })
            .collect(),
    )
}

/// The levels `channel` (or a category) can mark with, its own chosen.
fn notify_entries(ui: &Ui, store: &Store, channel: &Channel) -> Vec<Entry> {
    let own = crate::account::channel_notify(ui, store, channel.id).level;
    let first = if channel.kind == ChannelKind::Category || channel.parent.is_none() {
        t("Usar el servidor predeterminado")
    } else {
        t("Usar la categoría predeterminada")
    };
    let item = |key: &str, label: &str, chosen: bool| -> Entry {
        Item::new(format!("cl:notify:{}:{key}", channel.id), label)
            .radio(chosen)
            .into()
    };
    let mut entries = vec![
        Item::new(format!("cl:notify:{}:inherit", channel.id), first)
            .detail(level_label(inherited(ui, store, channel)))
            .radio(own.is_none())
            .into(),
    ];
    for level in [
        NotifyLevel::All,
        NotifyLevel::Mentions,
        NotifyLevel::Nothing,
    ] {
        entries.push(item(level.key(), level_label(level), own == Some(level)));
    }
    entries
}

/// The «Ajustes de notificaciones» item, its submenu the levels.
fn notify_item(ui: &Ui, store: &Store, channel: &Channel, now: Timestamp) -> Item {
    let (level, _) = effective(ui, store, channel, now);
    Item::new(
        format!("{}notify:{}", ids::SUBMENU, channel.id),
        t("Ajustes de notificaciones"),
    )
    .detail(level_label(level))
    .submenu(notify_entries(ui, store, channel))
}

/// «Mover a…»: to another category and to either end of its own, for a
/// channel; to either end of the list, for a category. Where it already is
/// is off.
fn move_item(store: &Store, channel: &Channel) -> Item {
    let id = channel.id;
    let tree = channel
        .server
        .map(|server| store.channel_tree(server))
        .unwrap_or_default();
    let end = |key: &str, label: &str, already: Option<&str>| -> Entry {
        Item::new(format!("cl:move:{id}:{key}"), label)
            .denied(already.map(str::to_owned))
            .into()
    };
    let entries = if channel.kind == ChannelKind::Category {
        let order: Vec<ChannelId> = tree
            .iter()
            .filter_map(|(category, _)| category.map(|category| category.id))
            .collect();
        let at = order.iter().position(|other| *other == id).unwrap_or(0);
        vec![
            end(
                "first",
                t("Primero de la lista"),
                (at == 0).then(|| t("Ya es la primera")),
            ),
            end(
                "last",
                t("Último de la lista"),
                (at + 1 >= order.len()).then(|| t("Ya es la última")),
            ),
        ]
    } else {
        let mut entries = vec![Entry::Heading(t("Categoría").into())];
        let here = |category: Option<ChannelId>| {
            (channel.parent == category).then(|| t("Ya está acá").to_owned())
        };
        entries.push(
            Item::new(format!("cl:move:{id}:to:none"), t("Sin categoría"))
                .denied(here(None))
                .into(),
        );
        for (category, _) in &tree {
            if let Some(category) = category {
                entries.push(
                    Item::new(
                        format!("cl:move:{id}:to:{}", category.id),
                        category.name.clone(),
                    )
                    .denied(here(Some(category.id)))
                    .into(),
                );
            }
        }
        let siblings = tree
            .iter()
            .find(|(_, channels)| channels.iter().any(|sibling| sibling.id == id))
            .map_or(1, |(_, channels)| channels.len());
        let at = tree
            .iter()
            .find_map(|(_, channels)| channels.iter().position(|sibling| sibling.id == id))
            .unwrap_or(0);
        entries.push(Entry::Separator);
        entries.push(end(
            "first",
            t("Primero de la categoría"),
            (at == 0).then(|| t("Ya es el primero")),
        ));
        entries.push(end(
            "last",
            t("Último de la categoría"),
            (at + 1 >= siblings).then(|| t("Ya es el último")),
        ));
        entries
    };
    Item::new(format!("{}move:{id}", ids::SUBMENU), t("Mover a…"))
        .submenu(entries)
        .denied(denied_order(store, channel.server))
}

/// «Copiar ID» for `id`, offered in developer mode only.
fn copy_id(ui: &Ui, id: ChannelId, label: &str) -> Vec<Entry> {
    if !ui.prefs.developer {
        return Vec::new();
    }
    vec![Item::new(format!("cl:copy-id:{id}"), label).into()]
}

/// The entries of the context menu on `target`, at `now`.
fn menu_entries(ui: &Ui, store: &Store, target: Target, now: Timestamp) -> Vec<Entry> {
    let denied = |permit: Option<&Permit>| permit.and_then(Permit::reason).map(str::to_owned);
    let read_label = if store.read_sync() {
        t("Marcar como leído")
    } else {
        t("Marcar como leído (solo en RUNE)")
    };
    match target {
        Target::List(server) => {
            let manage = store
                .server_abilities(server)
                .map(|abilities| abilities.manage_channels);
            let read = crate::server_menu::read_item(store, server);
            grouped(vec![
                vec![Item::new(read.id, t("Marcar el servidor como leído"))
                    .denied(read.denied)
                    .into()],
                vec![
                    Item::new("cl:hide-muted", t("Ocultar los canales silenciados"))
                        .checked(crate::account::hides_muted(ui, store, Some(server)))
                        .into(),
                    // Desktop (P6).
                    Item::new("cl:show-hidden", t("Mostrar los canales ocultos"))
                        .checked(ui.prefs.show_hidden)
                        .into(),
                ],
                vec![
                    Item::new(format!("newchan:{server}"), t("Crear un canal"))
                        .denied(denied(manage.as_ref()))
                        .into(),
                    Item::new(format!("newcat:{server}"), t("Crear categoría"))
                        .denied(denied(manage.as_ref()))
                        .into(),
                ],
            ])
        }
        Target::Category(id) => {
            let Some(category) = store.channel(id) else {
                return Vec::new();
            };
            let manage = store.abilities(id).map(|abilities| abilities.manage);
            let unread = category
                .server
                .map(|server| store.channel_tree(server))
                .unwrap_or_default()
                .into_iter()
                .find(|(found, _)| found.map(|found| found.id) == Some(id))
                .is_some_and(|(_, channels)| {
                    channels.iter().any(|channel| {
                        let read = store.read(channel.id);
                        read.unread() || read.mentions > 0
                    })
                });
            let server = category.server.unwrap_or_default();
            grouped(vec![
                vec![Item::new(format!("cl:read:{id}"), read_label)
                    .denied((!unread).then(|| t("No hay nada sin leer").to_owned()))
                    .into()],
                vec![
                    Item::new(format!("cl:collapse:{id}"), t("Minimizar categoría"))
                        .checked(ui.collapsed.contains(&id))
                        .into(),
                    Item::new(
                        format!("cl:collapse-all:{server}"),
                        t("Minimizar todas las categorías"),
                    )
                    .into(),
                ],
                vec![
                    mute_item(ui, store, category, now).into(),
                    notify_item(ui, store, category, now).into(),
                ],
                vec![move_item(store, category).into()],
                vec![
                    Item::new(format!("cs:open:{id}"), t("Editar categoría"))
                        .denied(denied(manage.as_ref()))
                        .into(),
                    Item::new(format!("cl:delete:{id}"), t("Eliminar categoría"))
                        .danger()
                        .denied(denied(manage.as_ref()))
                        .into(),
                ],
                copy_id(ui, id, t("Copiar ID de la categoría")),
            ])
        }
        Target::Channel(id) => {
            let Some(channel) = store.channel(id) else {
                return Vec::new();
            };
            let voice = channel.kind.is_voice();
            let manage = store.abilities(id).map(|abilities| abilities.manage);
            let create = denied_channels(store, channel.server);
            let read = store.read(id);
            let mut first = Vec::new();
            if !voice {
                first.push(
                    Item::new(format!("cl:read:{id}"), read_label)
                        .denied(
                            (!read.unread() && read.mentions == 0)
                                .then(|| t("No hay nada sin leer").to_owned()),
                        )
                        .into(),
                );
            }
            // Desktop (P6): a favorite of the server, first in its list.
            first.push(
                Item::new(format!("cl:favorite:{id}"), t("Favorito"))
                    .checked(ui.prefs.favorites.contains(&id))
                    .into(),
            );
            let mut notify = vec![mute_item(ui, store, channel, now).into()];
            if !voice {
                notify.push(notify_item(ui, store, channel, now).into());
            }
            grouped(vec![
                first,
                notify,
                vec![move_item(store, channel).into()],
                vec![
                    Item::new(format!("cs:open:{id}"), t("Editar canal"))
                        .denied(denied(manage.as_ref()))
                        .into(),
                    Item::new(format!("cl:duplicate:{id}"), t("Duplicar canal"))
                        .denied(create.clone())
                        .into(),
                    Item::new(
                        format!("cl:create:{id}"),
                        if voice {
                            t("Crear un canal de voz")
                        } else {
                            t("Crear un canal de texto")
                        },
                    )
                    .denied(create)
                    .into(),
                    Item::new(format!("cl:delete:{id}"), t("Eliminar canal"))
                        .danger()
                        .denied(denied(manage.as_ref()))
                        .into(),
                ],
                copy_id(ui, id, t("Copiar ID del canal")),
            ])
        }
    }
}

/// The tools of the header of `channel`: the bell of a server's channel and
/// the pin of any conversation with messages.
pub(crate) fn header_tools(
    ui: &Ui,
    store: &Store,
    channel: ChannelId,
    look: &Look,
    now: Timestamp,
    // A narrow header: the inbox goes in its «más» (`header_more`).
    narrow: bool,
) -> Vec<Element> {
    let Some(conversation) = store.channel(channel) else {
        return Vec::new();
    };
    if !conversation.kind.has_messages() {
        return Vec::new();
    }
    let mut tools = Vec::new();
    if !narrow {
        tools.push(
            limn::Button::new("inbox:open", t("Bandeja"), look.menu_palette())
                .variant(limn::ButtonVariant::Ghost)
                .size(limn::ControlSize::Compact)
                .render(),
        );
    }
    if conversation.server.is_some() {
        let (_, muted) = effective(ui, store, conversation, now);
        let mut silence: Vec<Entry> = vec![mute_item(ui, store, conversation, now).into()];
        // The bell shows a silenced thread's silence: it ends it too (2026-10-18).
        silence.extend(crate::threads::bell_entry(store, conversation, now));
        let entries = grouped(vec![silence, notify_entries(ui, store, conversation)]);
        tools.push(parts::bell(
            ui.channel_list.bell == Some(channel),
            muted,
            entries,
            look,
        ));
    }
    let abilities = store.abilities(channel);
    let denied = abilities
        .as_ref()
        .and_then(|abilities| abilities.view_pins.reason());
    let open = ui.channel_list.pins == Some(channel) && denied.is_none();
    let load = open.then_some(ui.channel_list.pins_load.as_ref()).flatten();
    let cards: Option<Vec<PinCard>> = load
        .filter(|load| matches!(load.phase, PinsPhase::Ready))
        .and_then(|_| store.pins(channel))
        .map(|pins| {
            pins.iter()
                .map(|message| pin_card(ui, store, conversation.server, message, look, now))
                .collect()
        });
    let content = match load.map(|load| &load.phase) {
        Some(PinsPhase::Failed(reason)) => PinsContent::Failed(reason),
        Some(PinsPhase::Ready) => PinsContent::Ready(cards.as_deref().unwrap_or_default()),
        _ => PinsContent::Loading,
    };
    let can_unpin = abilities
        .as_ref()
        .is_some_and(|abilities| abilities.pin.allowed());
    tools.push(parts::pins(open, content, can_unpin, denied, look));
    tools
}

/// The «más» of a narrow header (2026-10-18): the inbox and the list of
/// threads, which take the room the title needs. `None` with neither.
pub(crate) fn header_more(
    ui: &Ui,
    store: &Store,
    channel: ChannelId,
    look: &Look,
) -> Option<Element> {
    let conversation = store.channel(channel)?;
    let mut entries: Vec<Entry> = Vec::new();
    if conversation.kind.has_messages() {
        entries.push(Item::new("inbox:open", t("Bandeja")).into());
    }
    entries.extend(crate::threads::header_entry(store, channel));
    (!entries.is_empty()).then(|| parts::more(ui.channel_list.more == Some(channel), entries, look))
}

fn pin_card(
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    message: &rune_model::Message,
    look: &Look,
    now: Timestamp,
) -> PinCard {
    let author = store.user(message.author);
    PinCard {
        message: message.id.0,
        author: store.display_name(message.author, server),
        ink: server
            .and_then(|server| store.top_role(message.author, server))
            .and_then(|role| role.color)
            .map_or(look.text, |color| look.ink(color)),
        accent: author.map_or(look.faint, |author| look.ink(author.accent)),
        when: rune_components::time::stamp(message.timestamp, now),
        content: message.content.clone(),
        pictures: crate::identity::reference_pictures(ui, &message.text_references),
    }
}

/// The line over the composer of `channel` while its slow mode is on.
pub(crate) fn slowmode(store: &Store, channel: ChannelId, look: &Look) -> Option<Element> {
    let found = store
        .channel(channel)
        .filter(|found| found.slowmode_secs > 0 && found.kind.has_messages())?;
    // Who may manage the messages or the channel is not held back.
    let immune = found.permissions.contains(Permissions::MANAGE_MESSAGES)
        || found.permissions.contains(Permissions::MANAGE_CHANNELS);
    Some(parts::slowmode(found.slowmode_secs, immune, look))
}

/// What the list opens over the window: its dialogs, the open channel's
/// topic, and the context menu, on top of everything.
pub(crate) fn overlays(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let width = Modal::fit_width(context.window.width, DIALOG_WIDTH);
    let mut root = match &ui.channel_list.dialog {
        Some(ListDialog::Duplicate { channel, name }) => match store.channel(*channel) {
            Some(original) => {
                let form = DuplicateForm {
                    original: original.name.clone(),
                    voice: original.kind == ChannelKind::Voice,
                    category: original
                        .parent
                        .and_then(|parent| store.channel(parent))
                        .map(|parent| parent.name.clone()),
                    name: name.clone(),
                };
                parts::duplicate_dialog(root, &form, width, look)
            }
            None => root,
        },
        Some(ListDialog::Sync {
            channel, category, ..
        }) => match (store.channel(*channel), store.channel(*category)) {
            (Some(moved), Some(category)) => {
                let name = if moved.kind.is_voice() {
                    moved.name.clone()
                } else {
                    format!("#{}", moved.name)
                };
                parts::sync_dialog(root, &name, &category.name, look)
            }
            _ => root,
        },
        Some(ListDialog::Delete(channel)) => match store.channel(*channel) {
            Some(doomed) => {
                let kind = match doomed.kind {
                    ChannelKind::Category => Doomed::Category,
                    ChannelKind::Voice => Doomed::Voice,
                    _ => Doomed::Text,
                };
                parts::delete_dialog(root, &doomed.name, kind, look)
            }
            None => root,
        },
        None => root,
    };
    let topic = ui
        .channel_list
        .topic
        .filter(|channel| ui.channel() == Some(*channel))
        .and_then(|channel| store.channel(channel));
    if let Some(channel) = topic {
        if let Some(topic) = &channel.topic {
            let pictures = crate::identity::text_pictures(ui, &channel.topic_pictures);
            root = parts::topic_dialog(root, &channel.name, topic, &pictures, width, look);
        }
    }
    if let Some((target, at)) = ui.channel_list.menu {
        let entries = menu_entries(ui, store, target, context.wall);
        if !entries.is_empty() {
            root = Element::stack(vec![root, parts::context_menu(at, entries, look)]);
        }
    }
    root
}

/// The notification settings as kept: `channel:level:until` for each one
/// that differs from the default, `inherit` for no level of its own and `-`
/// for not muted.
fn kept_notify(ui: &Ui) -> Option<String> {
    let mut channels: Vec<_> = ui.channel_list.notify.iter().collect();
    channels.sort_by_key(|(channel, _)| **channel);
    let kept: Vec<String> = channels
        .into_iter()
        .map(|(channel, notify)| {
            let level = notify.level.map_or("inherit", NotifyLevel::key);
            let until = notify
                .muted_until
                .map_or_else(|| "-".to_owned(), |until| until.0.to_string());
            format!("{}:{level}:{until}", channel.0)
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join(";"))
}

fn kept_collapsed(ui: &Ui) -> Option<String> {
    let mut collapsed: Vec<u64> = ui.collapsed.iter().map(|category| category.0).collect();
    collapsed.sort_unstable();
    (!collapsed.is_empty()).then(|| {
        collapsed
            .iter()
            .map(u64::to_string)
            .collect::<Vec<_>>()
            .join(",")
    })
}

/// The settings of the list worth keeping between runs.
pub(crate) fn kept(ui: &Ui) -> Vec<(&'static str, Option<String>)> {
    vec![
        (NOTIFY_SETTING, kept_notify(ui)),
        (
            HIDE_MUTED_SETTING,
            Some(ui.channel_list.hide_muted.to_string()),
        ),
        (COLLAPSED_SETTING, kept_collapsed(ui)),
    ]
}

/// Take back what [`kept`] kept; what cannot be read is left out.
pub(crate) fn restore(ui: &mut Ui, setting: &impl Fn(&str) -> Option<String>) {
    ui.channel_list.notify = setting(NOTIFY_SETTING)
        .unwrap_or_default()
        .split(';')
        .filter_map(|entry| {
            let mut fields = entry.split(':');
            let channel = ChannelId(fields.next()?.parse().ok()?);
            let level = match fields.next()? {
                "inherit" => None,
                key => Some(NotifyLevel::from_key(key)?),
            };
            let muted_until = match fields.next()? {
                "-" => None,
                until => Some(Timestamp(until.parse().ok()?)),
            };
            Some((channel, ChannelNotify { level, muted_until }))
        })
        .collect();
    ui.channel_list.hide_muted = setting(HIDE_MUTED_SETTING).as_deref() == Some("true");
    ui.collapsed = setting(COLLAPSED_SETTING)
        .unwrap_or_default()
        .split(',')
        .filter_map(|id| id.parse().ok().map(ChannelId))
        .collect();
}
