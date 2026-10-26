//! The simulation's administration beyond roles and members: a made-up
//! audit log, bans, invitations and custom expressions for the server the
//! account owns, and the commands that page through and change them. The
//! made-up part draws from its own random stream, so the rest of the world
//! is the same with or without it.

use rune_i18n::{t, tr};
use rune_model::{
    valid_emoji_name, valid_sticker_name, valid_webhook_name, AuditAction, AuditEntry, AuditQuery,
    Ban, ChannelId, ChannelKind, CustomEmoji, EmojiId, ExpressionId, Integration, IntegrationId,
    Invite, Permissions, RoleId, ServerId, Status, Sticker, StickerId, Timestamp, UserId, Webhook,
    WebhookId, WebhookKind,
};

use super::{mix, World, ME};
use crate::{Created, Event, ExpressionKind, NewExpression, RequestId};

type Outcome = Result<Vec<Event>, String>;

/// Mixed into the administration's stream, apart from every other one.
const SALT: u64 = 0x4144_4D49;
/// The server the account owns, where the made-up administration lives.
const OWNED: ServerId = ServerId(2);
/// Made-up audit entries: enough for more than one page.
const AUDIT_ENTRIES: u64 = 72;
/// Ids of made-up entries, below those of anything made during a session.
const AUDIT_FIRST_ID: u64 = 500_000;
/// The most a page may hold, as services cap them.
const PAGE_MAX: usize = 100;
const EMOJIS: [&str; 6] = ["ferris", "limn_logo", "gpu", "pixel", "compila", "bug"];
const STICKERS: [&str; 3] = ["Cangrejo feliz", "Compila a la primera", "Render listo"];
const BAN_REASONS: [Option<&str>; 4] = [Some("spam"), None, Some("enlaces maliciosos"), None];
const NICKS: [&str; 4] = ["Mantenedora", "Rustacean", "Pixel", "Lab"];

impl World {
    /// Add the owned server's emojis, stickers, invitations, bans and a
    /// history of changes. Call once its members and channels exist.
    pub(super) fn add_admin(&mut self) {
        let mut rng = mix(&[self.seed, SALT]);
        let members: Vec<UserId> = self
            .members
            .iter()
            .filter(|member| member.server == OWNED)
            .map(|member| member.user)
            .collect();
        let outsiders: Vec<UserId> = self
            .users
            .iter()
            .map(|user| user.id)
            .filter(|user| *user != ME && !members.contains(user) && !self.is_bot(*user))
            .collect();
        let channels: Vec<_> = self
            .channels
            .iter()
            .filter(|channel| channel.server == Some(OWNED) && channel.kind == ChannelKind::Text)
            .map(|channel| (channel.id, channel.name.clone()))
            .collect();
        let Some(server) = self.servers.iter_mut().find(|server| server.id == OWNED) else {
            return;
        };
        let moderation = RoleId(OWNED.0 * 10 + 1);
        let collaboration = RoleId(OWNED.0 * 10 + 2);
        server.emojis = EMOJIS
            .iter()
            .enumerate()
            .map(|(index, name)| CustomEmoji {
                id: EmojiId(9_000 + index as u64),
                server: OWNED,
                name: (*name).into(),
                insert: format!(":{name}:"),
                animated: index == 3,
                available: true,
                roles: if *name == "bug" {
                    vec![moderation]
                } else {
                    Vec::new()
                },
                image: None,
            })
            .collect();
        server.stickers = STICKERS
            .iter()
            .enumerate()
            .map(|(index, name)| Sticker {
                id: StickerId(9_100 + index as u64),
                name: (*name).into(),
                image: None,
                available: true,
            })
            .collect();
        let pick =
            |rng: &mut super::Rng, from: &[UserId]| from[rng.below(from.len() as u64) as usize];
        const CODE: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        let limits = [None, Some(10), Some(50), None, Some(1)];
        let hours = [None, Some(24 * 7), Some(24), Some(12), None];
        let invites = limits
            .iter()
            .zip(hours)
            .enumerate()
            .map(|(index, (max_uses, hours))| {
                let code: String = (0..8)
                    .map(|_| CODE[rng.below(CODE.len() as u64) as usize] as char)
                    .collect();
                let uses = rng.below(max_uses.unwrap_or(40) as u64) as u32;
                Invite {
                    code,
                    channel: channels.get(index % channels.len().max(1)).map(|c| c.0),
                    inviter: Some(if index == 0 {
                        ME
                    } else {
                        pick(&mut rng, &members)
                    }),
                    uses: Some(uses),
                    max_uses: *max_uses,
                    expires: hours.map(|hours| self.history_end.plus_minutes(hours * 60)),
                    temporary: index == 2,
                }
            })
            .collect();
        self.invites.insert(OWNED, invites);
        self.bans.insert(
            OWNED,
            outsiders
                .iter()
                .enumerate()
                .map(|(index, user)| Ban {
                    user: *user,
                    reason: BAN_REASONS[index % BAN_REASONS.len()].map(str::to_owned),
                })
                .collect(),
        );
        // Who changed things: the account and whoever moderates.
        let mut actors: Vec<UserId> = self
            .members
            .iter()
            .filter(|member| member.server == OWNED && member.roles.contains(&moderation))
            .map(|member| member.user)
            .collect();
        actors.push(ME);
        let mut when = self
            .history_end
            .plus_minutes(-60 * 3 * AUDIT_ENTRIES as i64);
        let log = self.audit.entry(OWNED).or_default();
        for index in 0..AUDIT_ENTRIES {
            when = when.plus_minutes(rng.range(30, 300));
            let (channel, channel_name) =
                channels[rng.below(channels.len() as u64) as usize].clone();
            let member = pick(&mut rng, &members);
            let action = match index % 9 {
                0 => AuditAction::RoleUpdated {
                    role: collaboration,
                    name: "Colaboración".into(),
                },
                1 => AuditAction::ChannelUpdated {
                    channel,
                    name: channel_name,
                },
                2 => AuditAction::NickChanged {
                    user: member,
                    nick: Some(NICKS[rng.below(NICKS.len() as u64) as usize].into()),
                },
                3 => AuditAction::PermissionsChanged {
                    channel,
                    name: channel_name,
                },
                4 => AuditAction::MemberRolesChanged {
                    user: member,
                    added: vec![collaboration],
                    removed: Vec::new(),
                },
                5 if !outsiders.is_empty() => AuditAction::MemberBanned {
                    user: pick(&mut rng, &outsiders),
                },
                6 => AuditAction::EmojiUpdated {
                    name: EMOJIS[rng.below(EMOJIS.len() as u64) as usize].into(),
                },
                7 => AuditAction::ServerUpdated,
                _ => AuditAction::StickerUpdated {
                    name: STICKERS[rng.below(STICKERS.len() as u64) as usize].into(),
                },
            };
            log.push(AuditEntry {
                id: AUDIT_FIRST_ID + index,
                actor: pick(&mut rng, &actors),
                reason: matches!(action, AuditAction::MemberBanned { .. })
                    .then(|| "spam".to_owned()),
                action,
                when,
            });
        }
    }

    pub(super) fn is_bot(&self, user: UserId) -> bool {
        self.users.iter().any(|known| known.id == user && known.bot)
    }

    /// Entries matching `query`, newest first, older than `before`.
    pub(super) fn load_audit(
        &mut self,
        request: RequestId,
        server: ServerId,
        query: AuditQuery,
        before: Option<u64>,
        limit: usize,
    ) -> Outcome {
        self.require(server, Permissions::VIEW_AUDIT_LOG)?;
        let limit = limit.clamp(1, PAGE_MAX);
        let mut matching = self
            .audit
            .get(&server)
            .into_iter()
            .flatten()
            .rev()
            .filter(|entry| before.is_none_or(|before| entry.id < before) && query.matches(entry));
        let entries: Vec<AuditEntry> = matching.by_ref().take(limit).cloned().collect();
        Ok(vec![Event::AuditLoaded {
            request,
            server,
            query,
            before,
            entries,
            reached_end: matching.next().is_none(),
        }])
    }

    /// Bans by ascending user id, after `after`.
    pub(super) fn load_bans(
        &mut self,
        request: RequestId,
        server: ServerId,
        after: Option<UserId>,
        limit: usize,
    ) -> Outcome {
        self.require(server, Permissions::BAN)?;
        let limit = limit.clamp(1, PAGE_MAX);
        let mut bans: Vec<Ban> = self.bans.get(&server).cloned().unwrap_or_default();
        bans.sort_by_key(|ban| ban.user);
        bans.retain(|ban| after.is_none_or(|after| ban.user > after));
        let reached_end = bans.len() <= limit;
        bans.truncate(limit);
        Ok(vec![Event::BansLoaded {
            request,
            server,
            after,
            bans,
            reached_end,
        }])
    }

    pub(super) fn load_invites(&mut self, request: RequestId, server: ServerId) -> Outcome {
        self.require(server, Permissions::MANAGE_SERVER)?;
        Ok(vec![Event::InvitesLoaded {
            request,
            server,
            invites: self.invites.get(&server).cloned().unwrap_or_default(),
        }])
    }

    pub(super) fn revoke_invite(&mut self, server: ServerId, code: String) -> Outcome {
        self.require(server, Permissions::MANAGE_SERVER)?;
        let invites = self.invites.entry(server).or_default();
        let before = invites.len();
        invites.retain(|invite| invite.code != code);
        if invites.len() == before {
            return Err(t("esa invitación ya no existe").into());
        }
        Ok(vec![Event::InviteRemoved { server, code }])
    }

    /// The name of `expression` in `server`, if it exists.
    pub(super) fn expression_name(
        &self,
        server: ServerId,
        expression: ExpressionId,
    ) -> Option<String> {
        let found = self.server(server).ok()?;
        match expression {
            ExpressionId::Emoji(id) => found
                .emojis
                .iter()
                .find(|e| e.id == id)
                .map(|e| e.name.clone()),
            ExpressionId::Sticker(id) => found
                .stickers
                .iter()
                .find(|s| s.id == id)
                .map(|s| s.name.clone()),
        }
    }

    /// Rename `expression` (`Some`) or delete it (`None`).
    pub(super) fn change_expression(
        &mut self,
        server: ServerId,
        expression: ExpressionId,
        name: Option<String>,
    ) -> Outcome {
        self.require(server, Permissions::MANAGE_EMOJI)?;
        let name = name.map(|name| name.trim().to_owned());
        let found = self.server_mut(server)?;
        match expression {
            ExpressionId::Emoji(id) => {
                let index = found
                    .emojis
                    .iter()
                    .position(|emoji| emoji.id == id)
                    .ok_or(t("ese emoji ya no existe"))?;
                match name {
                    Some(name) if !valid_emoji_name(&name) => return Err(t(
                        "el nombre del emoji necesita de 2 a 32 letras, números o guiones bajos",
                    )
                    .into()),
                    Some(name) => {
                        let emoji = &mut found.emojis[index];
                        emoji.insert = format!(":{name}:");
                        emoji.name = name;
                    }
                    None => {
                        found.emojis.remove(index);
                    }
                }
            }
            ExpressionId::Sticker(id) => {
                let index = found
                    .stickers
                    .iter()
                    .position(|sticker| sticker.id == id)
                    .ok_or(t("ese sticker ya no existe"))?;
                match name {
                    Some(name) if !valid_sticker_name(&name) => {
                        return Err(t("el nombre del sticker necesita de 2 a 30 caracteres").into())
                    }
                    Some(name) => found.stickers[index].name = name,
                    None => {
                        found.stickers.remove(index);
                    }
                }
            }
        }
        Ok(vec![Event::ServerChanged(found.clone())])
    }
}

/// Webhooks made up for the owned server: a name, whether it follows
/// another server's announcements, and the channel it posts in (by index).
const WEBHOOKS: [(&str, WebhookKind); 2] = [
    ("Avisos de CI", WebhookKind::Incoming),
    ("Novedades de Rust", WebhookKind::Follower),
];
/// The longest timeout services allow.
const TIMEOUT_MAX_DAYS: i64 = 28;
/// A page of members for the administration.
const MEMBERS_PAGE: usize = 25;
/// Custom expressions a server may hold, as a server without boosts has.
const EMOJI_MAX: usize = 50;
const STICKER_MAX: usize = 5;

// Administration (19): timeouts, new invitations and expressions, member
// pages, pruning, webhooks and integrations.
impl World {
    /// Add the owned server's made-up webhooks and integrations. Call after
    /// [`World::add_admin`].
    pub(super) fn add_integrations(&mut self) {
        let channels: Vec<ChannelId> = self
            .channels
            .iter()
            .filter(|channel| channel.server == Some(OWNED) && channel.kind == ChannelKind::Text)
            .map(|channel| channel.id)
            .collect();
        let Some(&first) = channels.first() else {
            return;
        };
        let webhooks = WEBHOOKS
            .iter()
            .enumerate()
            .map(|(index, (name, kind))| Webhook {
                id: WebhookId(9_200 + index as u64),
                channel: channels.get(index).copied().unwrap_or(first),
                name: (*name).into(),
                kind: *kind,
                creator: Some(ME),
                avatar: None,
            })
            .collect();
        self.webhooks.insert(OWNED, webhooks);
        let bot = self
            .members
            .iter()
            .map(|member| member.user)
            .find(|user| self.is_bot(*user));
        let integrations = vec![
            Integration {
                id: IntegrationId(9_300),
                name: bot
                    .and_then(|bot| self.users.iter().find(|user| user.id == bot))
                    .map_or_else(|| "RuneBot".into(), |user| user.shown_name().to_owned()),
                service: t("Aplicación").into(),
                enabled: true,
                user: Some(ME),
                bot,
            },
            Integration {
                id: IntegrationId(9_301),
                name: "Canal de videos del taller".into(),
                service: "YouTube".into(),
                enabled: true,
                user: Some(ME),
                bot: None,
            },
        ];
        self.integrations.insert(OWNED, integrations);
    }

    /// Time `user` out until `until`, or lift it.
    pub(super) fn timeout(
        &mut self,
        server: ServerId,
        user: UserId,
        until: Option<Timestamp>,
    ) -> Outcome {
        self.require(server, Permissions::TIMEOUT_MEMBERS)?;
        self.require_over(server, user)?;
        if let Some(until) = until {
            if until <= self.now || until > self.now.plus_minutes(TIMEOUT_MAX_DAYS * 24 * 60) {
                return Err(t("el aislamiento dura de un minuto a 28 días").into());
            }
        }
        let member = self
            .members
            .iter_mut()
            .find(|member| member.server == server && member.user == user)
            .ok_or(t("esa persona no es miembro"))?;
        member.restrictions.timeout_until = until;
        Ok(vec![Event::MemberChanged(member.clone())])
    }

    /// Whom the account may act on in `server`: not itself, not who owns
    /// it, nobody ranked as high.
    pub(super) fn require_over(&self, server: ServerId, user: UserId) -> Result<(), String> {
        if user == ME {
            return Err(t("no puedes hacerte eso a ti").into());
        }
        let found = self.server(server)?;
        if found.owner == Some(user) {
            return Err(t("nadie puede hacerle eso a quien es dueño").into());
        }
        let theirs = self
            .member(server, user)
            .map(|member| member.roles.clone())
            .unwrap_or_default();
        let rank = found
            .roles
            .iter()
            .filter(|role| theirs.contains(&role.id))
            .map(|role| role.position)
            .max()
            .unwrap_or(0);
        if rank >= self.my_rank(server) {
            return Err(t("su rol está a la altura del tuyo o por encima").into());
        }
        Ok(())
    }

    /// A new invitation through `channel`.
    pub(super) fn create_invite(
        &mut self,
        request: RequestId,
        server: ServerId,
        channel: ChannelId,
        max_age_secs: u32,
        max_uses: u32,
        temporary: bool,
    ) -> Outcome {
        let found = self
            .channel(channel)
            .filter(|found| found.server == Some(server) && found.kind != ChannelKind::Category)
            .ok_or(t("ese canal no es de este servidor"))?;
        if !found.permissions.contains(Permissions::CREATE_INVITE) {
            return Err(t("no tienes permiso para invitar en ese canal").into());
        }
        if max_age_secs > 7 * 24 * 60 * 60 || max_uses > 100 {
            return Err(t("una invitación dura hasta 7 días y hasta 100 usos").into());
        }
        const CODE: &[u8] = b"abcdefghijkmnopqrstuvwxyzABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        // Its own stream per invitation: the same session makes the same codes.
        let mut rng = mix(&[self.seed, SALT, self.fresh_id()]);
        let code: String = (0..8)
            .map(|_| CODE[rng.below(CODE.len() as u64) as usize] as char)
            .collect();
        let invite = Invite {
            code: code.clone(),
            channel: Some(channel),
            inviter: Some(ME),
            uses: Some(0),
            max_uses: (max_uses > 0).then_some(max_uses),
            expires: (max_age_secs > 0)
                .then(|| Timestamp(self.now.0 + i64::from(max_age_secs) * 1000)),
            temporary,
        };
        self.invites
            .entry(server)
            .or_default()
            .insert(0, invite.clone());
        self.log_created(server, AuditAction::InviteCreated { code });
        Ok(vec![Event::InviteCreated {
            request,
            server,
            invite,
        }])
    }

    /// Add a custom emoji or sticker; its picture is named, not hosted.
    pub(super) fn create_expression(
        &mut self,
        request: RequestId,
        server: ServerId,
        expression: NewExpression,
    ) -> Outcome {
        self.require(server, Permissions::MANAGE_EMOJI)?;
        if let Some(problem) = expression.problem() {
            return Err(problem.to_lowercase().trim_end_matches('.').to_owned());
        }
        let id = self.fresh_id();
        let name = expression.name.trim().to_owned();
        let found = self.server_mut(server)?;
        let (made, action) = match expression.kind {
            ExpressionKind::Emoji => {
                if found.emojis.len() >= EMOJI_MAX {
                    return Err(tr!("el servidor ya tiene {max} emojis", max = EMOJI_MAX));
                }
                found.emojis.push(CustomEmoji {
                    id: EmojiId(id),
                    server,
                    name: name.clone(),
                    insert: format!(":{name}:"),
                    animated: expression.image.is_gif(),
                    available: true,
                    roles: Vec::new(),
                    image: None,
                });
                (
                    ExpressionId::Emoji(EmojiId(id)),
                    AuditAction::EmojiCreated { name },
                )
            }
            ExpressionKind::Sticker => {
                if found.stickers.len() >= STICKER_MAX {
                    return Err(tr!(
                        "el servidor ya tiene {max} stickers",
                        max = STICKER_MAX
                    ));
                }
                found.stickers.push(Sticker {
                    id: StickerId(id),
                    name: name.clone(),
                    image: None,
                    available: true,
                });
                (
                    ExpressionId::Sticker(StickerId(id)),
                    AuditAction::StickerCreated { name },
                )
            }
        };
        let changed = Event::ServerChanged(found.clone());
        self.log_created(server, action);
        Ok(vec![
            changed,
            Event::Created {
                request,
                created: Created::Expression(server, made),
            },
        ])
    }

    /// Members by ascending id after the one `after` names, a page at a time.
    pub(super) fn load_members(
        &mut self,
        request: RequestId,
        server: ServerId,
        after: Option<String>,
        query: rune_model::MemberQuery,
    ) -> Outcome {
        let mine = self.mine(server)?;
        if !mine.intersects(
            Permissions::KICK
                | Permissions::BAN
                | Permissions::TIMEOUT_MEMBERS
                | Permissions::MANAGE_ROLES
                | Permissions::MANAGE_NICKS
                | Permissions::MANAGE_SERVER,
        ) {
            return Err(t("no tienes permiso para ver los miembros").into());
        }
        let after = match after {
            Some(cursor) => Some(UserId(
                cursor.parse().map_err(|_| t("esa página no existe"))?,
            )),
            None => None,
        };
        // In the order asked for (P7): see `mock::server_admin`.
        let mut members = self.queried_members(server, query);
        let total = members.len() as u32;
        if let Some(after) = after {
            let at = members
                .iter()
                .position(|(member, _)| member.user == after)
                .ok_or(t("esa página no existe"))?;
            members.drain(..=at);
        }
        let more = members.len() > MEMBERS_PAGE;
        members.truncate(MEMBERS_PAGE);
        let next = more
            .then(|| members.last().map(|(member, _)| member.user.to_string()))
            .flatten();
        let (members, joins) = members.into_iter().unzip();
        Ok(vec![Event::MembersLoaded {
            request,
            server,
            after: after.map(|user| user.to_string()),
            members,
            next,
            total: Some(total),
            query,
            joins,
        }])
    }

    /// Members without a role who are offline: those a prune of `days`
    /// takes; with `remove`, take them.
    pub(super) fn prune(
        &mut self,
        request: RequestId,
        server: ServerId,
        days: u32,
        remove: bool,
    ) -> Outcome {
        self.require(server, Permissions::KICK | Permissions::MANAGE_SERVER)?;
        if !(1..=30).contains(&days) {
            return Err(t("se poda por 1 a 30 días sin actividad").into());
        }
        let owner = self.server(server)?.owner;
        let pruned: Vec<UserId> = self
            .members
            .iter()
            .filter(|member| {
                member.server == server
                    && member.roles.is_empty()
                    && member.user != ME
                    && Some(member.user) != owner
                    && self
                        .presences
                        .get(&member.user)
                        .is_none_or(|presence| presence.status == Status::Offline)
            })
            .map(|member| member.user)
            .collect();
        let count = Some(pruned.len() as u32);
        let mut events = Vec::new();
        if remove {
            self.members
                .retain(|member| !(member.server == server && pruned.contains(&member.user)));
            events.extend(pruned.iter().map(|user| Event::MemberRemoved {
                server,
                user: *user,
            }));
            self.log_created(
                server,
                AuditAction::Other {
                    what: tr!("podó {count} miembros inactivos", count = pruned.len()),
                },
            );
        }
        events.push(Event::Pruned {
            request,
            server,
            days,
            count,
            removed: remove,
        });
        Ok(events)
    }

    pub(super) fn load_webhooks(&mut self, request: RequestId, server: ServerId) -> Outcome {
        self.require(server, Permissions::MANAGE_WEBHOOKS)?;
        Ok(vec![Event::WebhooksLoaded {
            request,
            server,
            webhooks: self.webhooks.get(&server).cloned().unwrap_or_default(),
        }])
    }

    /// A channel of `server` a webhook can post in.
    fn webhook_channel(&self, server: ServerId, channel: ChannelId) -> Result<(), String> {
        self.channel(channel)
            .filter(|found| {
                found.server == Some(server)
                    && matches!(
                        found.kind,
                        ChannelKind::Text | ChannelKind::Announcement | ChannelKind::Forum
                    )
            })
            .map(|_| ())
            .ok_or_else(|| t("un webhook publica en un canal de texto del servidor").into())
    }

    pub(super) fn create_webhook(
        &mut self,
        server: ServerId,
        channel: ChannelId,
        name: String,
    ) -> Outcome {
        self.require(server, Permissions::MANAGE_WEBHOOKS)?;
        self.webhook_channel(server, channel)?;
        let name = name.trim().to_owned();
        if !valid_webhook_name(&name) {
            return Err(
                t("el nombre necesita de 1 a 80 caracteres, sin «discord» ni «clyde»").into(),
            );
        }
        let webhook = Webhook {
            id: WebhookId(self.fresh_id()),
            channel,
            name: name.clone(),
            kind: WebhookKind::Incoming,
            creator: Some(ME),
            avatar: None,
        };
        self.webhooks
            .entry(server)
            .or_default()
            .push(webhook.clone());
        self.log_created(
            server,
            AuditAction::Other {
                what: tr!("creó el webhook {name}", name),
            },
        );
        Ok(vec![Event::WebhookChanged { server, webhook }])
    }

    pub(super) fn edit_webhook(
        &mut self,
        server: ServerId,
        webhook: WebhookId,
        name: Option<String>,
        channel: Option<ChannelId>,
    ) -> Outcome {
        self.require(server, Permissions::MANAGE_WEBHOOKS)?;
        if let Some(channel) = channel {
            self.webhook_channel(server, channel)?;
        }
        let name = name.map(|name| name.trim().to_owned());
        if name
            .as_deref()
            .is_some_and(|name| !valid_webhook_name(name))
        {
            return Err(
                t("el nombre necesita de 1 a 80 caracteres, sin «discord» ni «clyde»").into(),
            );
        }
        let found = self
            .webhooks
            .entry(server)
            .or_default()
            .iter_mut()
            .find(|known| known.id == webhook)
            .ok_or(t("ese webhook ya no existe"))?;
        if found.kind != WebhookKind::Incoming {
            return Err(t("ese webhook es de otro servidor o de una aplicación").into());
        }
        if let Some(name) = name {
            found.name = name;
        }
        if let Some(channel) = channel {
            found.channel = channel;
        }
        let webhook = found.clone();
        self.log_created(
            server,
            AuditAction::Other {
                what: tr!("actualizó el webhook {name}", name = webhook.name),
            },
        );
        Ok(vec![Event::WebhookChanged { server, webhook }])
    }

    pub(super) fn delete_webhook(&mut self, server: ServerId, webhook: WebhookId) -> Outcome {
        self.require(server, Permissions::MANAGE_WEBHOOKS)?;
        let list = self.webhooks.entry(server).or_default();
        let index = list
            .iter()
            .position(|known| known.id == webhook)
            .ok_or(t("ese webhook ya no existe"))?;
        let gone = list.remove(index);
        self.log_created(
            server,
            AuditAction::Other {
                what: tr!("eliminó el webhook {name}", name = gone.name),
            },
        );
        Ok(vec![Event::WebhookRemoved { server, webhook }])
    }

    pub(super) fn load_integrations(&mut self, request: RequestId, server: ServerId) -> Outcome {
        self.require(server, Permissions::MANAGE_SERVER)?;
        Ok(vec![Event::IntegrationsLoaded {
            request,
            server,
            integrations: self.integrations.get(&server).cloned().unwrap_or_default(),
        }])
    }

    /// Remove an integration, and its bot from the server.
    pub(super) fn delete_integration(
        &mut self,
        server: ServerId,
        integration: IntegrationId,
    ) -> Outcome {
        self.require(server, Permissions::MANAGE_SERVER)?;
        let list = self.integrations.entry(server).or_default();
        let index = list
            .iter()
            .position(|known| known.id == integration)
            .ok_or(t("esa integración ya no existe"))?;
        let gone = list.remove(index);
        let mut events = Vec::new();
        if let Some(bot) = gone.bot {
            let before = self.members.len();
            self.members
                .retain(|member| !(member.server == server && member.user == bot));
            if self.members.len() != before {
                events.push(Event::MemberRemoved { server, user: bot });
            }
        }
        self.log_created(
            server,
            AuditAction::Other {
                what: tr!("quitó la integración {name}", name = gone.name),
            },
        );
        events.push(Event::IntegrationRemoved {
            server,
            integration,
        });
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockConfig;
    use crate::{Command, Failure};
    use rune_model::AuditActionKind;

    fn world() -> World {
        World::build(&MockConfig::default())
    }

    fn audit_page(world: &mut World, query: AuditQuery, before: Option<u64>) -> (Vec<u64>, bool) {
        let events = world.manage(
            Command::LoadAudit {
                request: 1,
                server: OWNED,
                query,
                before,
                limit: 30,
            },
            Timestamp(1),
        );
        let Some(Event::AuditLoaded {
            entries,
            reached_end,
            ..
        }) = events.first()
        else {
            panic!("{events:?}");
        };
        (entries.iter().map(|entry| entry.id).collect(), *reached_end)
    }

    #[test]
    fn the_made_up_administration_is_the_same_for_the_same_seed() {
        let config = MockConfig {
            history_end: Timestamp(1_700_000_000_000),
            ..MockConfig::default()
        };
        let (a, b) = (World::build(&config), World::build(&config));
        assert_eq!(a.invites, b.invites);
        assert_eq!(a.audit, b.audit);
        assert_eq!(a.bans, b.bans);
        let owned = a.server(OWNED).unwrap();
        assert_eq!(owned.emojis.len(), EMOJIS.len());
        assert_eq!(owned.stickers.len(), STICKERS.len());
        assert!(!a.bans[&OWNED].is_empty(), "someone to unban");
    }

    #[test]
    fn audit_pages_are_newest_first_and_continue_without_repeating() {
        let mut world = world();
        let (first, end) = audit_page(&mut world, AuditQuery::default(), None);
        assert_eq!(first.len(), 30);
        assert!(!end);
        assert!(first.windows(2).all(|pair| pair[0] > pair[1]));
        // Something new while paging does not shift the next page.
        let code = world.invites[&OWNED][0].code.clone();
        world.manage(
            Command::RevokeInvite {
                server: OWNED,
                code,
            },
            Timestamp(2),
        );
        let (second, _) = audit_page(&mut world, AuditQuery::default(), first.last().copied());
        assert!(second.iter().all(|id| !first.contains(id)));
        assert!(second[0] < *first.last().unwrap());
        let mut seen = first.clone();
        let mut before = second.last().copied();
        seen.extend(&second);
        loop {
            let (page, end) = audit_page(&mut world, AuditQuery::default(), before);
            seen.extend(&page);
            before = page.last().copied();
            if end {
                break;
            }
        }
        assert_eq!(seen.len() as u64, AUDIT_ENTRIES);
        // The provider filters: every entry of a filtered page matches.
        let query = AuditQuery {
            actor: None,
            action: Some(AuditActionKind::ServerUpdated),
        };
        let (filtered, end) = audit_page(&mut world, query, None);
        assert!(end && !filtered.is_empty());
        let log = &world.audit[&OWNED];
        assert!(filtered.iter().all(|id| log
            .iter()
            .any(|entry| entry.id == *id && entry.action == AuditAction::ServerUpdated)));
    }

    #[test]
    fn bans_are_paged_by_user_after_the_last_one() {
        let mut world = world();
        let total = world.bans[&OWNED].len();
        let page = |world: &mut World, after| {
            let events = world.manage(
                Command::LoadBans {
                    request: 1,
                    server: OWNED,
                    after,
                    limit: 2,
                },
                Timestamp(1),
            );
            match events.into_iter().next() {
                Some(Event::BansLoaded {
                    bans, reached_end, ..
                }) => (bans, reached_end),
                other => panic!("{other:?}"),
            }
        };
        let mut all = Vec::new();
        let mut after = None;
        loop {
            let (bans, end) = page(&mut world, after);
            after = bans.last().map(|ban| ban.user);
            all.extend(bans.into_iter().map(|ban| ban.user));
            if end {
                break;
            }
        }
        assert_eq!(all.len(), total);
        assert!(all.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn invites_and_expressions_change_only_with_the_permission() {
        let mut world = world();
        let code = world.invites[&OWNED][1].code.clone();
        let emoji = ExpressionId::Emoji(world.server(OWNED).unwrap().emojis[0].id);
        let renamed = world.manage(
            Command::RenameExpression {
                server: OWNED,
                expression: emoji,
                name: "cangrejo".into(),
            },
            Timestamp(1),
        );
        assert!(matches!(&renamed[0], Event::ServerChanged(server)
            if server.emojis[0].name == "cangrejo" && server.emojis[0].insert == ":cangrejo:"));
        let bad = world.manage(
            Command::RenameExpression {
                server: OWNED,
                expression: emoji,
                name: "con espacio".into(),
            },
            Timestamp(1),
        );
        assert!(matches!(
            &bad[0],
            Event::Failed {
                failure: Failure::Expression { .. },
                ..
            }
        ));
        // Without the permission nothing changes, and the refusal names what.
        let me = world
            .members
            .iter_mut()
            .find(|member| member.server == OWNED && member.user == ME)
            .unwrap();
        me.roles.clear();
        world
            .servers
            .iter_mut()
            .find(|server| server.id == OWNED)
            .unwrap()
            .owner = None;
        let refused = world.manage(
            Command::RevokeInvite {
                server: OWNED,
                code: code.clone(),
            },
            Timestamp(1),
        );
        assert!(matches!(&refused[0], Event::Failed {
            failure: Failure::Invite { code: named, .. }, ..
        } if *named == code));
        assert!(world.invites[&OWNED]
            .iter()
            .any(|invite| invite.code == code));
        let refused = world.manage(
            Command::DeleteExpression {
                server: OWNED,
                expression: emoji,
            },
            Timestamp(1),
        );
        assert!(matches!(&refused[0], Event::Failed { .. }));
        assert_eq!(world.server(OWNED).unwrap().emojis.len(), EMOJIS.len());
    }

    #[test]
    fn only_the_simulation_declares_the_admin_families() {
        use crate::Provider;
        let info = crate::mock::MockProvider::new(MockConfig::default()).info();
        assert!(info.capabilities.audit_log);
        assert!(info.capabilities.manage_invites);
        assert!(info.capabilities.manage_expressions);
        let none = crate::Capabilities::default();
        assert!(!none.manage_invites && !none.manage_expressions && !none.audit_log);
        assert!(info.capabilities.timeouts && info.capabilities.webhooks);
        // Its favorites stay on this device, and its codes lead nowhere.
        assert!(!info.capabilities.synced_gif_favorites);
        assert_eq!(info.capabilities.invite_links, None);
    }

    /// Someone of the owned server without a role, and not the account.
    fn plain_member(world: &World) -> UserId {
        world
            .members
            .iter()
            .find(|m| m.server == OWNED && m.user != ME && m.roles.is_empty())
            .unwrap()
            .user
    }

    fn logged(world: &World) -> AuditAction {
        world.audit[&OWNED].last().unwrap().action.clone()
    }

    fn failed(events: &[Event]) -> Option<&Failure> {
        match events.first() {
            Some(Event::Failed { failure, .. }) => Some(failure),
            _ => None,
        }
    }

    #[test]
    fn a_timeout_lasts_up_to_28_days_and_is_logged() {
        let mut world = world();
        let user = plain_member(&world);
        let now = Timestamp(1_000_000);
        let until = now.plus_minutes(60);
        let timeout = |until| Command::TimeoutMember {
            server: OWNED,
            user,
            until,
            reason: Some("spam".into()),
        };
        let timed = world.manage(timeout(Some(until)), now);
        assert!(matches!(&timed[0], Event::MemberChanged(member)
            if member.restrictions.timeout_until == Some(until)));
        assert_eq!(
            logged(&world),
            AuditAction::MemberTimedOut {
                user,
                until: Some(until)
            }
        );
        let too_long = world.manage(timeout(Some(now.plus_minutes(29 * 24 * 60))), now);
        assert!(matches!(failed(&too_long), Some(Failure::Member { .. })));
        let myself = world.manage(
            Command::TimeoutMember {
                server: OWNED,
                user: ME,
                until: Some(until),
                reason: None,
            },
            now,
        );
        assert!(failed(&myself).is_some());
        let lifted = world.manage(timeout(None), now);
        assert!(matches!(&lifted[0], Event::MemberChanged(member)
            if member.restrictions.timeout_until.is_none()));
    }

    #[test]
    fn a_new_invitation_is_listed_first_and_says_how_it_was_made() {
        let mut world = world();
        let channel = world
            .channels
            .iter()
            .find(|c| c.server == Some(OWNED) && c.kind == ChannelKind::Text)
            .unwrap()
            .id;
        let now = Timestamp(5_000);
        let invite = |request, max_age_secs, max_uses, temporary| Command::CreateInvite {
            request,
            server: OWNED,
            channel,
            max_age_secs,
            max_uses,
            temporary,
        };
        let made = world.manage(invite(7, 3600, 5, true), now);
        let Event::InviteCreated {
            request: 7,
            invite: made,
            ..
        } = &made[0]
        else {
            panic!("{made:?}");
        };
        assert_eq!(made.code.len(), 8);
        assert_eq!(
            (made.uses, made.max_uses, made.temporary),
            (Some(0), Some(5), true)
        );
        assert_eq!(made.expires, Some(Timestamp(5_000 + 3_600_000)));
        assert_eq!(world.invites[&OWNED][0], *made);
        assert_eq!(
            logged(&world),
            AuditAction::InviteCreated {
                code: made.code.clone()
            }
        );
        let forever = world.manage(invite(8, 0, 0, false), now);
        assert!(matches!(&forever[0], Event::InviteCreated { invite, .. }
            if invite.expires.is_none() && invite.max_uses.is_none()));
        let too_long = world.manage(invite(9, 8 * 24 * 60 * 60, 0, false), now);
        assert!(matches!(
            failed(&too_long),
            Some(Failure::NewInvite { request: 9, .. })
        ));
    }

    #[test]
    fn a_new_expression_joins_the_server_before_it_is_announced() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(64u32.to_be_bytes());
        png.extend(64u32.to_be_bytes());
        let image =
            crate::ExpressionImage::png(ExpressionKind::Emoji, bytes::Bytes::from(png)).unwrap();
        let emoji = |request, name: &str| Command::CreateExpression {
            request,
            server: OWNED,
            expression: NewExpression {
                kind: ExpressionKind::Emoji,
                name: name.into(),
                image: image.clone(),
                tags: String::new(),
                description: String::new(),
            },
        };
        let mut world = world();
        let events = world.manage(emoji(3, "cangrejo"), Timestamp(1));
        let [Event::ServerChanged(server), Event::Created {
            request: 3,
            created: Created::Expression(OWNED, ExpressionId::Emoji(id)),
        }] = events.as_slice()
        else {
            panic!("{events:?}");
        };
        assert!(server
            .emojis
            .iter()
            .any(|e| e.id == *id && e.insert == ":cangrejo:"));
        assert_eq!(
            logged(&world),
            AuditAction::EmojiCreated {
                name: "cangrejo".into()
            }
        );
        let bad = world.manage(emoji(4, "con espacio"), Timestamp(1));
        assert!(matches!(
            failed(&bad),
            Some(Failure::NewExpression { request: 4, .. })
        ));
    }

    #[test]
    fn member_pages_go_on_without_repeating_anyone() {
        let mut world = world();
        let total = world.members.iter().filter(|m| m.server == OWNED).count();
        let mut seen: Vec<UserId> = Vec::new();
        let mut after = None;
        for _ in 0..20 {
            let events = world.manage(
                Command::LoadMembers {
                    request: 1,
                    server: OWNED,
                    after: after.clone(),
                    query: Default::default(),
                },
                Timestamp(1),
            );
            let Some(Event::MembersLoaded {
                members,
                next,
                total: told,
                ..
            }) = events.into_iter().next()
            else {
                panic!("a page");
            };
            assert_eq!(told, Some(total as u32));
            assert!(members.len() <= MEMBERS_PAGE);
            seen.extend(members.iter().map(|m| m.user));
            after = next;
            if after.is_none() {
                break;
            }
        }
        assert_eq!(seen.len(), total);
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), total, "nobody twice");
    }

    #[test]
    fn a_prune_counts_first_and_removes_only_when_asked() {
        let mut world = world();
        let prune = |request, days, remove| Command::PruneMembers {
            request,
            server: OWNED,
            days,
            remove,
        };
        let count = |events: &[Event]| match events.last() {
            Some(Event::Pruned { count, .. }) => count.unwrap(),
            other => panic!("{other:?}"),
        };
        let counted = world.manage(prune(1, 7, false), Timestamp(1));
        let before = world.members.len();
        let would = count(&counted);
        let removed = world.manage(prune(2, 7, true), Timestamp(1));
        assert_eq!(count(&removed), would);
        assert_eq!(world.members.len(), before - would as usize);
        assert_eq!(removed.len(), would as usize + 1);
        let bad = world.manage(prune(3, 31, false), Timestamp(1));
        assert!(matches!(
            failed(&bad),
            Some(Failure::Prune { request: 3, .. })
        ));
    }

    #[test]
    fn webhooks_are_made_renamed_moved_and_deleted_and_integrations_take_their_bot() {
        let mut world = world();
        let channels: Vec<ChannelId> = world
            .channels
            .iter()
            .filter(|c| c.server == Some(OWNED) && c.kind == ChannelKind::Text)
            .map(|c| c.id)
            .collect();
        let create = |name: &str| Command::CreateWebhook {
            server: OWNED,
            channel: channels[0],
            name: name.into(),
        };
        let made = world.manage(create("Despliegues"), Timestamp(1));
        let Event::WebhookChanged { webhook, .. } = &made[0] else {
            panic!("{made:?}");
        };
        let id = webhook.id;
        let edited = world.manage(
            Command::EditWebhook {
                server: OWNED,
                webhook: id,
                name: Some("Despliegues de CI".into()),
                channel: Some(channels[1]),
            },
            Timestamp(1),
        );
        assert!(matches!(&edited[0], Event::WebhookChanged { webhook, .. }
            if webhook.name == "Despliegues de CI" && webhook.channel == channels[1]));
        let reserved = world.manage(create("Discord bot"), Timestamp(1));
        assert!(matches!(
            failed(&reserved),
            Some(Failure::Webhook { webhook: None, .. })
        ));
        // One that follows another server's announcements is not edited here.
        let follower = world.webhooks[&OWNED]
            .iter()
            .find(|w| w.kind == WebhookKind::Follower)
            .unwrap()
            .id;
        let refused = world.manage(
            Command::EditWebhook {
                server: OWNED,
                webhook: follower,
                name: Some("Otro".into()),
                channel: None,
            },
            Timestamp(1),
        );
        assert!(failed(&refused).is_some());
        let deleted = world.manage(
            Command::DeleteWebhook {
                server: OWNED,
                webhook: id,
            },
            Timestamp(1),
        );
        assert_eq!(
            deleted,
            [Event::WebhookRemoved {
                server: OWNED,
                webhook: id
            }]
        );
        let app = world.integrations[&OWNED]
            .iter()
            .find(|i| i.service == "Aplicación")
            .cloned()
            .unwrap();
        let removed = world.manage(
            Command::DeleteIntegration {
                server: OWNED,
                integration: app.id,
            },
            Timestamp(1),
        );
        assert_eq!(
            removed.last(),
            Some(&Event::IntegrationRemoved {
                server: OWNED,
                integration: app.id
            })
        );
        if let Some(bot) = app.bot {
            assert!(world.member(OWNED, bot).is_none());
        }
    }
}
