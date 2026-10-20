//! Servers, channels, members and people changing while a session runs:
//! created, edited, removed. Nothing here is shown before the provider
//! confirms it; a refusal arrives as `Event::Failed` and changes nothing.

use rune_model::{
    permissions, Channel, ChannelId, Member, Overwrite, OverwriteTarget, Permissions, RoleId,
    ServerId, User, UserId,
};
use rune_provider::Event;

use crate::{Applied, Store};

impl Store {
    /// Apply a change to a server, a channel, a member, a person or a ban list.
    pub(crate) fn apply_management(&mut self, event: Event) -> Applied {
        match event {
            Event::ServerChanged(server) => {
                let id = server.id;
                match self.servers.iter_mut().find(|known| known.id == id) {
                    Some(known) => *known = server,
                    None => self.servers.push(server),
                }
                self.reapply_expressions(id);
                Applied::Server(id)
            }
            Event::ServerRemoved { server, reason } => {
                self.forget_server(server);
                Applied::ServerRemoved { server, reason }
            }
            Event::ChannelChanged(channel) => {
                let id = channel.id;
                self.channels.insert(id, channel);
                if !self.abilities(id).is_some_and(|a| a.view_pins.allowed()) {
                    self.forget_pins(id);
                }
                if !self.abilities(id).is_some_and(|a| a.read_history.allowed()) {
                    self.forget_context(id);
                }
                Applied::Channel(id)
            }
            Event::ChannelRemoved { channel } => {
                self.forget_channel(channel);
                Applied::ChannelRemoved(channel)
            }
            Event::MemberChanged(member) => {
                let server = member.server;
                match self.member_index.get(&(server, member.user)).copied() {
                    Some(index) => {
                        if let Some(known) = self
                            .members
                            .get_mut(&server)
                            .and_then(|members| members.get_mut(index))
                        {
                            *known = member;
                        }
                    }
                    None => {
                        // Members with an order or a filter (2026-10-19).
                        self.list_newcomer(&member);
                        let members = self.members.entry(server).or_default();
                        self.member_index
                            .insert((server, member.user), members.len());
                        members.push(member);
                    }
                }
                Applied::Members(server)
            }
            Event::MemberRemoved { server, user } => {
                if let Some(members) = self.members.get_mut(&server) {
                    members.retain(|member| member.user != user);
                }
                self.index_members(server);
                self.voice.retain(|state| {
                    !(state.user == user
                        && self
                            .channels
                            .get(&state.channel)
                            .is_some_and(|channel| channel.server == Some(server)))
                });
                Applied::Members(server)
            }
            Event::UserChanged(mut user) => {
                let id = user.id;
                if let Some(previous) = self.users.get(&id) {
                    user.banner.inherit_unknown(&previous.banner);
                }
                self.users.insert(id, user);
                Applied::User(id)
            }
            event @ (Event::BansLoaded { .. }
            | Event::BanAdded { .. }
            | Event::BanRemoved { .. }
            | Event::AuditLoaded { .. }
            | Event::InvitesLoaded { .. }
            | Event::InviteRemoved { .. }) => self.apply_admin(event),
            Event::Created { request, created } => Applied::Created { request, created },
            _ => Applied::Nothing,
        }
    }

    fn index_members(&mut self, server: ServerId) {
        self.member_index.retain(|(of, _), _| *of != server);
        if let Some(members) = self.members.get(&server) {
            for (index, member) in members.iter().enumerate() {
                self.member_index.insert((server, member.user), index);
            }
        }
    }

    fn forget_channel(&mut self, channel: ChannelId) {
        self.channels.remove(&channel);
        self.timelines.remove(&channel);
        // An absent timeline cannot confirm, retry or discard these sends.
        self.pending.retain(|_, (known, _)| *known != channel);
        self.pending_files
            .retain(|nonce, _| self.pending.contains_key(nonce));
        self.forget_read_channel(channel);
        self.typing.remove(&channel);
        self.forget_pins(channel);
        self.forget_context(channel);
        self.voice.retain(|state| state.channel != channel);
        if self
            .call
            .as_ref()
            .is_some_and(|call| call.channel == channel)
        {
            self.call = None;
            self.call_connected = None;
        }
    }

    fn forget_server(&mut self, server: ServerId) {
        self.servers.retain(|known| known.id != server);
        let channels: Vec<ChannelId> = self
            .channels
            .values()
            .filter(|channel| channel.server == Some(server))
            .map(|channel| channel.id)
            .collect();
        for channel in channels {
            self.forget_channel(channel);
        }
        self.members.remove(&server);
        self.index_members(server);
        self.forget_admin(Some(server));
    }

    /// The people of `server`, as the provider listed them.
    pub fn members(&self, server: ServerId) -> &[Member] {
        self.members.get(&server).map_or(&[], Vec::as_slice)
    }

    /// `user` as a member of `server`.
    pub fn member_of(&self, server: ServerId, user: UserId) -> Option<&Member> {
        self.member(server, user)
    }

    /// Whether the account owns `server`.
    pub fn owns(&self, server: ServerId) -> bool {
        self.me.is_some()
            && self
                .server(server)
                .is_some_and(|found| found.owner == self.me)
    }

    /// What the account may do across `server`, before channel overwrites.
    pub fn server_permissions(&self, server: ServerId) -> Permissions {
        let (Some(found), Some(me)) = (self.server(server), self.me) else {
            return Permissions::NONE;
        };
        self.member(server, me).map_or(Permissions::NONE, |member| {
            permissions::in_server(found, member)
        })
    }

    /// The position of the account's highest role in `server`; above every
    /// role for the owner. Roles at or above it are out of the account's reach.
    pub fn my_rank(&self, server: ServerId) -> i32 {
        if self.owns(server) {
            return i32::MAX;
        }
        let (Some(found), Some(me)) = (self.server(server), self.me) else {
            return i32::MIN;
        };
        let Some(member) = self.member(server, me) else {
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

    /// The highest position among `user`'s roles in `server`.
    pub fn rank_of(&self, server: ServerId, user: UserId) -> i32 {
        let (Some(found), Some(member)) = (self.server(server), self.member(server, user)) else {
            return 0;
        };
        found
            .roles
            .iter()
            .filter(|role| member.roles.contains(&role.id))
            .map(|role| role.position)
            .max()
            .unwrap_or(0)
    }

    /// Whether the account sees `channel`: a channel it may not view is
    /// left out of the lists, and so is a private thread it is not in,
    /// unless it moderates threads.
    pub fn visible(channel: &Channel) -> bool {
        let private = channel.thread.as_ref().is_some_and(|thread| {
            thread.private
                && !thread.joined
                && !channel.permissions.contains(Permissions::MANAGE_THREADS)
        });
        (channel.server.is_none() || channel.permissions.contains(Permissions::VIEW)) && !private
    }

    /// `@everyone` of `channel`'s server, as the target of an overwrite.
    pub fn everyone(channel: &Channel) -> OverwriteTarget {
        OverwriteTarget::Role(RoleId(channel.server.map_or(0, |server| server.0)))
    }

    /// Whether `everyone`, the overwrite of `@everyone`, makes its channel
    /// private: it takes seeing the channel away.
    pub fn private_by(everyone: &Overwrite) -> bool {
        everyone.state(Permissions::VIEW) == Some(false)
    }

    /// Whether `channel` is private as the provider has it: `@everyone` may
    /// not see it.
    pub fn private(channel: &Channel) -> bool {
        let everyone = Self::everyone(channel);
        channel
            .overwrites
            .iter()
            .any(|overwrite| overwrite.target == everyone && Self::private_by(overwrite))
    }

    /// The account's own user, as the provider last reported it.
    pub fn me_user(&self) -> Option<&User> {
        self.me.and_then(|me| self.user(me))
    }
}
