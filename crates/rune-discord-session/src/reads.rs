//! Read state with the service, as Serein keeps it (its
//! `client-core/src/read_state.rs`, `discord-api/src/lib.rs` `mark_read` and
//! `discord-gateway/src/lib.rs`, read 2026-10-17): acknowledgements go one at
//! a time through the common REST lane, the newest per channel; the
//! account's other sessions' arrive as `MESSAGE_ACK`; READY carries the read
//! state and its version; servers the Gateway only summarizes send passive
//! updates. Remote searches go through the same lane (`http/reads.rs`).
//! Fixtures only: none of this was tried against Discord.

use rune_i18n::es;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use rune_model::{ChannelId, MessageId, Permissions, RemoteSearch, MAX_REMOTE_OFFSET, REMOTE_PAGE};
use rune_provider::{Command, Event, Snapshot};
use serde_json::Value;
use zeroize::Zeroizing;

use crate::{messages::holds_chat, wire, LoginError};

/// Acknowledgements waiting in the lane at once, the newest per channel: a
/// category marked read sends one per channel.
pub(crate) const MAX_QUEUED: usize = 64;
/// Channels one passive update names, at most.
const MAX_PASSIVE: usize = 4_000;

/// What the session keeps of the account's read state with the service.
#[derive(Default)]
pub(crate) struct Reads {
    /// RUNE's versions for what the service confirms: they only grow, across
    /// this provider's sessions, as the state keeps cursors across reconnections.
    versions: Arc<AtomicU64>,
    /// The newest version the service gave per channel, to drop a late ack.
    seen: HashMap<ChannelId, u64>,
    /// How far the service has each channel read, as last told.
    confirmed: HashMap<ChannelId, Option<MessageId>>,
    /// The newest message known per channel, for a whole server read.
    latest: HashMap<ChannelId, MessageId>,
    /// The legacy acknowledgement token the last answer gave, sent with the
    /// next one. Session-only and never shown, as Serein keeps it.
    pub(crate) token: Arc<Mutex<Option<Zeroizing<String>>>>,
}

impl Reads {
    /// Versions shared with the provider, so they keep growing after a reconnection.
    pub(crate) fn new(versions: Arc<AtomicU64>) -> Self {
        Self {
            versions,
            ..Default::default()
        }
    }

    fn next(&self) -> u64 {
        self.versions.fetch_add(1, Ordering::Relaxed) + 1
    }

    /// READY's read state is the service's newest truth: every cursor it
    /// names gets one version, above all earlier ones.
    pub(crate) fn ready(&mut self, data: &Value, snapshot: &mut Snapshot) {
        let version = self.next();
        let service = data["read_state"]["version"].as_u64().unwrap_or(0);
        self.seen.clear();
        self.confirmed.clear();
        self.latest = snapshot.last_messages.iter().copied().collect();
        *self
            .token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        snapshot.read_versions = snapshot
            .read_states
            .iter()
            .map(|(channel, read, _)| {
                self.seen.insert(*channel, service);
                self.confirmed.insert(*channel, *read);
                (*channel, version)
            })
            .collect();
    }

    /// Note the newest message of channels as events report them.
    pub(crate) fn observe(&mut self, event: &Event) {
        match event {
            Event::MessageCreated { message, .. } if !message.delivery.is_local() => {
                let latest = self.latest.entry(message.channel).or_insert(message.id);
                *latest = (*latest).max(message.id);
            }
            Event::LatestMessages(latest) => {
                for (channel, message) in latest {
                    let known = self.latest.entry(*channel).or_insert(*message);
                    *known = (*known).max(*message);
                }
            }
            _ => {}
        }
    }

    /// `MESSAGE_ACK`: the account read a channel on another device, or held
    /// it unread there (`manual`). A late or foreign one is dropped.
    pub(crate) fn dispatch(&mut self, snapshot: &Snapshot, data: &Value) -> Vec<Event> {
        let (Ok(channel), Some(me)) = (wire::id(&data["channel_id"]), snapshot.me.as_ref()) else {
            return Vec::new();
        };
        let channel = ChannelId(channel);
        if !readable(snapshot, channel) {
            return Vec::new();
        }
        let through = match cursor(&data["message_id"]) {
            Ok(through) => through,
            Err(()) => return Vec::new(),
        };
        let service = data["version"].as_u64();
        if let Some(version) = service {
            if self.seen.get(&channel).is_some_and(|seen| version < *seen) {
                return Vec::new();
            }
            self.seen.insert(channel, version);
        }
        if data["manual"].as_bool() == Some(true) {
            self.confirmed.insert(channel, through);
            return vec![Event::MarkedUnread {
                account: me.id,
                channel,
                through,
                anchor: None,
                mentions: data["mention_count"]
                    .as_u64()
                    .and_then(|count| u32::try_from(count).ok())
                    .unwrap_or(0),
                version: self.next(),
            }];
        }
        let Some(message) = through else {
            return Vec::new();
        };
        let known = self.confirmed.entry(channel).or_insert(None);
        if *known > Some(message) {
            return Vec::new();
        }
        *known = Some(message);
        vec![Event::ReadStateConfirmed {
            account: me.id,
            request: None,
            channel,
            message,
            version: self.next(),
        }]
    }

    /// What the service's answer to a read command confirms, as events.
    pub(crate) fn confirmed(&mut self, snapshot: &Snapshot, command: &Command) -> Vec<Event> {
        match command {
            Command::MarkRead {
                request,
                account,
                channel,
                message,
            } => {
                // A newer cursor confirmed meanwhile covers this one.
                let known = self.confirmed.entry(*channel).or_insert(None);
                let message = (*known).max(Some(*message)).unwrap_or(*message);
                *known = Some(message);
                vec![Event::ReadStateConfirmed {
                    account: *account,
                    request: Some(*request),
                    channel: *channel,
                    message,
                    version: self.next(),
                }]
            }
            Command::MarkUnread {
                account,
                channel,
                message,
                mentions,
            } => {
                let through = before(*message);
                self.confirmed.insert(*channel, through);
                vec![Event::MarkedUnread {
                    account: *account,
                    channel: *channel,
                    through,
                    anchor: Some(*message),
                    mentions: *mentions,
                    version: self.next(),
                }]
            }
            Command::MarkServerRead { account, server } => {
                let mut channels: Vec<(ChannelId, MessageId)> = snapshot
                    .channels
                    .iter()
                    .filter(|channel| {
                        channel.server == Some(*server) && readable(snapshot, channel.id)
                    })
                    .filter_map(|channel| Some((channel.id, *self.latest.get(&channel.id)?)))
                    .collect();
                channels.sort();
                channels
                    .into_iter()
                    .filter_map(|(channel, message)| {
                        let known = self.confirmed.entry(channel).or_insert(None);
                        if *known >= Some(message) {
                            return None;
                        }
                        *known = Some(message);
                        Some(Event::ReadStateConfirmed {
                            account: *account,
                            request: None,
                            channel,
                            message,
                            version: self.next(),
                        })
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

/// The cursor just before `message`, as the service takes a reminder: its
/// ids are numbers, the first unread one after the cursor.
pub(crate) fn before(message: MessageId) -> Option<MessageId> {
    message
        .0
        .checked_sub(1)
        .filter(|id| *id != 0)
        .map(MessageId)
}

/// A read-state cursor: an id, a number, or none (`0` or null).
fn cursor(value: &Value) -> Result<Option<MessageId>, ()> {
    match value {
        Value::Null => Ok(None),
        Value::Number(number) => Ok(number.as_u64().filter(|id| *id != 0).map(MessageId)),
        Value::String(text) if text == "0" => Ok(None),
        Value::String(_) => wire::id(value)
            .map(|id| Some(MessageId(id)))
            .map_err(|_| ()),
        _ => Err(()),
    }
}

fn readable(snapshot: &Snapshot, channel: ChannelId) -> bool {
    snapshot.channels.iter().any(|known| {
        known.id == channel
            && holds_chat(known.kind)
            && known
                .permissions
                .contains(Permissions::VIEW | Permissions::READ_HISTORY)
    })
}

/// The commands of this module.
pub(crate) fn is_read(command: &Command) -> bool {
    matches!(
        command,
        Command::MarkRead { .. }
            | Command::MarkUnread { .. }
            | Command::MarkServerRead { .. }
            | Command::SearchMessages { .. }
    )
}

/// An acknowledgement, which the newest for its channel replaces in the lane.
pub(crate) fn ack_channel(command: &Command) -> Option<ChannelId> {
    match command {
        Command::MarkRead { channel, .. } | Command::MarkUnread { channel, .. } => Some(*channel),
        _ => None,
    }
}

/// Whether the session may send `command`, checked as it is queued and
/// again as it leaves.
pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    let me = snapshot.me.as_ref().map(|me| me.id);
    let unavailable = || LoginError::new(es("Esa conversación ya no está disponible para leer."));
    match command {
        Command::MarkRead {
            account,
            channel,
            message,
            ..
        }
        | Command::MarkUnread {
            account,
            channel,
            message,
            ..
        } => {
            if me != Some(*account) || message.0 == 0 {
                return Err(wire::invalid());
            }
            readable(snapshot, *channel)
                .then_some(())
                .ok_or_else(unavailable)
        }
        Command::MarkServerRead { account, server } => {
            if me != Some(*account) {
                return Err(wire::invalid());
            }
            snapshot
                .servers
                .iter()
                .any(|known| known.id == *server)
                .then_some(())
                .ok_or_else(|| LoginError::new(es("Ya no estás en ese servidor.")))
        }
        Command::SearchMessages { search, offset, .. } => searchable(snapshot, search, *offset),
        _ => Err(LoginError::new(es(
            "Esta acción todavía no está disponible.",
        ))),
    }
}

fn searchable(snapshot: &Snapshot, search: &RemoteSearch, offset: u32) -> Result<(), LoginError> {
    search.validate().map_err(LoginError::new)?;
    if offset > MAX_REMOTE_OFFSET || !(offset as usize).is_multiple_of(REMOTE_PAGE) {
        return Err(LoginError::new(es("Esa página de resultados no existe.")));
    }
    let started = snapshot
        .channels
        .iter()
        .find(|known| known.id == search.channel)
        .ok_or_else(|| LoginError::new(es("Esa conversación ya no está disponible.")))?;
    if started.server != search.server {
        return Err(wire::invalid());
    }
    if !readable(snapshot, search.channel) {
        return Err(LoginError::new(es(
            "No tienes permiso para leer el historial de esa conversación.",
        )));
    }
    if search
        .channels
        .iter()
        .any(|channel| !readable(snapshot, *channel))
    {
        return Err(LoginError::new(es(
            "No puedes leer uno de los canales elegidos.",
        )));
    }
    Ok(())
}

/// A passive update, V1 or V2, of a server the Gateway only summarizes: the
/// newest message of its channels, and who is in its voice channels, as
/// `VOICE_STATE_UPDATE`s for the calls to apply. Members are left out: the
/// list and the profile read them.
pub(crate) fn passive(
    snapshot: &Snapshot,
    data: &Value,
) -> (Vec<(ChannelId, MessageId)>, Vec<Value>) {
    let Ok(server) = wire::id(&data["guild_id"]) else {
        return (Vec::new(), Vec::new());
    };
    if !snapshot.servers.iter().any(|known| known.id.0 == server) {
        return (Vec::new(), Vec::new());
    }
    let rows = |key: &str| {
        data[key]
            .as_array()
            .map(|rows| &rows[..rows.len().min(MAX_PASSIVE)])
            .unwrap_or_default()
    };
    let channels = if data.get("updated_channels").is_some() {
        rows("updated_channels")
    } else {
        rows("channels")
    };
    let latest = channels
        .iter()
        .filter_map(|row| {
            let channel = ChannelId(wire::id(&row["id"]).ok()?);
            let message = cursor(&row["last_message_id"]).ok()??;
            snapshot
                .channels
                .iter()
                .any(|known| known.id == channel && known.server.is_some_and(|id| id.0 == server))
                .then_some((channel, message))
        })
        .collect();
    let joined = if data.get("updated_voice_states").is_some() {
        rows("updated_voice_states")
    } else {
        rows("voice_states")
    };
    // Only objects: setting a key on any other JSON value panics.
    let mut voice: Vec<Value> = joined
        .iter()
        .filter(|row| row.is_object())
        .map(|row| {
            let mut row = row.clone();
            row["guild_id"] = Value::String(server.to_string());
            row
        })
        .collect();
    voice.extend(rows("removed_voice_states").iter().filter_map(|user| {
        let user = wire::id(user).ok()?;
        Some(serde_json::json!({"guild_id": server.to_string(), "user_id": user.to_string(), "channel_id": null}))
    }));
    (latest, voice)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use rune_model::{ServerId, UserId};
    use serde_json::json;

    /// A server with two readable channels and a direct conversation.
    pub(crate) fn snapshot() -> Snapshot {
        let me = wire::user(&json!({"id":"42", "username":"me"})).unwrap();
        crate::bootstrap::ready(
            &json!({
                "user":{"id":"42"},
                "guilds":[{
                    "id":"10", "properties":{"name":"Fixture","owner_id":"99"},
                    "roles":[{"id":"10","name":"@everyone","permissions":"66560"}],
                    "channels":[
                        {"id":"30","type":0,"name":"a","permission_overwrites":[],"last_message_id":"120"},
                        {"id":"31","type":0,"name":"b","permission_overwrites":[],"last_message_id":"50"}]
                }],
                "merged_members":[[{"user_id":"42","roles":[]}]],
                "private_channels":[{"id":"20","type":1,"recipients":[{"id":"43","username":"peer"}]}],
                "read_state":{"entries":[{"id":"30","last_message_id":"100","mention_count":0}]}
            }),
            &me,
        )
        .unwrap()
    }

    #[test]
    fn versions_grow_across_readies_and_late_acks_are_dropped() {
        let versions = Arc::default();
        let mut reads = Reads::new(Arc::clone(&versions));
        let mut first = snapshot();
        reads.ready(
            &json!({"read_state":{"version":7,"entries":[]}}),
            &mut first,
        );
        assert_eq!(first.read_versions, vec![(ChannelId(30), 1)]);
        let acked = reads.dispatch(
            &first,
            &json!({"channel_id":"30","message_id":"110","version":8}),
        );
        assert!(matches!(
            acked[..],
            [Event::ReadStateConfirmed {
                message: MessageId(110),
                version: 2,
                request: None,
                ..
            }]
        ));
        // An older service version, or a cursor behind the confirmed one.
        assert!(reads
            .dispatch(
                &first,
                &json!({"channel_id":"30","message_id":"115","version":6})
            )
            .is_empty());
        assert!(reads
            .dispatch(
                &first,
                &json!({"channel_id":"30","message_id":"105","version":9})
            )
            .is_empty());
        // Unknown or unreadable channels and malformed cursors say nothing.
        assert!(reads
            .dispatch(&first, &json!({"channel_id":"99","message_id":"1"}))
            .is_empty());
        assert!(reads
            .dispatch(&first, &json!({"channel_id":"30","message_id":{}}))
            .is_empty());
        // A new provider session keeps counting up.
        let mut again = Reads::new(versions);
        let mut second = snapshot();
        again.ready(&json!({}), &mut second);
        assert!(second.read_versions[0].1 > 2);
    }

    #[test]
    fn a_manual_ack_holds_the_channel_unread_from_after_its_cursor() {
        let mut reads = Reads::new(Arc::default());
        let mut known = snapshot();
        reads.ready(&json!({}), &mut known);
        let held = reads.dispatch(
            &known,
            &json!({"channel_id":"30","message_id":"99","manual":true,"mention_count":2,"version":3}),
        );
        assert!(matches!(
            held[..],
            [Event::MarkedUnread {
                through: Some(MessageId(99)),
                anchor: None,
                mentions: 2,
                ..
            }]
        ));
        // After a reminder, an ack before the old cursor counts again.
        assert_eq!(
            reads
                .dispatch(&known, &json!({"channel_id":"30","message_id":"100"}))
                .len(),
            1
        );
    }

    #[test]
    fn answers_confirm_the_newest_cursor_and_a_server_read_names_its_channels() {
        let mut reads = Reads::new(Arc::default());
        let mut known = snapshot();
        reads.ready(&json!({}), &mut known);
        let read = Command::MarkRead {
            request: 5,
            account: UserId(42),
            channel: ChannelId(30),
            message: MessageId(101),
        };
        reads.dispatch(&known, &json!({"channel_id":"30","message_id":"110"}));
        assert!(matches!(
            reads.confirmed(&known, &read)[..],
            [Event::ReadStateConfirmed {
                request: Some(5),
                message: MessageId(110),
                ..
            }]
        ));
        let unread = Command::MarkUnread {
            account: UserId(42),
            channel: ChannelId(30),
            message: MessageId(105),
            mentions: 1,
        };
        assert!(matches!(
            reads.confirmed(&known, &unread)[..],
            [Event::MarkedUnread {
                through: Some(MessageId(104)),
                anchor: Some(MessageId(105)),
                ..
            }]
        ));
        let whole = Command::MarkServerRead {
            account: UserId(42),
            server: ServerId(10),
        };
        let events = reads.confirmed(&known, &whole);
        let channels: Vec<_> = events
            .iter()
            .map(|event| match event {
                Event::ReadStateConfirmed {
                    channel, message, ..
                } => (*channel, *message),
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(
            channels,
            vec![
                (ChannelId(30), MessageId(120)),
                (ChannelId(31), MessageId(50))
            ]
        );
        assert!(reads.confirmed(&known, &whole).is_empty(), "nothing newer");
    }

    #[test]
    fn commands_are_checked_against_the_session() {
        let known = snapshot();
        let read = |channel: u64, account: u64| Command::MarkRead {
            request: 1,
            account: UserId(account),
            channel: ChannelId(channel),
            message: MessageId(5),
        };
        assert!(allowed(&known, &read(30, 42)).is_ok());
        assert!(allowed(&known, &read(30, 43)).is_err());
        assert!(allowed(&known, &read(99, 42)).is_err());
        let search = |channel: u64, server: Option<u64>, offset| Command::SearchMessages {
            request: 1,
            search: RemoteSearch {
                channel: ChannelId(channel),
                server: server.map(ServerId),
                text: "hola".into(),
                ..Default::default()
            },
            offset,
        };
        assert!(allowed(&known, &search(30, Some(10), 0)).is_ok());
        assert!(allowed(&known, &search(30, Some(10), 25)).is_ok());
        assert!(allowed(&known, &search(30, Some(10), 3)).is_err());
        assert!(
            allowed(&known, &search(30, None, 0)).is_err(),
            "scope must match"
        );
        assert!(allowed(&known, &search(20, None, 0)).is_ok());
    }

    #[test]
    fn passive_updates_of_both_versions_name_channels_and_voice() {
        let known = snapshot();
        let (latest, voice) = passive(
            &known,
            &json!({"guild_id":"10","updated_channels":[{"id":"30","last_message_id":"130"},
                {"id":"99","last_message_id":"5"},{"id":"31","last_message_id":null}],
                "updated_voice_states":[{"user_id":"7","channel_id":"40"}],
                "removed_voice_states":["8"]}),
        );
        assert_eq!(latest, vec![(ChannelId(30), MessageId(130))]);
        assert_eq!(voice.len(), 2);
        assert_eq!(voice[0]["guild_id"], "10");
        assert!(voice[1]["channel_id"].is_null());
        let (latest, voice) = passive(
            &known,
            &json!({"guild_id":"10","channels":[{"id":"31","last_message_id":"60"}],
                "voice_states":[{"user_id":"7","channel_id":"40"}]}),
        );
        assert_eq!(latest, vec![(ChannelId(31), MessageId(60))]);
        assert_eq!(voice.len(), 1);
        assert_eq!(
            passive(&known, &json!({"guild_id":"11","channels":[]})).0,
            vec![]
        );
        let (_, voice) = passive(
            &known,
            &json!({"guild_id":"10","voice_states":[1,"x",[2],null,{"user_id":"7"}]}),
        );
        assert_eq!(voice.len(), 1, "rows that are not objects are skipped");
    }
}
