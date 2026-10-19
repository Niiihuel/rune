//! Message actions (P4) by the routes Serein uses (its `discord-api/src/lib.rs`
//! `forward_message`, `create_poll`, `expire_poll` and `reaction_users`, read
//! 2026-10-17) and, for who voted and a conversation read by its id, by
//! Discord's documented routes, which Serein does not call. Writes go through
//! [`Http::request_answer`], so a challenge the service sets on one is
//! offered like any other's. Fixtures only.

use reqwest::Method;
use rune_i18n::es;
use rune_model::{ChannelId, ServerId, UserId};
use rune_provider::{Command, Event, NewPoll, RequestId};
use serde_json::{json, Value};

use super::{mapped_message, Http, RestError};
use crate::admin::Echo;
use crate::message_actions::PEOPLE_PAGE;
use crate::{wire, SessionSecret};

/// The most a message or a channel answer reads.
const ANSWER: usize = 1024 * 1024;
/// The most a page of people reads: a hundred accounts with their profiles.
const PEOPLE: usize = 512 * 1024;

impl Http {
    /// Carry out a message action. `source` is the server of the message a
    /// forward copies, when it has one.
    pub(crate) async fn message_action(
        &self,
        secret: &SessionSecret,
        command: &Command,
        nonce: &str,
        source: Option<ServerId>,
    ) -> Result<Vec<Event>, RestError> {
        match command {
            Command::ForwardMessage {
                channel,
                message,
                to,
                nonce: id,
            } => {
                let mut reference = json!({"type": 1, "channel_id": channel.to_string(),
                    "message_id": message.to_string()});
                if let Some(server) = source {
                    reference["guild_id"] = json!(server.to_string());
                }
                let body = json!({"message_reference": reference, "nonce": nonce,
                    "allowed_mentions": {"parse": [], "replied_user": false}});
                let value = self
                    .request(
                        secret,
                        Method::POST,
                        &format!("/channels/{to}/messages"),
                        Some(body),
                        ANSWER,
                    )
                    .await?;
                mapped_message(&value, *to, None, Some(*id), true)
            }
            Command::CreatePoll {
                channel,
                poll,
                nonce: id,
            } => {
                let body = json!({"poll": poll_body(poll), "nonce": nonce,
                    "allowed_mentions": {"parse": [], "replied_user": false}});
                let value = self
                    .request(
                        secret,
                        Method::POST,
                        &format!("/channels/{channel}/messages"),
                        Some(body),
                        ANSWER,
                    )
                    .await?;
                mapped_message(&value, *channel, None, Some(*id), true)
            }
            Command::EndPoll { channel, message } => {
                let value = self
                    .request_answer(
                        secret,
                        Method::POST,
                        &format!("/channels/{channel}/polls/{message}/expire"),
                        None,
                        ANSWER,
                        true,
                    )
                    .await?;
                // A body is the poll's message, ended; without one the
                // Gateway's update tells.
                if value.is_null() {
                    return Ok(Vec::new());
                }
                mapped_message(&value, *channel, Some(*message), None, true)
            }
            Command::LoadReactions {
                request,
                channel,
                message,
                emoji,
                after,
            } => {
                // The whole emoji is one path segment, every byte escaped.
                let encoded: String = emoji.bytes().map(|byte| format!("%{byte:02X}")).collect();
                let path = paged(
                    format!("/channels/{channel}/messages/{message}/reactions/{encoded}"),
                    *after,
                );
                let value = self.read_people(secret, &path).await?;
                people(*request, &value)
            }
            Command::LoadPollVoters {
                request,
                channel,
                message,
                answer,
                after,
            } => {
                let path = paged(
                    format!("/channels/{channel}/polls/{message}/answers/{answer}"),
                    *after,
                );
                let value = self.read_people(secret, &path).await?;
                people(*request, &value["users"])
            }
            Command::LoadChannel { request, channel } => {
                let value = self
                    .read_people(secret, &format!("/channels/{channel}"))
                    .await?;
                self.load_channel(*request, *channel, &value)
            }
            _ => Err(RestError::rejected(es(
                "Esta acción todavía no está disponible.",
            ))),
        }
    }

    /// A read: its failure is never ambiguous.
    async fn read_people(&self, secret: &SessionSecret, path: &str) -> Result<Value, RestError> {
        self.request(secret, Method::GET, path, None, PEOPLE)
            .await
            .map_err(|error| RestError {
                ambiguous: false,
                ..error
            })
    }

    /// A conversation outside servers is reported at once; one of a server
    /// is kept as its creation's dispatch would, with what the account may
    /// do in it worked out, before [`Event::ChannelLoaded`].
    fn load_channel(
        &self,
        request: RequestId,
        channel: ChannelId,
        value: &Value,
    ) -> Result<Vec<Event>, RestError> {
        let invalid = || RestError::response(wire::invalid(), false);
        if wire::id(&value["id"]).map_err(|_| invalid())? != channel.0 {
            return Err(invalid());
        }
        let loaded = Event::ChannelLoaded { request, channel };
        if value.get("guild_id").is_none_or(Value::is_null) {
            let (found, users) = crate::bootstrap::private_channel(value).map_err(|_| invalid())?;
            let mut events: Vec<Event> = users.into_iter().map(Event::UserChanged).collect();
            events.extend([Event::ChannelChanged(found), loaded]);
            return Ok(events);
        }
        let thread = matches!(value["type"].as_u64(), Some(10..=12));
        self.keep_echoes(vec![Echo::Dispatch(
            if thread {
                "THREAD_CREATE"
            } else {
                "CHANNEL_CREATE"
            },
            value.clone(),
        )]);
        Ok(vec![loaded])
    }
}

/// `path` asking for a page of people after `after`.
fn paged(path: String, after: Option<UserId>) -> String {
    match after {
        Some(after) => format!("{path}?limit={PEOPLE_PAGE}&after={after}"),
        None => format!("{path}?limit={PEOPLE_PAGE}"),
    }
}

/// A page of accounts as RUNE's people, after their accounts.
fn people(request: RequestId, value: &Value) -> Result<Vec<Event>, RestError> {
    let invalid = || RestError::response(wire::invalid(), false);
    let rows = value
        .as_array()
        .filter(|rows| rows.len() <= PEOPLE_PAGE)
        .ok_or_else(invalid)?;
    let mut events = Vec::new();
    let mut people = Vec::new();
    for row in rows {
        let user = wire::user(row).map_err(|_| invalid())?;
        people.push(user.id);
        events.push(Event::UserChanged(user));
    }
    events.push(Event::PeopleLoaded {
        request,
        reached_end: people.len() < PEOPLE_PAGE,
        people,
    });
    Ok(events)
}

/// A poll as Discord takes it: answers with their Unicode emoji, the
/// default layout.
fn poll_body(poll: &NewPoll) -> Value {
    let answers: Vec<Value> = poll
        .answers
        .iter()
        .map(|answer| {
            let mut media = json!({ "text": answer.text.trim() });
            if let Some(emoji) = &answer.emoji {
                media["emoji"] = json!({ "name": emoji });
            }
            json!({ "poll_media": media })
        })
        .collect();
    json!({
        "question": { "text": poll.question.trim() },
        "answers": answers,
        "duration": poll.hours,
        "allow_multiselect": poll.multiselect,
        "layout_type": 1,
    })
}
