//! What an account may do, and how a server's roles and a channel's
//! overwrites decide it. The rules are the usual ones of chat servers with
//! roles: a member has what `@everyone` and each of their roles grant; the
//! owner and administrators have everything; in a channel, overwrites for
//! `@everyone`, then for the member's roles, then for the member, deny and
//! allow on top of that. A provider may compute permissions itself and send
//! them in [`Channel::permissions`](crate::Channel); the simulation uses
//! [`in_channel`].

use serde::{Deserialize, Serialize};

use crate::{Channel, Member, RoleId, Server, UserId};

/// What an account may do, as bits.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Permissions(pub u64);

impl Permissions {
    pub const NONE: Self = Self(0);
    pub const VIEW: Self = Self(1);
    pub const SEND: Self = Self(1 << 1);
    pub const REACT: Self = Self(1 << 2);
    pub const ATTACH: Self = Self(1 << 3);
    pub const MANAGE_MESSAGES: Self = Self(1 << 4);
    pub const CONNECT: Self = Self(1 << 5);
    pub const SPEAK: Self = Self(1 << 6);
    pub const MANAGE_CHANNELS: Self = Self(1 << 7);
    pub const MANAGE_ROLES: Self = Self(1 << 8);
    pub const MANAGE_SERVER: Self = Self(1 << 9);
    pub const KICK: Self = Self(1 << 10);
    pub const BAN: Self = Self(1 << 11);
    pub const CREATE_INVITE: Self = Self(1 << 12);
    pub const CHANGE_NICK: Self = Self(1 << 13);
    pub const MANAGE_NICKS: Self = Self(1 << 14);
    pub const READ_HISTORY: Self = Self(1 << 15);
    pub const MENTION_EVERYONE: Self = Self(1 << 16);
    pub const EMBED_LINKS: Self = Self(1 << 17);
    pub const MUTE_MEMBERS: Self = Self(1 << 18);
    pub const DEAFEN_MEMBERS: Self = Self(1 << 19);
    pub const MOVE_MEMBERS: Self = Self(1 << 20);
    pub const STREAM: Self = Self(1 << 21);
    pub const VIEW_AUDIT_LOG: Self = Self(1 << 22);
    pub const MANAGE_EMOJI: Self = Self(1 << 23);
    pub const EXTERNAL_EMOJI: Self = Self(1 << 24);
    pub const USE_VOICE_ACTIVITY: Self = Self(1 << 25);
    pub const PRIORITY_SPEAKER: Self = Self(1 << 26);
    pub const TIMEOUT_MEMBERS: Self = Self(1 << 27);
    pub const MANAGE_THREADS: Self = Self(1 << 28);
    pub const SEND_IN_THREADS: Self = Self(1 << 29);
    /// Start threads everyone in the channel sees, and forum posts' threads.
    pub const CREATE_THREADS: Self = Self(1 << 30);
    /// Every permission, in every channel, whatever the overwrites say.
    pub const ADMINISTRATOR: Self = Self(1 << 31);
    /// Start threads only those invited and moderation see.
    pub const CREATE_PRIVATE_THREADS: Self = Self(1 << 32);
    /// Raise a hand to speak on a stage.
    pub const REQUEST_TO_SPEAK: Self = Self(1 << 33);
    /// Make, rename, move and delete a server's webhooks.
    pub const MANAGE_WEBHOOKS: Self = Self(1 << 34);
    /// Run apps' commands and use their buttons and menus (P3).
    pub const USE_APP_COMMANDS: Self = Self(1 << 35);
    // Message actions and moderation (P4).
    /// Start polls in a channel; sending them needs `SEND` too.
    pub const SEND_POLLS: Self = Self(1 << 36);
    // Reactions, pins and search (2026-10-18).
    /// Pin and unpin messages, apart from managing them; the official client
    /// asks for it with [`READ_HISTORY`](Self::READ_HISTORY), not for
    /// [`MANAGE_MESSAGES`](Self::MANAGE_MESSAGES).
    pub const PIN_MESSAGES: Self = Self(1 << 37);
    // Emoji and states (2026-10-18).
    /// Send another server's stickers here; its emoji are `EXTERNAL_EMOJI`.
    pub const EXTERNAL_STICKERS: Self = Self(1 << 38);
    // Scheduled events (eventos-programados).
    /// Plan events, and edit or delete those the account planned.
    pub const CREATE_EVENTS: Self = Self(1 << 39);
    /// Edit, start, end and delete anyone's events.
    pub const MANAGE_EVENTS: Self = Self(1 << 40);
    pub const ALL: Self = Self((1 << 41) - 1);
    /// Everything a member of a channel usually has.
    pub const MEMBER: Self = Self(
        Self::VIEW.0
            | Self::SEND.0
            | Self::REACT.0
            | Self::ATTACH.0
            | Self::CONNECT.0
            | Self::SPEAK.0,
    );
    /// What `@everyone` has in a new server.
    pub const EVERYONE: Self = Self(
        Self::MEMBER.0
            | Self::READ_HISTORY.0
            | Self::CREATE_INVITE.0
            | Self::CHANGE_NICK.0
            | Self::EMBED_LINKS.0
            | Self::EXTERNAL_EMOJI.0
            | Self::USE_VOICE_ACTIVITY.0
            | Self::STREAM.0
            | Self::SEND_IN_THREADS.0
            | Self::CREATE_THREADS.0
            | Self::CREATE_PRIVATE_THREADS.0
            | Self::REQUEST_TO_SPEAK.0
            | Self::USE_APP_COMMANDS.0
            | Self::SEND_POLLS.0
            | Self::EXTERNAL_STICKERS.0,
    );

    /// Whether every bit of `other` is set.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether any bit of `other` is set.
    pub fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// These, with `other` added.
    pub fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// These, with `other` taken out.
    pub fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// These, with `other` set to `on`.
    pub fn set(self, other: Self, on: bool) -> Self {
        if on {
            self.with(other)
        } else {
            self.without(other)
        }
    }
}

impl std::ops::BitOr for Permissions {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        self.with(other)
    }
}

impl std::ops::BitAnd for Permissions {
    type Output = Self;
    fn bitand(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

/// Whom a channel overwrite is for.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum OverwriteTarget {
    /// A role; `@everyone` is the role whose id is the server's.
    Role(RoleId),
    Member(UserId),
}

/// What a channel grants or takes away from a role or a member, on top of
/// what the server gives them. A bit in neither set is left as the server has
/// it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Overwrite {
    pub target: OverwriteTarget,
    pub allow: Permissions,
    pub deny: Permissions,
}

impl Overwrite {
    /// What the overwrite says about `permission`: `Some(true)` allowed,
    /// `Some(false)` denied, `None` left as the server has it.
    pub fn state(&self, permission: Permissions) -> Option<bool> {
        if self.allow.contains(permission) {
            Some(true)
        } else if self.deny.contains(permission) {
            Some(false)
        } else {
            None
        }
    }

    /// The overwrite with `permission` allowed, denied, or left alone.
    pub fn with_state(mut self, permission: Permissions, state: Option<bool>) -> Self {
        self.allow = self.allow.set(permission, state == Some(true));
        self.deny = self.deny.set(permission, state == Some(false));
        self
    }

    pub fn is_empty(&self) -> bool {
        self.allow.is_empty() && self.deny.is_empty()
    }
}

/// What `member` may do across `server`, before any channel's overwrites.
pub fn in_server(server: &Server, member: &Member) -> Permissions {
    if server.owner == Some(member.user) {
        return Permissions::ALL;
    }
    let everyone = server
        .everyone()
        .map_or(Permissions::EVERYONE, |role| role.permissions);
    let granted = server
        .roles
        .iter()
        .filter(|role| member.roles.contains(&role.id))
        .fold(everyone, |granted, role| granted | role.permissions);
    if granted.contains(Permissions::ADMINISTRATOR) {
        Permissions::ALL
    } else {
        granted
    }
}

/// What `member` may do in `channel` of `server`.
pub fn in_channel(server: &Server, member: &Member, channel: &Channel) -> Permissions {
    let base = in_server(server, member);
    if base.contains(Permissions::ADMINISTRATOR) {
        return Permissions::ALL;
    }
    let find = |target| {
        channel
            .overwrites
            .iter()
            .find(|overwrite| overwrite.target == target)
    };
    let mut permissions = base;
    if let Some(everyone) = find(OverwriteTarget::Role(server.everyone_id())) {
        permissions = permissions.without(everyone.deny).with(everyone.allow);
    }
    let (mut allow, mut deny) = (Permissions::NONE, Permissions::NONE);
    for role in &member.roles {
        if let Some(overwrite) = find(OverwriteTarget::Role(*role)) {
            allow = allow | overwrite.allow;
            deny = deny | overwrite.deny;
        }
    }
    permissions = permissions.without(deny).with(allow);
    if let Some(own) = find(OverwriteTarget::Member(member.user)) {
        permissions = permissions.without(own.deny).with(own.allow);
    }
    // Someone who cannot see a channel can do nothing in it.
    if !permissions.contains(Permissions::VIEW) {
        return Permissions::NONE;
    }
    permissions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChannelId, ChannelKind, Rgb, Role, ServerId};

    const MOD: RoleId = RoleId(11);

    fn server() -> Server {
        Server {
            stickers: Vec::new(),
            emojis: Vec::new(),
            icon: None,
            id: ServerId(1),
            name: "taller".into(),
            accent: Rgb(0),
            roles: vec![
                Role::everyone(ServerId(1), Permissions::EVERYONE),
                Role {
                    style: Default::default(),
                    id: MOD,
                    name: "Moderación".into(),
                    color: None,
                    position: 1,
                    hoisted: true,
                    mentionable: false,
                    permissions: Permissions::MANAGE_MESSAGES | Permissions::KICK,
                },
            ],
            owner: Some(UserId(99)),
            description: None,
            options: Default::default(),
        }
    }

    fn member(user: u64, roles: &[RoleId]) -> Member {
        Member {
            user: UserId(user),
            server: ServerId(1),
            nick: None,
            roles: roles.to_vec(),
            ..Default::default()
        }
    }

    fn channel(overwrites: Vec<Overwrite>) -> Channel {
        Channel {
            id: ChannelId(5),
            server: Some(ServerId(1)),
            kind: ChannelKind::Text,
            name: "anuncios".into(),
            topic: None,
            topic_pictures: Vec::new(),
            parent: None,
            position: 0,
            recipients: Vec::new(),
            owner: None,
            permissions: Permissions::NONE,
            overwrites,
            slowmode_secs: 0,
            nsfw: false,
            icon: None,
            thread: None,
            tags: Vec::new(),
            forum: Default::default(),
        }
    }

    #[test]
    fn roles_add_to_everyone_and_the_owner_has_everything() {
        let server = server();
        let plain = in_server(&server, &member(1, &[]));
        assert_eq!(plain, Permissions::EVERYONE);
        let moderator = in_server(&server, &member(2, &[MOD]));
        assert!(moderator.contains(Permissions::KICK | Permissions::SEND));
        assert_eq!(in_server(&server, &member(99, &[])), Permissions::ALL);
    }

    #[test]
    fn overwrites_go_from_everyone_to_roles_to_the_member() {
        let server = server();
        let read_only = channel(vec![
            Overwrite {
                target: OverwriteTarget::Role(server.everyone_id()),
                allow: Permissions::NONE,
                deny: Permissions::SEND,
            },
            Overwrite {
                target: OverwriteTarget::Role(MOD),
                allow: Permissions::SEND,
                deny: Permissions::NONE,
            },
        ]);
        let plain = in_channel(&server, &member(1, &[]), &read_only);
        assert!(plain.contains(Permissions::VIEW) && !plain.contains(Permissions::SEND));
        assert!(in_channel(&server, &member(2, &[MOD]), &read_only).contains(Permissions::SEND));
        let mut muted = read_only.clone();
        muted.overwrites.push(Overwrite {
            target: OverwriteTarget::Member(UserId(2)),
            allow: Permissions::NONE,
            deny: Permissions::SEND,
        });
        assert!(
            !in_channel(&server, &member(2, &[MOD]), &muted).contains(Permissions::SEND),
            "the member's own overwrite has the last word"
        );
    }

    #[test]
    fn a_hidden_channel_allows_nothing_and_administrators_see_everything() {
        let mut server = server();
        let hidden = channel(vec![Overwrite {
            target: OverwriteTarget::Role(server.everyone_id()),
            allow: Permissions::NONE,
            deny: Permissions::VIEW,
        }]);
        assert_eq!(
            in_channel(&server, &member(1, &[]), &hidden),
            Permissions::NONE
        );
        server.roles[1].permissions = Permissions::ADMINISTRATOR;
        assert_eq!(
            in_channel(&server, &member(2, &[MOD]), &hidden),
            Permissions::ALL
        );
    }

    #[test]
    fn an_overwrite_states_each_permission() {
        let overwrite = Overwrite {
            target: OverwriteTarget::Member(UserId(1)),
            allow: Permissions::NONE,
            deny: Permissions::NONE,
        }
        .with_state(Permissions::SEND, Some(false))
        .with_state(Permissions::REACT, Some(true));
        assert_eq!(overwrite.state(Permissions::SEND), Some(false));
        assert_eq!(overwrite.state(Permissions::REACT), Some(true));
        assert_eq!(overwrite.state(Permissions::VIEW), None);
        assert!(overwrite
            .with_state(Permissions::SEND, None)
            .with_state(Permissions::REACT, None)
            .is_empty());
    }
}
