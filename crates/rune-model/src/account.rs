//! What the account keeps with its provider beside its profile: how each
//! server and channel notifies, how the rail groups servers in folders, who
//! may write to it, and the private notes it keeps on people. A provider
//! reports these as the service has them and RUNE asks for changes; nothing
//! here is a protocol.

use serde::{Deserialize, Serialize};

use crate::{ChannelId, Rgb, ServerId, Timestamp};

/// How much something tells of what is new.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum NotifyLevel {
    /// Everything unread is marked.
    #[default]
    All,
    /// Only mentions are marked.
    Mentions,
    /// Nothing is marked.
    Nothing,
}

impl NotifyLevel {
    /// The word its action and its kept setting end with.
    pub fn key(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Mentions => "mentions",
            Self::Nothing => "nothing",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        [Self::All, Self::Mentions, Self::Nothing]
            .into_iter()
            .find(|level| level.key() == key)
    }
}

/// Muted until turned back on.
pub const MUTED_FOREVER: Timestamp = Timestamp(i64::MAX);

/// How a server notifies, or the direct conversations (`server: None`), as
/// the account keeps it with the provider.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Notifications {
    pub server: Option<ServerId>,
    /// `None` takes the server's own default.
    pub level: Option<NotifyLevel>,
    /// Muted until then; [`MUTED_FOREVER`] until turned back on.
    pub muted_until: Option<Timestamp>,
    /// `@everyone` and `@here` do not count as mentions.
    pub suppress_everyone: bool,
    /// Mentions of the account's roles do not count as mentions.
    pub suppress_roles: bool,
    /// Muted channels are left out of the list.
    pub hide_muted: bool,
    /// The channels and categories set apart from the server.
    pub channels: Vec<ChannelNotifications>,
}

/// How one channel or category notifies, set apart from its server.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ChannelNotifications {
    pub channel: ChannelId,
    /// `None` takes the one above it: its category's, then the server's.
    pub level: Option<NotifyLevel>,
    pub muted_until: Option<Timestamp>,
}

/// One change to the notifications of a server, or of the direct
/// conversations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotificationChange {
    Level(NotifyLevel),
    /// Muted until then, [`MUTED_FOREVER`] for good, `None` to turn it back on.
    Mute(Option<Timestamp>),
    SuppressEveryone(bool),
    SuppressRoles(bool),
    HideMuted(bool),
    /// `None` takes the one above it again.
    ChannelLevel(ChannelId, Option<NotifyLevel>),
    ChannelMute(ChannelId, Option<Timestamp>),
}

impl Notifications {
    /// The settings of `channel`, or none of its own.
    pub fn channel(&self, channel: ChannelId) -> ChannelNotifications {
        self.channels
            .iter()
            .find(|found| found.channel == channel)
            .copied()
            .unwrap_or(ChannelNotifications {
                channel,
                ..ChannelNotifications::default()
            })
    }

    /// What `change` leaves, as a service keeps it: a channel back to
    /// nothing of its own is dropped.
    pub fn apply(&mut self, change: NotificationChange) {
        let mut channel = |id: ChannelId, edit: &dyn Fn(&mut ChannelNotifications)| {
            let mut entry = self.channel(id);
            edit(&mut entry);
            self.channels.retain(|found| found.channel != id);
            if entry.level.is_some() || entry.muted_until.is_some() {
                self.channels.push(entry);
            }
        };
        match change {
            NotificationChange::Level(level) => self.level = Some(level),
            NotificationChange::Mute(until) => self.muted_until = until,
            NotificationChange::SuppressEveryone(on) => self.suppress_everyone = on,
            NotificationChange::SuppressRoles(on) => self.suppress_roles = on,
            NotificationChange::HideMuted(on) => self.hide_muted = on,
            NotificationChange::ChannelLevel(id, level) => {
                channel(id, &|entry| entry.level = level)
            }
            NotificationChange::ChannelMute(id, until) => {
                channel(id, &|entry| entry.muted_until = until)
            }
        }
    }
}

/// The longest name of a folder of servers.
pub const FOLDER_NAME_MAX: usize = 100;
/// The most entries and servers a layout of folders holds: the service keeps
/// servers left long ago in it too.
pub const FOLDERS_MAX: usize = 1000;

/// One entry of the rail: a folder of servers (`id` set), or a server on its
/// own (`id: None`, one server).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ServerFolder {
    pub id: Option<u64>,
    pub name: Option<String>,
    pub color: Option<Rgb>,
    pub servers: Vec<ServerId>,
}

impl ServerFolder {
    /// A server on its own.
    pub fn single(server: ServerId) -> Self {
        Self {
            servers: vec![server],
            ..Self::default()
        }
    }
}

/// Whether `folders` is a layout a service keeps: each server once, folder
/// ids unique and not zero, a lone server with exactly one, names bounded and
/// printable, colors in 0xRRGGBB.
pub fn valid_folders(folders: &[ServerFolder]) -> bool {
    let mut servers = std::collections::HashSet::new();
    let mut ids = std::collections::HashSet::new();
    folders.len() <= FOLDERS_MAX
        && folders.iter().all(|folder| {
            folder.id.is_none_or(|id| id != 0 && ids.insert(id))
                && (folder.id.is_some() || folder.servers.len() == 1)
                && folder.color.is_none_or(|color| color.0 <= 0xFF_FF_FF)
                && folder.name.as_ref().is_none_or(|name| {
                    name.chars().count() <= FOLDER_NAME_MAX && !name.chars().any(char::is_control)
                })
                && folder
                    .servers
                    .iter()
                    .all(|server| server.0 != 0 && servers.insert(*server))
        })
        && servers.len() <= FOLDERS_MAX
}

/// Which direct messages are checked for spam.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum SpamFilter {
    /// Never set: the service's default, which checks those of non-friends.
    #[default]
    Default,
    Off,
    NonFriends,
    Everyone,
}

/// What is done with an image the service takes to be explicit.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ContentFilter {
    /// Never set: the service's default.
    #[default]
    Default,
    Show,
    Blur,
    Block,
}

/// Who may send the account a friend request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FriendSources {
    pub everyone: bool,
    pub friends_of_friends: bool,
    pub server_members: bool,
}

impl Default for FriendSources {
    fn default() -> Self {
        Self {
            everyone: true,
            friends_of_friends: true,
            server_members: true,
        }
    }
}

/// The most servers each list of exceptions holds.
pub const PRIVACY_SERVERS_MAX: usize = 4096;

/// Who may write to the account and what is filtered, as it keeps it with
/// the provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagingPrivacy {
    /// Members of a server joined from now on may write directly.
    pub server_dms: bool,
    /// Servers whose members may not, sorted.
    pub dms_off: Vec<ServerId>,
    /// Messages of server members one may not know go to the requests,
    /// in servers joined from now on.
    pub request_filter: bool,
    /// Servers whose members' messages are not filtered, sorted.
    pub requests_off: Vec<ServerId>,
    pub spam: SpamFilter,
    /// Explicit images in servers, from friends and from others.
    pub explicit_servers: ContentFilter,
    pub explicit_friends: ContentFilter,
    pub explicit_others: ContentFilter,
    pub friend_sources: FriendSources,
}

impl Default for MessagingPrivacy {
    fn default() -> Self {
        Self {
            server_dms: true,
            dms_off: Vec::new(),
            request_filter: true,
            requests_off: Vec::new(),
            spam: SpamFilter::Default,
            explicit_servers: ContentFilter::Default,
            explicit_friends: ContentFilter::Default,
            explicit_others: ContentFilter::Default,
            friend_sources: FriendSources::default(),
        }
    }
}

/// One change to the account's messaging privacy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivacyChange {
    ServerDms(bool),
    /// Whether members of this one server may write directly.
    ServerDmsIn(ServerId, bool),
    RequestFilter(bool),
    /// Whether this one server's members are filtered to the requests.
    RequestFilterIn(ServerId, bool),
    Spam(SpamFilter),
    ExplicitServers(ContentFilter),
    ExplicitFriends(ContentFilter),
    ExplicitOthers(ContentFilter),
    FriendSources(FriendSources),
}

impl MessagingPrivacy {
    /// Whether members of `server` may write directly.
    pub fn dms_in(&self, server: ServerId) -> bool {
        self.dms_off.binary_search(&server).is_err()
    }

    /// Whether `server`'s members are filtered to the requests.
    pub fn requests_in(&self, server: ServerId) -> bool {
        self.requests_off.binary_search(&server).is_err()
    }

    pub fn apply(&mut self, change: PrivacyChange) {
        fn set(list: &mut Vec<ServerId>, server: ServerId, on: bool) {
            match list.binary_search(&server) {
                Ok(index) if on => {
                    list.remove(index);
                }
                Err(index) if !on => list.insert(index, server),
                _ => {}
            }
        }
        match change {
            PrivacyChange::ServerDms(on) => self.server_dms = on,
            PrivacyChange::ServerDmsIn(server, on) => set(&mut self.dms_off, server, on),
            PrivacyChange::RequestFilter(on) => self.request_filter = on,
            PrivacyChange::RequestFilterIn(server, on) => set(&mut self.requests_off, server, on),
            PrivacyChange::Spam(filter) => self.spam = filter,
            PrivacyChange::ExplicitServers(filter) => self.explicit_servers = filter,
            PrivacyChange::ExplicitFriends(filter) => self.explicit_friends = filter,
            PrivacyChange::ExplicitOthers(filter) => self.explicit_others = filter,
            PrivacyChange::FriendSources(sources) => self.friend_sources = sources,
        }
    }

    /// Lists sorted, without repeats or zero, and bounded.
    pub fn valid(&self) -> bool {
        [&self.dms_off, &self.requests_off].into_iter().all(|list| {
            list.len() <= PRIVACY_SERVERS_MAX
                && list.iter().all(|server| server.0 != 0)
                && list.windows(2).all(|pair| pair[0] < pair[1])
        })
    }
}

/// The longest private note on someone.
pub const NOTE_MAX: usize = 256;

/// Whether `note` can be kept: bounded, and no control characters but line
/// breaks and tabs.
pub fn valid_note(note: &str) -> bool {
    note.chars().count() <= NOTE_MAX
        && !note
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_channel_back_to_nothing_of_its_own_is_dropped() {
        let mut settings = Notifications::default();
        settings.apply(NotificationChange::ChannelMute(
            ChannelId(4),
            Some(MUTED_FOREVER),
        ));
        settings.apply(NotificationChange::ChannelLevel(
            ChannelId(4),
            Some(NotifyLevel::Mentions),
        ));
        assert_eq!(
            settings.channel(ChannelId(4)),
            ChannelNotifications {
                channel: ChannelId(4),
                level: Some(NotifyLevel::Mentions),
                muted_until: Some(MUTED_FOREVER),
            }
        );
        settings.apply(NotificationChange::ChannelMute(ChannelId(4), None));
        settings.apply(NotificationChange::ChannelLevel(ChannelId(4), None));
        assert!(settings.channels.is_empty());
    }

    #[test]
    fn folder_layouts_keep_each_server_once() {
        let folder = ServerFolder {
            id: Some(7),
            name: Some("Juegos".into()),
            color: Some(Rgb(0x5865F2)),
            servers: vec![ServerId(1), ServerId(2)],
        };
        assert!(valid_folders(&[
            folder.clone(),
            ServerFolder::single(ServerId(3))
        ]));
        assert!(!valid_folders(&[
            folder.clone(),
            ServerFolder::single(ServerId(2))
        ]));
        assert!(!valid_folders(&[ServerFolder {
            id: None,
            ..folder.clone()
        }]));
        assert!(!valid_folders(&[ServerFolder {
            name: Some("a\nb".into()),
            ..folder.clone()
        }]));
        assert!(!valid_folders(&[folder.clone(), folder]));
    }

    #[test]
    fn privacy_exceptions_stay_sorted() {
        let mut privacy = MessagingPrivacy::default();
        for server in [9, 3, 5] {
            privacy.apply(PrivacyChange::ServerDmsIn(ServerId(server), false));
        }
        privacy.apply(PrivacyChange::ServerDmsIn(ServerId(5), true));
        assert_eq!(privacy.dms_off, vec![ServerId(3), ServerId(9)]);
        assert!(!privacy.dms_in(ServerId(3)) && privacy.dms_in(ServerId(5)));
        assert!(privacy.valid());
        assert!(valid_note("línea\notra") && !valid_note("\u{7}"));
        assert!(!valid_note(&"x".repeat(NOTE_MAX + 1)));
    }
}
