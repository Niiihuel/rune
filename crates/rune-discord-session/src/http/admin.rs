//! Server administration requests, by the routes Serein uses (its
//! `discord-api` crate: `create_guild` in `lib.rs`, `server_settings.rs`,
//! `server_actions.rs`, `server_roles.rs`, `server_admin.rs`,
//! `server_audit_log.rs`, `server_invites.rs`, `server_integrations.rs` and
//! `gif_favorites.rs`, read 2026-10-17) and, where Serein has none (bans,
//! timeouts, the audit log reason), the ones Discord documents. Each write
//! is one attempt, never repeated; a write that first reads what it changes
//! (a role's or a channel's permissions, the webhooks, the GIF favorites)
//! fails definitely while reading. What an answer changes in the session
//! goes back to the provider loop as [`Echo`]es, taken with
//! [`Http::take_echoes`]. Fixtures only: none was tried against Discord.

use rune_i18n::es;
use std::sync::{Arc, Mutex, PoisonError};

use base64::{engine::general_purpose::STANDARD, Engine};
use bytes::Bytes;
use reqwest::{
    header::{HeaderValue, CONTENT_TYPE},
    Method, RequestBuilder,
};
use rune_model::{
    Ban, ChannelId, ChannelKind, EmojiId, ExpressionId, Overwrite, OverwriteTarget, Permissions,
    ServerId, StickerId, Timestamp, User, UserId, Webhook, WebhookKind,
};
use rune_provider::{
    Command, Created, Event, ExpressionImage, ExpressionKind, NewExpression, VoiceModeration,
};
use serde_json::{json, Value};

use super::{Http, RestError};
use crate::admin::{self, rest, Context, Echo, Favorites};
use crate::{bootstrap, wire, SessionSecret};

/// Answers about one thing, and lists of them.
const ANSWER: usize = 256 * 1024;
const LIST: usize = 4 * 1024 * 1024;
/// The template the official client and Serein make an empty server from.
const TEMPLATE: &str = "2TffvPucqHkN";
/// Audit pages read for one answer when a filter empties them.
const AUDIT_SCANS: usize = 5;
/// The GIF favorites' settings, as Serein reads and writes them.
const SETTINGS: &str = "/users/@me/settings-proto/2";

type Answer = Result<(Vec<Event>, Vec<Echo>), RestError>;

/// What a clone of [`Http`] adds to one administration request, and the
/// echoes its answers leave for the provider loop, shared by every clone.
#[derive(Clone, Default)]
pub(crate) struct Extras {
    /// The documented `X-Audit-Log-Reason`, percent-encoded.
    reason: Option<HeaderValue>,
    /// A multipart body and its content type, for a sticker.
    form: Option<(HeaderValue, Bytes)>,
    echoes: Arc<Mutex<Vec<Echo>>>,
}

impl Extras {
    pub(super) fn apply(&self, mut request: RequestBuilder) -> RequestBuilder {
        if let Some(reason) = &self.reason {
            request = request.header("X-Audit-Log-Reason", reason.clone());
        }
        if let Some((kind, body)) = &self.form {
            request = request
                .header(CONTENT_TYPE, kind.clone())
                .body(body.clone());
        }
        request
    }
}

impl Http {
    /// What the last administration answer changes in the session; taken once.
    /// Leave `echoes` for the provider loop, as an administration answer
    /// does (P4: a conversation read by its id).
    pub(crate) fn keep_echoes(&self, echoes: Vec<Echo>) {
        *self
            .admin
            .echoes
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = echoes;
    }

    pub(crate) fn take_echoes(&self) -> Vec<Echo> {
        std::mem::take(
            &mut *self
                .admin
                .echoes
                .lock()
                .unwrap_or_else(PoisonError::into_inner),
        )
    }

    /// Carry out `command`, as the session's `context` has it, and report
    /// what the answer says; its echoes wait for [`Self::take_echoes`].
    pub(crate) async fn admin(
        &self,
        secret: &SessionSecret,
        command: &Command,
        context: Context,
    ) -> Result<Vec<Event>, RestError> {
        self.take_echoes();
        let (events, echoes) = self.administer(secret, command, &context).await?;
        // Kept only once every request is done: an answer cut short leaves none.
        *self
            .admin
            .echoes
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = echoes;
        Ok(events)
    }

    async fn administer(
        &self,
        secret: &SessionSecret,
        command: &Command,
        context: &Context,
    ) -> Answer {
        match command {
            Command::CreateServer { request, name } => {
                let value = self
                    .send(
                        secret,
                        Method::POST,
                        "/guilds",
                        Some(json!({"name": name.trim(), "icon": null, "channels": [],
                            "system_channel_id": null, "guild_template_code": TEMPLATE})),
                        LIST,
                    )
                    .await?;
                let id = ServerId(wire::id(&value["id"]).map_err(|_| unclear())?);
                Ok((
                    vec![Event::Created {
                        request: *request,
                        created: Created::Server(id),
                    }],
                    vec![Echo::Dispatch("GUILD_CREATE", value)],
                ))
            }
            Command::EditServer { server, edit } => {
                let mut body = json!({});
                if let Some(name) = &edit.name {
                    body["name"] = json!(name.trim());
                }
                if let Some(description) = &edit.description {
                    body["description"] = json!(description.as_deref().map_or("", str::trim));
                }
                if let Some(accent) = edit.accent {
                    body["brand_color_primary"] = json!(format!("#{:06x}", accent.0));
                }
                // Server administration, rest (P7): its picture and traits.
                rest::profile_fields(&mut body, edit);
                let mut echoes = Vec::new();
                if body.as_object().is_some_and(|fields| !fields.is_empty()) {
                    let value = self
                        .send(
                            secret,
                            Method::PATCH,
                            &format!("/guilds/{server}/profile"),
                            Some(body),
                            ANSWER,
                        )
                        .await?;
                    echoes.push(Echo::Dispatch(
                        "GUILD_UPDATE",
                        profile_echo(&value, *server)?,
                    ));
                }
                // The guild's own settings; the two writes are not one, as in
                // Serein: a later failure says the first may have happened.
                if rest::touches_guild(edit) {
                    let wrote = !echoes.is_empty();
                    let fresh = if rest::needs_fresh_guild(edit) {
                        Some(
                            self.read(
                                secret,
                                Method::GET,
                                &format!("/guilds/{server}"),
                                None,
                                LIST,
                            )
                            .await
                            .map_err(|error| partial(error, wrote))?,
                        )
                    } else {
                        None
                    };
                    let body = rest::guild_body(edit, fresh.as_ref())
                        .map_err(|reason| partial(definite(reason), wrote))?;
                    let value = self
                        .send(
                            secret,
                            Method::PATCH,
                            &format!("/guilds/{server}"),
                            Some(body),
                            LIST,
                        )
                        .await
                        .map_err(|error| partial(error, wrote))?;
                    if wire::id(&value["id"]).ok() != Some(server.0) {
                        return Err(unclear());
                    }
                    echoes.push(Echo::Dispatch("GUILD_UPDATE", rest::option_keys(&value)));
                }
                Ok((Vec::new(), echoes))
            }
            Command::DeleteServer { server } | Command::LeaveServer { server } => {
                let (path, body) = match command {
                    Command::DeleteServer { .. } => (format!("/guilds/{server}"), None),
                    _ => (
                        format!("/users/@me/guilds/{server}"),
                        Some(json!({"lurking": false})),
                    ),
                };
                self.send(secret, Method::DELETE, &path, body, ANSWER)
                    .await?;
                Ok((
                    Vec::new(),
                    vec![Echo::Dispatch(
                        "GUILD_DELETE",
                        json!({ "id": server.to_string() }),
                    )],
                ))
            }
            Command::CreateChannel {
                request,
                server,
                kind,
                name,
                parent,
                private,
            } => {
                let mut body = json!({"name": name.trim(), "type": channel_type(*kind)});
                if let Some(parent) = parent {
                    body["parent_id"] = json!(parent.to_string());
                }
                if *private {
                    // Hidden from @everyone; the account keeps seeing what it made.
                    let view = admin::discord_bits(Permissions::VIEW).to_string();
                    let mut overwrites = vec![
                        json!({"id": server.to_string(), "type": 0, "allow": "0", "deny": &view}),
                    ];
                    if let Some(me) = context.me {
                        overwrites.push(
                            json!({"id": me.to_string(), "type": 1, "allow": &view, "deny": "0"}),
                        );
                    }
                    body["permission_overwrites"] = json!(overwrites);
                }
                let value = self
                    .send(
                        secret,
                        Method::POST,
                        &format!("/guilds/{server}/channels"),
                        Some(body),
                        ANSWER,
                    )
                    .await?;
                let (id, data) = channel_answer(value, *server, None)?;
                Ok((
                    vec![Event::Created {
                        request: *request,
                        created: Created::Channel(id),
                    }],
                    vec![Echo::Dispatch("CHANNEL_CREATE", data)],
                ))
            }
            Command::EditChannel { channel, edit } => {
                let server = context.server.ok_or_else(gone)?;
                let mut body = json!({});
                if let Some(name) = &edit.name {
                    body["name"] = json!(name.trim());
                }
                if let Some(topic) = &edit.topic {
                    body["topic"] = json!(topic);
                }
                if let Some(parent) = edit.parent {
                    body["parent_id"] = json!(parent.map(|parent| parent.to_string()));
                }
                if let Some(position) = edit.position {
                    body["position"] = json!(position);
                }
                if let Some(secs) = edit.slowmode_secs {
                    body["rate_limit_per_user"] = json!(secs);
                }
                if let Some(nsfw) = edit.nsfw {
                    body["nsfw"] = json!(nsfw);
                }
                // Server administration, rest (P7): a forum's tags and
                // defaults; its flags are written whole, so read fresh.
                let flags = match edit.forum {
                    Some(_) => {
                        let fresh = self
                            .read(
                                secret,
                                Method::GET,
                                &format!("/channels/{channel}"),
                                None,
                                ANSWER,
                            )
                            .await?;
                        Some(fresh["flags"].as_u64().unwrap_or(0))
                    }
                    None => None,
                };
                rest::forum_fields(&mut body, edit, flags);
                let value = self
                    .send(
                        secret,
                        Method::PATCH,
                        &format!("/channels/{channel}"),
                        Some(body),
                        ANSWER,
                    )
                    .await?;
                let (_, data) = channel_answer(value, server, Some(*channel))?;
                Ok((Vec::new(), vec![Echo::Dispatch("CHANNEL_UPDATE", data)]))
            }
            Command::DeleteChannel { channel } => {
                let value = self
                    .send(
                        secret,
                        Method::DELETE,
                        &format!("/channels/{channel}"),
                        None,
                        ANSWER,
                    )
                    .await?;
                // The answer, when there is one, is the channel deleted.
                if !value.is_null() && wire::id(&value["id"]).ok() != Some(channel.0) {
                    return Err(unclear());
                }
                Ok((
                    Vec::new(),
                    vec![Echo::Dispatch(
                        "CHANNEL_DELETE",
                        json!({ "id": channel.to_string() }),
                    )],
                ))
            }
            Command::SetOverwrite { channel, overwrite } => {
                self.overwrite(secret, context, *channel, overwrite.target, Some(overwrite))
                    .await
            }
            Command::RemoveOverwrite { channel, target } => {
                self.overwrite(secret, context, *channel, *target, None)
                    .await
            }
            Command::CreateRole {
                request,
                server,
                name,
            } => {
                // Nothing but a name: no permission until one is given.
                let value = self
                    .send(
                        secret,
                        Method::POST,
                        &format!("/guilds/{server}/roles"),
                        Some(json!({"name": name.trim(), "permissions": "0"})),
                        ANSWER,
                    )
                    .await?;
                let role = bootstrap::role(&value).map_err(|_| unclear())?;
                Ok((
                    vec![Event::Created {
                        request: *request,
                        created: Created::Role(*server, role.id),
                    }],
                    vec![Echo::Dispatch(
                        "GUILD_ROLE_CREATE",
                        json!({"guild_id": server.to_string(), "role": value}),
                    )],
                ))
            }
            Command::EditRole { server, role, edit } => {
                let mut body = json!({});
                if let Some(name) = &edit.name {
                    body["name"] = json!(name.trim());
                }
                if let Some(color) = edit.color {
                    body["color"] = json!(color.map_or(0, |color| color.0));
                }
                if let Some(hoisted) = edit.hoisted {
                    body["hoist"] = json!(hoisted);
                }
                if let Some(mentionable) = edit.mentionable {
                    body["mentionable"] = json!(mentionable);
                }
                // Server administration, rest (P7): its picture, emoji and gradient.
                rest::role_fields(
                    &mut body,
                    edit,
                    context.role.as_ref().and_then(|known| known.color),
                );
                if let Some(permissions) = edit.permissions {
                    // The bits RUNE does not model stay as Discord has them now.
                    let roles = self
                        .read(
                            secret,
                            Method::GET,
                            &format!("/guilds/{server}/roles"),
                            None,
                            LIST,
                        )
                        .await?;
                    let found = roles
                        .as_array()
                        .ok_or_else(unreadable)?
                        .iter()
                        .find(|found| wire::id(&found["id"]).ok() == Some(role.0))
                        .ok_or_else(|| RestError::rejected(es("Ese rol ya no existe.")))?;
                    let raw = admin::raw_bits(&found["permissions"]).ok_or_else(unreadable)?;
                    body["permissions"] = json!(admin::merged_bits(raw, permissions).to_string());
                }
                let mut echoes = Vec::new();
                if body.as_object().is_some_and(|fields| !fields.is_empty()) {
                    let value = self
                        .send(
                            secret,
                            Method::PATCH,
                            &format!("/guilds/{server}/roles/{role}"),
                            Some(body),
                            ANSWER,
                        )
                        .await?;
                    if bootstrap::role(&value).map_err(|_| unclear())?.id != *role {
                        return Err(unclear());
                    }
                    echoes.push(Echo::Dispatch(
                        "GUILD_ROLE_UPDATE",
                        json!({"guild_id": server.to_string(), "role": value}),
                    ));
                }
                if let Some(position) = edit.position {
                    // Its own route; the answer is every role, in order.
                    let value = self
                        .send(
                            secret,
                            Method::PATCH,
                            &format!("/guilds/{server}/roles"),
                            Some(json!([{"id": role.to_string(), "position": position}])),
                            LIST,
                        )
                        .await
                        .map_err(|error| partial(error, !echoes.is_empty()))?;
                    if !value.is_array() {
                        return Err(unclear());
                    }
                    echoes.push(Echo::Dispatch(
                        "GUILD_UPDATE",
                        json!({"id": server.to_string(), "roles": value}),
                    ));
                }
                Ok((Vec::new(), echoes))
            }
            Command::DeleteRole { server, role } => {
                self.send(
                    secret,
                    Method::DELETE,
                    &format!("/guilds/{server}/roles/{role}"),
                    None,
                    ANSWER,
                )
                .await?;
                Ok((
                    Vec::new(),
                    vec![Echo::Dispatch(
                        "GUILD_ROLE_DELETE",
                        json!({"guild_id": server.to_string(), "role_id": role.to_string()}),
                    )],
                ))
            }
            Command::SetMemberRoles {
                server,
                user,
                roles,
            } => {
                let member = context.member.clone().ok_or_else(|| {
                    RestError::rejected(es(
                        "Cargá a esa persona en la lista de miembros antes de cambiar sus roles.",
                    ))
                })?;
                let changes: Vec<_> = roles
                    .iter()
                    .filter(|role| !member.roles.contains(role))
                    .map(|role| (*role, true))
                    .chain(
                        member
                            .roles
                            .iter()
                            .filter(|role| !roles.contains(role))
                            .map(|role| (*role, false)),
                    )
                    .collect();
                if changes.is_empty() {
                    return Ok((vec![Event::MemberChanged(member)], Vec::new()));
                }
                // One request per role, as Serein gives and takes them; the
                // first refusal stops the rest.
                for (index, (role, add)) in changes.into_iter().enumerate() {
                    self.send(
                        secret,
                        if add { Method::PUT } else { Method::DELETE },
                        &format!("/guilds/{server}/members/{user}/roles/{role}"),
                        None,
                        ANSWER,
                    )
                    .await
                    .map_err(|error| partial(error, index > 0))?;
                }
                // Read back once, as Serein confirms it.
                let value = self
                    .read(
                        secret,
                        Method::GET,
                        &format!("/guilds/{server}/members/{user}"),
                        None,
                        ANSWER,
                    )
                    .await
                    .map_err(|error| partial(error, true))?;
                Ok((Vec::new(), vec![member_echo(value, *server, *user)?]))
            }
            Command::SetNick { server, user, nick } => {
                // The account renames itself by Discord's documented `@me`.
                let target = if Some(*user) == context.me {
                    "@me".to_owned()
                } else {
                    user.to_string()
                };
                let nick = nick
                    .as_deref()
                    .map(str::trim)
                    .filter(|nick| !nick.is_empty());
                let value = self
                    .send(
                        secret,
                        Method::PATCH,
                        &format!("/guilds/{server}/members/{target}"),
                        Some(json!({ "nick": nick })),
                        ANSWER,
                    )
                    .await?;
                member_answer(value, *server, *user)
            }
            Command::TimeoutMember {
                server,
                user,
                until,
                reason,
            } => {
                let until = until.map(iso).transpose()?;
                let value = self
                    .reasoned(reason.as_deref())
                    .send(
                        secret,
                        Method::PATCH,
                        &format!("/guilds/{server}/members/{user}"),
                        Some(json!({ "communication_disabled_until": until })),
                        ANSWER,
                    )
                    .await?;
                member_answer(value, *server, *user)
            }
            // Voice moderation (P4), as Serein's `server_admin.rs` sends it;
            // server deafening by Discord's documented `deaf`, which Serein
            // has not. The Gateway's voice update tells what changed.
            Command::ModerateVoice {
                server,
                user,
                change,
            } => {
                let body = match change {
                    VoiceModeration::Mute(on) => json!({ "mute": on }),
                    VoiceModeration::Deafen(on) => json!({ "deaf": on }),
                    VoiceModeration::Move(to) => json!({ "channel_id": to.to_string() }),
                    VoiceModeration::Disconnect => json!({ "channel_id": null }),
                };
                let value = self
                    .send(
                        secret,
                        Method::PATCH,
                        &format!("/guilds/{server}/members/{user}"),
                        Some(body),
                        ANSWER,
                    )
                    .await?;
                member_answer(value, *server, *user)
            }
            Command::Kick {
                server,
                user,
                reason,
            } => {
                self.reasoned(reason.as_deref())
                    .send(
                        secret,
                        Method::DELETE,
                        &format!("/guilds/{server}/members/{user}"),
                        None,
                        ANSWER,
                    )
                    .await?;
                Ok((Vec::new(), vec![removed(*server, *user)]))
            }
            Command::Ban {
                server,
                user,
                reason,
            } => {
                self.reasoned(reason.as_deref())
                    .send(
                        secret,
                        Method::PUT,
                        &format!("/guilds/{server}/bans/{user}"),
                        Some(json!({"delete_message_seconds": 0})),
                        ANSWER,
                    )
                    .await?;
                let reason = reason
                    .as_deref()
                    .map(str::trim)
                    .filter(|reason| !reason.is_empty())
                    .map(str::to_owned);
                Ok((
                    vec![Event::BanAdded {
                        server: *server,
                        ban: Ban {
                            user: *user,
                            reason,
                        },
                    }],
                    vec![removed(*server, *user)],
                ))
            }
            Command::Unban { server, user } => {
                self.send(
                    secret,
                    Method::DELETE,
                    &format!("/guilds/{server}/bans/{user}"),
                    None,
                    ANSWER,
                )
                .await?;
                Ok((
                    vec![Event::BanRemoved {
                        server: *server,
                        user: *user,
                    }],
                    Vec::new(),
                ))
            }
            Command::LoadBans {
                request,
                server,
                after,
                limit,
            } => {
                let limit = (*limit).clamp(1, admin::MAX_ROWS);
                let mut path = format!("/guilds/{server}/bans?limit={limit}");
                if let Some(after) = after {
                    path.push_str(&format!("&after={after}"));
                }
                let value = self.read(secret, Method::GET, &path, None, LIST).await?;
                let (bans, users) = admin::bans(&value, *after, limit).map_err(definite)?;
                let mut events: Vec<Event> = users.into_iter().map(Event::UserChanged).collect();
                events.push(Event::BansLoaded {
                    request: *request,
                    server: *server,
                    after: *after,
                    reached_end: bans.len() < limit,
                    bans,
                });
                Ok((events, Vec::new()))
            }
            Command::LoadInvites { request, server } => {
                let value = self
                    .read(
                        secret,
                        Method::GET,
                        &format!("/guilds/{server}/invites"),
                        None,
                        LIST,
                    )
                    .await?;
                let mut events = Vec::new();
                let mut invites = Vec::new();
                for row in rows(&value)? {
                    let (invite, inviter) = admin::invite(row).map_err(definite)?;
                    events.extend(inviter.map(Event::UserChanged));
                    invites.push(invite);
                }
                events.push(Event::InvitesLoaded {
                    request: *request,
                    server: *server,
                    invites,
                });
                Ok((events, Vec::new()))
            }
            Command::RevokeInvite { server, code } => {
                if !admin::valid_code(code) {
                    return Err(RestError::rejected(es("Esa invitación no es válida.")));
                }
                let value = self
                    .send(
                        secret,
                        Method::DELETE,
                        &format!("/invites/{code}"),
                        None,
                        ANSWER,
                    )
                    .await?;
                if !value.is_null() && value["code"].as_str() != Some(code) {
                    return Err(unclear());
                }
                Ok((
                    vec![Event::InviteRemoved {
                        server: *server,
                        code: code.clone(),
                    }],
                    Vec::new(),
                ))
            }
            Command::CreateInvite {
                request,
                server,
                channel,
                max_age_secs,
                max_uses,
                temporary,
            } => {
                let value = self
                    .send(
                        secret,
                        Method::POST,
                        &format!("/channels/{channel}/invites"),
                        Some(json!({"max_age": max_age_secs, "max_uses": max_uses,
                            "temporary": temporary, "unique": true})),
                        ANSWER,
                    )
                    .await?;
                let (invite, inviter) = admin::invite(&value).map_err(|_| unclear())?;
                if invite.channel.is_some_and(|answered| answered != *channel) {
                    return Err(unclear());
                }
                let mut events: Vec<Event> = inviter.map(Event::UserChanged).into_iter().collect();
                events.push(Event::InviteCreated {
                    request: *request,
                    server: *server,
                    invite,
                });
                Ok((events, Vec::new()))
            }
            Command::RenameExpression {
                server,
                expression,
                name,
            } => {
                let (path, id) = expression_path(*server, *expression);
                let value = self
                    .send(
                        secret,
                        Method::PATCH,
                        &path,
                        Some(json!({ "name": name.trim() })),
                        ANSWER,
                    )
                    .await?;
                if wire::id(&value["id"]).ok() != Some(id) {
                    return Err(unclear());
                }
                Ok((
                    Vec::new(),
                    vec![Echo::Expression {
                        server: *server,
                        expression: *expression,
                        answer: Some(value),
                    }],
                ))
            }
            Command::DeleteExpression { server, expression } => {
                let (path, _) = expression_path(*server, *expression);
                self.send(secret, Method::DELETE, &path, None, ANSWER)
                    .await?;
                Ok((
                    Vec::new(),
                    vec![Echo::Expression {
                        server: *server,
                        expression: *expression,
                        answer: None,
                    }],
                ))
            }
            Command::CreateExpression {
                request,
                server,
                expression,
            } => {
                let value = match expression.kind {
                    ExpressionKind::Emoji => {
                        self.send(
                            secret,
                            Method::POST,
                            &format!("/guilds/{server}/emojis"),
                            Some(json!({"name": expression.name.trim(),
                                "image": data_uri(&expression.image)})),
                            ANSWER,
                        )
                        .await?
                    }
                    // Documented as a multipart form only.
                    ExpressionKind::Sticker => {
                        self.with_form(sticker_form(expression)?)
                            .send(
                                secret,
                                Method::POST,
                                &format!("/guilds/{server}/stickers"),
                                None,
                                ANSWER,
                            )
                            .await?
                    }
                };
                let id = wire::id(&value["id"]).map_err(|_| unclear())?;
                let made = match expression.kind {
                    ExpressionKind::Emoji => ExpressionId::Emoji(EmojiId(id)),
                    ExpressionKind::Sticker => ExpressionId::Sticker(StickerId(id)),
                };
                Ok((
                    vec![Event::Created {
                        request: *request,
                        created: Created::Expression(*server, made),
                    }],
                    vec![Echo::Expression {
                        server: *server,
                        expression: made,
                        answer: Some(value),
                    }],
                ))
            }
            Command::LoadAudit {
                request,
                server,
                query,
                before,
                limit,
            } => {
                let limit = (*limit).clamp(1, 100);
                let mut cursor = *before;
                let mut events = Vec::new();
                // One type at a time, as the route takes them; none, unfiltered.
                let types = query.action.map(admin::action_types).unwrap_or_default();
                let kinds: Vec<Option<u64>> = if types.is_empty() {
                    vec![None]
                } else {
                    types.iter().copied().map(Some).collect()
                };
                for _ in 0..AUDIT_SCANS {
                    let mut pages = Vec::with_capacity(kinds.len());
                    for kind in &kinds {
                        let mut path = format!("/guilds/{server}/audit-logs?limit={limit}");
                        if let Some(actor) = query.actor {
                            path.push_str(&format!("&user_id={actor}"));
                        }
                        if let Some(kind) = kind {
                            path.push_str(&format!("&action_type={kind}"));
                        }
                        if let Some(cursor) = cursor {
                            path.push_str(&format!("&before={cursor}"));
                        }
                        let value = self.read(secret, Method::GET, &path, None, LIST).await?;
                        let mut page =
                            admin::audit_page(&value, cursor, limit).map_err(definite)?;
                        events.extend(page.users.drain(..).map(Event::UserChanged));
                        pages.push(page);
                    }
                    let (entries, next) = admin::audit_merge(pages, limit);
                    let more = next.is_some();
                    // Discord's types are broader than RUNE's kinds (24 holds
                    // a nickname and a timeout): what the query names stays.
                    let entries: Vec<_> = entries
                        .into_iter()
                        .filter(|entry| query.matches(entry))
                        .collect();
                    // A page the filter emptied while more remain is not one.
                    if !entries.is_empty() || !more {
                        events.push(Event::AuditLoaded {
                            request: *request,
                            server: *server,
                            query: *query,
                            before: *before,
                            entries,
                            reached_end: !more,
                        });
                        return Ok((events, Vec::new()));
                    }
                    cursor = next;
                }
                Err(RestError::rejected(es(
                    "Muchas entradas seguidas no coinciden con el filtro; probá con otro.",
                )))
            }
            Command::LoadMembers {
                request,
                server,
                after,
                query,
            } => {
                let mut body = json!({"limit": admin::MEMBERS_PAGE});
                // Server administration, rest (P7): its order and filter.
                rest::member_query(&mut body, *query, crate::messages::wall_now().0);
                if let Some(cursor) = after {
                    let (user, joined) = admin::parse_cursor(cursor).ok_or_else(|| {
                        RestError::rejected(es("El cursor de la página no es válido."))
                    })?;
                    body["after"] = json!({"user_id": user.to_string(), "guild_joined_at": joined});
                }
                // A search, not a write: Serein reads members this way.
                let value = self
                    .read(
                        secret,
                        Method::POST,
                        &format!("/guilds/{server}/members-search"),
                        Some(body),
                        LIST,
                    )
                    .await?;
                let page = admin::member_page(&value, *server).map_err(definite)?;
                let mut events: Vec<Event> =
                    page.users.into_iter().map(Event::UserChanged).collect();
                events.push(Event::MembersLoaded {
                    request: *request,
                    server: *server,
                    after: after.clone(),
                    members: page.members,
                    next: page.next,
                    total: page.total,
                    query: *query,
                    joins: page.joins,
                });
                let echoes = page
                    .updates
                    .into_iter()
                    .map(|data| Echo::Quiet("GUILD_MEMBER_UPDATE", data))
                    .collect();
                Ok((events, echoes))
            }
            Command::PruneMembers {
                request,
                server,
                days,
                remove,
            } => {
                let value = if *remove {
                    self.send(
                        secret,
                        Method::POST,
                        &format!("/guilds/{server}/prune"),
                        Some(json!({"days": days, "compute_prune_count": false})),
                        ANSWER,
                    )
                    .await?
                } else {
                    self.read(
                        secret,
                        Method::GET,
                        &format!("/guilds/{server}/prune?days={days}"),
                        None,
                        ANSWER,
                    )
                    .await?
                };
                // Removing does not count them: the Gateway reports who left.
                let count = match &value["pruned"] {
                    Value::Null if *remove => None,
                    pruned => Some(
                        pruned
                            .as_u64()
                            .and_then(|count| u32::try_from(count).ok())
                            .ok_or_else(|| if *remove { unclear() } else { unreadable() })?,
                    ),
                };
                Ok((
                    vec![Event::Pruned {
                        request: *request,
                        server: *server,
                        days: *days,
                        count,
                        removed: *remove,
                    }],
                    Vec::new(),
                ))
            }
            Command::LoadWebhooks { request, server } => {
                let (webhooks, users) = self.webhooks(secret, *server).await?;
                let mut events: Vec<Event> = users.into_iter().map(Event::UserChanged).collect();
                events.push(Event::WebhooksLoaded {
                    request: *request,
                    server: *server,
                    webhooks,
                });
                Ok((events, Vec::new()))
            }
            Command::CreateWebhook {
                server,
                channel,
                name,
            } => {
                let value = self
                    .send(
                        secret,
                        Method::POST,
                        &format!("/channels/{channel}/webhooks"),
                        Some(json!({ "name": name.trim() })),
                        ANSWER,
                    )
                    .await?;
                webhook_answer(&value, *server, None, *channel)
            }
            Command::EditWebhook {
                server,
                webhook,
                name,
                channel,
            } => {
                // `/webhooks/{id}` is not the server's: it must be one of its.
                let (webhooks, _) = self.webhooks(secret, *server).await?;
                match webhooks.iter().find(|known| known.id == *webhook) {
                    None => Err(RestError::rejected(es("Ese webhook ya no existe."))),
                    Some(known) if known.kind != WebhookKind::Incoming => Err(RestError::rejected(
                        es("Solo se cambian los webhooks creados en el servidor."),
                    )),
                    Some(known) => {
                        let mut body = json!({});
                        if let Some(name) = name {
                            body["name"] = json!(name.trim());
                        }
                        if let Some(channel) = channel {
                            body["channel_id"] = json!(channel.to_string());
                        }
                        let value = self
                            .send(
                                secret,
                                Method::PATCH,
                                &format!("/webhooks/{webhook}"),
                                Some(body),
                                ANSWER,
                            )
                            .await?;
                        webhook_answer(
                            &value,
                            *server,
                            Some(*webhook),
                            channel.unwrap_or(known.channel),
                        )
                    }
                }
            }
            Command::DeleteWebhook { server, webhook } => {
                let (webhooks, _) = self.webhooks(secret, *server).await?;
                // Not among the server's any more: nothing to delete.
                if webhooks.iter().any(|known| known.id == *webhook) {
                    self.send(
                        secret,
                        Method::DELETE,
                        &format!("/webhooks/{webhook}"),
                        None,
                        ANSWER,
                    )
                    .await?;
                }
                Ok((
                    vec![Event::WebhookRemoved {
                        server: *server,
                        webhook: *webhook,
                    }],
                    Vec::new(),
                ))
            }
            Command::LoadIntegrations { request, server } => {
                let value = self
                    .read(
                        secret,
                        Method::GET,
                        &format!("/guilds/{server}/integrations"),
                        None,
                        LIST,
                    )
                    .await?;
                let mut events = Vec::new();
                let mut integrations = Vec::new();
                for row in rows(&value)? {
                    let (integration, users) = admin::integration(row).map_err(definite)?;
                    events.extend(users.into_iter().map(Event::UserChanged));
                    integrations.push(integration);
                }
                events.push(Event::IntegrationsLoaded {
                    request: *request,
                    server: *server,
                    integrations,
                });
                Ok((events, Vec::new()))
            }
            Command::DeleteIntegration {
                server,
                integration,
            } => {
                self.send(
                    secret,
                    Method::DELETE,
                    &format!("/guilds/{server}/integrations/{integration}"),
                    None,
                    ANSWER,
                )
                .await?;
                Ok((
                    vec![Event::IntegrationRemoved {
                        server: *server,
                        integration: *integration,
                    }],
                    Vec::new(),
                ))
            }
            Command::LoadGifFavorites => {
                let current = self.favorites(secret).await?;
                Ok((vec![Event::GifFavorites(current.list())], Vec::new()))
            }
            Command::SetGifFavorite { gif, favorite } => {
                // Changed against a fresh read, as Serein does: the other
                // device's favorites stay.
                let current = self.favorites(secret).await?;
                if current.contains(&gif.url) == *favorite {
                    return Ok((vec![Event::GifFavorites(current.list())], Vec::new()));
                }
                let patch = current.patch(gif, *favorite).map_err(definite)?;
                let value = self
                    .send(
                        secret,
                        Method::PATCH,
                        SETTINGS,
                        Some(json!({"settings": patch, "required_data_version": current.version})),
                        admin::favorites::MAX_RESPONSE,
                    )
                    .await?;
                // Too late (`out_of_date`) is a refusal: nothing was saved.
                let saved = Favorites::decode(&value).map_err(|reason| {
                    if value["out_of_date"].as_bool() == Some(true) {
                        definite(reason)
                    } else {
                        unclear()
                    }
                })?;
                if saved.contains(&gif.url) != *favorite {
                    return Err(unclear());
                }
                Ok((vec![Event::GifFavorites(saved.list())], Vec::new()))
            }
            // Server administration, rest (P7).
            Command::LoadServerProfile { server, .. } => {
                let value = self
                    .read(
                        secret,
                        Method::GET,
                        &format!("/guilds/{server}/profile"),
                        None,
                        ANSWER,
                    )
                    .await?;
                let update = profile_echo(&value, *server).map_err(|_| unreadable())?;
                Ok((Vec::new(), vec![Echo::Dispatch("GUILD_UPDATE", update)]))
            }
            Command::ReorderChannels { server, moves } => {
                let value = self
                    .send(
                        secret,
                        Method::PATCH,
                        &format!("/guilds/{server}/channels"),
                        Some(rest::moves_body(moves)),
                        ANSWER,
                    )
                    .await?;
                // Answered without a body; the Gateway tells each move again.
                if !value.is_null() && !value.is_array() {
                    return Err(unclear());
                }
                Ok((
                    Vec::new(),
                    moves
                        .iter()
                        .map(|step| Echo::Dispatch("CHANNEL_UPDATE", rest::moved(*server, step)))
                        .collect(),
                ))
            }
            Command::LoadRoleCounts { server, .. } => {
                let value = self
                    .read(
                        secret,
                        Method::GET,
                        &format!("/guilds/{server}/roles/member-counts"),
                        None,
                        ANSWER,
                    )
                    .await?;
                let counts = rest::role_counts(&value).map_err(definite)?;
                Ok((
                    vec![Event::RoleCounts {
                        server: *server,
                        counts,
                    }],
                    Vec::new(),
                ))
            }
            _ => Err(RestError::rejected(es(
                "Esta acción todavía no está disponible.",
            ))),
        }
    }

    /// A write: one attempt; a success without a body answers `null`.
    async fn send(
        &self,
        secret: &SessionSecret,
        method: Method,
        path: &str,
        body: Option<Value>,
        max: usize,
    ) -> Result<Value, RestError> {
        self.request_answer(secret, method, path, body, max, true)
            .await
    }

    /// A read, a GET or a search: its failure is never ambiguous.
    async fn read(
        &self,
        secret: &SessionSecret,
        method: Method,
        path: &str,
        body: Option<Value>,
        max: usize,
    ) -> Result<Value, RestError> {
        self.request_answer(secret, method, path, body, max, true)
            .await
            .map_err(|error| RestError {
                ambiguous: false,
                ..error
            })
    }

    /// This client, telling the audit log `reason` with its requests.
    fn reasoned(&self, reason: Option<&str>) -> Http {
        let mut http = self.clone();
        http.admin.reason = reason
            .map(str::trim)
            .filter(|reason| !reason.is_empty())
            .and_then(|reason| HeaderValue::from_str(&percent(reason)).ok());
        http
    }

    /// This client, sending `form` as its next request's body.
    fn with_form(&self, form: (HeaderValue, Bytes)) -> Http {
        let mut http = self.clone();
        http.admin.form = Some(form);
        http
    }

    /// Change one overwrite of `channel` (`None` takes it out): Serein
    /// writes the whole list, read fresh so nobody else's change is lost,
    /// and keeps the bits RUNE does not model as they are.
    async fn overwrite(
        &self,
        secret: &SessionSecret,
        context: &Context,
        channel: ChannelId,
        target: OverwriteTarget,
        set: Option<&Overwrite>,
    ) -> Answer {
        let server = context.server.ok_or_else(gone)?;
        let path = format!("/channels/{channel}");
        let fresh = self.read(secret, Method::GET, &path, None, ANSWER).await?;
        let (_, fresh) = channel_answer(fresh, server, Some(channel)).map_err(|_| unreadable())?;
        let rows = fresh["permission_overwrites"]
            .as_array()
            .filter(|rows| rows.len() <= 1000)
            .ok_or_else(unreadable)?;
        let (id, kind) = match target {
            OverwriteTarget::Role(role) => (role.0, 0),
            OverwriteTarget::Member(user) => (user.0, 1),
        };
        let mut list = Vec::with_capacity(rows.len() + 1);
        let mut found = false;
        for row in rows {
            if wire::id(&row["id"]).ok() != Some(id) || row["type"].as_u64() != Some(kind) {
                list.push(json!({"id": row["id"], "type": row["type"],
                    "allow": row["allow"], "deny": row["deny"]}));
                continue;
            }
            found = true;
            if let Some(set) = set {
                let raw = |key: &str| admin::raw_bits(&row[key]).ok_or_else(unreadable);
                list.push(json!({"id": id.to_string(), "type": kind,
                    "allow": admin::merged_bits(raw("allow")?, set.allow).to_string(),
                    "deny": admin::merged_bits(raw("deny")?, set.deny).to_string()}));
            }
        }
        match (found, set) {
            (false, Some(set)) => list.push(json!({"id": id.to_string(), "type": kind,
                "allow": admin::discord_bits(set.allow).to_string(),
                "deny": admin::discord_bits(set.deny).to_string()})),
            // Already gone: nothing to write; the channel as it is now.
            (false, None) => {
                return Ok((Vec::new(), vec![Echo::Dispatch("CHANNEL_UPDATE", fresh)]));
            }
            _ => {}
        }
        let value = self
            .send(
                secret,
                Method::PATCH,
                &path,
                Some(json!({ "permission_overwrites": list })),
                ANSWER,
            )
            .await?;
        let (_, data) = channel_answer(value, server, Some(channel))?;
        Ok((Vec::new(), vec![Echo::Dispatch("CHANNEL_UPDATE", data)]))
    }

    /// Every webhook of `server` and who made them; their tokens are dropped.
    async fn webhooks(
        &self,
        secret: &SessionSecret,
        server: ServerId,
    ) -> Result<(Vec<Webhook>, Vec<User>), RestError> {
        let value = self
            .read(
                secret,
                Method::GET,
                &format!("/guilds/{server}/webhooks"),
                None,
                LIST,
            )
            .await?;
        let mut webhooks = Vec::new();
        let mut users = Vec::new();
        for row in rows(&value)? {
            let (webhook, creator) = admin::webhook(row, server).map_err(definite)?;
            users.extend(creator);
            webhooks.push(webhook);
        }
        Ok((webhooks, users))
    }

    async fn favorites(&self, secret: &SessionSecret) -> Result<Favorites, RestError> {
        let value = self
            .read(
                secret,
                Method::GET,
                SETTINGS,
                None,
                admin::favorites::MAX_RESPONSE,
            )
            .await?;
        Favorites::decode(&value).map_err(definite)
    }
}

/// What a profile answer keeps in the session, as `GUILD_UPDATE` carries
/// it: its picture's hash as the guild's icon, and its traits (P7).
fn profile_echo(value: &Value, server: ServerId) -> Result<Value, RestError> {
    if wire::id(&value["id"]).ok() != Some(server.0) {
        return Err(unclear());
    }
    let mut update = json!({ "id": server.to_string() });
    for key in ["name", "description", "brand_color_primary", "traits"] {
        if let Some(saved) = value.get(key) {
            // An empty description is none.
            update[key] = if saved == "" {
                Value::Null
            } else {
                saved.clone()
            };
        }
    }
    if let Some(icon) = value.get("icon_hash") {
        update["icon"] = icon.clone();
    }
    Ok(update)
}

/// The write left but its answer cannot be read: it may have happened.
fn unclear() -> RestError {
    RestError::response(wire::invalid(), true)
}

/// A read whose answer cannot be read: nothing changed.
fn unreadable() -> RestError {
    RestError::response(wire::invalid(), false)
}

fn definite(reason: crate::LoginError) -> RestError {
    RestError::response(reason, false)
}

fn gone() -> RestError {
    RestError::rejected(es("Ese servidor o canal ya no está disponible."))
}

/// A failure after an earlier write of the same command succeeded: the
/// command may have happened in part.
fn partial(error: RestError, wrote: bool) -> RestError {
    RestError {
        ambiguous: error.ambiguous || wrote,
        ..error
    }
}

fn rows(value: &Value) -> Result<&[Value], RestError> {
    value
        .as_array()
        .filter(|rows| rows.len() <= admin::MAX_ROWS)
        .map(Vec::as_slice)
        .ok_or_else(unreadable)
}

/// Discord's channel type for a kind RUNE creates.
fn channel_type(kind: ChannelKind) -> u8 {
    match kind {
        ChannelKind::Voice => 2,
        ChannelKind::Category => 4,
        ChannelKind::Announcement => 5,
        ChannelKind::Stage => 13,
        ChannelKind::Forum => 15,
        _ => 0,
    }
}

/// A channel answer of `server` (and `expected`, when named), as the
/// Gateway's dispatch carries it.
fn channel_answer(
    mut value: Value,
    server: ServerId,
    expected: Option<ChannelId>,
) -> Result<(ChannelId, Value), RestError> {
    let id = ChannelId(wire::id(&value["id"]).map_err(|_| unclear())?);
    let foreign = value
        .get("guild_id")
        .filter(|guild| !guild.is_null())
        .is_some_and(|guild| wire::id(guild).ok() != Some(server.0));
    if foreign || expected.is_some_and(|expected| expected != id) {
        return Err(unclear());
    }
    value["guild_id"] = json!(server.to_string());
    Ok((id, value))
}

/// A member answer as the Gateway's `GUILD_MEMBER_UPDATE`.
fn member_echo(mut value: Value, server: ServerId, user: UserId) -> Result<Echo, RestError> {
    if wire::id(&value["user"]["id"]).ok() != Some(user.0) || !value["roles"].is_array() {
        return Err(unclear());
    }
    value["guild_id"] = json!(server.to_string());
    Ok(Echo::Dispatch("GUILD_MEMBER_UPDATE", value))
}

/// A change to a member; without an answer the Gateway reports it.
fn member_answer(value: Value, server: ServerId, user: UserId) -> Answer {
    if value.is_null() {
        return Ok((Vec::new(), Vec::new()));
    }
    Ok((Vec::new(), vec![member_echo(value, server, user)?]))
}

fn removed(server: ServerId, user: UserId) -> Echo {
    Echo::Dispatch(
        "GUILD_MEMBER_REMOVE",
        json!({"guild_id": server.to_string(), "user": {"id": user.to_string()}}),
    )
}

/// A webhook answer: `webhook` (when named) of `server`, made here, posting in `channel`.
fn webhook_answer(
    value: &Value,
    server: ServerId,
    webhook: Option<rune_model::WebhookId>,
    channel: ChannelId,
) -> Answer {
    let (saved, creator) = admin::webhook(value, server).map_err(|_| unclear())?;
    if webhook.is_some_and(|webhook| webhook != saved.id)
        || saved.channel != channel
        || saved.kind != WebhookKind::Incoming
    {
        return Err(unclear());
    }
    let mut events: Vec<Event> = creator.map(Event::UserChanged).into_iter().collect();
    events.push(Event::WebhookChanged {
        server,
        webhook: saved,
    });
    Ok((events, Vec::new()))
}

fn expression_path(server: ServerId, expression: ExpressionId) -> (String, u64) {
    match expression {
        ExpressionId::Emoji(id) => (format!("/guilds/{server}/emojis/{id}"), id.0),
        ExpressionId::Sticker(id) => (format!("/guilds/{server}/stickers/{id}"), id.0),
    }
}

/// When a timeout ends, as Discord reads it.
fn iso(until: Timestamp) -> Result<String, RestError> {
    chrono::DateTime::from_timestamp_millis(until.0)
        .map(|until| until.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .ok_or_else(|| RestError::rejected(es("El aislamiento dura hasta 28 días.")))
}

/// Everything but unreserved characters percent-encoded, as Discord reads
/// `X-Audit-Log-Reason`.
fn percent(text: &str) -> String {
    text.bytes()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                (byte as char).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

fn data_uri(image: &ExpressionImage) -> String {
    format!(
        "data:image/{};base64,{}",
        if image.is_gif() { "gif" } else { "png" },
        STANDARD.encode(image.bytes())
    )
}

/// A sticker as Discord takes it: a multipart form with its name,
/// description, tags and file, under a boundary found in no part.
fn sticker_form(expression: &NewExpression) -> Result<(HeaderValue, Bytes), RestError> {
    let (filename, kind) = if expression.image.is_gif() {
        ("sticker.gif", "image/gif")
    } else {
        ("sticker.png", "image/png")
    };
    let texts = [
        ("name", expression.name.trim()),
        ("description", expression.description.trim()),
        ("tags", expression.tags.trim()),
    ];
    let file = expression.image.bytes();
    let boundary = (0u32..64)
        .map(|attempt| format!("rune-sticker-{attempt:08x}"))
        .find(|boundary| {
            !file
                .windows(boundary.len())
                .any(|window| window == boundary.as_bytes())
                && texts
                    .iter()
                    .all(|(_, text)| !text.contains(boundary.as_str()))
        })
        .ok_or_else(|| RestError::rejected(es("No se pudo preparar el sticker.")))?;
    let mut body = Vec::with_capacity(file.len() + 1024);
    for (name, value) in texts {
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                .as_bytes(),
        );
    }
    body.extend_from_slice(
        format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: {kind}\r\n\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(file);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    let header = HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}"))
        .map_err(|_| RestError::rejected(es("No se pudo preparar el sticker.")))?;
    Ok((header, Bytes::from(body)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reason_travels_percent_encoded() {
        assert_eq!(percent("spam y más"), "spam%20y%20m%C3%A1s");
        assert_eq!(percent("a-b_c.d~e"), "a-b_c.d~e");
        assert!(HeaderValue::from_str(&percent("línea\r\nX-Injected: 1")).is_ok());
    }

    #[test]
    fn a_sticker_form_names_its_parts_under_a_free_boundary() {
        let mut gif = b"GIF89a".to_vec();
        gif.extend([0x40, 0x01, 0x40, 0x01]);
        // The first boundary appears in the file: the next one is used.
        gif.extend(b"rune-sticker-00000000");
        let expression = NewExpression {
            kind: ExpressionKind::Sticker,
            name: "Cangrejo".into(),
            image: ExpressionImage::gif(ExpressionKind::Sticker, Bytes::from(gif)).unwrap(),
            tags: "🦀".into(),
            description: String::new(),
        };
        let (header, body) = sticker_form(&expression).unwrap();
        assert_eq!(
            header.to_str().unwrap(),
            "multipart/form-data; boundary=rune-sticker-00000001"
        );
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("name=\"name\"\r\n\r\nCangrejo\r\n"));
        assert!(text.contains("filename=\"sticker.gif\"\r\nContent-Type: image/gif"));
        assert!(text.ends_with("\r\n--rune-sticker-00000001--\r\n"));
    }

    #[test]
    fn a_timeout_ends_at_an_rfc3339_instant() {
        assert_eq!(
            iso(Timestamp(1_760_000_000_000)).unwrap(),
            "2025-10-09T08:53:20.000Z"
        );
    }
}
