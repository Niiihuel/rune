//! Threads, forum posts and forum tags from documented channel objects
//! (types 10–12, 15 and 16) and the THREAD_* dispatches, and what the REST
//! writes about threads may ask and confirm (`allowed`, `confirmed`; the
//! requests are in `http::threads`). A malformed optional part (metadata,
//! tags, counts) is dropped and the rest of the thread kept; a malformed
//! thread is dropped and the rest of the payload kept.

use rune_i18n::es;
use std::collections::HashSet;

use rune_model::{
    Channel, ChannelId, ChannelKind, ForumDefaults, ForumTag, Permissions, ServerId, TagId, Thread,
    Timestamp, UserId, MAX_APPLIED_TAGS, MAX_TAGS, MAX_TAG_NAME, MAX_THREAD_NAME,
};
use rune_provider::{Command, Event, Snapshot, ThreadScope};
use serde_json::Value;

use super::{decode_channel, optional_id, remove_channel, rows, MAX_ITEMS};
use crate::{wire, LoginError};

/// Milliseconds from the Unix epoch to Discord's.
const DISCORD_EPOCH: u64 = 1_420_070_400_000;

/// When a Discord id was made.
fn made(id: u64) -> Timestamp {
    Timestamp(((id >> 22) + DISCORD_EPOCH) as i64)
}

fn count(value: &Value) -> Option<u32> {
    value.as_u64().map(|n| n.min(u32::MAX as u64) as u32)
}

/// What a thread object says of itself. Fields it leaves out keep what
/// `previous` knew: an update without its member object is not a leave.
pub(super) fn metadata(data: &Value, id: ChannelId, previous: Option<&Thread>) -> Thread {
    let known = previous.cloned().unwrap_or_default();
    let meta = data.get("thread_metadata").filter(|m| m.is_object());
    let flag = |key: &str, old: bool| {
        meta.and_then(|m| m.get(key))
            .map_or(old, |v| v.as_bool().unwrap_or(old))
    };
    let last_message = optional_id(&data["last_message_id"]).ok().flatten();
    Thread {
        archived: flag("archived", known.archived),
        locked: flag("locked", known.locked),
        private: data["type"].as_u64() == Some(12),
        joined: if data.get("member").is_some_and(Value::is_object) {
            true
        } else {
            known.joined
        },
        owner: optional_id(&data["owner_id"])
            .ok()
            .flatten()
            .map(UserId)
            .or(known.owner),
        messages: count(&data["message_count"]).unwrap_or(known.messages),
        members: count(&data["member_count"]).unwrap_or(known.members),
        last_activity: last_message.map_or_else(
            || known.last_activity.max(made(id.0)),
            |message| made(message).max(known.last_activity),
        ),
        tags: match data.get("applied_tags") {
            Some(value) => applied(value),
            None => known.tags,
        },
        // Threads, forums and stages (2026-10-18).
        muted_until: match data.get("member").filter(|m| m.is_object()) {
            Some(member) => muted_until(member),
            None => known.muted_until,
        },
        starter: known.starter,
    }
}

/// Until when the account's membership `member` of a thread silences it,
/// as the official client reads it: `muted`, and `mute_config.end_time`
/// (none is for good). A malformed end is not a silence.
pub(crate) fn muted_until(member: &Value) -> Option<Timestamp> {
    if member["muted"].as_bool() != Some(true) {
        return None;
    }
    match &member["mute_config"]["end_time"] {
        Value::Null => Some(rune_model::MUTED_FOREVER),
        end => wire::timestamp(end).ok(),
    }
}

/// The tags put on a post: at most five distinct ids, else none.
fn applied(value: &Value) -> Vec<TagId> {
    let Some(values) = value.as_array().filter(|v| v.len() <= MAX_APPLIED_TAGS) else {
        return Vec::new();
    };
    let mut tags = Vec::new();
    for value in values {
        match wire::id(value) {
            Ok(id) if !tags.contains(&TagId(id)) => tags.push(TagId(id)),
            _ => return Vec::new(),
        }
    }
    tags
}

/// The tags a forum offers. A malformed tag is left out; an overlong list
/// is left out entirely.
pub(super) fn forum_tags(value: &Value) -> Vec<ForumTag> {
    let Some(values) = value.as_array().filter(|v| v.len() <= MAX_TAGS) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    values
        .iter()
        .filter_map(|tag| {
            let id = TagId(wire::id(&tag["id"]).ok()?);
            let name = tag["name"].as_str()?;
            if name.trim().is_empty()
                || name.chars().count() > MAX_TAG_NAME
                || name.chars().any(char::is_control)
                || !seen.insert(id)
            {
                return None;
            }
            // A custom emoji's name is not something to draw; only Unicode is kept.
            let emoji = tag["emoji_name"]
                .as_str()
                .filter(|e| tag["emoji_id"].is_null() && !e.is_empty() && e.chars().count() <= 8);
            Some(ForumTag {
                id,
                name: name.to_owned(),
                emoji: emoji.map(str::to_owned),
                moderated: tag["moderated"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

/// A thread of `server` from `data`, or `None` when it is not one that can
/// be shown: wrong kind, no parent, hidden, or malformed.
fn decode(snapshot: &Snapshot, data: &Value, server: ServerId) -> Option<Channel> {
    let mut thread = decode_channel(data, Some(server), &snapshot.users).ok()??;
    if !thread.is_thread() {
        return None;
    }
    // The parent must be a known channel of the server that can hold threads.
    snapshot.channels.iter().find(|c| {
        Some(c.id) == thread.parent
            && c.server == Some(server)
            && matches!(
                c.kind,
                ChannelKind::Text | ChannelKind::Announcement | ChannelKind::Forum
            )
    })?;
    let previous = snapshot
        .channels
        .iter()
        .find(|c| c.id == thread.id)
        .and_then(|c| c.thread.as_ref());
    thread.thread = Some(metadata(data, thread.id, previous));
    Some(thread)
}

/// Put `thread` in the snapshot, in place of what it was.
fn keep(snapshot: &mut Snapshot, thread: Channel) -> Result<(), LoginError> {
    if let Some(known) = snapshot.channels.iter_mut().find(|c| c.id == thread.id) {
        *known = thread;
    } else if snapshot.channels.len() >= MAX_ITEMS {
        return Err(super::BAD);
    } else {
        snapshot.channels.push(thread);
    }
    Ok(())
}

/// The active threads of a guild object (READY, GUILD_CREATE). They are an
/// optional part: a malformed list or entry is left out.
pub(super) fn add_active(snapshot: &mut Snapshot, data: &Value, server: ServerId) {
    let Some(threads) = data["threads"].as_array().filter(|t| t.len() <= MAX_ITEMS) else {
        return;
    };
    for data in threads {
        if let Some(thread) = decode(snapshot, data, server) {
            if !snapshot.channels.iter().any(|c| c.id == thread.id) {
                let _ = keep(snapshot, thread);
            }
        }
    }
}

/// After a new READY of the same account: the threads of `before` it does
/// not name stay, as the pages of threads read them (a READY names only
/// some open ones), under a parent it still names and shows, with what that
/// parent allows now. An open one the account followed goes: the READY
/// would have named it. The same rule as the Store's `keep_paged_threads`,
/// so both keep the same threads.
pub(crate) fn keep_paged(snapshot: &mut Snapshot, before: Vec<Channel>) {
    for thread in before {
        let followed_open = thread
            .thread
            .as_ref()
            .is_some_and(|info| info.joined && !info.archived);
        let parent_shown = snapshot.channels.iter().any(|c| {
            Some(c.id) == thread.parent
                && c.server == thread.server
                && !c.is_thread()
                && c.permissions.contains(Permissions::VIEW)
        });
        if thread.is_thread()
            && !followed_open
            && parent_shown
            && !snapshot.channels.iter().any(|c| c.id == thread.id)
        {
            let _ = keep(snapshot, thread);
        }
    }
    super::recompute(snapshot);
}

fn server_of(snapshot: &Snapshot, data: &Value) -> Result<Option<ServerId>, LoginError> {
    let server = ServerId(wire::id(&data["guild_id"])?);
    Ok(snapshot
        .servers
        .iter()
        .any(|s| s.id == server)
        .then_some(server))
}

fn me(snapshot: &Snapshot) -> Option<UserId> {
    snapshot.me.as_ref().map(|me| me.id)
}

/// Apply a THREAD_* dispatch. Unknown guilds and threads that cannot be
/// shown change nothing; a known thread that becomes hidden is removed.
pub(super) fn update(
    snapshot: &mut Snapshot,
    kind: &str,
    data: &Value,
) -> Result<Vec<Event>, LoginError> {
    let Some(server) = server_of(snapshot, data)? else {
        return Ok(Vec::new());
    };
    if kind == "THREAD_LIST_SYNC" {
        return sync(snapshot, data, server);
    }
    let id = ChannelId(wire::id(&data["id"])?);
    let known_thread = snapshot
        .channels
        .iter()
        .any(|c| c.id == id && c.is_thread() && c.server == Some(server));
    let removed = || vec![Event::ChannelRemoved { channel: id }];
    match kind {
        "THREAD_CREATE" | "THREAD_UPDATE" => match decode(snapshot, data, server) {
            Some(thread) => keep(snapshot, thread)?,
            // Hidden now, or no longer something that can be shown.
            None if known_thread => {
                remove_channel(snapshot, id);
                return Ok(removed());
            }
            None => return Ok(Vec::new()),
        },
        "THREAD_DELETE" if known_thread => {
            remove_channel(snapshot, id);
            return Ok(removed());
        }
        "THREAD_MEMBER_UPDATE" | "THREAD_MEMBERS_UPDATE" if known_thread => {
            let me = me(snapshot);
            // The account's own membership, when this names it: its silence.
            let (own, joined, left) = if kind == "THREAD_MEMBER_UPDATE" {
                let user = optional_id(&data["user_id"])?.map(UserId);
                let mine = user.is_none_or(|user| Some(user) == me);
                (mine.then_some(data), mine, false)
            } else {
                let own = rows(&data["added_members"], 100)?
                    .iter()
                    .find(|m| optional_id(&m["user_id"]).ok().flatten().map(UserId) == me);
                let removed = rows(&data["removed_member_ids"], 100)?
                    .iter()
                    .any(|m| wire::id(m).ok().map(UserId) == me);
                (own, own.is_some(), removed)
            };
            let channel = snapshot.channels.iter_mut().find(|c| c.id == id);
            if let Some(thread) = channel.and_then(|c| c.thread.as_mut()) {
                thread.members = count(&data["member_count"]).unwrap_or(thread.members);
                thread.joined = (thread.joined || joined) && !left;
                if let Some(member) = own {
                    thread.muted_until = muted_until(member);
                }
                if left {
                    thread.muted_until = None;
                }
            }
        }
        _ => return Ok(Vec::new()),
    }
    // Its access comes from its parent: work it out before reporting it.
    super::recompute(snapshot);
    Ok(snapshot
        .channels
        .iter()
        .filter(|c| c.id == id)
        .cloned()
        .map(Event::ChannelChanged)
        .collect())
}

/// The active threads of some parents (or the whole guild, without
/// `channel_ids`) replace the ones known there.
fn sync(snapshot: &mut Snapshot, data: &Value, server: ServerId) -> Result<Vec<Event>, LoginError> {
    let scope: Option<HashSet<ChannelId>> = match data.get("channel_ids") {
        None | Some(Value::Null) => None,
        Some(value) => Some(
            rows(value, MAX_ITEMS)?
                .iter()
                .map(|id| wire::id(id).map(ChannelId))
                .collect::<Result<_, _>>()?,
        ),
    };
    let in_scope = |channel: &Channel| {
        channel.server == Some(server)
            && scope
                .as_ref()
                .is_none_or(|scope| channel.parent.is_some_and(|p| scope.contains(&p)))
    };
    let mut incoming: Vec<Channel> = Vec::new();
    for data in rows(&data["threads"], MAX_ITEMS)? {
        if let Some(thread) = decode(snapshot, data, server) {
            if in_scope(&thread) && !incoming.iter().any(|t| t.id == thread.id) {
                incoming.push(thread);
            }
        }
    }
    let me = me(snapshot);
    for member in rows(&data["members"], MAX_ITEMS)? {
        let user = optional_id(&member["user_id"]).ok().flatten().map(UserId);
        let Ok(thread) = wire::id(&member["id"]).map(ChannelId) else {
            continue;
        };
        if user.is_none_or(|user| Some(user) == me) {
            if let Some(info) = incoming
                .iter_mut()
                .find(|t| t.id == thread)
                .and_then(|t| t.thread.as_mut())
            {
                info.joined = true;
                info.muted_until = muted_until(member);
            }
        }
    }
    let gone: Vec<ChannelId> = snapshot
        .channels
        .iter()
        .filter(|c| {
            c.is_thread()
                && in_scope(c)
                && !c.thread.as_ref().is_some_and(|t| t.archived)
                && !incoming.iter().any(|t| t.id == c.id)
        })
        .map(|c| c.id)
        .collect();
    let mut events = Vec::new();
    for id in gone {
        remove_channel(snapshot, id);
        events.push(Event::ChannelRemoved { channel: id });
    }
    let ids: Vec<ChannelId> = incoming.iter().map(|t| t.id).collect();
    for thread in incoming {
        keep(snapshot, thread)?;
    }
    super::recompute(snapshot);
    events.extend(
        snapshot
            .channels
            .iter()
            .filter(|c| ids.contains(&c.id))
            .cloned()
            .map(Event::ChannelChanged),
    );
    Ok(events)
}

/// A forum's defaults from a channel object; a field it leaves out keeps
/// what `known` says. Discord's documented values: layout 2 is the gallery,
/// sort order 1 is by creation, and flag `1 << 4` asks posts for a tag.
pub(super) fn forum_defaults(data: &Value, known: &ForumDefaults) -> ForumDefaults {
    let number = |key: &str| data.get(key).and_then(Value::as_u64);
    ForumDefaults {
        gallery: number("default_forum_layout").map_or(known.gallery, |layout| layout == 2),
        by_creation: number("default_sort_order").map_or(known.by_creation, |order| order == 1),
        require_tag: number("flags").map_or(known.require_tag, |flags| flags & (1 << 4) != 0),
        // Server administration, rest (P7).
        reaction: data.get("default_reaction_emoji").map_or_else(
            || known.reaction.clone(),
            crate::admin::rest::default_reaction,
        ),
        // Server emoji, archive and tag filter (2026-10-19).
        reaction_emoji: data.get("default_reaction_emoji").map_or(
            known.reaction_emoji,
            crate::admin::rest::default_reaction_emoji,
        ),
        archive_after: number("default_auto_archive_duration")
            .and_then(|after| u32::try_from(after).ok())
            .filter(|after| rune_model::ARCHIVE_AFTER.contains(after))
            .or(known.archive_after),
        match_all_tags: data
            .get("default_tag_setting")
            .and_then(Value::as_str)
            .map_or(known.match_all_tags, |setting| setting == "match_all"),
    }
}

/// A thread as a REST answer names it. Its permissions come from its parent
/// once it is kept: see [`confirmed`].
pub(crate) fn thread_answer(data: &Value) -> Result<Channel, LoginError> {
    let server = ServerId(wire::id(&data["guild_id"])?);
    decode_channel(data, Some(server), &[])?
        .filter(Channel::is_thread)
        .ok_or(super::BAD)
}

/// A thread's or a post's name: 1 to 100 characters on one line, as Discord
/// takes them.
fn valid_name(name: &str) -> Result<(), LoginError> {
    let name = name.trim();
    if name.is_empty()
        || name.chars().count() > MAX_THREAD_NAME
        || name.chars().any(char::is_control)
    {
        return Err(LoginError::new(es(
            "El nombre del hilo tiene de 1 a 100 caracteres, en una línea.",
        )));
    }
    Ok(())
}

/// Whether a post of `forum` may carry exactly `tags`, `before` being the
/// ones it carries now.
fn valid_tags(
    forum: &Channel,
    before: &[TagId],
    tags: &[TagId],
    moderates: bool,
) -> Result<(), LoginError> {
    let distinct: HashSet<&TagId> = tags.iter().collect();
    if tags.len() > MAX_APPLIED_TAGS
        || distinct.len() != tags.len()
        || tags
            .iter()
            .any(|tag| !forum.tags.iter().any(|t| t.id == *tag))
    {
        return Err(LoginError::new(es("Esas etiquetas no son del foro.")));
    }
    if forum.forum.require_tag && tags.is_empty() {
        return Err(LoginError::new(es("Este foro pide al menos una etiqueta.")));
    }
    let moderated = |tag: &TagId| forum.tags.iter().any(|t| t.id == *tag && t.moderated);
    if !moderates
        && tags
            .iter()
            .any(|tag| !before.contains(tag) && moderated(tag))
    {
        return Err(LoginError::new(es(
            "Esa etiqueta es solo para la moderación.",
        )));
    }
    Ok(())
}

/// A cursor a page of threads gave: how many open posts came before, an
/// archive instant for the archived public and private lists (see
/// [`ArchiveCursor`]), a thread id for the account's own private ones.
pub(crate) fn valid_cursor(scope: ThreadScope, cursor: &str) -> bool {
    match scope {
        ThreadScope::Active => cursor
            .parse::<usize>()
            .is_ok_and(|offset| offset > 0 && offset < crate::http::MAX_ACTIVE),
        ThreadScope::Joined => cursor.parse::<u64>().is_ok_and(|id| id > 0),
        ThreadScope::Archived | ThreadScope::Private => ArchiveCursor::parse(cursor).is_some(),
    }
}

/// Threads read at a page's last archive instant that a cursor carries, at
/// most; past that the next page starts strictly before the instant.
pub(crate) const MAX_EDGE: usize = 100;

/// Where an archived list goes on: the archive instant of the last thread
/// read, as Discord wrote it, and, after `|`, the ids of the threads read
/// archived at exactly that instant. With them, the next page is asked a
/// millisecond past the instant and leaves those out, so none archived in
/// the same instant is skipped (the route's `before` is exclusive);
/// without them, strictly before it.
#[derive(Debug)]
pub(crate) struct ArchiveCursor<'a> {
    pub stamp: &'a str,
    pub at: chrono::DateTime<chrono::FixedOffset>,
    pub seen: Option<Vec<u64>>,
}

impl<'a> ArchiveCursor<'a> {
    pub(crate) fn parse(cursor: &'a str) -> Option<Self> {
        let (stamp, seen) = match cursor.split_once('|') {
            Some((stamp, ids)) => {
                let ids: Vec<u64> = ids
                    .split(',')
                    .map(|id| id.parse().ok().filter(|id| *id > 0))
                    .collect::<Option<_>>()?;
                if ids.len() > MAX_EDGE {
                    return None;
                }
                (stamp, Some(ids))
            }
            None => (cursor, None),
        };
        if stamp.len() > 64 {
            return None;
        }
        let at = chrono::DateTime::parse_from_rfc3339(stamp).ok()?;
        Some(Self { stamp, at, seen })
    }
}

const NOT_ALLOWED: &str = es("No tenés permiso para esta acción en el canal.");

/// Whether a command about threads may go to Discord, checked against what
/// this session knows; Discord stays the last word.
pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    let channel = |id: &ChannelId| {
        snapshot
            .channels
            .iter()
            .find(|c| {
                c.id == *id && c.server.is_some() && c.permissions.contains(Permissions::VIEW)
            })
            .ok_or(LoginError::new(es("El canal ya no está disponible.")))
    };
    let thread = |id: &ChannelId| {
        channel(id)?
            .thread
            .as_ref()
            .ok_or(LoginError::new(es("El hilo ya no está disponible.")))
    };
    let may = |found: &Channel, needed: Permissions| {
        if found.permissions.contains(needed) {
            Ok(())
        } else {
            Err(LoginError::new(NOT_ALLOWED))
        }
    };
    match command {
        Command::CreateThread {
            channel: parent,
            message,
            name,
            private,
            ..
        } => {
            let parent = channel(parent)?;
            let kind_ok = match parent.kind {
                ChannelKind::Text => !(*private && message.is_some()),
                ChannelKind::Announcement => !*private,
                _ => false,
            };
            if !kind_ok || message.is_some_and(|message| message.0 == 0) {
                return Err(LoginError::new(es("En este canal no se abre ese hilo.")));
            }
            valid_name(name)?;
            let needed = if *private {
                Permissions::CREATE_PRIVATE_THREADS
            } else {
                Permissions::CREATE_THREADS
            };
            let needed = if message.is_some() {
                needed | Permissions::READ_HISTORY
            } else {
                needed
            };
            may(parent, needed)?;
            crate::messages::unrestricted(snapshot, parent.id)
        }
        Command::CreatePost {
            forum,
            title,
            tags,
            content,
            files,
            ..
        } => {
            let forum = channel(forum)?;
            if forum.kind != ChannelKind::Forum {
                return Err(LoginError::new(es("Solo un foro tiene publicaciones.")));
            }
            valid_name(title)?;
            if files.is_empty() || !content.trim().is_empty() {
                crate::messages::validate_content(content)?;
            }
            may(forum, Permissions::SEND)?;
            if !files.is_empty() {
                may(forum, Permissions::ATTACH)?;
                let ceiling = crate::uploads::CEILING;
                if files.len() > ceiling.max_files
                    || files
                        .iter()
                        .map(|file| file.metadata().size_bytes)
                        .sum::<u64>()
                        > ceiling.max_message_bytes
                {
                    return Err(LoginError::new(es(
                        "La publicación supera los archivos o el tamaño permitidos.",
                    )));
                }
            }
            let moderates = forum.permissions.contains(Permissions::MANAGE_THREADS);
            valid_tags(forum, &[], tags, moderates)?;
            crate::messages::unrestricted(snapshot, forum.id)
        }
        Command::LoadThreads {
            parent,
            scope,
            before,
            ..
        } => {
            let parent = channel(parent)?;
            let fits = match parent.kind {
                ChannelKind::Text => true,
                ChannelKind::Announcement | ChannelKind::Forum => {
                    matches!(scope, ThreadScope::Active | ThreadScope::Archived)
                }
                _ => false,
            };
            if !fits
                || before
                    .as_deref()
                    .is_some_and(|cursor| !valid_cursor(*scope, cursor))
            {
                return Err(wire::invalid());
            }
            may(parent, Permissions::READ_HISTORY)?;
            if *scope == ThreadScope::Private {
                may(parent, Permissions::MANAGE_THREADS)?;
            }
            Ok(())
        }
        Command::FollowThread { thread: id, .. } => {
            if thread(id)?.archived {
                return Err(LoginError::new(es(
                    "Abrí el hilo otra vez antes de seguirlo o dejarlo.",
                )));
            }
            Ok(())
        }
        Command::EditThread { thread: id, edit } => {
            let found = channel(id)?;
            let info = thread(id)?;
            let moderates = found.permissions.contains(Permissions::MANAGE_THREADS);
            let owns = info.owner.is_some() && info.owner == me(snapshot);
            if edit.is_empty()
                || !(moderates || owns)
                || (edit.locked.is_some() && !moderates)
                || (info.locked && !moderates && edit.archived.is_some())
            {
                return Err(LoginError::new(NOT_ALLOWED));
            }
            if let Some(name) = &edit.name {
                valid_name(name)?;
            }
            if let Some(tags) = &edit.tags {
                let forum = found
                    .parent
                    .and_then(|parent| snapshot.channels.iter().find(|c| c.id == parent))
                    .filter(|parent| parent.kind == ChannelKind::Forum)
                    .ok_or(LoginError::new(es(
                        "Solo las publicaciones de un foro llevan etiquetas.",
                    )))?;
                valid_tags(forum, &info.tags, tags, moderates)?;
            }
            crate::messages::unrestricted(snapshot, found.id)
        }
        Command::DeleteThread { thread: id } => {
            thread(id)?;
            may(channel(id)?, Permissions::MANAGE_THREADS)
        }
        // Threads, forums and stages (2026-10-18): as the official client
        // offers it, only to a member of the thread.
        Command::MuteThread { thread: id, until } => {
            if !thread(id)?.joined {
                return Err(LoginError::new(es("Seguí el hilo antes de silenciarlo.")));
            }
            if until.is_some_and(|until| until <= crate::messages::wall_now()) {
                return Err(LoginError::new(es("Elegí cuánto tiempo.")));
            }
            Ok(())
        }
        _ => Err(LoginError::new(es(
            "Esta acción todavía no está disponible.",
        ))),
    }
}

/// Whether `command` is one [`allowed`] and [`confirmed`] answer for.
pub(crate) fn is_thread_command(command: &Command) -> bool {
    matches!(
        command,
        Command::CreateThread { .. }
            | Command::CreatePost { .. }
            | Command::LoadThreads { .. }
            | Command::FollowThread { .. }
            | Command::EditThread { .. }
            | Command::DeleteThread { .. }
            | Command::MuteThread { .. }
    )
}

/// What Discord's answer to `command` changes in the session, as events with
/// the permissions each thread takes from its parent. A thread whose parent
/// this session does not know is left out, and so is its creation; a page
/// of archived threads never replaces a thread known already, which the
/// Gateway keeps more current.
pub(crate) fn confirmed(
    snapshot: &mut Snapshot,
    command: &Command,
    events: Vec<Event>,
) -> Vec<Event> {
    match command {
        Command::FollowThread { thread, follow } => {
            let Some(channel) = snapshot.channels.iter_mut().find(|c| c.id == *thread) else {
                return Vec::new();
            };
            let moderates = channel.permissions.contains(Permissions::MANAGE_THREADS);
            let Some(info) = channel.thread.as_mut() else {
                return Vec::new();
            };
            if info.joined != *follow {
                info.joined = *follow;
                info.members = if *follow {
                    info.members.saturating_add(1)
                } else {
                    info.members.saturating_sub(1)
                };
            }
            // Leaving it ends its silence, as THREAD_MEMBERS_UPDATE says.
            if !*follow {
                info.muted_until = None;
            }
            // Leaving a private thread hides it from all but moderation.
            if !*follow && info.private && !moderates {
                remove_channel(snapshot, *thread);
                return vec![Event::ChannelRemoved { channel: *thread }];
            }
            return vec![Event::ChannelChanged(channel.clone())];
        }
        Command::DeleteThread { thread } => remove_channel(snapshot, *thread),
        // Threads, forums and stages (2026-10-18).
        Command::MuteThread { thread, until } => {
            let Some(channel) = snapshot.channels.iter_mut().find(|c| c.id == *thread) else {
                return Vec::new();
            };
            let Some(info) = channel.thread.as_mut() else {
                return Vec::new();
            };
            info.muted_until = *until;
            return vec![Event::ChannelChanged(channel.clone())];
        }
        _ => {}
    }
    let page = matches!(command, Command::LoadThreads { .. });
    let created = matches!(
        command,
        Command::CreateThread { .. } | Command::CreatePost { .. }
    );
    let me = me(snapshot);
    let mut kept = Vec::new();
    let mut dropped = Vec::new();
    let mut rest = Vec::new();
    for event in events {
        let Event::ChannelChanged(mut thread) = event else {
            rest.push(event);
            continue;
        };
        let parent_known = snapshot.channels.iter().any(|c| {
            Some(c.id) == thread.parent
                && c.server == thread.server
                && matches!(
                    c.kind,
                    ChannelKind::Text | ChannelKind::Announcement | ChannelKind::Forum
                )
        });
        let known = snapshot.channels.iter().find(|c| c.id == thread.id);
        if !thread.is_thread() || !parent_known {
            dropped.push(thread.id);
            continue;
        }
        if page && known.is_some() {
            // Only what a card shows of its first message, which nothing
            // else tells (2026-10-18).
            let starter = thread.thread.and_then(|info| info.starter);
            if let Some(info) = snapshot
                .channels
                .iter_mut()
                .find(|c| c.id == thread.id)
                .and_then(|c| c.thread.as_mut())
                .filter(|info| starter.is_some() && info.starter != starter)
            {
                info.starter = starter;
                kept.push(thread.id);
            }
            continue;
        }
        let previous = known.and_then(|c| c.thread.clone());
        if let Some(info) = thread.thread.as_mut() {
            if let Some(previous) = previous {
                // An answer without its member object or counts says nothing of them.
                info.joined |= previous.joined;
                info.owner = info.owner.or(previous.owner);
                if info.messages == 0 {
                    info.messages = previous.messages;
                }
                if info.members == 0 {
                    info.members = previous.members;
                }
                // Nor of the account's silence, nor of a card's first message.
                info.muted_until = info.muted_until.or(previous.muted_until);
                info.starter = info.starter.take().or(previous.starter);
                info.last_activity = info.last_activity.max(previous.last_activity);
            }
            if created {
                // Whoever starts a thread is in it.
                info.joined = true;
                info.owner = info.owner.or(me);
            }
        }
        let id = thread.id;
        if keep(snapshot, thread).is_ok() {
            kept.push(id);
        }
    }
    super::recompute(snapshot);
    let mut reported: Vec<Event> = snapshot
        .channels
        .iter()
        .filter(|c| kept.contains(&c.id))
        .cloned()
        .map(Event::ChannelChanged)
        .collect();
    reported.extend(rest.into_iter().filter(|event| {
        !matches!(event, Event::Created { created: rune_provider::Created::Channel(id), .. }
            if dropped.contains(id))
    }));
    reported
}
