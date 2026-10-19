//! A server's scheduled events on the personal session (eventos-programados).
//! Routes and bodies are the official web client's (docs/REA.md, «eventos
//! programados») and Discord's documented ones:
//! `GET /guilds/{g}/scheduled-events?with_user_count=true`, what interests
//! the account in `GET /users/@me/scheduled-events?guild_ids={g}`, who an
//! event interests in `GET …/{e}/users`, «Me interesa» as
//! `PUT …/{e}/users/@me` with `{response: 1}` and `DELETE` of the same,
//! and `POST`/`PATCH`/`DELETE` of the events themselves; the Gateway
//! reports `GUILD_SCHEDULED_EVENT_*`. Serein has none of it. Fixtures only:
//! none was tried against Discord.

use rune_i18n::es;
use rune_model::scheduled_events::{MAX_EVENT_DESCRIPTION, MAX_EVENT_LOCATION, MAX_EVENT_NAME};
use rune_model::{
    ChannelKind, EventDraft, EventPlace, EventStatus, Permissions, ScheduledEvent,
    ScheduledEventId, ServerId, Timestamp, UserId,
};
use rune_provider::{Command, Event, Snapshot};
use serde_json::{json, Value};

use crate::{wire, LoginError};

/// The most a list of events or of interested people may weigh.
pub(crate) const MAX_LIST: usize = 2 * 1024 * 1024;
/// Events of one server RUNE reads, at most (Discord allows 100 active).
const MAX_EVENTS: usize = 200;
/// The interested people asked for at once, as the official client asks.
pub(crate) const INTERESTED_PAGE: usize = 100;
/// `privacy_level` of an event only members see, the one Discord takes.
const GUILD_ONLY: u64 = 2;
/// The `response` that says an event interests someone.
pub(crate) const INTERESTED: u64 = 1;

/// Whether `command` is about scheduled events.
pub(crate) fn is_events(command: &Command) -> bool {
    matches!(
        command,
        Command::LoadScheduledEvents { .. }
            | Command::LoadEventInterested { .. }
            | Command::SetEventInterest { .. }
            | Command::CreateScheduledEvent { .. }
            | Command::EditScheduledEvent { .. }
            | Command::SetScheduledEventStatus { .. }
            | Command::DeleteScheduledEvent { .. }
    )
}

/// Whether `command` only reads: a lost answer is never ambiguous.
pub(crate) fn is_read(command: &Command) -> bool {
    matches!(
        command,
        Command::LoadScheduledEvents { .. } | Command::LoadEventInterested { .. }
    )
}

fn server_of(command: &Command) -> Option<ServerId> {
    match command {
        Command::LoadScheduledEvents { server, .. }
        | Command::LoadEventInterested { server, .. }
        | Command::SetEventInterest { server, .. }
        | Command::CreateScheduledEvent { server, .. }
        | Command::EditScheduledEvent { server, .. }
        | Command::SetScheduledEventStatus { server, .. }
        | Command::DeleteScheduledEvent { server, .. } => Some(*server),
        _ => None,
    }
}

fn now() -> Timestamp {
    Timestamp(chrono::Utc::now().timestamp_millis())
}

/// Whether `command` may go to Discord, checked against what this session
/// knows; Discord stays the last word (whose event it is, among others).
pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    let me = snapshot.me.as_ref().ok_or_else(wire::invalid)?.id;
    let server = server_of(command)
        .and_then(|id| snapshot.servers.iter().find(|known| known.id == id))
        .ok_or(LoginError::new(es("Ese servidor ya no está disponible.")))?;
    let mine = snapshot
        .members
        .iter()
        .find(|known| known.server == server.id && known.user == me);
    let owner = server.owner == Some(me);
    let permissions = match mine {
        _ if owner => Permissions::ALL,
        Some(mine) => rune_model::permissions::in_server(server, mine),
        None => {
            return Err(LoginError::new(es(
                "Tu membresía en este servidor todavía no cargó.",
            )))
        }
    };
    let writes = matches!(
        command,
        Command::CreateScheduledEvent { .. }
            | Command::EditScheduledEvent { .. }
            | Command::SetScheduledEventStatus { .. }
            | Command::DeleteScheduledEvent { .. }
    );
    if writes && !permissions.intersects(Permissions::CREATE_EVENTS | Permissions::MANAGE_EVENTS) {
        return Err(LoginError::new(es(
            "No tenés permiso para cambiar eventos en este servidor.",
        )));
    }
    let draft = match command {
        Command::CreateScheduledEvent { draft, .. } => Some((draft, true)),
        // Whether it started is Discord's to say: its start is not judged.
        Command::EditScheduledEvent { draft, .. } => Some((draft, false)),
        Command::SetScheduledEventStatus { status, .. }
            if !matches!(status, EventStatus::Active | EventStatus::Completed) =>
        {
            return Err(LoginError::new(es(
                "Un evento sólo se inicia o se termina.",
            )))
        }
        _ => None,
    };
    if let Some((draft, starts)) = draft {
        if draft.problem(now(), starts).is_some() {
            return Err(LoginError::new(es("El evento no está completo: revisalo.")));
        }
        let wanted = match draft.place {
            EventPlace::Stage(_) => Some(ChannelKind::Stage),
            EventPlace::Voice(_) => Some(ChannelKind::Voice),
            EventPlace::External(_) => None,
        };
        if let (Some(kind), Some(channel)) = (wanted, draft.place.channel()) {
            if !snapshot.channels.iter().any(|known| {
                known.id == channel && known.server == Some(server.id) && known.kind == kind
            }) {
                return Err(LoginError::new(es("Ese canal ya no está en el servidor.")));
            }
        }
    }
    Ok(())
}

/// A text of at most `chars` characters, as an event carries it: absent is
/// `None`, a longer one is malformed.
fn text(value: &Value, chars: usize) -> Result<Option<String>, LoginError> {
    match value {
        Value::Null => Ok(None),
        Value::String(text) if text.chars().count() <= chars => {
            let text = text.trim();
            Ok((!text.is_empty()).then(|| {
                text.chars()
                    .filter(|c| !c.is_control() || *c == '\n')
                    .collect()
            }))
        }
        _ => Err(wire::invalid()),
    }
}

/// One event as Discord sends it, in `server` when known. `None` for one
/// RUNE does not show: a kind or a status it does not know.
pub(crate) fn event(
    value: &Value,
    server: Option<ServerId>,
) -> Result<Option<ScheduledEvent>, LoginError> {
    let id = ScheduledEventId(wire::id(&value["id"])?);
    let guild = ServerId(wire::id(&value["guild_id"])?);
    if server.is_some_and(|server| server != guild) {
        return Err(wire::invalid());
    }
    let channel =
        || -> Result<_, LoginError> { Ok(rune_model::ChannelId(wire::id(&value["channel_id"])?)) };
    let place = match value["entity_type"].as_u64() {
        Some(1) => EventPlace::Stage(channel()?),
        Some(2) => EventPlace::Voice(channel()?),
        Some(3) => EventPlace::External(
            text(&value["entity_metadata"]["location"], MAX_EVENT_LOCATION)?
                .ok_or_else(wire::invalid)?,
        ),
        _ => return Ok(None),
    };
    let status = match value["status"].as_u64() {
        Some(1) => EventStatus::Scheduled,
        Some(2) => EventStatus::Active,
        Some(3) => EventStatus::Completed,
        Some(4) => EventStatus::Canceled,
        _ => return Ok(None),
    };
    let end = match &value["scheduled_end_time"] {
        Value::Null => None,
        end => Some(wire::timestamp(end)?),
    };
    let creator = match &value["creator_id"] {
        Value::Null => None,
        creator => Some(UserId(wire::id(creator)?)),
    };
    let interested = match &value["user_count"] {
        Value::Null => None,
        count => Some(count.as_u64().ok_or_else(wire::invalid)?),
    };
    Ok(Some(ScheduledEvent {
        id,
        server: guild,
        name: text(&value["name"], MAX_EVENT_NAME)?.ok_or_else(wire::invalid)?,
        description: text(&value["description"], MAX_EVENT_DESCRIPTION)?,
        start: wire::timestamp(&value["scheduled_start_time"])?,
        end,
        place,
        status,
        image: wire::cdn(&format!("guild-events/{id}"), true, &value["image"]),
        creator,
        interested,
        // What interests the account is read apart (`interests`).
        account_interested: false,
    }))
}

/// `server`'s events, from a list as Discord sends it.
pub(crate) fn events(value: &Value, server: ServerId) -> Result<Vec<ScheduledEvent>, LoginError> {
    let list = value
        .as_array()
        .filter(|list| list.len() <= MAX_EVENTS)
        .ok_or_else(wire::invalid)?;
    let mut events = Vec::new();
    for value in list {
        events.extend(event(value, Some(server))?);
    }
    Ok(events)
}

/// The events that interest the account, from the list of what it
/// answered: those with `response` 1, or with none (the documented shape),
/// and never one occurrence of a repeating event.
pub(crate) fn interests(value: &Value) -> Result<Vec<ScheduledEventId>, LoginError> {
    let list = value
        .as_array()
        .filter(|list| list.len() <= MAX_EVENTS * 4)
        .ok_or_else(wire::invalid)?;
    let mut found = Vec::new();
    for answer in list {
        let interested = match &answer["response"] {
            Value::Null => true,
            response => response.as_u64() == Some(INTERESTED),
        };
        if interested && answer["guild_scheduled_event_exception_id"].is_null() {
            found.push(ScheduledEventId(wire::id(
                &answer["guild_scheduled_event_id"],
            )?));
        }
    }
    Ok(found)
}

/// Who an event interests, from the list Discord sends: the documented
/// array, or the official client's `{users: […]}`.
pub(crate) fn interested(value: &Value) -> Result<Vec<rune_model::User>, LoginError> {
    let list = value
        .as_array()
        .or_else(|| value["users"].as_array())
        .filter(|list| list.len() <= INTERESTED_PAGE)
        .ok_or_else(wire::invalid)?;
    list.iter()
        .map(|entry| wire::user(&entry["user"]))
        .collect()
}

/// An instant as Discord takes it.
fn instant(at: Timestamp) -> Result<String, LoginError> {
    chrono::DateTime::from_timestamp_millis(at.0)
        .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .ok_or_else(wire::invalid)
}

/// The body that plans or changes an event as `draft` says. Its cover and
/// a repetition, which RUNE does not edit, are left out, so they stay.
pub(crate) fn body(draft: &EventDraft) -> Result<Value, LoginError> {
    let (kind, channel, metadata) = match &draft.place {
        EventPlace::Stage(channel) => (1, Some(channel.to_string()), Value::Null),
        EventPlace::Voice(channel) => (2, Some(channel.to_string()), Value::Null),
        EventPlace::External(location) => (3, None, json!({ "location": location.trim() })),
    };
    Ok(json!({
        "name": draft.name.trim(),
        "description": draft.description.as_deref().map(str::trim).filter(|text| !text.is_empty()),
        "privacy_level": GUILD_ONLY,
        "scheduled_start_time": instant(draft.start)?,
        "scheduled_end_time": draft.end.map(instant).transpose()?,
        "entity_type": kind,
        "channel_id": channel,
        "entity_metadata": metadata,
    }))
}

/// Discord's number of a status RUNE asks for.
pub(crate) fn status(status: EventStatus) -> u64 {
    match status {
        EventStatus::Scheduled => 1,
        EventStatus::Active => 2,
        EventStatus::Completed => 3,
        EventStatus::Canceled => 4,
    }
}

/// What the Gateway reports about events, for a server this session has.
/// `None` for any other dispatch; a malformed one is dropped.
pub(crate) fn dispatch(snapshot: &Snapshot, kind: &str, data: &Value) -> Option<Vec<Event>> {
    if !kind.starts_with("GUILD_SCHEDULED_EVENT_") {
        return None;
    }
    let server = ServerId(wire::id(&data["guild_id"]).ok()?);
    if !snapshot.servers.iter().any(|known| known.id == server) {
        return Some(Vec::new());
    }
    let event_id = || {
        wire::id(&data["guild_scheduled_event_id"])
            .ok()
            .map(ScheduledEventId)
    };
    Some(
        match kind {
            "GUILD_SCHEDULED_EVENT_CREATE" | "GUILD_SCHEDULED_EVENT_UPDATE" => {
                event(data, Some(server))
                    .ok()
                    .flatten()
                    .map(|event| Event::ScheduledEventChanged {
                        request: None,
                        event,
                    })
            }
            "GUILD_SCHEDULED_EVENT_DELETE" => {
                wire::id(&data["id"])
                    .ok()
                    .map(|id| Event::ScheduledEventDeleted {
                        request: None,
                        server,
                        event: ScheduledEventId(id),
                    })
            }
            // One occurrence of a repeating event is not the event.
            "GUILD_SCHEDULED_EVENT_USER_ADD" | "GUILD_SCHEDULED_EVENT_USER_REMOVE"
                if data["guild_scheduled_event_exception_id"].is_null() =>
            {
                event_id()
                    .zip(wire::id(&data["user_id"]).ok())
                    .map(|(event, user)| Event::EventInterest {
                        request: None,
                        server,
                        event,
                        user: UserId(user),
                        interested: kind == "GUILD_SCHEDULED_EVENT_USER_ADD",
                    })
            }
            _ => None,
        }
        .into_iter()
        .collect(),
    )
}

/// The account's own interest, as REST answers it: the account is named
/// here, where the session knows it.
pub(crate) fn confirmed(snapshot: &Snapshot, updates: Vec<Event>) -> Vec<Event> {
    let me = snapshot.me.as_ref().map(|me| me.id);
    updates
        .into_iter()
        .filter_map(|update| match update {
            Event::EventInterest {
                request: Some(request),
                server,
                event,
                interested,
                ..
            } => me.map(|user| Event::EventInterest {
                request: Some(request),
                server,
                event,
                user,
                interested,
            }),
            update => Some(update),
        })
        .collect()
}

#[cfg(test)]
mod tests;
