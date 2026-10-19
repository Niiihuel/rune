//! Writes and pages about threads and forum posts, by the routes Serein uses
//! (its `discord-api` crate: `channel_actions.rs`, `archives.rs` and
//! `create_post`, read 2026-10-17) and Discord documents. Each write is one
//! attempt, never repeated. A thread the answer names is reported only once
//! the provider keeps it beside its parent (`bootstrap::thread_confirmed`).
//! Fixtures only: none of these routes has been tried against Discord.

use reqwest::Method;
use rune_i18n::es;
use rune_model::{Channel, ChannelId, ServerId, TagId};
use rune_provider::{Command, Created, Event, ThreadEdit, ThreadScope};
use serde_json::{json, Value};

use super::{Http, RestError};
use crate::{bootstrap, wire, SessionSecret};

const ANSWER: usize = 1024 * 1024;
/// Threads of one page, as Serein asks for them.
const PAGE: usize = 25;
/// Open posts of a forum read by search, at most, as Serein reads them.
pub(crate) const MAX_ACTIVE: usize = 200;
/// How long a thread stays active without messages: three days, as Serein
/// and the official client start them.
const AUTO_ARCHIVE_MINUTES: u32 = 4320;
/// Past a cursor's instant, so the archived threads of that very instant
/// come again: a millisecond, in case Discord compares no finer.
const EDGE_STEP: chrono::TimeDelta = chrono::TimeDelta::milliseconds(1);

impl Http {
    /// The new post `command` (a `CreatePost`): its files staged first, as
    /// Serein does, reported as the post's progress to `events` when given.
    pub(crate) async fn create_post(
        &self,
        secret: &SessionSecret,
        command: &Command,
        events: Option<&rune_provider::EventSink>,
    ) -> Result<Vec<Event>, RestError> {
        let Command::CreatePost {
            request,
            forum,
            title,
            tags,
            content,
            files,
        } = command
        else {
            return Err(unclear());
        };
        let mut body = json!({
            "name": title.trim(),
            "auto_archive_duration": AUTO_ARCHIVE_MINUTES,
            "message": {
                "content": content,
                "allowed_mentions": {"parse": ["users"], "replied_user": false},
            },
        });
        if !tags.is_empty() {
            body["applied_tags"] = ids(tags);
        }
        // Nothing is posted yet, whatever happens while staging.
        if !files.is_empty() {
            let staged = self
                .post_files(
                    secret,
                    *forum,
                    files,
                    events.map(|events| (events, *request)),
                )
                .await
                .map_err(|error| RestError {
                    ambiguous: false,
                    ..error
                })?;
            body["message"]["attachments"] = json!(staged);
        }
        let value = self
            .thread_write(
                secret,
                Method::POST,
                &format!("/channels/{forum}/threads"),
                body,
            )
            .await?;
        created(*request, &value, *forum)
    }

    pub(super) async fn threads(
        &self,
        secret: &SessionSecret,
        command: &Command,
    ) -> Result<Vec<Event>, RestError> {
        match command {
            Command::CreateThread {
                request,
                channel,
                message,
                name,
                private,
            } => {
                let path = match message {
                    Some(message) => format!("/channels/{channel}/messages/{message}/threads"),
                    None => format!("/channels/{channel}/threads"),
                };
                let mut body =
                    json!({"name": name.trim(), "auto_archive_duration": AUTO_ARCHIVE_MINUTES});
                if message.is_none() {
                    body["type"] = json!(if *private { 12 } else { 11 });
                }
                let value = self.thread_write(secret, Method::POST, &path, body).await?;
                created(*request, &value, *channel)
            }
            Command::CreatePost { .. } => self.create_post(secret, command, None).await,
            Command::LoadThreads {
                request,
                parent,
                scope,
                before,
            } => {
                self.archived(secret, *request, *parent, *scope, before.as_deref())
                    .await
            }
            Command::FollowThread { thread, follow } => {
                // The answer is empty; the provider marks it once this succeeds.
                self.request_answer(
                    secret,
                    if *follow { Method::PUT } else { Method::DELETE },
                    &format!("/channels/{thread}/thread-members/@me"),
                    None,
                    64 * 1024,
                    true,
                )
                .await?;
                Ok(Vec::new())
            }
            Command::EditThread { thread, edit } => {
                let value = self
                    .thread_write(
                        secret,
                        Method::PATCH,
                        &format!("/channels/{thread}"),
                        edit_body(edit),
                    )
                    .await?;
                let changed = bootstrap::thread_answer(&value).map_err(|_| unclear())?;
                if changed.id != *thread {
                    return Err(unclear());
                }
                Ok(vec![Event::ChannelChanged(changed)])
            }
            Command::DeleteThread { thread } => {
                let value = self
                    .request_answer(
                        secret,
                        Method::DELETE,
                        &format!("/channels/{thread}"),
                        None,
                        ANSWER,
                        true,
                    )
                    .await?;
                // The answer, when there is one, is the thread deleted.
                if !value.is_null() && wire::id(&value["id"]).ok() != Some(thread.0) {
                    return Err(unclear());
                }
                Ok(vec![Event::ChannelRemoved { channel: *thread }])
            }
            // Threads, forums and stages (2026-10-18): the account's member
            // settings, by the route and body Serein and the official web
            // client send (`setNotificationSettings`). The answer is not
            // read; the provider marks it once this succeeds and the
            // Gateway's THREAD_MEMBER_UPDATE has the last word.
            Command::MuteThread { thread, until } => {
                let body = match until {
                    None => json!({"muted": false}),
                    Some(until) => {
                        let (end, window) = if *until == rune_model::MUTED_FOREVER {
                            (Value::Null, -1)
                        } else {
                            let end = chrono::DateTime::from_timestamp_millis(until.0)
                                .map(|end| end.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
                                .ok_or_else(|| RestError::rejected(es("Elegí cuánto tiempo.")))?;
                            // The window chosen, to the minute: 900 for 15 minutes.
                            let left = until.0 - crate::messages::wall_now().0;
                            (json!(end), ((left + 30_000) / 60_000).max(1) * 60)
                        };
                        json!({"muted": true, "mute_config": {
                            "end_time": end, "selected_time_window": window}})
                    }
                };
                self.request_answer(
                    secret,
                    Method::PATCH,
                    &format!("/channels/{thread}/thread-members/@me/settings"),
                    Some(body),
                    64 * 1024,
                    true,
                )
                .await?;
                Ok(Vec::new())
            }
            _ => Err(RestError::rejected(es(
                "Esta acción todavía no está disponible.",
            ))),
        }
    }

    /// One page of `parent`'s open threads, which the Gateway tells a
    /// session only of when the account follows them. A forum's come from
    /// the search Serein reads (`offset` is the cursor, up to
    /// [`MAX_ACTIVE`]); a channel's, and a forum's whose search is refused
    /// on its first page, from the guild's list of open threads, in one page.
    pub(crate) async fn active_threads(
        &self,
        secret: &SessionSecret,
        request: rune_provider::RequestId,
        parent: ChannelId,
        server: ServerId,
        forum: bool,
        before: Option<&str>,
    ) -> Result<Vec<Event>, RestError> {
        let offset = match before {
            None => 0,
            Some(cursor) if bootstrap::valid_cursor(ThreadScope::Active, cursor) => {
                cursor.parse::<usize>().unwrap_or(MAX_ACTIVE)
            }
            Some(_) => {
                return Err(RestError::rejected(es(
                    "El cursor de la página no es válido.",
                )))
            }
        };
        if forum {
            let path = format!(
                "/channels/{parent}/threads/search?archived=false&sort_by=last_message_time&sort_order=desc&limit={PAGE}&offset={offset}"
            );
            match self
                .request(secret, Method::GET, &path, None, 2 * 1024 * 1024)
                .await
            {
                Ok(value) => {
                    let more = value["has_more"].as_bool().unwrap_or(false);
                    let mut events = open_page(&value, parent, PAGE)?;
                    // Every event but the page itself is one of its threads.
                    let found = events.len() - 1;
                    let next = (offset + found).min(MAX_ACTIVE);
                    if let Some(Event::ThreadsLoaded {
                        request: answered,
                        next: following,
                        ..
                    }) = events.last_mut()
                    {
                        *answered = request;
                        *following =
                            (more && found > 0 && next < MAX_ACTIVE).then(|| next.to_string());
                    }
                    return Ok(events);
                }
                Err(error) if error.ends_session || offset > 0 => return Err(error),
                // Not offered for this forum: the guild's list stands in.
                Err(_) => {}
            }
        }
        let value = self
            .request(
                secret,
                Method::GET,
                &format!("/guilds/{server}/threads/active"),
                None,
                8 * 1024 * 1024,
            )
            .await?;
        let mut events = open_page(&value, parent, usize::MAX)?;
        if let Some(Event::ThreadsLoaded {
            request: answered, ..
        }) = events.last_mut()
        {
            *answered = request;
        }
        Ok(events)
    }

    async fn thread_write(
        &self,
        secret: &SessionSecret,
        method: Method,
        path: &str,
        body: Value,
    ) -> Result<Value, RestError> {
        // Documented as 201 for a new thread, 200 for a change: either answers.
        let value = self
            .request_answer(secret, method, path, Some(body), ANSWER, true)
            .await?;
        if value.is_object() {
            Ok(value)
        } else {
            Err(unclear())
        }
    }

    /// One page of `parent`'s archived threads in `scope`, newest archived
    /// first, checked as Serein checks it: all archived, of `parent`, of the
    /// scope's kind, in order and before the cursor; a page claiming more
    /// with nothing in it is malformed. A page of the public or private
    /// lists is asked with its cursor's instant included and leaves out the
    /// threads the cursor says were read ([`bootstrap::ArchiveCursor`]).
    async fn archived(
        &self,
        secret: &SessionSecret,
        request: rune_provider::RequestId,
        parent: ChannelId,
        scope: ThreadScope,
        before: Option<&str>,
    ) -> Result<Vec<Event>, RestError> {
        let route = match scope {
            // Open threads need the guild: see `active_threads`.
            ThreadScope::Active => {
                return Err(RestError::rejected(es(
                    "Esta acción todavía no está disponible.",
                )))
            }
            ThreadScope::Archived => "threads/archived/public",
            ThreadScope::Private => "threads/archived/private",
            ThreadScope::Joined => "users/@me/threads/archived/private",
        };
        let mut path = format!("/channels/{parent}/{route}?limit={PAGE}");
        let mut edge = None;
        if let Some(before) = before {
            if !bootstrap::valid_cursor(scope, before) {
                return Err(RestError::rejected(es(
                    "El cursor de la página no es válido.",
                )));
            }
            edge = (scope != ThreadScope::Joined)
                .then(|| bootstrap::ArchiveCursor::parse(before))
                .flatten();
            let asked = match &edge {
                Some(cursor) if cursor.seen.is_some() => {
                    (cursor.at + EDGE_STEP).to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
                }
                Some(cursor) => cursor.stamp.to_owned(),
                None => before.to_owned(),
            };
            let encoded: String = asked.bytes().map(|byte| format!("%{byte:02X}")).collect();
            path.push_str(&format!("&before={encoded}"));
        }
        let bad = || RestError::response(wire::invalid(), false);
        let value = self
            .request(secret, Method::GET, &path, None, 2 * 1024 * 1024)
            .await?;
        let items = value["threads"]
            .as_array()
            .filter(|items| items.len() <= PAGE)
            .ok_or_else(bad)?;
        let more = value["has_more"].as_bool().ok_or_else(bad)?;
        // The account's membership of each, when it is in it.
        let joined = memberships(&value);
        let id_bound = match (scope, before) {
            (ThreadScope::Joined, Some(before)) => before.parse::<u64>().ok(),
            _ => None,
        };
        let mut threads: Vec<Channel> = Vec::new();
        let mut listed: Vec<u64> = Vec::new();
        // The page's last thread, and the ids of those archived in its instant.
        let mut last: Option<(u64, chrono::DateTime<chrono::FixedOffset>, String)> = None;
        let mut at_last: Vec<u64> = Vec::new();
        for item in items {
            let mut thread = bootstrap::thread_answer(item).map_err(|_| bad())?;
            let info = thread.thread.as_mut().ok_or_else(bad)?;
            let stamp = item["thread_metadata"]["archive_timestamp"]
                .as_str()
                .filter(|stamp| stamp.len() <= 64)
                .ok_or_else(bad)?;
            let archived_at = chrono::DateTime::parse_from_rfc3339(stamp).map_err(|_| bad())?;
            let private = scope != ThreadScope::Archived;
            if !info.archived
                || info.private != private
                || thread.parent != Some(parent)
                || listed.contains(&thread.id.0)
            {
                return Err(bad());
            }
            let ordered = match (&last, scope) {
                (None, _) => true,
                (Some((id, _, _)), ThreadScope::Joined) => thread.id.0 < *id,
                (Some((_, at, _)), _) => archived_at <= *at,
            };
            let within = match (&edge, id_bound) {
                (_, Some(id)) => thread.id.0 < id,
                (Some(cursor), _) if cursor.seen.is_some() => archived_at <= cursor.at + EDGE_STEP,
                (Some(cursor), _) => archived_at <= cursor.at,
                (None, None) => true,
            };
            if !ordered || !within {
                return Err(bad());
            }
            listed.push(thread.id.0);
            match &last {
                Some((_, at, _)) if *at == archived_at => at_last.push(thread.id.0),
                _ => at_last = vec![thread.id.0],
            }
            last = Some((thread.id.0, archived_at, stamp.to_owned()));
            // Read on an earlier page: archived after the cursor's instant,
            // or in it and named by the cursor.
            let read = edge.as_ref().is_some_and(|cursor| {
                cursor.seen.as_ref().is_some_and(|seen| {
                    archived_at > cursor.at
                        || (archived_at == cursor.at && seen.contains(&thread.id.0))
                })
            });
            if read {
                continue;
            }
            let membership = joined.iter().find(|(id, _)| *id == thread.id.0);
            info.joined |= scope == ThreadScope::Joined || membership.is_some();
            if let Some((_, until)) = membership {
                info.muted_until = *until;
            }
            threads.push(thread);
        }
        let next = match (more, last) {
            (false, _) => None,
            (true, None) => return Err(bad()),
            (true, Some((id, _, _))) if scope == ThreadScope::Joined => Some(id.to_string()),
            (true, Some((_, at, stamp))) => {
                // Those of the same instant read on earlier pages count too.
                if let Some(seen) = edge
                    .as_ref()
                    .filter(|cursor| cursor.at == at)
                    .and_then(|cursor| cursor.seen.as_ref())
                {
                    for id in seen {
                        if !at_last.contains(id) {
                            at_last.push(*id);
                        }
                    }
                }
                let stuck = threads.is_empty() && edge.as_ref().is_some_and(|c| c.seen.is_some());
                Some(if stuck || at_last.len() > bootstrap::MAX_EDGE {
                    // shortcut: more threads archived in one instant than a
                    // page or a cursor holds go on strictly before it, and
                    // the rest of that instant is skipped; a page per
                    // instant would fix it.
                    stamp
                } else {
                    let ids: Vec<String> = at_last.iter().map(u64::to_string).collect();
                    format!("{stamp}|{}", ids.join(","))
                })
            }
        };
        let names = threads.iter().map(|thread| thread.id).collect();
        Ok(threads
            .into_iter()
            .map(Event::ChannelChanged)
            .chain([Event::ThreadsLoaded {
                request,
                parent,
                scope,
                threads: names,
                next,
            }])
            .collect())
    }
}

/// The body of a change to a thread: only what changes.
fn edit_body(edit: &ThreadEdit) -> Value {
    let mut body = json!({});
    if let Some(name) = &edit.name {
        body["name"] = json!(name.trim());
    }
    if let Some(archived) = edit.archived {
        body["archived"] = json!(archived);
    }
    if let Some(locked) = edit.locked {
        body["locked"] = json!(locked);
    }
    if let Some(tags) = &edit.tags {
        body["applied_tags"] = ids(tags);
    }
    body
}

fn ids(tags: &[TagId]) -> Value {
    Value::Array(tags.iter().map(|tag| json!(tag.0.to_string())).collect())
}

/// The thread a creation's answer names, under the parent asked for.
fn created(
    request: rune_provider::RequestId,
    value: &Value,
    parent: ChannelId,
) -> Result<Vec<Event>, RestError> {
    let mut thread = bootstrap::thread_answer(value).map_err(|_| unclear())?;
    if thread.parent != Some(parent) {
        return Err(unclear());
    }
    // A post's answer carries its first message: its card at once.
    if let (Ok((message, _)), Some(info)) =
        (wire::message(&value["message"]), thread.thread.as_mut())
    {
        if message.channel == thread.id {
            info.starter = Some(rune_model::PostStarter::of(&message));
        }
    }
    let id = thread.id;
    Ok(vec![
        Event::ChannelChanged(thread),
        Event::Created {
            request,
            created: Created::Channel(id),
        },
    ])
}

/// The write left but its answer cannot be read: it may have happened.
fn unclear() -> RestError {
    RestError::response(wire::invalid(), true)
}

/// The account's memberships a page names (`members`): each thread's id
/// and until when it silences it.
fn memberships(value: &Value) -> Vec<(u64, Option<rune_model::Timestamp>)> {
    value["members"]
        .as_array()
        .map(|members| {
            members
                .iter()
                .filter_map(|member| {
                    Some((
                        wire::id(&member["id"]).ok()?,
                        bootstrap::muted_until(member),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The open threads of `parent` an answer lists (`threads`, with the
/// account's `members`), at most `limit`; the guild's list names others too,
/// which are left out. A thread listed twice, archived, or of another kind
/// makes the page malformed. The page event's `request` and `next` are the
/// caller's to set.
fn open_page(value: &Value, parent: ChannelId, limit: usize) -> Result<Vec<Event>, RestError> {
    let bad = || RestError::response(wire::invalid(), false);
    let items = value["threads"]
        .as_array()
        .filter(|items| items.len() <= limit.min(10_000))
        .ok_or_else(bad)?;
    let joined = memberships(value);
    let mut threads: Vec<Channel> = Vec::new();
    for item in items {
        // Another parent's, in the guild's list: not this page's.
        if wire::id(&item["parent_id"]).ok() != Some(parent.0) {
            if limit == usize::MAX {
                continue;
            }
            return Err(bad());
        }
        let mut thread = bootstrap::thread_answer(item).map_err(|_| bad())?;
        let info = thread.thread.as_mut().ok_or_else(bad)?;
        if info.archived || threads.iter().any(|known| known.id == thread.id) {
            return Err(bad());
        }
        if let Some((_, until)) = joined.iter().find(|(id, _)| *id == thread.id.0) {
            info.joined = true;
            info.muted_until = *until;
        }
        threads.push(thread);
    }
    // Threads, forums and stages (2026-10-18): what each card shows of its
    // post's first message, from `first_messages`, as Serein reads them
    // (not documented; a malformed one is left out).
    for message in value["first_messages"]
        .as_array()
        .map_or(&[][..], |messages| &messages[..messages.len().min(PAGE)])
    {
        let Ok((message, _)) = wire::message(message) else {
            continue;
        };
        if let Some(info) = threads
            .iter_mut()
            .find(|thread| thread.id == message.channel)
            .and_then(|thread| thread.thread.as_mut())
        {
            info.starter = Some(rune_model::PostStarter::of(&message));
        }
    }
    let names = threads.iter().map(|thread| thread.id).collect();
    Ok(threads
        .into_iter()
        .map(Event::ChannelChanged)
        .chain([Event::ThreadsLoaded {
            request: 0,
            parent,
            scope: ThreadScope::Active,
            threads: names,
            next: None,
        }])
        .collect())
}
