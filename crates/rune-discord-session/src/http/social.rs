//! Writes about people and conversations outside servers, by the routes
//! Serein uses (its `discord-api` crate, `user_actions.rs` and
//! `group_actions.rs`, read 2026-10-17) and, for making groups and changing
//! their people or owner, the official web client's private channel actions
//! (`openPrivateChannel`, `addRecipient`, `removeRecipient`, `setDMOwner`,
//! docs/DISCORD.md). Each is one attempt, never repeated. The service confirms
//! what changed: the answer itself names a conversation opened, made,
//! renamed, handed over or left; the Gateway's `RELATIONSHIP_*` and
//! `CHANNEL_RECIPIENT_*` dispatches carry every change to a relationship
//! and to a group's people, so nothing is reported here for those.
//! Fixtures only: none of these routes has been tried against Discord.

use base64::Engine;
use reqwest::Method;
use rune_i18n::es;
use rune_model::{Channel, ChannelId, ChannelKind, Relationship};
use rune_provider::{Command, Created, Event};
use serde_json::{json, Value};

use super::{Http, RestError};
use crate::{wire, SessionSecret};

const ANSWER: usize = 64 * 1024;

impl Http {
    pub(super) async fn social(
        &self,
        secret: &SessionSecret,
        command: &Command,
    ) -> Result<Vec<Event>, RestError> {
        let relationship =
            |user: &dyn std::fmt::Display| format!("/users/@me/relationships/{user}");
        match command {
            Command::SendFriendRequest { username } => {
                let username = username.trim().trim_start_matches('@').to_lowercase();
                if !valid_username(&username) {
                    return Err(RestError::rejected(
                        es("Escribí un nombre de usuario válido: de 2 a 32 letras minúsculas, números, «_» o «.»."),
                    ));
                }
                self.write(
                    secret,
                    Method::POST,
                    "/users/@me/relationships",
                    Some(json!({"username": username, "discriminator": null})),
                )
                .await
            }
            Command::AcceptFriendRequest { user } => {
                self.write(secret, Method::PUT, &relationship(user), Some(json!({})))
                    .await
            }
            Command::RemoveRelationship { user } => {
                self.write(secret, Method::DELETE, &relationship(user), None)
                    .await
            }
            Command::BlockUser { user } => {
                self.write(
                    secret,
                    Method::PUT,
                    &relationship(user),
                    Some(json!({"type": 2})),
                )
                .await
            }
            Command::SetFriendNickname { user, nickname } => {
                let nickname = nickname
                    .as_deref()
                    .map(str::trim)
                    .filter(|name| !name.is_empty());
                if nickname.is_some_and(|name| !valid_nickname(name)) {
                    return Err(RestError::rejected(es(
                        "El apodo puede tener hasta 32 caracteres, sin saltos de línea.",
                    )));
                }
                self.write(
                    secret,
                    Method::PATCH,
                    &relationship(user),
                    Some(json!({ "nickname": nickname })),
                )
                .await
            }
            Command::OpenDirect { request, user } => {
                let value = self
                    .request_answer(
                        secret,
                        Method::POST,
                        "/users/@me/channels",
                        Some(json!({"recipient_id": user.to_string()})),
                        ANSWER,
                        false,
                    )
                    .await?;
                let (channel, mut events) = conversation(&value)?;
                if channel.kind != ChannelKind::Direct || channel.recipients != [*user] {
                    return Err(unclear());
                }
                let id = channel.id;
                events.push(Event::ChannelChanged(channel));
                events.push(Event::Created {
                    request: *request,
                    created: Created::Channel(id),
                });
                Ok(events)
            }
            Command::CreateGroup { request, users } => {
                let recipients: Vec<String> = users.iter().map(ToString::to_string).collect();
                let value = self
                    .request_answer(
                        secret,
                        Method::POST,
                        "/users/@me/channels",
                        Some(json!({ "recipients": recipients })),
                        ANSWER,
                        false,
                    )
                    .await?;
                let (channel, mut events) = conversation(&value)?;
                if channel.kind != ChannelKind::Group
                    || !users.iter().all(|user| channel.recipients.contains(user))
                {
                    return Err(unclear());
                }
                let id = channel.id;
                events.push(Event::ChannelChanged(channel));
                events.push(Event::Created {
                    request: *request,
                    created: Created::Channel(id),
                });
                Ok(events)
            }
            Command::AddGroupMember { channel, user } => {
                self.write(secret, Method::PUT, &recipient(*channel, *user), None)
                    .await
            }
            Command::RemoveGroupMember { channel, user } => {
                self.write(secret, Method::DELETE, &recipient(*channel, *user), None)
                    .await
            }
            Command::SetGroupOwner { channel, user } => {
                let events = self
                    .edit_group(secret, *channel, json!({ "owner": user.to_string() }))
                    .await?;
                // The answer is the group as it is now: handed over, or unclear.
                let owner = events.iter().find_map(|event| match event {
                    Event::ChannelChanged(group) => group.owner,
                    _ => None,
                });
                if owner != Some(*user) {
                    return Err(unclear());
                }
                Ok(events)
            }
            Command::RenameGroup { channel, name } => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(RestError::rejected(es(
                        "Esta conexión no deja un grupo sin nombre: escribí uno.",
                    )));
                }
                if name.chars().count() > 100 || name.chars().any(char::is_control) {
                    return Err(RestError::rejected(es(
                        "El nombre puede tener hasta 100 caracteres, sin saltos de línea.",
                    )));
                }
                self.edit_group(secret, *channel, json!({ "name": name }))
                    .await
            }
            Command::SetGroupIcon { channel, icon } => {
                let icon = icon.as_ref().map(|icon| {
                    format!(
                        "data:image/png;base64,{}",
                        base64::engine::general_purpose::STANDARD.encode(icon.png())
                    )
                });
                self.edit_group(secret, *channel, json!({ "icon": icon }))
                    .await
            }
            Command::LeaveGroup { channel } | Command::CloseDirect { channel } => {
                let value = self
                    .request_answer(
                        secret,
                        Method::DELETE,
                        &format!("/channels/{channel}"),
                        None,
                        ANSWER,
                        true,
                    )
                    .await?;
                // The answer, when there is one, is the conversation left.
                if !value.is_null() && wire::id(&value["id"]).ok() != Some(channel.0) {
                    return Err(unclear());
                }
                Ok(vec![Event::ChannelRemoved { channel: *channel }])
            }
            _ => Err(RestError::rejected(es(
                "Esta acción todavía no está disponible.",
            ))),
        }
    }

    /// A write the Gateway confirms: its answer carries nothing to report.
    async fn write(
        &self,
        secret: &SessionSecret,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Vec<Event>, RestError> {
        self.request_answer(secret, method, path, body, ANSWER, true)
            .await?;
        Ok(Vec::new())
    }

    async fn edit_group(
        &self,
        secret: &SessionSecret,
        channel: ChannelId,
        body: Value,
    ) -> Result<Vec<Event>, RestError> {
        let value = self
            .request_answer(
                secret,
                Method::PATCH,
                &format!("/channels/{channel}"),
                Some(body),
                ANSWER,
                false,
            )
            .await?;
        let (group, mut events) = conversation(&value)?;
        if group.id != channel || group.kind != ChannelKind::Group {
            return Err(unclear());
        }
        events.push(Event::ChannelChanged(group));
        Ok(events)
    }
}

/// Someone in a group, as the official client names them.
fn recipient(channel: ChannelId, user: rune_model::UserId) -> String {
    format!("/channels/{channel}/recipients/{user}")
}

/// The conversation an answer names, and its people as events.
fn conversation(value: &Value) -> Result<(Channel, Vec<Event>), RestError> {
    let (channel, users) = crate::bootstrap::private_channel(value).map_err(|_| unclear())?;
    Ok((channel, users.into_iter().map(Event::UserChanged).collect()))
}

/// The write left but its answer cannot be read: it may have happened.
fn unclear() -> RestError {
    RestError::response(wire::invalid(), true)
}

/// A Discord username, as Serein checks it before sending a request.
fn valid_username(name: &str) -> bool {
    (2..=32).contains(&name.len())
        && !name.contains("..")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.".contains(&byte))
}

fn valid_nickname(name: &str) -> bool {
    name.chars().count() <= Relationship::NICKNAME_MAX && !name.chars().any(char::is_control)
}
