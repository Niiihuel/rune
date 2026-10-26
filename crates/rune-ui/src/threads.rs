//! Threads and forum posts as a person handles them: the list of a channel's
//! threads (followed, active, archived a page at a time), starting a thread
//! from a message or from a channel, starting a post in a forum, a thread's
//! menu (follow, mark read, copy its link, edit, close, lock, delete) and a
//! forum's order, layout and tag filter. Nothing remote is shown before the
//! provider reports it: a new thread opens once the provider names it, and
//! changes show when it reports the thread again. Every permit is asked of
//! the Store again right before a command goes. Actions start with `th:`.

use std::collections::HashMap;
use std::time::Instant;

use limn::{ButtonVariant, Component, Element, Icon, IconButton, UiAction};
use rune_components::channel_menu::{Entry, Item};
use rune_components::forum::{ForumBar, ForumView, PostCard};
use rune_components::server_menu::MUTE_DURATIONS;
use rune_components::threads::{
    self as parts, ids, EditThreadForm, NewThreadForm, PostFiles, TagChoice, ThreadList, ThreadRow,
};
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::{Channel, ChannelId, ChannelKind, MessageId, TagId, Timestamp, MAX_THREAD_NAME};
use rune_provider::{Command, Created, Failure, RequestId, ThreadEdit};
use rune_state::{Applied, Permit, Store};

use crate::{open, Intent, Ui};

#[derive(Debug, Default)]
pub struct Threads {
    /// The channel whose list of threads is open.
    list: Option<ChannelId>,
    creating: Option<NewThread>,
    editing: Option<EditThread>,
    /// The thread asked about before deleting it.
    deleting: Option<ChannelId>,
    /// The menu of a thread, and where it opened.
    menu: Option<(ChannelId, (f32, f32))>,
    /// How each forum is shown, when the person chose.
    forums: HashMap<ChannelId, ForumChoice>,
}

#[derive(Debug)]
struct NewThread {
    /// The channel, or the forum of a post.
    parent: ChannelId,
    message: Option<MessageId>,
    name: String,
    content: String,
    private: bool,
    tags: Vec<TagId>,
    /// The thread asked of the provider, until it names it.
    waiting: Option<RequestId>,
    /// How far a post's files went while it is asked for: bytes handed
    /// over of the whole.
    progress: Option<(u64, u64)>,
}

#[derive(Debug)]
struct EditThread {
    thread: ChannelId,
    name: String,
    tags: Vec<TagId>,
}

#[derive(Debug, Default)]
struct ForumChoice {
    by_creation: Option<bool>,
    gallery: Option<bool>,
    /// The tags shown; none shows every post.
    tags: Vec<TagId>,
}

/// Whether a dialog of threads covers the conversation.
pub(crate) fn covering(ui: &Ui) -> bool {
    let threads = &ui.threads;
    threads.list.is_some()
        || threads.creating.is_some()
        || threads.editing.is_some()
        || threads.deleting.is_some()
}

fn is_forum(store: &Store, channel: ChannelId) -> bool {
    store
        .channel(channel)
        .is_some_and(|found| found.kind == ChannelKind::Forum)
}

/// Start a thread from `message` of `channel`: the dialog that names it.
pub(crate) fn start_from(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    message: MessageId,
    now: Instant,
) -> Vec<Intent> {
    start(ui, store, channel, Some(message), now)
}

fn start(
    ui: &mut Ui,
    store: &Store,
    parent: ChannelId,
    message: Option<MessageId>,
    now: Instant,
) -> Vec<Intent> {
    let permit = if is_forum(store, parent) {
        store.create_post(parent)
    } else {
        store.create_thread(parent, false, message.is_some())
    };
    if let Permit::Denied(reason) = permit {
        ui.notify(t("No se puede"), Some(reason), true, now);
        return Vec::new();
    }
    // A thread from a message takes the start of what it says as its name.
    let name = message
        .and_then(|message| store.timeline(parent)?.get(message).cloned())
        .map(|found| {
            found
                .content
                .lines()
                .next()
                .unwrap_or_default()
                .chars()
                .take(MAX_THREAD_NAME)
                .collect::<String>()
        })
        .unwrap_or_default();
    ui.threads.creating = Some(NewThread {
        parent,
        message,
        name: name.clone(),
        content: String::new(),
        private: false,
        tags: Vec::new(),
        waiting: None,
        progress: None,
    });
    vec![
        Intent::SetInput {
            id: ids::NEW_NAME.into(),
            value: name,
        },
        Intent::SetInput {
            id: ids::NEW_CONTENT.into(),
            value: String::new(),
        },
        Intent::Focus(ids::NEW_NAME.into()),
    ]
}

/// Why the thread or post being written cannot go now, if it cannot.
fn refused(ui: &Ui, store: &Store, new: &NewThread) -> Option<String> {
    if new.waiting.is_some() {
        return Some(t("Esperando al proveedor…").into());
    }
    let files = crate::uploads::post_files(ui, new.parent);
    if crate::uploads::post_picking(ui, new.parent).is_some() {
        return Some(t("Espera a que se preparen los archivos").into());
    }
    let post = is_forum(store, new.parent);
    let permit = if post {
        store.create_post(new.parent)
    } else {
        store.create_thread(new.parent, new.private, new.message.is_some())
    };
    if let Permit::Denied(reason) = permit {
        return Some(reason);
    }
    let name = new.name.trim();
    if name.is_empty() {
        return Some(
            if post {
                t("Escribe un título")
            } else {
                t("Escribe un nombre")
            }
            .into(),
        );
    }
    if name.chars().count() > MAX_THREAD_NAME {
        return Some(tr!(
            "El nombre tiene hasta {max} caracteres",
            max = MAX_THREAD_NAME
        ));
    }
    if post {
        let content = new.content.trim();
        if content.is_empty() && files.is_empty() {
            return Some(t("Escribe el primer mensaje o adjunta un archivo").into());
        }
        if !files.is_empty() {
            if let Permit::Denied(reason) = store.post_files(new.parent) {
                return Some(reason);
            }
        }
        if content.encode_utf16().count() > 2000 {
            return Some(t("El primer mensaje tiene hasta 2000 caracteres").into());
        }
        if let Permit::Denied(reason) = store.post_tags(new.parent, &[], &new.tags) {
            return Some(reason);
        }
    }
    None
}

fn create(ui: &mut Ui, store: &Store, now: Instant) -> Vec<Intent> {
    let Some(new) = ui.threads.creating.as_ref() else {
        return Vec::new();
    };
    if new.waiting.is_some() {
        return Vec::new();
    }
    if let Some(reason) = refused(ui, store, new) {
        ui.notify(t("No se puede"), Some(reason), true, now);
        return Vec::new();
    }
    let files: Vec<_> = crate::uploads::post_files(ui, new.parent)
        .iter()
        .map(|file| file.token)
        .collect();
    let request = ui.fresh_request();
    let Some(new) = ui.threads.creating.as_mut() else {
        return Vec::new();
    };
    new.waiting = Some(request);
    new.progress = None;
    if is_forum(store, new.parent) && !files.is_empty() {
        // The files are with the services, which add them to the post.
        return vec![Intent::CreatePost {
            request,
            forum: new.parent,
            title: new.name.trim().to_owned(),
            tags: new.tags.clone(),
            content: new.content.trim().to_owned(),
            files,
        }];
    }
    let command = if is_forum(store, new.parent) {
        Command::CreatePost {
            request,
            forum: new.parent,
            title: new.name.trim().to_owned(),
            tags: new.tags.clone(),
            content: new.content.trim().to_owned(),
            files: Vec::new(),
        }
    } else {
        Command::CreateThread {
            request,
            channel: new.parent,
            message: new.message,
            name: new.name.trim().to_owned(),
            private: new.private,
        }
    };
    vec![Intent::Command(command)]
}

/// The post `request`, with files, was refused before it went: the dialog
/// can send it again.
pub fn post_refused(ui: &mut Ui, request: RequestId, error: rune_provider::UploadError) {
    if let Some(new) = ui
        .threads
        .creating
        .as_mut()
        .filter(|new| new.waiting == Some(request))
    {
        new.waiting = None;
    }
    ui.notify(
        t("No se pudo crear la publicación"),
        Some(error.label().into()),
        true,
        Instant::now(),
    );
}

/// Answer what the provider reported about threads.
pub(crate) fn on_applied(ui: &mut Ui, store: &mut Store, applied: &Applied) -> Vec<Intent> {
    match applied {
        Applied::Created {
            request,
            created: Created::Channel(channel),
        } => {
            let made = ui
                .threads
                .creating
                .as_ref()
                .is_some_and(|new| new.waiting == Some(*request));
            if !made {
                return Vec::new();
            }
            // Its files went with it.
            if let Some(new) = ui.threads.creating.take() {
                ui.uploads.drafts.remove(&new.parent);
            }
            ui.threads.list = None;
            match store.channel(*channel).filter(|found| found.is_thread()) {
                Some(found) => {
                    let server = found.server;
                    open(ui, store, server, *channel)
                }
                None => Vec::new(),
            }
        }
        // Its files' bytes, while the post is asked for (envios-y-subidas).
        Applied::PostProgress {
            request,
            sent_bytes,
            total_bytes,
        } => {
            if let Some(new) = ui
                .threads
                .creating
                .as_mut()
                .filter(|new| new.waiting == Some(*request))
            {
                new.progress = Some((*sent_bytes, *total_bytes));
            }
            Vec::new()
        }
        // The dialog stays, to try again; the corner says why.
        Applied::Failed {
            failure: Failure::NewThread { request },
            ..
        } => {
            if let Some(new) = ui
                .threads
                .creating
                .as_mut()
                .filter(|new| new.waiting == Some(*request))
            {
                new.waiting = None;
            }
            Vec::new()
        }
        // A new session: the open list asks again for the open threads it
        // is not told of; an open forum does as `on_ready` opens it again.
        Applied::Ready => match ui.threads.list {
            Some(channel) => opened(ui, store, channel),
            None => Vec::new(),
        },
        Applied::Channel(channel) | Applied::ChannelRemoved(channel) => {
            if store.channel(*channel).is_some_and(Store::visible) {
                return Vec::new();
            }
            let threads = &mut ui.threads;
            if threads.list == Some(*channel) {
                threads.list = None;
            }
            if threads
                .creating
                .as_ref()
                .is_some_and(|new| new.parent == *channel)
            {
                threads.creating = None;
            }
            if threads
                .editing
                .as_ref()
                .is_some_and(|edit| edit.thread == *channel)
            {
                threads.editing = None;
            }
            if threads.deleting == Some(*channel) {
                threads.deleting = None;
            }
            if threads.menu.is_some_and(|(of, _)| of == *channel) {
                threads.menu = None;
            }
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// A secondary click on a thread of the column or a post of a forum opens
/// its menu.
pub(crate) fn secondary_click(
    ui: &mut Ui,
    store: &Store,
    target: &str,
    at: limn::Point,
) -> Option<Vec<Intent>> {
    let id = target
        .strip_prefix("thread:")
        .or_else(|| target.strip_prefix("post:"))?;
    let thread = ChannelId(id.parse().ok()?);
    store.thread_abilities(thread)?;
    ui.threads.menu = Some((thread, (at.x, at.y)));
    Some(Vec::new())
}

/// Answer the actions of threads; `None` when the action is not one.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let (id, value, clicked) = match action {
        UiAction::Clicked { id } => (id.as_str(), None, true),
        UiAction::InputChanged { id, value } => (id.as_str(), Some(value), false),
        UiAction::Submitted { id, .. } => (id.as_str(), None, false),
        _ => return None,
    };
    // An item that only opens a submenu keeps the menu open.
    if clicked && id.starts_with(rune_components::channel_menu::ids::SUBMENU) {
        return None;
    }
    if clicked {
        ui.threads.menu = None;
        // A thread opened from the list closes it; the window opens it.
        if id.starts_with("thread:") || id.starts_with("post:") {
            ui.threads.list = None;
            return None;
        }
    }
    let rest = id.strip_prefix("th:")?;
    // The header's bell offers a thread's silence too (2026-10-18).
    if clicked {
        ui.channel_list.bell = None;
    }
    let threads = &mut ui.threads;
    match id {
        // A dialog's own close button reports its ID with `:close` after it.
        _ if id.starts_with(ids::CLOSE) => {
            threads.list = None;
            return Some(Vec::new());
        }
        _ if id.starts_with(ids::NEW_CLOSE) => {
            let forum = threads.creating.take().map(|new| new.parent);
            return Some(
                forum.map_or_else(Vec::new, |forum| crate::uploads::release_post(ui, forum)),
            );
        }
        ids::NEW_ATTACH => {
            let Some(forum) = threads
                .creating
                .as_ref()
                .filter(|new| new.waiting.is_none() && is_forum(store, new.parent))
                .map(|new| new.parent)
            else {
                return Some(Vec::new());
            };
            return Some(match crate::uploads::begin_post_import(ui, store, forum) {
                Ok(request) => vec![Intent::PickAttachment {
                    request,
                    channel: forum,
                }],
                Err(failure) => {
                    crate::uploads::error(ui, failure);
                    Vec::new()
                }
            });
        }
        _ if id.starts_with(ids::EDIT_CLOSE) => {
            threads.editing = None;
            return Some(Vec::new());
        }
        ids::CONFIRM_CANCEL => {
            threads.deleting = None;
            return Some(Vec::new());
        }
        ids::CONFIRM => {
            let Some(thread) = threads.deleting.take() else {
                return Some(Vec::new());
            };
            let allowed = store
                .thread_abilities(thread)
                .is_some_and(|can| can.delete.allowed());
            return Some(if allowed {
                vec![Intent::Command(Command::DeleteThread { thread })]
            } else {
                Vec::new()
            });
        }
        ids::NEW_NAME | ids::NEW_CONTENT => {
            if let (Some(new), Some(value)) = (threads.creating.as_mut(), value) {
                if id == ids::NEW_NAME {
                    new.name = value.clone();
                } else {
                    new.content = value.clone();
                }
            }
            // Enter in the name sends it, as a dialog's main field does.
            if matches!(action, UiAction::Submitted { .. }) && id == ids::NEW_NAME {
                return Some(create(ui, store, now));
            }
            return Some(Vec::new());
        }
        ids::NEW_PRIVATE => {
            if let Some(new) = threads.creating.as_mut() {
                let wanted = !new.private;
                if !wanted || store.create_thread(new.parent, true, false).allowed() {
                    new.private = wanted;
                }
            }
            return Some(Vec::new());
        }
        ids::NEW_CREATE => return Some(create(ui, store, now)),
        ids::EDIT_NAME => {
            if let (Some(edit), Some(value)) = (threads.editing.as_mut(), value) {
                edit.name = value.clone();
            }
            if matches!(action, UiAction::Submitted { .. }) {
                return Some(save(ui, store, now));
            }
            return Some(Vec::new());
        }
        ids::EDIT_SAVE => return Some(save(ui, store, now)),
        _ => {}
    }
    if !clicked {
        return Some(Vec::new());
    }
    let number = |prefix: &str| {
        rest.strip_prefix(prefix)
            .and_then(|n| n.parse::<u64>().ok())
    };
    if let Some(tag) = id.strip_prefix(ids::NEW_TAG).and_then(|t| t.parse().ok()) {
        if let Some(new) = threads.creating.as_mut() {
            // Only moderation puts a moderated tag on.
            let tag = TagId(tag);
            let mut wanted = new.tags.clone();
            toggle(&mut wanted, tag);
            if !wanted.contains(&tag) || store.post_tags(new.parent, &[], &[tag]).allowed() {
                new.tags = wanted;
            }
        }
        return Some(Vec::new());
    }
    if let Some(tag) = id.strip_prefix(ids::EDIT_TAG).and_then(|t| t.parse().ok()) {
        if let Some(edit) = threads.editing.as_mut() {
            let tag = TagId(tag);
            let before = store
                .channel(edit.thread)
                .and_then(|found| Some((found.parent?, found.thread.clone()?.tags)));
            let mut wanted = edit.tags.clone();
            toggle(&mut wanted, tag);
            let allowed = !wanted.contains(&tag)
                || before.is_some_and(|(forum, before)| {
                    store.post_tags(forum, &before, &[tag]).allowed() || before.contains(&tag)
                });
            if allowed {
                edit.tags = wanted;
            }
        }
        return Some(Vec::new());
    }
    if let Some(channel) = number("list:").map(ChannelId) {
        threads.list = store
            .channel(channel)
            .is_some_and(Store::visible)
            .then_some(channel);
        return Some(opened(ui, store, channel));
    }
    if let Some(channel) = number("start:").map(ChannelId) {
        return Some(start(ui, store, channel, None, now));
    }
    if let Some(parent) = number("more:").map(ChannelId) {
        let request = ui.fresh_request();
        return Some(match store.load_threads(parent, request) {
            Some(command) => vec![Intent::Command(command)],
            None => {
                if let Permit::Denied(reason) = store.more_threads(parent) {
                    ui.notify(t("No se pueden cargar más"), Some(reason), true, now);
                }
                Vec::new()
            }
        });
    }
    if let Some(choice) = forum_choice(ui, store, rest) {
        return Some(choice);
    }
    let (thread, what) = rest.split_once(':')?;
    let thread = ChannelId(thread.parse().ok()?);
    Some(act(ui, store, thread, what, now))
}

fn toggle(tags: &mut Vec<TagId>, tag: TagId) {
    match tags.iter().position(|known| *known == tag) {
        Some(at) => {
            tags.remove(at);
        }
        None => tags.push(tag),
    }
}

/// A forum's order (`sort:`), layout (`layout:`) or tag filter (`filter:`).
fn forum_choice(ui: &mut Ui, store: &Store, rest: &str) -> Option<Vec<Intent>> {
    let (kind, rest) = rest.split_once(':')?;
    if !matches!(kind, "sort" | "layout" | "filter") {
        return None;
    }
    let (forum, value) = rest.split_once(':')?;
    let forum = ChannelId(forum.parse().ok()?);
    let found = store
        .channel(forum)
        .filter(|c| c.kind == ChannelKind::Forum)?;
    let tags: Vec<TagId> = found.tags.iter().map(|tag| tag.id).collect();
    let choice = ui.threads.forums.entry(forum).or_default();
    match (kind, value) {
        ("sort", "created") => choice.by_creation = Some(true),
        ("sort", "activity") => choice.by_creation = Some(false),
        ("layout", "gallery") => choice.gallery = Some(true),
        ("layout", "list") => choice.gallery = Some(false),
        ("filter", "clear") => choice.tags.clear(),
        ("filter", tag) => {
            let tag = TagId(tag.parse().ok()?);
            if tags.contains(&tag) {
                toggle(&mut choice.tags, tag);
            }
        }
        _ => return None,
    }
    Some(Vec::new())
}

/// An action of a thread's menu, asked of the Store again first.
fn act(ui: &mut Ui, store: &mut Store, thread: ChannelId, what: &str, now: Instant) -> Vec<Intent> {
    let (Some(can), Some(found)) = (store.thread_abilities(thread), store.channel(thread)) else {
        return Vec::new();
    };
    let info = found.thread.clone().unwrap_or_default();
    let refuse = |ui: &mut Ui, permit: &Permit| {
        if let Permit::Denied(reason) = permit {
            ui.notify(t("No se puede"), Some(reason.clone()), true, now);
        }
        Vec::new()
    };
    let command = |edit: ThreadEdit| vec![Intent::Command(Command::EditThread { thread, edit })];
    match what {
        "follow" | "unfollow" if can.follow.allowed() => {
            vec![Intent::Command(Command::FollowThread {
                thread,
                follow: what == "follow",
            })]
        }
        "follow" | "unfollow" => refuse(ui, &can.follow),
        "read" if can.mark_read.allowed() => {
            ui.new_marker.remove(&thread);
            store
                .mark_read_command(thread)
                .map(Intent::Command)
                .into_iter()
                .collect()
        }
        "read" => refuse(ui, &can.mark_read),
        "link" => match store.thread_link(thread) {
            Some(link) => {
                ui.notify(t("Enlace copiado"), None, false, now);
                vec![Intent::Copy(link)]
            }
            None => refuse(ui, &can.link),
        },
        "edit" if can.edit.allowed() => {
            let name = found.name.clone();
            ui.threads.editing = Some(EditThread {
                thread,
                name: name.clone(),
                tags: info.tags,
            });
            vec![
                Intent::SetInput {
                    id: ids::EDIT_NAME.into(),
                    value: name,
                },
                Intent::Focus(ids::EDIT_NAME.into()),
            ]
        }
        "edit" => refuse(ui, &can.edit),
        "close" | "open" if can.close.allowed() => command(ThreadEdit {
            archived: Some(what == "close"),
            ..ThreadEdit::default()
        }),
        "close" | "open" => refuse(ui, &can.close),
        "lock" | "unlock" if can.lock.allowed() => command(ThreadEdit {
            locked: Some(what == "lock"),
            ..ThreadEdit::default()
        }),
        "lock" | "unlock" => refuse(ui, &can.lock),
        "delete" if can.delete.allowed() => {
            ui.threads.deleting = Some(thread);
            Vec::new()
        }
        "delete" => refuse(ui, &can.delete),
        // Threads, forums and stages (2026-10-18).
        "unmute" if can.mute.allowed() => vec![Intent::Command(Command::MuteThread {
            thread,
            until: None,
        })],
        "unmute" => refuse(ui, &can.mute),
        what if what.starts_with("mute:") => {
            let key = &what["mute:".len()..];
            let Some((_, _, length)) = MUTE_DURATIONS.iter().find(|(known, _, _)| *known == key)
            else {
                return Vec::new();
            };
            if !can.mute.allowed() {
                return refuse(ui, &can.mute);
            }
            let until = length.map_or(rune_model::MUTED_FOREVER, |length| {
                Timestamp(crate::server_menu::wall_now().0 + length)
            });
            vec![Intent::Command(Command::MuteThread {
                thread,
                until: Some(until),
            })]
        }
        _ => Vec::new(),
    }
}

fn save(ui: &mut Ui, store: &Store, now: Instant) -> Vec<Intent> {
    let Some(edit) = ui.threads.editing.as_ref() else {
        return Vec::new();
    };
    let thread = edit.thread;
    let (Some(can), Some(found)) = (store.thread_abilities(thread), store.channel(thread)) else {
        ui.threads.editing = None;
        return Vec::new();
    };
    let before = found.thread.clone().unwrap_or_default().tags;
    let name = edit.name.trim().to_owned();
    let mut change = ThreadEdit::default();
    let forum = found.parent.filter(|parent| is_forum(store, *parent));
    let refusal = if !can.edit.allowed() {
        can.edit.reason().map(str::to_owned)
    } else if name.is_empty() || name.chars().count() > MAX_THREAD_NAME {
        Some(tr!(
            "El nombre tiene de 1 a {max} caracteres",
            max = MAX_THREAD_NAME
        ))
    } else {
        forum.filter(|_| edit.tags != before).and_then(|forum| {
            store
                .post_tags(forum, &before, &edit.tags)
                .reason()
                .map(str::to_owned)
        })
    };
    if let Some(reason) = refusal {
        ui.notify(t("No se puede"), Some(reason), true, now);
        return Vec::new();
    }
    if name != found.name {
        change.name = Some(name);
    }
    if forum.is_some() && edit.tags != before {
        change.tags = Some(edit.tags.clone());
    }
    ui.threads.editing = None;
    if change.is_empty() {
        return Vec::new();
    }
    vec![Intent::Command(Command::EditThread {
        thread,
        edit: change,
    })]
}

/// Whether `channel`'s header offers its list of threads.
fn has_list(store: &Store, channel: ChannelId) -> bool {
    store.channel(channel).is_some_and(|found| {
        matches!(found.kind, ChannelKind::Text | ChannelKind::Announcement)
            && found.server.is_some()
    })
}

/// The list of threads as an entry of a narrow header's «más» (2026-10-18).
pub(crate) fn header_entry(store: &Store, channel: ChannelId) -> Option<Entry> {
    has_list(store, channel).then(|| {
        let mut item = Item::new(ids::list(channel), t("Hilos"));
        item.icon = Some(Icon::Chat);
        item.into()
    })
}

/// The button of a channel's header that opens its list of threads.
pub(crate) fn header_tool(store: &Store, channel: ChannelId, look: &Look) -> Option<Element> {
    if !has_list(store, channel) {
        return None;
    }
    Some(
        IconButton::new(ids::list(channel), Icon::Chat, look.menu_palette())
            .variant(ButtonVariant::Ghost)
            .tooltip(t("Hilos"))
            .accessibility_label(t("Hilos"))
            .ink(look.muted)
            .render(),
    )
}

fn messages(count: u32) -> String {
    match count {
        1 => t("1 mensaje").into(),
        n => tr!("{n} mensajes", n),
    }
}

fn row(ui: &Ui, store: &Store, thread: &Channel, wall: Timestamp) -> ThreadRow {
    let info = thread.thread.clone().unwrap_or_default();
    ThreadRow {
        thread: thread.id.0,
        name: thread.name.clone(),
        detail: format!(
            "{} · {}",
            messages(info.messages),
            rune_components::time::stamp(info.last_activity, wall)
        ),
        private: info.private,
        archived: info.archived,
        locked: info.locked,
        followed: info.joined,
        // Marked as the column marks it: its silence and its level, own or
        // inherited (2026-10-18).
        unread: crate::channel_list::marks(ui, store, thread, wall).0,
    }
}

/// The first page of `channel`'s open threads, when none was read: a service
/// may tell a session only of those the account follows.
pub(crate) fn opened(ui: &mut Ui, store: &mut Store, channel: ChannelId) -> Vec<Intent> {
    if store.thread_pages(channel).is_some() || !store.more_threads(channel).allowed() {
        return Vec::new();
    }
    let request = ui.fresh_request();
    store
        .load_threads(channel, request)
        .map(Intent::Command)
        .into_iter()
        .collect()
}

/// The label of the button that asks for more threads, and why not.
fn more(store: &Store, parent: ChannelId, forum: bool) -> (String, Option<String>) {
    // The archived ones start where the open ones end.
    let archived_next = store.thread_pages(parent).is_some_and(|pages| {
        pages.scope == rune_provider::ThreadScope::Archived && pages.next.is_none()
    });
    let label = match (archived_next, forum) {
        (false, _) => t("Cargar más"),
        (true, true) => t("Ver archivadas"),
        (true, false) => t("Ver archivados"),
    };
    (
        label.into(),
        store.more_threads(parent).reason().map(str::to_owned),
    )
}

fn list(ui: &Ui, store: &Store, channel: ChannelId, wall: Timestamp) -> Option<ThreadList> {
    let found = store.channel(channel)?;
    let (followed, active): (Vec<&Channel>, Vec<&Channel>) = store
        .threads(channel, false)
        .into_iter()
        .partition(|thread| thread.thread.as_ref().is_some_and(|info| info.joined));
    let archived = store
        .threads(channel, true)
        .into_iter()
        .filter(|thread| thread.thread.as_ref().is_some_and(|info| info.archived));
    let rows = |threads: Vec<&Channel>| threads.iter().map(|t| row(ui, store, t, wall)).collect();
    Some(ThreadList {
        channel: channel.0,
        title: format!("#{}", found.name),
        start_denied: store
            .create_thread(channel, false, false)
            .reason()
            .map(str::to_owned),
        followed: rows(followed),
        active: rows(active),
        archived: rows(archived.collect()),
        more: more(store, channel, false),
    })
}

fn tag_label(tag: &rune_model::ForumTag) -> String {
    match &tag.emoji {
        Some(emoji) => format!("{emoji} {}", tag.name),
        None => tag.name.clone(),
    }
}

fn choices(forum: &Channel, chosen: &[TagId]) -> Vec<TagChoice> {
    let moderates = forum
        .permissions
        .contains(rune_model::Permissions::MANAGE_THREADS);
    forum
        .tags
        .iter()
        .map(|tag| TagChoice {
            tag: tag.id.0,
            label: tag_label(tag),
            chosen: chosen.contains(&tag.id),
            locked: tag.moderated && !moderates,
        })
        .collect()
}

fn new_form(ui: &Ui, store: &Store, new: &NewThread) -> Option<NewThreadForm> {
    let parent = store.channel(new.parent)?;
    let post = parent.kind == ChannelKind::Forum;
    let subtitle = match new.message {
        Some(message) => {
            let author = store
                .timeline(new.parent)
                .and_then(|timeline| timeline.get(message).map(|found| found.author))
                .map(|author| store.display_name(author, parent.server))
                .unwrap_or_else(|| t("alguien").into());
            tr!(
                "En #{channel}, a partir del mensaje de {author}",
                channel = parent.name,
                author
            )
        }
        None => format!("#{}", parent.name),
    };
    let private = (!post && new.message.is_none() && parent.kind == ChannelKind::Text).then(|| {
        (
            new.private,
            store
                .create_thread(new.parent, true, false)
                .reason()
                .map(str::to_owned),
        )
    });
    Some(NewThreadForm {
        title: if post {
            t("Nueva publicación")
        } else {
            t("Crear hilo")
        }
        .into(),
        subtitle,
        name: new.name.clone(),
        content: post.then(|| new.content.clone()),
        private,
        tags: if post {
            choices(parent, &new.tags)
        } else {
            Vec::new()
        },
        tag_required: post && parent.forum.require_tag,
        denied: refused(ui, store, new),
        files: post.then(|| PostFiles {
            forum: new.parent.0,
            files: crate::uploads::post_files(ui, new.parent).to_vec(),
            picking: crate::uploads::post_picking(ui, new.parent),
            waiting: new.waiting.is_some(),
            denied: store.post_files(new.parent).reason().map(str::to_owned),
            // From none of them until the service says more.
            progress: new.waiting.and_then(|_| {
                let files = crate::uploads::post_files(ui, new.parent);
                (!files.is_empty()).then(|| {
                    new.progress
                        .unwrap_or((0, files.iter().map(|file| file.size_bytes).sum()))
                })
            }),
        }),
    })
}

fn edit_form(store: &Store, edit: &EditThread) -> Option<EditThreadForm> {
    let thread = store.channel(edit.thread)?;
    let can = store.thread_abilities(edit.thread)?;
    let forum = thread
        .parent
        .and_then(|parent| store.channel(parent))
        .filter(|parent| parent.kind == ChannelKind::Forum);
    let before = thread.thread.clone().unwrap_or_default().tags;
    let name = edit.name.trim();
    Some(EditThreadForm {
        title: thread.name.clone(),
        name: edit.name.clone(),
        tags: forum.map_or_else(Vec::new, |forum| {
            let mut tags = choices(forum, &edit.tags);
            // A moderated tag already on it may be taken off.
            for tag in &mut tags {
                tag.locked &= !before.contains(&TagId(tag.tag));
            }
            tags
        }),
        denied: if !can.edit.allowed() {
            can.edit.reason().map(str::to_owned)
        } else if name.is_empty() {
            Some(t("Escribe un nombre").into())
        } else {
            forum.filter(|_| edit.tags != before).and_then(|forum| {
                store
                    .post_tags(forum.id, &before, &edit.tags)
                    .reason()
                    .map(str::to_owned)
            })
        },
    })
}

/// Silence the thread for a while or for good, in a submenu, or hear it
/// again with when its silence ends beneath (2026-10-18).
fn mute_item(
    thread: ChannelId,
    info: &rune_model::Thread,
    post: bool,
    permit: &Permit,
    wall: Timestamp,
) -> Entry {
    let mut item = if info.muted(wall) {
        let until = match info
            .muted_until
            .filter(|until| *until != rune_model::MUTED_FOREVER)
        {
            Some(until) => tr!(
                "Silenciado hasta: {until}",
                until = rune_components::time::stamp(until, wall)
            ),
            None => t("Silenciado hasta reactivarlo").into(),
        };
        let label = if post {
            t("Dejar de silenciar la publicación")
        } else {
            t("Dejar de silenciar el hilo")
        };
        Item::new(ids::of(thread, "unmute"), label).detail(until)
    } else {
        let label = if post {
            t("Silenciar publicación")
        } else {
            t("Silenciar hilo")
        };
        let durations = MUTE_DURATIONS
            .iter()
            .zip(crate::channel_list::MUTE_LABELS)
            .map(|((key, _, _), label)| {
                Item::new(ids::of(thread, &format!("mute:{key}")), t(label)).into()
            })
            .collect();
        let id = format!(
            "{}th:{}:mute",
            rune_components::channel_menu::ids::SUBMENU,
            thread
        );
        let mut item = Item::new(id, label);
        if permit.allowed() {
            item = item.submenu(durations);
        }
        item
    };
    item.icon = Some(if info.muted(wall) {
        Icon::Volume
    } else {
        Icon::VolumeMuted
    });
    item.denied = permit.reason().map(str::to_owned);
    Entry::Item(item)
}

/// A silenced thread's way back from its header's bell (2026-10-18), with
/// the ids and permit of its own menu's.
pub(crate) fn bell_entry(store: &Store, thread: &Channel, wall: Timestamp) -> Option<Entry> {
    let info = thread
        .thread
        .as_ref()
        .filter(|info| thread.is_thread() && info.joined && info.muted(wall))?;
    let can = store.thread_abilities(thread.id)?;
    let post = thread.parent.is_some_and(|parent| is_forum(store, parent));
    Some(mute_item(thread.id, info, post, &can.mute, wall))
}

/// What the menu of a thread offers.
fn menu(store: &Store, thread: ChannelId, wall: Timestamp) -> Vec<Entry> {
    let (Some(can), Some(found)) = (store.thread_abilities(thread), store.channel(thread)) else {
        return Vec::new();
    };
    let info = found.thread.clone().unwrap_or_default();
    let post = found.parent.is_some_and(|parent| is_forum(store, parent));
    let noun = if post { t("publicación") } else { t("hilo") };
    let item = |action: &str, label: String, icon: Icon, permit: &Permit, danger: bool| {
        let mut item = Item::new(ids::of(thread, action), label);
        item.icon = Some(icon);
        item.denied = permit.reason().map(str::to_owned);
        item.danger = danger;
        Entry::Item(item)
    };
    vec![
        if info.joined {
            item(
                "unfollow",
                t("Dejar de seguir").into(),
                Icon::BellSlash,
                &can.follow,
                false,
            )
        } else {
            item("follow", t("Seguir").into(), Icon::Bell, &can.follow, false)
        },
        mute_item(thread, &info, post, &can.mute, wall),
        item(
            "read",
            t("Marcar como leído").into(),
            Icon::Check,
            &can.mark_read,
            false,
        ),
        item(
            "link",
            t("Copiar enlace").into(),
            Icon::Copy,
            &can.link,
            false,
        ),
        Entry::Separator,
        item(
            "edit",
            tr!("Editar {noun}", noun),
            Icon::Edit,
            &can.edit,
            false,
        ),
        if info.archived {
            item(
                "open",
                tr!("Abrir {noun}", noun),
                Icon::ArchiveUp,
                &can.close,
                false,
            )
        } else {
            item(
                "close",
                tr!("Cerrar {noun}", noun),
                Icon::Archive,
                &can.close,
                false,
            )
        },
        if info.locked {
            item(
                "unlock",
                tr!("Desbloquear {noun}", noun),
                Icon::Lock,
                &can.lock,
                false,
            )
        } else {
            item(
                "lock",
                tr!("Bloquear {noun}", noun),
                Icon::Lock,
                &can.lock,
                false,
            )
        },
        Entry::Separator,
        item(
            "delete",
            tr!("Eliminar {noun}", noun),
            Icon::Trash,
            &can.delete,
            true,
        ),
    ]
}

/// The list, dialogs, menu and question of threads, over `root`.
pub(crate) fn overlay(
    ui: &Ui,
    store: &Store,
    mut root: Element,
    look: &Look,
    wall: Timestamp,
) -> Element {
    let threads = &ui.threads;
    if let Some(view) = threads
        .list
        .and_then(|channel| list(ui, store, channel, wall))
    {
        root = parts::list(root, &view, look);
    }
    if let Some(form) = threads
        .creating
        .as_ref()
        .and_then(|new| new_form(ui, store, new))
    {
        root = parts::create_dialog(root, &form, look);
    }
    if let Some(form) = threads
        .editing
        .as_ref()
        .and_then(|edit| edit_form(store, edit))
    {
        root = parts::edit_dialog(root, &form, look);
    }
    if let Some((thread, at)) = threads.menu {
        let entries = menu(store, thread, wall);
        if !entries.is_empty() {
            root = Element::stack(vec![
                root,
                rune_components::channel_menu::context_menu(at, entries, look),
            ]);
        }
    }
    if let Some(thread) = threads.deleting {
        root = parts::confirm(
            root,
            &tr!("Eliminar «{name}»", name = store.channel_title(thread)),
            t("Se borran el hilo y todos sus mensajes, para todos. No se puede deshacer."),
            t("Eliminar"),
            look,
        );
    }
    root
}

/// A post as its forum's card shows it.
fn card(ui: &Ui, store: &Store, forum: &Channel, post: &Channel, wall: Timestamp) -> PostCard {
    let info = post.thread.clone().unwrap_or_default();
    // A blocked person's words and picture stay folded, as in the timeline.
    let starter = info
        .starter
        .clone()
        .filter(|_| !info.owner.is_some_and(|owner| store.blocked(owner)))
        .unwrap_or_default();
    PostCard {
        thread: post.id.0,
        title: post.name.clone(),
        tags: info
            .tags
            .iter()
            .filter_map(|id| forum.tags.iter().find(|tag| tag.id == *id))
            .map(tag_label)
            .collect(),
        author: info.owner.map_or_else(
            || t("alguien").into(),
            |owner| store.display_name(owner, forum.server),
        ),
        replies: info.messages,
        activity: rune_components::time::stamp(info.last_activity, wall),
        archived: info.archived,
        locked: info.locked,
        // Marked as the column marks a thread: its silence and its level,
        // own or inherited (2026-10-18).
        unread: crate::channel_list::marks(ui, store, post, wall).0,
        excerpt: starter.excerpt.clone(),
        image: ui
            .prefs
            .image_previews
            .then(|| ui.image(&starter.image))
            .flatten(),
        // Only once its first message is known, as the official client.
        reaction: info
            .starter
            .as_ref()
            .and_then(|starter| starter.reaction(forum.forum.reaction.as_deref())),
    }
}

/// A forum as it opens: its tools, then its posts in the order, layout and
/// filter chosen, or the forum's own until something is chosen.
pub(crate) fn forum_view(ui: &Ui, store: &Store, forum: &Channel, wall: Timestamp) -> ForumView {
    let choice = ui.threads.forums.get(&forum.id);
    let by_creation = choice
        .and_then(|choice| choice.by_creation)
        .unwrap_or(forum.forum.by_creation);
    let gallery = choice
        .and_then(|choice| choice.gallery)
        .unwrap_or(forum.forum.gallery);
    let filter: &[TagId] = choice.map_or(&[], |choice| &choice.tags);
    // Every tag chosen, where the forum asks for all of them (2026-10-19).
    let all = forum.forum.match_all_tags;
    let shown = |post: &&Channel| {
        filter.is_empty()
            || post.thread.as_ref().is_some_and(|info| {
                if all {
                    filter.iter().all(|tag| info.tags.contains(tag))
                } else {
                    info.tags.iter().any(|tag| filter.contains(tag))
                }
            })
    };
    let order = |mut posts: Vec<&Channel>| {
        if by_creation {
            // Ids grow with time: the newest post has the highest.
            posts.sort_by_key(|post| std::cmp::Reverse(post.id));
        }
        posts
            .into_iter()
            .map(|post| card(ui, store, forum, post, wall))
            .collect()
    };
    let active = store
        .threads(forum.id, false)
        .into_iter()
        .filter(shown)
        .collect();
    let archived = store
        .threads(forum.id, true)
        .into_iter()
        .filter(|post| post.thread.as_ref().is_some_and(|info| info.archived))
        .filter(shown)
        .collect();
    ForumView {
        id: format!("forum:{}", forum.id),
        bar: ForumBar {
            forum: forum.id.0,
            start_denied: store.create_post(forum.id).reason().map(str::to_owned),
            by_creation,
            gallery,
            tags: forum
                .tags
                .iter()
                .map(|tag| (tag.id.0, tag_label(tag), filter.contains(&tag.id)))
                .collect(),
        },
        cards: order(active),
        archived: order(archived),
        more: more(store, forum.id, true),
        empty: if filter.is_empty() {
            t("Todavía no hay publicaciones activas que puedas ver en este foro.").into()
        } else {
            t("Ninguna publicación activa tiene esas etiquetas.").into()
        },
    }
}

/// A forum, open, in place of a timeline.
pub(crate) fn forum_page(
    ui: &Ui,
    store: &Store,
    forum: &Channel,
    width: f32,
    wall: Timestamp,
    look: &Look,
) -> Element {
    rune_components::forum::view(forum_view(ui, store, forum, wall), width, look)
}
