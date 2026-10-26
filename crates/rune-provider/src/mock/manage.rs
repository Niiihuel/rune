//! Servers, channels, roles, members, the profile, the custom status and
//! pinned messages in the simulation. Each
//! change checks what the account may do the way a service would, applies
//! it, and reports the changed objects; when roles or overwrites change, the
//! account's permissions in each channel are worked out again and the
//! channels whose permissions moved are reported too.

use std::collections::HashMap;

use rune_i18n::{es, t, tr};
use rune_model::{
    permissions, AuditAction, AuditEntry, Ban, Channel, ChannelId, ChannelKind, Member, Overwrite,
    OverwriteTarget, Permissions, Role, RoleId, Server, ServerId, Timestamp,
};

use super::{ChannelSim, World, ME};
use crate::{ChannelEdit, Command, Created, Event, Failure, ProfileEdit, RoleEdit, ServerEdit};

type Outcome = Result<Vec<Event>, String>;

const NOT_ALLOWED: &str = es("no tienes permiso");

impl World {
    /// Carry out a management command and report what changed, or why not.
    pub(super) fn manage(&mut self, command: Command, now: Timestamp) -> Vec<Event> {
        self.now = now;
        let failure = Failure::of(&command).unwrap_or(Failure::Other);
        // Described before it runs: a deletion still knows the name.
        let logged = self.describe(&command);
        let outcome = match command {
            Command::CreateServer { request, name } => self.create_server(request, name),
            Command::EditServer { server, edit } => self.edit_server(server, edit),
            Command::DeleteServer { server } => self.delete_server(server),
            Command::LeaveServer { server } => self.leave_server(server),
            Command::CreateChannel {
                request,
                server,
                kind,
                name,
                parent,
                private,
            } => self.create_channel(request, server, kind, name, parent, private),
            Command::EditChannel { channel, edit } => self.edit_channel(channel, edit),
            Command::DeleteChannel { channel } => self.delete_channel(channel),
            Command::SetOverwrite { channel, overwrite } => {
                self.overwrite(channel, overwrite.target, Some(overwrite))
            }
            Command::RemoveOverwrite { channel, target } => self.overwrite(channel, target, None),
            Command::CreateRole {
                request,
                server,
                name,
            } => self.create_role(request, server, name),
            Command::EditRole { server, role, edit } => self.edit_role(server, role, edit),
            Command::DeleteRole { server, role } => self.delete_role(server, role),
            Command::SetMemberRoles {
                server,
                user,
                roles,
            } => self.set_member_roles(server, user, roles),
            Command::SetNick { server, user, nick } => self.set_nick(server, user, nick),
            Command::Kick {
                server,
                user,
                reason: _,
            } => self.remove_member(server, user, None),
            Command::Ban {
                server,
                user,
                reason,
                ..
            } => self.remove_member(server, user, Some(reason)),
            Command::Unban { server, user } => self.unban(server, user),
            Command::LoadBans {
                request,
                server,
                after,
                limit,
            } => self.load_bans(request, server, after, limit),
            Command::LoadInvites { request, server } => self.load_invites(request, server),
            Command::RevokeInvite { server, code } => self.revoke_invite(server, code),
            Command::RenameExpression {
                server,
                expression,
                name,
            } => self.change_expression(server, expression, Some(name)),
            Command::DeleteExpression { server, expression } => {
                self.change_expression(server, expression, None)
            }
            Command::EditProfile(edit) => self.edit_profile(edit),
            Command::EditServerProfile { server, edit } => self.edit_server_profile(server, edit),
            Command::SetStatus(status) => {
                Ok(self.set_presence(|presence| presence.status = status))
            }
            Command::SetCustomStatus { text, emoji, .. } => self.set_custom_status(text, emoji),
            Command::LoadAudit {
                request,
                server,
                query,
                before,
                limit,
            } => self.load_audit(request, server, query, before, limit),
            Command::PinMessage { channel, message } => self.pin(channel, message, true),
            Command::UnpinMessage { channel, message } => self.pin(channel, message, false),
            Command::LoadPins { request, channel } => match self.channel(channel) {
                Some(_) => Ok(vec![Event::PinsLoaded {
                    request,
                    channel,
                    messages: self.pins(channel),
                }]),
                None => Err(t("el canal no existe").into()),
            },
            Command::LoadProfile {
                request,
                user,
                server,
                effect,
            } => self.load_profile(request, user, server, effect),
            command @ (Command::CreateThread { .. }
            | Command::CreatePost { .. }
            | Command::LoadThreads { .. }
            | Command::FollowThread { .. }
            | Command::EditThread { .. }
            | Command::DeleteThread { .. }
            | Command::MuteThread { .. }) => self.thread_command(command),
            command @ (Command::RaiseHand { .. } | Command::SetSpeaker { .. }) => {
                self.stage_command(command)
            }
            // Administration (19): see `mock::admin`.
            Command::TimeoutMember {
                server,
                user,
                until,
                reason: _,
            } => self.timeout(server, user, until),
            Command::CreateInvite {
                request,
                server,
                channel,
                max_age_secs,
                max_uses,
                temporary,
            } => self.create_invite(request, server, channel, max_age_secs, max_uses, temporary),
            Command::CreateExpression {
                request,
                server,
                expression,
            } => self.create_expression(request, server, expression),
            Command::LoadMembers {
                request,
                server,
                after,
                query,
            } => self.load_members(request, server, after, query),
            Command::PruneMembers {
                request,
                server,
                days,
                remove,
            } => self.prune(request, server, days, remove),
            Command::LoadWebhooks { request, server } => self.load_webhooks(request, server),
            Command::CreateWebhook {
                server,
                channel,
                name,
            } => self.create_webhook(server, channel, name),
            Command::EditWebhook {
                server,
                webhook,
                name,
                channel,
            } => self.edit_webhook(server, webhook, name, channel),
            Command::DeleteWebhook { server, webhook } => self.delete_webhook(server, webhook),
            Command::LoadIntegrations { request, server } => {
                self.load_integrations(request, server)
            }
            Command::DeleteIntegration {
                server,
                integration,
            } => self.delete_integration(server, integration),
            Command::LoadGifFavorites | Command::SetGifFavorite { .. } => {
                Err(t("la simulación guarda los favoritos en este equipo").into())
            }
            // Invitations of every kind: see `mock::invitations`.
            Command::AcceptInvitation {
                request,
                target,
                destination,
                ..
            } => Ok(self.accept_invitation(request, &target, destination)),
            // Rules and onboarding (2026-10-18): see `mock::onboarding`.
            command @ (Command::LoadServerRules { .. }
            | Command::AnswerServerRules { .. }
            | Command::LoadOnboarding { .. }
            | Command::AnswerOnboarding { .. }
            | Command::SaveOnboarding { .. }) => return self.onboarding(command),
            // Scheduled events (eventos-programados): see `mock::events`.
            command @ (Command::LoadScheduledEvents { .. }
            | Command::LoadEventInterested { .. }
            | Command::SetEventInterest { .. }
            | Command::CreateScheduledEvent { .. }
            | Command::EditScheduledEvent { .. }
            | Command::SetScheduledEventStatus { .. }
            | Command::DeleteScheduledEvent { .. }) => return self.scheduled_events(command),
            command => match self
                .account_command(&command)
                .or_else(|| self.app_command(&command))
                // Server administration, rest (P7).
                .or_else(|| self.rest_admin(&command))
                // AutoMod (automod).
                .or_else(|| self.automod_command(&command))
            {
                Some(outcome) => outcome,
                None => self
                    .social(command)
                    .unwrap_or_else(|| Err(t("la simulación no sabe hacer eso").into())),
            },
        };
        match outcome {
            Ok(events) => {
                if let Some((server, action, reason)) = logged {
                    let id = self.fresh_id();
                    self.audit.entry(server).or_default().push(AuditEntry {
                        id,
                        actor: ME,
                        action,
                        when: now,
                        reason,
                    });
                }
                events
            }
            Err(reason) => vec![Event::Failed {
                failure,
                reason,
                ambiguous: false,
            }],
        }
    }

    /// What the audit log will say about `command` if it succeeds.
    fn describe(&self, command: &Command) -> Option<(ServerId, AuditAction, Option<String>)> {
        let channel_name = |channel: ChannelId| {
            self.channel(channel)
                .map(|found| (found.server, found.name.clone()))
        };
        let role_name = |server: ServerId, role: RoleId| {
            self.server(server)
                .ok()?
                .roles
                .iter()
                .find(|candidate| candidate.id == role)
                .map(|found| found.name.clone())
        };
        Some(match command {
            Command::EditServer { server, .. } => (*server, AuditAction::ServerUpdated, None),
            Command::EditChannel { channel, .. } => {
                let (server, name) = channel_name(*channel)?;
                (
                    server?,
                    AuditAction::ChannelUpdated {
                        channel: *channel,
                        name,
                    },
                    None,
                )
            }
            Command::DeleteChannel { channel } => {
                let (server, name) = channel_name(*channel)?;
                (server?, AuditAction::ChannelDeleted { name }, None)
            }
            Command::SetOverwrite { channel, .. } | Command::RemoveOverwrite { channel, .. } => {
                let (server, name) = channel_name(*channel)?;
                (
                    server?,
                    AuditAction::PermissionsChanged {
                        channel: *channel,
                        name,
                    },
                    None,
                )
            }
            Command::EditRole { server, role, .. } => (
                *server,
                AuditAction::RoleUpdated {
                    role: *role,
                    name: role_name(*server, *role)?,
                },
                None,
            ),
            Command::DeleteRole { server, role } => (
                *server,
                AuditAction::RoleDeleted {
                    name: role_name(*server, *role)?,
                },
                None,
            ),
            Command::SetMemberRoles {
                server,
                user,
                roles,
            } => {
                let current = &self.member(*server, *user)?.roles;
                (
                    *server,
                    AuditAction::MemberRolesChanged {
                        user: *user,
                        added: roles
                            .iter()
                            .filter(|role| !current.contains(role))
                            .copied()
                            .collect(),
                        removed: current
                            .iter()
                            .filter(|role| !roles.contains(role))
                            .copied()
                            .collect(),
                    },
                    None,
                )
            }
            Command::SetNick { server, user, nick } => (
                *server,
                AuditAction::NickChanged {
                    user: *user,
                    nick: nick.clone(),
                },
                None,
            ),
            Command::Kick {
                server,
                user,
                reason,
            } => (
                *server,
                AuditAction::MemberKicked { user: *user },
                reason.clone(),
            ),
            Command::Ban {
                server,
                user,
                reason,
                ..
            } => (
                *server,
                AuditAction::MemberBanned { user: *user },
                reason.clone(),
            ),
            Command::Unban { server, user } => {
                (*server, AuditAction::MemberUnbanned { user: *user }, None)
            }
            Command::TimeoutMember {
                server,
                user,
                until,
                reason,
            } => (
                *server,
                AuditAction::MemberTimedOut {
                    user: *user,
                    until: *until,
                },
                reason.clone(),
            ),
            Command::RevokeInvite { server, code } => (
                *server,
                AuditAction::InviteDeleted { code: code.clone() },
                None,
            ),
            Command::RenameExpression {
                server,
                expression,
                name,
            } => {
                let name = name.trim().to_owned();
                self.expression_name(*server, *expression)?;
                let action = match expression {
                    rune_model::ExpressionId::Emoji(_) => AuditAction::EmojiUpdated { name },
                    rune_model::ExpressionId::Sticker(_) => AuditAction::StickerUpdated { name },
                };
                (*server, action, None)
            }
            Command::DeleteExpression { server, expression } => {
                let name = self.expression_name(*server, *expression)?;
                let action = match expression {
                    rune_model::ExpressionId::Emoji(_) => AuditAction::EmojiDeleted { name },
                    rune_model::ExpressionId::Sticker(_) => AuditAction::StickerDeleted { name },
                };
                (*server, action, None)
            }
            // Made things are logged once they have an id: see `log_created`.
            _ => return None,
        })
    }

    /// Log something a create command made.
    pub(super) fn log_created(&mut self, server: ServerId, action: AuditAction) {
        let id = self.fresh_id();
        let when = self.now;
        self.audit.entry(server).or_default().push(AuditEntry {
            id,
            actor: ME,
            action,
            when,
            reason: None,
        });
    }

    pub(super) fn server(&self, id: ServerId) -> Result<&Server, String> {
        self.servers
            .iter()
            .find(|server| server.id == id)
            .ok_or_else(|| t("el servidor no existe").to_owned())
    }

    pub(super) fn server_mut(&mut self, id: ServerId) -> Result<&mut Server, String> {
        self.servers
            .iter_mut()
            .find(|server| server.id == id)
            .ok_or_else(|| t("el servidor no existe").to_owned())
    }

    pub(super) fn member(&self, server: ServerId, user: rune_model::UserId) -> Option<&Member> {
        self.members
            .iter()
            .find(|member| member.server == server && member.user == user)
    }

    /// What the account may do across `server`.
    pub(super) fn mine(&self, server: ServerId) -> Result<Permissions, String> {
        let found = self.server(server)?;
        let member = self
            .member(server, ME)
            .ok_or(t("no eres miembro del servidor"))?;
        Ok(permissions::in_server(found, member))
    }

    pub(super) fn require(&self, server: ServerId, needed: Permissions) -> Result<(), String> {
        if self.mine(server)?.contains(needed) {
            Ok(())
        } else {
            Err(t(NOT_ALLOWED).into())
        }
    }

    /// The position of the account's highest role; the owner outranks all.
    pub(super) fn my_rank(&self, server: ServerId) -> i32 {
        let Ok(found) = self.server(server) else {
            return i32::MIN;
        };
        if found.owner == Some(ME) {
            return i32::MAX;
        }
        let Some(member) = self.member(server, ME) else {
            return i32::MIN;
        };
        found
            .roles
            .iter()
            .filter(|role| member.roles.contains(&role.id))
            .map(|role| role.position)
            .max()
            .unwrap_or(0)
    }

    /// Roles at or above the account's highest are out of its reach.
    fn require_below(&self, server: ServerId, role: RoleId) -> Result<(), String> {
        let position = self
            .server(server)?
            .roles
            .iter()
            .find(|candidate| candidate.id == role)
            .ok_or(t("el rol no existe"))?
            .position;
        if position < self.my_rank(server) {
            Ok(())
        } else {
            Err(t("ese rol está a la altura del tuyo o por encima").into())
        }
    }

    pub(super) fn fresh_id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    /// Work out again what the account may do in each channel of `server`,
    /// reporting the channels whose permissions changed. A thread can do
    /// what its parent can, as a service has it: a later change to the
    /// parent reaches its threads too.
    pub(super) fn refresh_permissions(&mut self, server: ServerId) -> Vec<Event> {
        let (Ok(found), Some(member)) = (self.server(server), self.member(server, ME)) else {
            return Vec::new();
        };
        let (found, member) = (found.clone(), member.clone());
        let parents: HashMap<ChannelId, Permissions> = self
            .channels
            .iter()
            .filter(|channel| channel.server == Some(server) && !channel.is_thread())
            .map(|channel| {
                (
                    channel.id,
                    permissions::in_channel(&found, &member, channel),
                )
            })
            .collect();
        let mut changed = Vec::new();
        for channel in self
            .channels
            .iter_mut()
            .filter(|channel| channel.server == Some(server))
        {
            let own = if channel.is_thread() {
                channel.parent
            } else {
                Some(channel.id)
            };
            let now = own
                .and_then(|id| parents.get(&id).copied())
                .unwrap_or(Permissions::NONE);
            if now != channel.permissions {
                channel.permissions = now;
                changed.push(Event::ChannelChanged(channel.clone()));
            }
        }
        changed
    }

    fn server_changed(&self, server: ServerId) -> Result<Event, String> {
        Ok(Event::ServerChanged(self.server(server)?.clone()))
    }

    fn create_server(&mut self, request: crate::RequestId, name: String) -> Outcome {
        let name = valid_name(&name)?;
        let id = ServerId(self.fresh_id());
        let server = Server {
            stickers: Vec::new(),
            emojis: Vec::new(),
            icon: None,
            id,
            name,
            accent: rune_model::Rgb(0x5E_6A_D2),
            roles: vec![Role::everyone(id, Permissions::EVERYONE)],
            owner: Some(ME),
            description: None,
            options: Default::default(),
        };
        self.servers.push(server.clone());
        let member = Member {
            user: ME,
            server: id,
            nick: None,
            roles: Vec::new(),
            ..Default::default()
        };
        self.members.push(member.clone());
        let mut events = vec![Event::ServerChanged(server), Event::MemberChanged(member)];
        // Like a new server anywhere: a text and a voice category, one of each.
        for (position, (category, channel, kind)) in [
            ("Canales de texto", "general", ChannelKind::Text),
            ("Canales de voz", "General", ChannelKind::Voice),
        ]
        .into_iter()
        .enumerate()
        {
            let parent = self.add_channel(id, ChannelKind::Category, category, None, position)?;
            self.add_channel(id, kind, channel, Some(parent), 0)?;
        }
        self.refresh_permissions(id);
        events.extend(
            self.channels
                .iter()
                .filter(|channel| channel.server == Some(id))
                .cloned()
                .map(Event::ChannelChanged),
        );
        events.push(Event::Created {
            request,
            created: Created::Server(id),
        });
        Ok(events)
    }

    fn edit_server(&mut self, server: ServerId, edit: ServerEdit) -> Outcome {
        self.require(server, Permissions::MANAGE_SERVER)?;
        let name = edit.name.as_deref().map(valid_name).transpose()?;
        // Server administration, rest (P7): see `mock::server_admin`.
        self.edit_options(server, &edit)?;
        let found = self.server_mut(server)?;
        if let Some(name) = name {
            found.name = name;
        }
        if let Some(description) = edit.description {
            found.description = description.filter(|text| !text.trim().is_empty());
        }
        if let Some(accent) = edit.accent {
            found.accent = accent;
        }
        Ok(vec![self.server_changed(server)?])
    }

    fn delete_server(&mut self, server: ServerId) -> Outcome {
        if self.server(server)?.owner != Some(ME) {
            return Err(t("solo quien es dueño puede borrar el servidor").into());
        }
        Ok(self.forget_server(server, Some(t("El servidor se borró").into())))
    }

    fn leave_server(&mut self, server: ServerId) -> Outcome {
        if self.server(server)?.owner == Some(ME) {
            return Err(t("eres dueño: borra el servidor en vez de abandonarlo").into());
        }
        Ok(self.forget_server(server, None))
    }

    /// Take `server` out of the account's world.
    fn forget_server(&mut self, server: ServerId, reason: Option<String>) -> Vec<Event> {
        let channels: Vec<ChannelId> = self
            .channels
            .iter()
            .filter(|channel| channel.server == Some(server))
            .map(|channel| channel.id)
            .collect();
        let mut events = Vec::new();
        if self.my_call().is_some_and(|call| channels.contains(&call)) {
            events.extend(self.leave_voice());
        }
        self.servers.retain(|candidate| candidate.id != server);
        self.channels
            .retain(|channel| channel.server != Some(server));
        self.members.retain(|member| member.server != server);
        self.voice
            .retain(|state| !channels.contains(&state.channel));
        for channel in channels {
            self.sims.remove(&channel);
        }
        events.push(Event::ServerRemoved { server, reason });
        events
    }

    fn add_channel(
        &mut self,
        server: ServerId,
        kind: ChannelKind,
        name: &str,
        parent: Option<ChannelId>,
        position: usize,
    ) -> Result<ChannelId, String> {
        let id = ChannelId(self.fresh_id());
        self.channels.push(Channel {
            id,
            server: Some(server),
            kind,
            name: name.to_owned(),
            topic: None,
            topic_pictures: Vec::new(),
            parent,
            position: position as i32,
            recipients: Vec::new(),
            owner: None,
            permissions: Permissions::NONE,
            overwrites: Vec::new(),
            slowmode_secs: 0,
            nsfw: false,
            icon: None,
            thread: None,
            tags: Vec::new(),
            forum: Default::default(),
        });
        if kind.has_messages() {
            self.sims.insert(
                id,
                ChannelSim {
                    generated: 0,
                    sitting_gap: 60,
                    extra: Vec::new(),
                    overlay: HashMap::new(),
                },
            );
        }
        Ok(id)
    }

    fn create_channel(
        &mut self,
        request: crate::RequestId,
        server: ServerId,
        kind: ChannelKind,
        name: String,
        parent: Option<ChannelId>,
        private: bool,
    ) -> Outcome {
        self.require(server, Permissions::MANAGE_CHANNELS)?;
        if !matches!(
            kind,
            ChannelKind::Text | ChannelKind::Voice | ChannelKind::Category
        ) {
            return Err(t("un servidor solo tiene canales de texto, de voz y categorías").into());
        }
        let name = if kind == ChannelKind::Text {
            text_channel_name(&name)?
        } else {
            valid_name(&name)?
        };
        if let Some(parent) = parent {
            let category = self.channel(parent).ok_or(t("la categoría no existe"))?;
            if category.kind != ChannelKind::Category || category.server != Some(server) {
                return Err(t("solo se puede poner un canal dentro de una categoría").into());
            }
        }
        let position = self
            .channels
            .iter()
            .filter(|channel| channel.server == Some(server) && channel.parent == parent)
            .filter(|channel| {
                (channel.kind == ChannelKind::Category) == (kind == ChannelKind::Category)
            })
            .count();
        let id = self.add_channel(server, kind, &name, parent, position)?;
        if private {
            // Hidden from everyone but the account, which made it.
            let everyone = RoleId(server.0);
            let channel = self
                .channels
                .iter_mut()
                .find(|channel| channel.id == id)
                .ok_or(t("el canal no existe"))?;
            channel.overwrites = vec![
                Overwrite {
                    target: OverwriteTarget::Role(everyone),
                    allow: Permissions::NONE,
                    deny: Permissions::VIEW,
                },
                Overwrite {
                    target: OverwriteTarget::Member(ME),
                    allow: Permissions::VIEW,
                    deny: Permissions::NONE,
                },
            ];
        }
        self.refresh_permissions(server);
        let channel = self.channel(id).cloned().ok_or(t("el canal no existe"))?;
        self.log_created(
            server,
            AuditAction::ChannelCreated {
                channel: id,
                name: channel.name.clone(),
            },
        );
        Ok(vec![
            Event::ChannelChanged(channel),
            Event::Created {
                request,
                created: Created::Channel(id),
            },
        ])
    }

    /// The server of a channel the account may manage.
    pub(super) fn managed_channel(
        &self,
        channel: ChannelId,
        needed: Permissions,
    ) -> Result<ServerId, String> {
        let found = self.channel(channel).ok_or(t("el canal no existe"))?;
        let server = found
            .server
            .ok_or(t("una conversación directa no se administra"))?;
        if found.permissions.contains(needed) {
            Ok(server)
        } else {
            Err(t(NOT_ALLOWED).into())
        }
    }

    fn edit_channel(&mut self, channel: ChannelId, edit: ChannelEdit) -> Outcome {
        let server = self.managed_channel(channel, Permissions::MANAGE_CHANNELS)?;
        let kind = self.channel(channel).map(|found| found.kind);
        let name = match &edit.name {
            Some(name) if kind == Some(ChannelKind::Text) => Some(text_channel_name(name)?),
            Some(name) => Some(valid_name(name)?),
            None => None,
        };
        if let Some(Some(parent)) = edit.parent {
            let category = self.channel(parent).ok_or(t("la categoría no existe"))?;
            if category.kind != ChannelKind::Category || category.server != Some(server) {
                return Err(t("solo se puede poner un canal dentro de una categoría").into());
            }
            if kind == Some(ChannelKind::Category) {
                return Err(t("una categoría no puede ir dentro de otra").into());
            }
        }
        if edit.slowmode_secs.is_some_and(|secs| secs > 6 * 60 * 60) {
            return Err(t("el modo lento llega hasta 6 horas").into());
        }
        // Server administration, rest (P7): see `mock::server_admin`.
        self.edit_forum(channel, &edit)?;
        let found = self
            .channels
            .iter_mut()
            .find(|candidate| candidate.id == channel)
            .ok_or(t("el canal no existe"))?;
        if let Some(name) = name {
            found.name = name;
        }
        if let Some(topic) = edit.topic {
            found.topic = topic.filter(|text| !text.trim().is_empty());
            found.topic_pictures = found
                .topic
                .as_deref()
                .map(super::identity::pictures_in)
                .unwrap_or_default();
        }
        if let Some(parent) = edit.parent {
            found.parent = parent;
        }
        if let Some(secs) = edit.slowmode_secs {
            found.slowmode_secs = secs;
        }
        if let Some(nsfw) = edit.nsfw {
            found.nsfw = nsfw;
        }
        if edit.position.is_none() && edit.parent.is_none() {
            return Ok(vec![Event::ChannelChanged(found.clone())]);
        }
        Ok(self.place(channel, edit.position))
    }

    /// Put `channel` at `position` among its siblings (same server, same
    /// category, both categories or both not), at the end when `None`; the
    /// others move to make room and are numbered again from 0. Reports the
    /// channel and each sibling that moved.
    fn place(&mut self, channel: ChannelId, position: Option<i32>) -> Vec<Event> {
        let Some(found) = self.channel(channel).cloned() else {
            return Vec::new();
        };
        let category = found.kind == ChannelKind::Category;
        let mut siblings: Vec<(i32, ChannelId)> = self
            .channels
            .iter()
            .filter(|other| {
                other.id != channel
                    && other.server == found.server
                    && other.parent == found.parent
                    && (other.kind == ChannelKind::Category) == category
            })
            .map(|other| (other.position, other.id))
            .collect();
        siblings.sort();
        let mut order: Vec<ChannelId> = siblings.into_iter().map(|(_, id)| id).collect();
        let index = position.map_or(order.len(), |position| {
            (position.max(0) as usize).min(order.len())
        });
        order.insert(index, channel);
        let mut events = Vec::new();
        for (rank, id) in order.into_iter().enumerate() {
            let Some(sibling) = self.channels.iter_mut().find(|other| other.id == id) else {
                continue;
            };
            if sibling.position != rank as i32 || id == channel {
                sibling.position = rank as i32;
                events.push(Event::ChannelChanged(sibling.clone()));
            }
        }
        events
    }

    fn delete_channel(&mut self, channel: ChannelId) -> Outcome {
        self.managed_channel(channel, Permissions::MANAGE_CHANNELS)?;
        let mut events = Vec::new();
        if self.my_call() == Some(channel) {
            events.extend(self.leave_voice());
        }
        self.channels.retain(|candidate| candidate.id != channel);
        self.sims.remove(&channel);
        self.voice.retain(|state| state.channel != channel);
        // What a deleted category held stays, without a category.
        for child in self
            .channels
            .iter_mut()
            .filter(|candidate| candidate.parent == Some(channel))
        {
            child.parent = None;
            events.push(Event::ChannelChanged(child.clone()));
        }
        events.push(Event::ChannelRemoved { channel });
        Ok(events)
    }

    fn overwrite(
        &mut self,
        channel: ChannelId,
        target: OverwriteTarget,
        overwrite: Option<Overwrite>,
    ) -> Outcome {
        let server = self.managed_channel(channel, Permissions::MANAGE_ROLES)?;
        if self.channel(channel).is_some_and(Channel::is_thread) {
            return Err(t("un hilo toma los permisos de su canal").into());
        }
        if let OverwriteTarget::Role(role) = target {
            if role != RoleId(server.0) {
                self.require_below(server, role)?;
            }
        }
        let found = self
            .channels
            .iter_mut()
            .find(|candidate| candidate.id == channel)
            .ok_or(t("el canal no existe"))?;
        found
            .overwrites
            .retain(|existing| existing.target != target);
        if let Some(overwrite) = overwrite.filter(|overwrite| !overwrite.is_empty()) {
            found.overwrites.push(overwrite);
        }
        let mut events = vec![Event::ChannelChanged(found.clone())];
        // The account's own permissions may have moved: report the channel again if so.
        events.extend(self.refresh_permissions(server));
        Ok(events)
    }

    fn create_role(
        &mut self,
        request: crate::RequestId,
        server: ServerId,
        name: String,
    ) -> Outcome {
        self.require(server, Permissions::MANAGE_ROLES)?;
        let name = valid_name(&name)?;
        let id = RoleId(self.fresh_id());
        let found = self.server_mut(server)?;
        // A new role starts at the bottom, just above @everyone.
        for role in found.roles.iter_mut().filter(|role| role.position > 0) {
            role.position += 1;
        }
        found.roles.push(Role {
            style: Default::default(),
            id,
            name: name.clone(),
            color: None,
            position: 1,
            hoisted: false,
            mentionable: false,
            permissions: Permissions::NONE,
        });
        self.log_created(server, AuditAction::RoleCreated { role: id, name });
        Ok(vec![
            self.server_changed(server)?,
            Event::Created {
                request,
                created: Created::Role(server, id),
            },
        ])
    }

    fn edit_role(&mut self, server: ServerId, role: RoleId, edit: RoleEdit) -> Outcome {
        self.require(server, Permissions::MANAGE_ROLES)?;
        let everyone = role == RoleId(server.0);
        if !everyone {
            self.require_below(server, role)?;
        }
        if everyone && (edit.name.is_some() || edit.position.is_some() || edit.color.is_some()) {
            return Err(t("@everyone solo cambia sus permisos").into());
        }
        // Server administration, rest (P7): see `mock::server_admin`.
        let style = self.role_style(server, role, &edit)?;
        // Nobody can grant what they do not have themselves.
        let mine = self.mine(server)?;
        if let Some(permissions) = edit.permissions {
            let previous = self
                .server(server)?
                .roles
                .iter()
                .find(|candidate| candidate.id == role)
                .map_or(Permissions::NONE, |found| found.permissions);
            let added = permissions.without(previous);
            if !mine.contains(added) {
                return Err(t("no puedes dar permisos que no tienes").into());
            }
        }
        if let Some(position) = edit.position {
            if position >= self.my_rank(server) {
                return Err(t("no puedes subir un rol por encima del tuyo").into());
            }
        }
        let name = edit.name.as_deref().map(valid_name).transpose()?;
        let found = self.server_mut(server)?;
        let target = found
            .roles
            .iter_mut()
            .find(|candidate| candidate.id == role)
            .ok_or(t("el rol no existe"))?;
        if let Some(name) = name {
            target.name = name;
        }
        if let Some(color) = edit.color {
            target.color = color;
        }
        if let Some(hoisted) = edit.hoisted {
            target.hoisted = hoisted;
        }
        if let Some(mentionable) = edit.mentionable {
            target.mentionable = mentionable;
        }
        if let Some(permissions) = edit.permissions {
            target.permissions = permissions;
        }
        if let Some(style) = style {
            target.style = style;
        }
        if let Some(position) = edit.position {
            reorder(&mut found.roles, role, position);
        }
        let mut events = vec![self.server_changed(server)?];
        events.extend(self.refresh_permissions(server));
        Ok(events)
    }

    fn delete_role(&mut self, server: ServerId, role: RoleId) -> Outcome {
        self.require(server, Permissions::MANAGE_ROLES)?;
        if role == RoleId(server.0) {
            return Err(t("@everyone no se puede borrar").into());
        }
        self.require_below(server, role)?;
        let found = self.server_mut(server)?;
        found.roles.retain(|candidate| candidate.id != role);
        let mut events = vec![self.server_changed(server)?];
        for member in self
            .members
            .iter_mut()
            .filter(|member| member.server == server && member.roles.contains(&role))
        {
            member.roles.retain(|candidate| *candidate != role);
            events.push(Event::MemberChanged(member.clone()));
        }
        for channel in self
            .channels
            .iter_mut()
            .filter(|channel| channel.server == Some(server))
        {
            let before = channel.overwrites.len();
            channel
                .overwrites
                .retain(|overwrite| overwrite.target != OverwriteTarget::Role(role));
            if channel.overwrites.len() != before {
                events.push(Event::ChannelChanged(channel.clone()));
            }
        }
        events.extend(self.refresh_permissions(server));
        Ok(events)
    }

    fn set_member_roles(
        &mut self,
        server: ServerId,
        user: rune_model::UserId,
        roles: Vec<RoleId>,
    ) -> Outcome {
        self.require(server, Permissions::MANAGE_ROLES)?;
        let current = self
            .member(server, user)
            .ok_or(t("esa persona no es miembro"))?
            .roles
            .clone();
        let everyone = RoleId(server.0);
        let known: Vec<RoleId> = self
            .server(server)?
            .roles
            .iter()
            .map(|role| role.id)
            .collect();
        // Only roles below the account's own can be given or taken away.
        for role in roles
            .iter()
            .filter(|role| !current.contains(role))
            .chain(current.iter().filter(|role| !roles.contains(role)))
        {
            if *role == everyone || !known.contains(role) {
                return Err(t("ese rol no existe").into());
            }
            self.require_below(server, *role)?;
        }
        let member = self
            .members
            .iter_mut()
            .find(|member| member.server == server && member.user == user)
            .ok_or(t("esa persona no es miembro"))?;
        member.roles = roles;
        let mut events = vec![Event::MemberChanged(member.clone())];
        if user == ME {
            events.extend(self.refresh_permissions(server));
        }
        Ok(events)
    }

    fn set_nick(
        &mut self,
        server: ServerId,
        user: rune_model::UserId,
        nick: Option<String>,
    ) -> Outcome {
        let needed = if user == ME {
            Permissions::CHANGE_NICK
        } else {
            Permissions::MANAGE_NICKS
        };
        self.require(server, needed)?;
        let nick = nick
            .map(|nick| nick.trim().to_owned())
            .filter(|nick| !nick.is_empty());
        if nick.as_ref().is_some_and(|nick| nick.chars().count() > 32) {
            return Err(t("el apodo puede tener hasta 32 caracteres").into());
        }
        let member = self
            .members
            .iter_mut()
            .find(|member| member.server == server && member.user == user)
            .ok_or(t("esa persona no es miembro"))?;
        member.nick = nick;
        Ok(vec![Event::MemberChanged(member.clone())])
    }

    /// Kick `user`, or ban them when `ban` carries the reason.
    fn remove_member(
        &mut self,
        server: ServerId,
        user: rune_model::UserId,
        ban: Option<Option<String>>,
    ) -> Outcome {
        let needed = if ban.is_some() {
            Permissions::BAN
        } else {
            Permissions::KICK
        };
        self.require(server, needed)?;
        if user == ME {
            return Err(t("no puedes hacerte eso a ti").into());
        }
        if self.server(server)?.owner == Some(user) {
            return Err(t("nadie puede sacar a quien es dueño").into());
        }
        let theirs = self
            .member(server, user)
            .map(|member| member.roles.clone())
            .unwrap_or_default();
        let found = self.server(server)?;
        let their_rank = found
            .roles
            .iter()
            .filter(|role| theirs.contains(&role.id))
            .map(|role| role.position)
            .max()
            .unwrap_or(0);
        if their_rank >= self.my_rank(server) {
            return Err(t("su rol está a la altura del tuyo o por encima").into());
        }
        let was_member = self.member(server, user).is_some();
        if !was_member && ban.is_none() {
            return Err(t("esa persona no es miembro").into());
        }
        self.members
            .retain(|member| !(member.server == server && member.user == user));
        let channels: Vec<ChannelId> = self
            .channels
            .iter()
            .filter(|channel| channel.server == Some(server))
            .map(|channel| channel.id)
            .collect();
        let mut events = Vec::new();
        self.voice.retain(|state| {
            let gone = state.user == user && channels.contains(&state.channel);
            if gone {
                events.push(Event::VoiceLeft {
                    user,
                    channel: state.channel,
                });
            }
            !gone
        });
        if was_member {
            events.push(Event::MemberRemoved { server, user });
        }
        if let Some(reason) = ban {
            let bans = self.bans.entry(server).or_default();
            bans.retain(|ban| ban.user != user);
            let ban = Ban { user, reason };
            bans.push(ban.clone());
            events.push(Event::BanAdded { server, ban });
        }
        Ok(events)
    }

    fn unban(&mut self, server: ServerId, user: rune_model::UserId) -> Outcome {
        self.require(server, Permissions::BAN)?;
        let bans = self.bans.entry(server).or_default();
        let before = bans.len();
        bans.retain(|ban| ban.user != user);
        if bans.len() == before {
            return Err(t("esa persona no estaba vetada").into());
        }
        Ok(vec![Event::BanRemoved { server, user }])
    }

    fn edit_profile(&mut self, edit: ProfileEdit) -> Outcome {
        let clean = |text: Option<String>| text.filter(|text| !text.trim().is_empty());
        if let Some(Some(name)) = &edit.display_name {
            if name.chars().count() > 32 {
                return Err(t("el nombre puede tener hasta 32 caracteres").into());
            }
        }
        if let Some(Some(about)) = &edit.about {
            if about.chars().count() > 190 {
                return Err(t("«Sobre mí» puede tener hasta 190 caracteres").into());
            }
        }
        let me = self
            .users
            .iter_mut()
            .find(|user| user.id == ME)
            .ok_or(t("la cuenta no existe"))?;
        if let Some(name) = edit.display_name {
            me.display_name = clean(name);
        }
        if let Some(pronouns) = edit.pronouns {
            me.pronouns = clean(pronouns);
        }
        if let Some(about) = edit.about {
            me.about = clean(about);
        }
        if let Some(accent) = edit.accent {
            me.accent = accent;
        }
        let version = self.next_id + 1;
        let avatar = edit
            .avatar
            .map(|avatar| avatar.map(|image| self.host("avatar", version, &image)));
        let banner = edit.banner.map(|banner| match banner {
            Some(image) => rune_model::ProfileBanner::Image(self.host("banner", version, &image)),
            None => rune_model::ProfileBanner::Absent,
        });
        let me = self
            .users
            .iter_mut()
            .find(|user| user.id == ME)
            .ok_or(t("la cuenta no existe"))?;
        if let Some(avatar) = avatar {
            me.avatar = avatar;
        }
        if let Some(banner) = banner {
            me.banner = banner;
        }
        let me = me.clone();
        self.next_id = version;
        Ok(vec![
            Event::UserChanged(me.clone()),
            Event::ProfileChanged(rune_model::UserProfile {
                user: ME,
                about: me.about,
                pronouns: me.pronouns,
                banner: me.banner,
                accent: Some(me.accent),
                ..Default::default()
            }),
        ])
    }

    /// The line beside the account's status, kept as a service keeps it:
    /// trimmed, none when nothing is left, refused when too long.
    fn set_custom_status(&mut self, text: Option<String>, emoji: Option<String>) -> Outcome {
        let text = text
            .map(|text| text.trim().to_owned())
            .filter(|text| !text.is_empty());
        // The line shown is the emoji, a space and the text.
        let text = match (emoji.filter(|emoji| !emoji.is_empty()), text) {
            (Some(emoji), Some(text)) => Some(format!("{emoji} {text}")),
            (emoji, text) => emoji.or(text),
        };
        if text
            .as_ref()
            .is_some_and(|text| text.chars().count() > CUSTOM_STATUS_MAX)
        {
            return Err(tr!(
                "el estado puede tener hasta {max} caracteres",
                max = CUSTOM_STATUS_MAX
            ));
        }
        Ok(self.set_presence(|presence| presence.custom = text))
    }

    /// Change the account's presence and report it.
    fn set_presence(&mut self, change: impl FnOnce(&mut rune_model::Presence)) -> Vec<Event> {
        let presence = self.presences.entry(ME).or_default();
        change(presence);
        vec![Event::PresenceChanged {
            user: ME,
            presence: presence.clone(),
        }]
    }
}

/// The longest line the simulation keeps beside a status.
pub const CUSTOM_STATUS_MAX: usize = 128;

/// A name with something in it and not too long.
fn valid_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(t("el nombre no puede quedar vacío").into());
    }
    if name.chars().count() > 100 {
        return Err(t("el nombre puede tener hasta 100 caracteres").into());
    }
    Ok(name.to_owned())
}

/// A text channel's name as such names go: lowercase, dashes for spaces.
pub(crate) fn text_channel_name(name: &str) -> Result<String, String> {
    let name: String = valid_name(name)?
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    Ok(name)
}

/// Put `role` at `position` among the roles above `@everyone`, numbering
/// them again from 1 so positions stay dense.
fn reorder(roles: &mut [Role], role: RoleId, position: i32) {
    let mut order: Vec<RoleId> = {
        let mut ranked: Vec<&Role> = roles.iter().filter(|role| role.position > 0).collect();
        ranked.sort_by_key(|role| role.position);
        ranked.iter().map(|role| role.id).collect()
    };
    order.retain(|id| *id != role);
    let index = (position.max(1) as usize - 1).min(order.len());
    order.insert(index, role);
    for found in roles.iter_mut() {
        if let Some(rank) = order.iter().position(|id| *id == found.id) {
            found.position = rank as i32 + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockConfig;
    use rune_model::UserId;

    fn world() -> World {
        World::build(&MockConfig::default())
    }

    impl World {
        fn manage_now(&mut self, command: Command) -> Vec<Event> {
            self.manage(command, Timestamp(1))
        }
    }

    #[test]
    fn changes_are_logged_with_the_names_they_had() {
        let mut world = world();
        let (owned, _) = servers(&world);
        let made = world.manage_now(Command::CreateRole {
            request: 1,
            server: owned,
            name: "Temporal".into(),
        });
        let Some(Event::Created {
            created: Created::Role(_, role),
            ..
        }) = made.last()
        else {
            panic!("{made:?}");
        };
        let role = *role;
        world.manage_now(Command::DeleteRole {
            server: owned,
            role,
        });
        let log = world.manage_now(Command::LoadAudit {
            request: 2,
            server: owned,
            query: Default::default(),
            before: None,
            limit: 50,
        });
        let Some(Event::AuditLoaded { entries, .. }) = log.first() else {
            panic!("{log:?}");
        };
        assert!(
            matches!(&entries[0].action, AuditAction::RoleDeleted { name } if name == "Temporal")
        );
        assert!(
            matches!(&entries[1].action, AuditAction::RoleCreated { name, .. } if name == "Temporal")
        );
    }

    /// The server the account owns, and one it does not.
    fn servers(world: &World) -> (ServerId, ServerId) {
        let owned = world
            .servers
            .iter()
            .find(|server| server.owner == Some(ME))
            .unwrap()
            .id;
        let other = world
            .servers
            .iter()
            .find(|server| server.owner != Some(ME))
            .unwrap()
            .id;
        (owned, other)
    }

    fn failed(events: &[Event]) -> Option<&str> {
        events.iter().find_map(|event| match event {
            Event::Failed { reason, .. } => Some(reason.as_str()),
            _ => None,
        })
    }

    #[test]
    fn the_account_can_only_manage_where_it_may() {
        let mut world = world();
        let (owned, other) = servers(&world);
        let renamed = world.manage_now(Command::EditServer {
            server: owned,
            edit: ServerEdit {
                name: Some("Mi laboratorio".into()),
                ..ServerEdit::default()
            },
        });
        assert!(
            matches!(&renamed[0], Event::ServerChanged(server) if server.name == "Mi laboratorio")
        );
        let refused = world.manage_now(Command::EditServer {
            server: other,
            edit: ServerEdit {
                name: Some("No".into()),
                ..ServerEdit::default()
            },
        });
        assert_eq!(failed(&refused), Some(NOT_ALLOWED));
        assert!(failed(&world.manage_now(Command::LeaveServer { server: owned })).is_some());
    }

    #[test]
    fn announcements_are_read_only_until_a_role_lets_the_account_write() {
        let mut world = world();
        let (_, other) = servers(&world);
        let announcements = world
            .channels
            .iter()
            .find(|channel| channel.server == Some(other) && channel.name == "anuncios");
        let Some(announcements) = announcements.cloned() else {
            return;
        };
        assert!(!announcements.permissions.contains(Permissions::SEND));
        assert!(announcements.permissions.contains(Permissions::VIEW));
        // The owner gives the account the moderation role (done here directly:
        // the account itself may not).
        let moderation = RoleId(other.0 * 10 + 1);
        world
            .members
            .iter_mut()
            .find(|member| member.server == other && member.user == ME)
            .unwrap()
            .roles
            .push(moderation);
        let changed = world.refresh_permissions(other);
        assert!(changed.iter().any(|event| matches!(
            event,
            Event::ChannelChanged(channel)
                if channel.id == announcements.id && channel.permissions.contains(Permissions::SEND)
        )));
    }

    #[test]
    fn roles_are_made_edited_given_and_deleted() {
        let mut world = world();
        let (owned, _) = servers(&world);
        let made = world.manage_now(Command::CreateRole {
            request: 7,
            server: owned,
            name: "Diseño".into(),
        });
        let Some(Event::Created {
            request: 7,
            created: Created::Role(_, role),
        }) = made.last()
        else {
            panic!("{made:?}");
        };
        let role = *role;
        let edited = world.manage_now(Command::EditRole {
            server: owned,
            role,
            edit: RoleEdit {
                color: Some(Some(rune_model::Rgb(0x11_22_33))),
                hoisted: Some(true),
                permissions: Some(Permissions::MANAGE_MESSAGES),
                position: Some(99),
                ..RoleEdit::default()
            },
        });
        assert!(failed(&edited).is_none(), "{edited:?}");
        let server = world.server(owned).unwrap();
        let top = server.assignable_roles()[0];
        assert_eq!(top.id, role, "moved to the top");
        let someone = world
            .members
            .iter()
            .find(|member| member.server == owned && member.user != ME)
            .unwrap()
            .user;
        let given = world.manage_now(Command::SetMemberRoles {
            server: owned,
            user: someone,
            roles: vec![role],
        });
        assert!(
            matches!(&given[0], Event::MemberChanged(member) if member.roles == vec![role]),
            "{given:?}"
        );
        let deleted = world.manage_now(Command::DeleteRole {
            server: owned,
            role,
        });
        assert!(deleted.iter().any(
            |event| matches!(event, Event::MemberChanged(member) if member.user == someone && member.roles.is_empty())
        ));
        assert!(failed(&world.manage_now(Command::DeleteRole {
            server: owned,
            role: RoleId(owned.0),
        }))
        .is_some());
    }

    #[test]
    fn a_private_channel_hides_from_everyone_but_its_maker() {
        let mut world = world();
        let (owned, _) = servers(&world);
        let made = world.manage_now(Command::CreateChannel {
            request: 1,
            server: owned,
            kind: ChannelKind::Text,
            name: "Sala Secreta".into(),
            parent: None,
            private: true,
        });
        let Some(Event::ChannelChanged(channel)) = made.first() else {
            panic!("{made:?}");
        };
        assert_eq!(channel.name, "sala-secreta");
        let server = world.server(owned).unwrap().clone();
        let someone = Member {
            user: UserId(10),
            server: owned,
            nick: None,
            roles: Vec::new(),
            ..Default::default()
        };
        assert_eq!(
            permissions::in_channel(&server, &someone, channel),
            Permissions::NONE
        );
        assert!(channel.permissions.contains(Permissions::VIEW));
    }

    #[test]
    fn a_ban_removes_and_keeps_out_until_lifted() {
        let mut world = world();
        let (owned, _) = servers(&world);
        let someone = world
            .members
            .iter()
            .find(|member| member.server == owned && member.user != ME)
            .unwrap()
            .user;
        let banned = world.manage_now(Command::Ban {
            server: owned,
            user: someone,
            reason: Some("spam".into()),
            delete_message_seconds: 3600,
        });
        assert!(banned
            .iter()
            .any(|event| matches!(event, Event::MemberRemoved { user, .. } if *user == someone)));
        assert!(banned
            .iter()
            .any(|event| matches!(event, Event::BanAdded { ban, .. } if ban.user == someone)));
        let listed = world.manage_now(Command::LoadBans {
            request: 3,
            server: owned,
            after: None,
            limit: 100,
        });
        assert!(
            matches!(&listed[0], Event::BansLoaded { bans, reached_end: true, .. }
            if bans.iter().any(|ban| ban.user == someone))
        );
        let lifted = world.manage_now(Command::Unban {
            server: owned,
            user: someone,
        });
        assert_eq!(
            lifted,
            vec![Event::BanRemoved {
                server: owned,
                user: someone
            }]
        );
        assert!(failed(&world.manage_now(Command::Kick {
            server: owned,
            user: ME,
            reason: None,
        }))
        .is_some());
    }

    #[test]
    fn a_new_server_comes_with_its_first_channels() {
        let mut world = world();
        let made = world.manage_now(Command::CreateServer {
            request: 9,
            name: "Mi servidor".into(),
        });
        let Some(Event::Created {
            created: Created::Server(server),
            ..
        }) = made.last()
        else {
            panic!("{made:?}");
        };
        let channels: Vec<&str> = made
            .iter()
            .filter_map(|event| match event {
                Event::ChannelChanged(channel) if channel.server == Some(*server) => {
                    Some(channel.name.as_str())
                }
                _ => None,
            })
            .collect();
        assert!(channels.contains(&"general") && channels.contains(&"General"));
        let deleted = world.manage_now(Command::DeleteServer { server: *server });
        assert!(matches!(deleted.last(), Some(Event::ServerRemoved { .. })));
    }

    #[test]
    fn the_profile_changes_and_says_what_is_too_long() {
        let mut world = world();
        let changed = world.manage_now(Command::EditProfile(ProfileEdit {
            pronouns: Some(Some("elle".into())),
            about: Some(Some("Hola".into())),
            ..ProfileEdit::default()
        }));
        assert!(
            matches!(&changed[0], Event::UserChanged(user) if user.pronouns.as_deref() == Some("elle"))
        );
        let refused = world.manage_now(Command::EditProfile(ProfileEdit {
            about: Some(Some("x".repeat(191))),
            ..ProfileEdit::default()
        }));
        assert!(failed(&refused).is_some());
    }

    #[test]
    fn the_custom_status_is_kept_trimmed_and_refused_when_too_long() {
        let mut world = world();
        let set = world.manage_now(Command::SetCustomStatus {
            text: Some("  de viaje ✈️ ".into()),
            emoji: None,
            until: None,
        });
        assert!(
            matches!(&set[..], [Event::PresenceChanged { user: ME, presence }]
                if presence.custom.as_deref() == Some("de viaje ✈️")
                    && presence.status == rune_model::Status::Online),
            "{set:?}"
        );
        let refused = world.manage_now(Command::SetCustomStatus {
            text: Some("x".repeat(CUSTOM_STATUS_MAX + 1)),
            emoji: None,
            until: None,
        });
        assert!(failed(&refused).is_some());
        assert_eq!(world.presences[&ME].custom.as_deref(), Some("de viaje ✈️"));
        let cleared = world.manage_now(Command::SetCustomStatus {
            text: Some("   ".into()),
            emoji: None,
            until: None,
        });
        assert!(
            matches!(&cleared[..], [Event::PresenceChanged { presence, .. }] if presence.custom.is_none())
        );
    }

    fn named(world: &World, server: ServerId, name: &str) -> ChannelId {
        world
            .channels
            .iter()
            .find(|channel| channel.server == Some(server) && channel.name == name)
            .unwrap()
            .id
    }

    fn pins_of(world: &mut World, channel: ChannelId) -> Vec<rune_model::Message> {
        match world
            .manage_now(Command::LoadPins {
                request: 4,
                channel,
            })
            .remove(0)
        {
            Event::PinsLoaded {
                request: 4,
                messages,
                ..
            } => messages,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn messages_are_pinned_where_the_account_may_and_listed_newest_first() {
        let mut world = world();
        let (owned, other) = servers(&world);
        let core = named(&world, owned, "limn-core");
        let made = pins_of(&mut world, core);
        assert!(!made.is_empty(), "the busier channels start with pins");
        assert!(made.iter().all(|message| message.pinned));
        assert!(made.windows(2).all(|pair| pair[0].id > pair[1].id));
        let newest = world.history(core, None, 1).0[0].id;
        let pinned = world.manage_now(Command::PinMessage {
            channel: core,
            message: newest,
        });
        assert!(
            matches!(&pinned[..], [Event::MessageUpdated(message)] if message.pinned && message.id == newest)
        );
        assert_eq!(pins_of(&mut world, core)[0].id, newest);
        assert!(
            world.history(core, None, 1).0[0].pinned,
            "history says so too"
        );
        let unpinned = world.manage_now(Command::UnpinMessage {
            channel: core,
            message: newest,
        });
        assert!(matches!(&unpinned[..], [Event::MessageUpdated(message)] if !message.pinned));
        assert_eq!(pins_of(&mut world, core), made);
        // A plain member of someone else's server may not.
        let general = named(&world, other, "general");
        let theirs = world.history(general, None, 1).0[0].id;
        let refused = world.manage_now(Command::PinMessage {
            channel: general,
            message: theirs,
        });
        assert_eq!(
            failed(&refused),
            Some("no tienes permiso para fijar mensajes")
        );
        assert!(matches!(
            &refused[0],
            Event::Failed {
                failure: Failure::Pin { .. },
                ..
            }
        ));
        // There, the pin bit with the history pins; managing messages alone
        // does not, as in the official client.
        let pin_with = |world: &mut World, permissions: Permissions| {
            let found = world.channels.iter_mut().find(|found| found.id == general);
            found.unwrap().permissions = permissions;
            world.manage_now(Command::PinMessage {
                channel: general,
                message: theirs,
            })
        };
        let base = Permissions::MEMBER | Permissions::READ_HISTORY;
        let moderator = pin_with(&mut world, base | Permissions::MANAGE_MESSAGES);
        assert_eq!(
            failed(&moderator),
            Some("no tienes permiso para fijar mensajes")
        );
        let pinner = pin_with(&mut world, base | Permissions::PIN_MESSAGES);
        assert!(
            matches!(&pinner[..], [Event::MessageUpdated(message)] if message.pinned && message.id == theirs)
        );
    }

    #[test]
    fn moving_a_channel_makes_room_among_its_new_siblings() {
        let mut world = world();
        let (owned, _) = servers(&world);
        let order = |world: &World, parent: ChannelId| -> Vec<String> {
            let mut children: Vec<&Channel> = world
                .channels
                .iter()
                .filter(|channel| channel.parent == Some(parent))
                .collect();
            children.sort_by_key(|channel| (channel.position, channel.id));
            children
                .iter()
                .map(|channel| channel.name.clone())
                .collect()
        };
        let rendimiento = named(&world, owned, "rendimiento");
        let core = named(&world, owned, "limn-core");
        let development = world.channel(core).unwrap().parent.unwrap();
        let voice = named(&world, owned, "Voz");
        let moved = world.manage_now(Command::EditChannel {
            channel: rendimiento,
            edit: ChannelEdit {
                position: Some(0),
                ..ChannelEdit::default()
            },
        });
        assert_eq!(moved.len(), 3, "it and the two it passed: {moved:?}");
        assert_eq!(
            order(&world, development),
            ["rendimiento", "limn-core", "componentes"]
        );
        world.manage_now(Command::EditChannel {
            channel: core,
            edit: ChannelEdit {
                parent: Some(Some(voice)),
                position: Some(1),
                ..ChannelEdit::default()
            },
        });
        assert_eq!(order(&world, voice), ["Reunión semanal", "limn-core"]);
        assert_eq!(order(&world, development), ["rendimiento", "componentes"]);
        // Categories move among categories, never inside one.
        world.manage_now(Command::EditChannel {
            channel: voice,
            edit: ChannelEdit {
                position: Some(0),
                ..ChannelEdit::default()
            },
        });
        assert!(
            world.channel(voice).unwrap().position < world.channel(development).unwrap().position
        );
        assert!(failed(&world.manage_now(Command::EditChannel {
            channel: voice,
            edit: ChannelEdit {
                parent: Some(Some(development)),
                ..ChannelEdit::default()
            },
        }))
        .is_some());
    }
}
