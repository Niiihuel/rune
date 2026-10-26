//! Calls in direct and group conversations (2026-10-17), as the official
//! web client makes them (static REA of its public bundle, docs/DISCORD.md
//! «Llamadas privadas») and Serein does (`discord-gateway/src/voice.rs`,
//! `discord-api/src/lib.rs`, read the same day; no code copied):
//!
//! - The account starts or answers one with op 4 and a null `guild_id`; op
//!   13 (`{channel_id}`) first asks what the service knows of it, and again
//!   when a conversation is first read.
//! - `CALL_CREATE` and `CALL_UPDATE` say whom it rings (`ongoing_rings`, the
//!   official client's, or `ringing`, Serein's) and, the first, who is in
//!   it; `CALL_DELETE` ends it.
//! - Ringing is REST, by the common path (so a verification can stop it):
//!   `POST /channels/{id}/call/ring` once the service has the call, held
//!   until its `CALL_CREATE` as the official client queues it, and
//!   `/call/stop-ringing` to stop it or decline (`recipients` with only the
//!   account, as Serein does).
//! - Its voice server comes by the channel, and the voice connection goes
//!   by the channel too (`server_id`); a stream's key is
//!   `call:{channel}:{owner}`.
//! - Before calling someone in a direct conversation, the official client
//!   asks `GET /channels/{id}/call` (2026-10-18, llamadas-y-panel-de-voz):
//!   with an answer it joins, and rings only if `ringable` is `true`; when
//!   it is refused it neither joins nor rings, and offers to add them as a
//!   friend. Groups are not asked. Serein does not ask.
//!
//! Fixtures only: none of this has been tried against Discord.

use rune_i18n::es;
use rune_model::{Channel, ChannelId, ChannelKind, PrivateCall, UserId};
use rune_provider::{Command, Event, Snapshot};
use serde_json::{json, Value};

use super::diag::note;
use super::{Calls, MAX_STATES};
use crate::{wire, LoginError};

/// The scope of the private calls' diagnostic lines.
pub(crate) const PRIVATE: &str = "voice private call";
/// Conversations whose call is kept at once, and conversations asked about
/// (op 13) in one Gateway session.
const KEPT: usize = 64;
const ASKED: usize = 256;

/// A direct or group conversation, which has calls of its own.
pub(crate) fn is_private(channel: &Channel) -> bool {
    matches!(channel.kind, ChannelKind::Direct | ChannelKind::Group)
}

/// Whether `command`, a ring or a stop, names a conversation the account is
/// in and only people of it, or a check names one of its direct
/// conversations: checked before it is queued and again when it goes.
pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    if let Command::CheckCall { channel } = command {
        let direct = snapshot
            .channels
            .iter()
            .any(|known| known.id == *channel && known.kind == ChannelKind::Direct);
        return direct
            .then_some(())
            .ok_or_else(|| LoginError::new(es("Esa conversación ya no está en tu lista.")));
    }
    let (Command::RingCall { channel, users } | Command::StopRinging { channel, users }) = command
    else {
        return Err(LoginError::new(es(
            "Esta acción todavía no está disponible.",
        )));
    };
    let Some(conversation) = snapshot
        .channels
        .iter()
        .find(|known| known.id == *channel && is_private(known))
    else {
        return Err(LoginError::new(es(
            "Esa conversación ya no está en tu lista.",
        )));
    };
    let me = snapshot.me.as_ref().map(|me| me.id);
    // Stopping may name the account itself: that declines.
    let stop = matches!(command, Command::StopRinging { .. });
    let fits = users.len() <= PrivateCall::MAX_RINGING
        && users
            .iter()
            .all(|user| conversation.recipients.contains(user) || (stop && Some(*user) == me));
    if fits && !(stop && users.is_empty()) {
        Ok(())
    } else {
        Err(LoginError::new(es(
            "Esa persona no es válida para esta acción.",
        )))
    }
}

/// The path and body of `command`'s request, ringing or stopping.
pub(crate) fn request(command: &Command) -> Option<(String, Value)> {
    let (channel, users, route) = match command {
        Command::RingCall { channel, users } => (channel, users, "ring"),
        Command::StopRinging { channel, users } => (channel, users, "stop-ringing"),
        _ => return None,
    };
    // `null` rings everyone in it but the account, as the official client asks.
    let recipients =
        (!users.is_empty()).then(|| users.iter().map(ToString::to_string).collect::<Vec<_>>());
    Some((
        format!("/channels/{channel}/call/{route}"),
        json!({ "recipients": recipients }),
    ))
}

/// Whom a `CALL_CREATE` or `CALL_UPDATE` says the call rings, of the
/// people `allowed`; `None` when it does not say.
fn ringing(data: &Value, allowed: impl Fn(UserId) -> bool) -> Option<Vec<UserId>> {
    let listed = data["ringing"].as_array();
    // Whom it rings, each to who rang them; a null value rings nobody.
    let ongoing = data["ongoing_rings"].as_object();
    if listed.is_none() && ongoing.is_none() {
        return None;
    }
    let listed = listed
        .into_iter()
        .flatten()
        .filter_map(|id| wire::id(id).ok());
    let ongoing = ongoing
        .into_iter()
        .flatten()
        .filter(|(_, by)| !by.is_null())
        .filter_map(|(id, _)| wire::id(&Value::String(id.clone())).ok());
    let mut found: Vec<UserId> = Vec::new();
    for user in listed
        .chain(ongoing)
        .take(2 * PrivateCall::MAX_RINGING)
        .map(UserId)
    {
        if allowed(user) && !found.contains(&user) && found.len() < PrivateCall::MAX_RINGING {
            found.push(user);
        }
    }
    Some(found)
}

impl Calls {
    /// Ask the service what it knows of the call of the conversation
    /// `channel` (op 13): once a Gateway session when first read, as the
    /// official client does, and before joining it each time, as Serein.
    pub(crate) fn ask_status(&mut self, channel: ChannelId) {
        if self.asked.len() >= ASKED {
            self.asked.clear();
        }
        if !self.asked.contains(&channel.0) {
            self.asked.push(channel.0);
        }
        self.status_request = self.status_request.wrapping_add(1);
        note!(
            PRIVATE,
            "asking the Gateway what it knows of the call (op 13)"
        );
        self.status
            .send_replace(Some((channel.0, self.status_request)));
    }

    /// The conversation `channel` is read: its call is asked about, once.
    pub(crate) fn opened(&mut self, snapshot: &Snapshot, channel: ChannelId) {
        let private = snapshot
            .channels
            .iter()
            .any(|known| known.id == channel && is_private(known));
        if private && !self.asked.contains(&channel.0) {
            self.ask_status(channel);
        }
    }

    /// What the Gateway is to ask of a conversation's call (op 13), and what
    /// changes to ask again.
    pub(crate) fn status_watch(&self) -> tokio::sync::watch::Receiver<Option<(u64, u32)>> {
        self.status.subscribe()
    }

    /// Where `command`, a ring or a stop, goes now: `Ok(Some)` to send it;
    /// `Ok(None)` held until the service has the call, as the official
    /// client queues it; or why not.
    pub(crate) fn ring(&mut self, command: Command) -> Result<Option<Command>, LoginError> {
        let Command::RingCall { channel, .. } = &command else {
            return Ok(Some(command));
        };
        if self
            .call
            .as_ref()
            .is_none_or(|call| call.channel != *channel)
        {
            return Err(LoginError::new(es(
                "Entra a la llamada para hacerla sonar.",
            )));
        }
        if self.known.iter().any(|(known, _)| *known == channel.0) {
            note!(PRIVATE, "ringing (call/ring)");
            return Ok(Some(command));
        }
        note!(
            PRIVATE,
            "ring held until the service has the call (CALL_CREATE)"
        );
        self.ring = Some(command);
        Ok(None)
    }

    /// The ring held, once the service has its call.
    pub(crate) fn take_ring(&mut self) -> Option<Command> {
        let Some(Command::RingCall { channel, .. }) = &self.ring else {
            return None;
        };
        let channel = channel.0;
        if !self.known.iter().any(|(known, _)| *known == channel) {
            return None;
        }
        note!(PRIVATE, "the service has the call: ringing (call/ring)");
        self.ring.take()
    }

    /// A `CALL_CREATE`, `CALL_UPDATE` or `CALL_DELETE`.
    pub(super) fn private_call(
        &mut self,
        snapshot: &mut Snapshot,
        kind: &str,
        data: &Value,
    ) -> Vec<Event> {
        let conversation = wire::id(&data["channel_id"]).ok().and_then(|id| {
            snapshot
                .channels
                .iter()
                .find(|known| known.id.0 == id && is_private(known))
                .cloned()
        });
        let Some(conversation) = conversation else {
            note!(PRIVATE, "{kind} of no conversation in the list: ignored");
            return Vec::new();
        };
        let channel = conversation.id;
        let me = snapshot.me.as_ref().map(|me| me.id);
        if kind == "CALL_DELETE" {
            let unavailable = data["unavailable"].as_bool() == Some(true);
            note!(
                PRIVATE,
                "CALL_DELETE{}",
                if unavailable {
                    ": unavailable (an outage at the service)"
                } else {
                    ""
                }
            );
            self.known.retain(|(known, _)| *known != channel.0);
            if matches!(&self.ring, Some(Command::RingCall { channel: held, .. }) if *held == channel)
            {
                self.ring = None;
            }
            let mut events = vec![Event::PrivateCallEnded { channel }];
            // Whoever was still in it is not any more.
            snapshot.voice.retain(|state| {
                let gone = state.channel == channel;
                if gone {
                    events.push(Event::VoiceLeft {
                        user: state.user,
                        channel,
                    });
                }
                !gone
            });
            // The account's call there ends, the service told (op 4), as the
            // official client leaves it.
            if self
                .call
                .as_ref()
                .is_some_and(|call| call.channel == channel)
            {
                events.extend(self.leave_events(Some(if unavailable {
                    es("La llamada dejó de estar disponible en Discord.")
                } else {
                    es("La llamada terminó.")
                })));
            }
            return events;
        }
        let allowed = |user: UserId| Some(user) == me || conversation.recipients.contains(&user);
        let before = self
            .known
            .iter()
            .position(|(known, _)| *known == channel.0)
            .map(|at| self.known.remove(at).1);
        // Not said: as it was.
        let ringing = ringing(data, allowed).or(before).unwrap_or_default();
        if self.known.len() >= KEPT {
            self.known.remove(0);
        }
        self.known.push((channel.0, ringing.clone()));
        let mut events = vec![Event::PrivateCall(PrivateCall {
            channel,
            ringing: ringing.clone(),
        })];
        // Who is in it, as their own voice states.
        let states = data["voice_states"].as_array();
        let mut present = 0;
        for row in states.into_iter().flatten().take(MAX_STATES) {
            let user = wire::id(&row["user_id"]).ok().map(UserId);
            if !user.is_some_and(allowed) {
                continue;
            }
            let mut row = row.clone();
            row["channel_id"] = json!(channel.to_string());
            present += 1;
            events.extend(self.voice_state(snapshot, &row));
        }
        note!(
            PRIVATE,
            "{kind}: rings {} ({}), {}",
            ringing.len(),
            if me.is_some_and(|me| ringing.contains(&me)) {
                "the account among them: a call coming in"
            } else {
                "not the account"
            },
            match states {
                Some(_) => format!("{present} in it"),
                None => "who is in it not said".into(),
            }
        );
        events
    }
}
