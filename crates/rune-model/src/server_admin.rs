//! The rest of a server's administration (P7): its safety and engagement
//! settings, the traits its profile shows, how its member pages are asked
//! for and how each member joined, and where its channels go when they are
//! reordered at once. Every field defaults, so a server kept before these
//! existed still reads.

use serde::{Deserialize, Serialize};

use crate::{ChannelId, EmojiId, RemoteImage, UserId};

/// Traits a server's profile shows, at most.
pub const MAX_TRAITS: usize = 5;
/// Characters of a trait's label, at most.
pub const MAX_TRAIT_LABEL: usize = 24;
/// The idle times after which someone in voice is moved to the idle
/// channel, in seconds, as services offer them.
pub const AFK_TIMEOUTS: [u32; 5] = [60, 300, 900, 1800, 3600];

/// What a server sets for everyone beyond its name and look.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerOptions {
    /// Where the service posts its own messages (welcomes, boosts, tips);
    /// none when `None`.
    #[serde(default)]
    pub system_channel: Option<ChannelId>,
    /// Which of those it posts there.
    #[serde(default)]
    pub system_messages: SystemMessages,
    /// New members are notified only when mentioned, not of every message.
    #[serde(default)]
    pub mentions_only: bool,
    /// Where people idle in voice are moved; none when `None`.
    #[serde(default)]
    pub afk_channel: Option<ChannelId>,
    /// Seconds idle before that: one of [`AFK_TIMEOUTS`].
    #[serde(default = "default_afk")]
    pub afk_timeout: u32,
    #[serde(default)]
    pub verification: Verification,
    #[serde(default)]
    pub media_scan: MediaScan,
    /// Nobody new joins through an invitation until it is resumed.
    #[serde(default)]
    pub invites_paused: bool,
    /// Its roles may show a picture or an emoji beside names.
    #[serde(default)]
    pub role_icons: bool,
    /// Its roles may show a gradient of colors.
    #[serde(default)]
    pub role_gradients: bool,
    /// A community: it keeps verification at least low and scans everyone's media.
    #[serde(default)]
    pub community: bool,
    /// What its profile says it is about; `None` until its profile is read.
    #[serde(default)]
    pub traits: Option<Vec<ServerTrait>>,
    // Rules and onboarding (2026-10-18).
    /// New members accept its rules before they take part.
    #[serde(default)]
    pub rules_screening: bool,
    /// New members are asked its onboarding's questions.
    #[serde(default)]
    pub onboarding: bool,
    // Icon, banner and community (2026-10-18).
    /// Its picture may move: a GIF chosen for it stays animated.
    #[serde(default)]
    pub animated_icon: bool,
    /// It may show a banner, and the banner may move.
    #[serde(default)]
    pub banner_allowed: bool,
    #[serde(default)]
    pub animated_banner: bool,
    /// Its banner, once it has one.
    #[serde(default)]
    pub banner: Option<RemoteImage>,
    /// A community's channel of rules, and the one where the service posts
    /// news for its moderation; none when `None`.
    #[serde(default)]
    pub rules_channel: Option<ChannelId>,
    #[serde(default)]
    pub updates_channel: Option<ChannelId>,
}

fn default_afk() -> u32 {
    300
}

impl Default for ServerOptions {
    fn default() -> Self {
        Self {
            system_channel: None,
            system_messages: SystemMessages::default(),
            mentions_only: false,
            afk_channel: None,
            afk_timeout: default_afk(),
            verification: Verification::default(),
            media_scan: MediaScan::default(),
            invites_paused: false,
            role_icons: false,
            role_gradients: false,
            community: false,
            traits: None,
            rules_screening: false,
            onboarding: false,
            animated_icon: false,
            banner_allowed: false,
            animated_banner: false,
            banner: None,
            rules_channel: None,
            updates_channel: None,
        }
    }
}

impl ServerOptions {
    /// Whether these options let a server become a community: a rules
    /// channel and an updates channel, at least low verification and
    /// everyone's media scanned.
    pub fn community_ready(&self) -> bool {
        self.rules_channel.is_some()
            && self.updates_channel.is_some()
            && self.verification != Verification::None
            && self.media_scan == MediaScan::Everyone
    }
}

/// Which of its own messages the service posts in the system channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemMessages {
    /// A welcome when someone joins.
    #[serde(default = "yes")]
    pub joins: bool,
    /// A thanks when someone boosts the server.
    #[serde(default = "yes")]
    pub boosts: bool,
    /// Tips to set the server up.
    #[serde(default = "yes")]
    pub tips: bool,
    /// A button to answer a welcome with a sticker.
    #[serde(default = "yes")]
    pub join_replies: bool,
}

fn yes() -> bool {
    true
}

impl Default for SystemMessages {
    fn default() -> Self {
        Self {
            joins: true,
            boosts: true,
            tips: true,
            join_replies: true,
        }
    }
}

/// What a member needs before writing, from nothing to a verified phone.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Verification {
    #[default]
    None,
    /// A verified email.
    Low,
    /// And an account older than five minutes.
    Medium,
    /// And ten minutes in the server.
    High,
    /// And a verified phone.
    Highest,
}

impl Verification {
    pub const ALL: [Self; 5] = [
        Self::None,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Highest,
    ];
}

/// Whose pictures and files the service scans for explicit content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaScan {
    #[default]
    Off,
    /// Members without a role.
    WithoutRoles,
    Everyone,
}

impl MediaScan {
    pub const ALL: [Self; 3] = [Self::Off, Self::WithoutRoles, Self::Everyone];
}

/// One thing a server's profile says it is about: a short label and an
/// optional emoji before it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerTrait {
    pub label: String,
    /// A Unicode emoji, or the name of the server's own one `emoji_id` names.
    #[serde(default)]
    pub emoji: Option<String>,
    // Server emoji (2026-10-18).
    /// One of the server's own emojis, by id, and whether it moves.
    #[serde(default)]
    pub emoji_id: Option<EmojiId>,
    #[serde(default)]
    pub emoji_animated: bool,
}

impl ServerTrait {
    /// Whether it can be saved: a label of 1 to [`MAX_TRAIT_LABEL`]
    /// characters on one line, and at most a short emoji (or the name of
    /// one of the server's own).
    pub fn valid(&self) -> bool {
        let label = self.label.trim();
        let longest = if self.emoji_id.is_some() { 32 } else { 8 };
        (1..=MAX_TRAIT_LABEL).contains(&label.chars().count())
            && !label.chars().any(char::is_control)
            && self.emoji.as_deref().is_none_or(|emoji| {
                (1..=longest).contains(&emoji.chars().count())
                    && !emoji.chars().any(char::is_control)
            })
    }
}

/// How a server's member pages are asked for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MemberQuery {
    pub order: MemberOrder,
    /// Only those who joined in the last seven days.
    pub recent: bool,
}

/// In which order a server's member pages list them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MemberOrder {
    /// Who joined last, first.
    #[default]
    NewestMembers,
    OldestMembers,
    /// The newest accounts first.
    NewestAccounts,
    OldestAccounts,
}

impl MemberOrder {
    pub const ALL: [Self; 4] = [
        Self::NewestMembers,
        Self::OldestMembers,
        Self::NewestAccounts,
        Self::OldestAccounts,
    ];
}

/// How someone joined a server, as its administration sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JoinMethod {
    /// Through an invitation; its code when the provider says.
    Invite(Option<String>),
    /// Through the server's own short link.
    VanityLink,
    /// Added with a bot.
    Bot,
    /// Through a connected service.
    Integration,
    /// Found in the service's directory.
    Discovery,
    /// Through a hub of a school or a community.
    Hub,
    /// Through an application.
    App,
    /// Some other way, or one the provider does not tell.
    Other,
}

/// Who joined how, for the members a page listed.
pub type JoinedBy = (UserId, JoinMethod);

/// Where one channel goes when a server's channels are reordered at once:
/// under `parent` (a category; `None` for none), at `position` among its
/// siblings, the smallest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelMove {
    pub channel: ChannelId,
    pub parent: Option<ChannelId>,
    pub position: i32,
    /// Moved into `parent`, it takes the category's permissions in place of
    /// its own (Discord's `lock_permissions`).
    #[serde(default)]
    pub lock_permissions: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_kept_before_its_options_reads_with_the_defaults() {
        let options: ServerOptions = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(options, ServerOptions::default());
        assert_eq!(options.afk_timeout, 300);
        assert!(options.system_messages.joins && options.traits.is_none());
    }

    #[test]
    fn a_move_kept_before_it_could_sync_permissions_reads_without() {
        let step: ChannelMove =
            serde_json::from_value(serde_json::json!({"channel": 1, "parent": 2, "position": 0}))
                .unwrap();
        assert!(!step.lock_permissions);
    }

    #[test]
    fn a_trait_is_a_short_line_with_at_most_a_short_emoji() {
        let mut found = ServerTrait {
            label: "Rust".into(),
            emoji: Some("🦀".into()),
            emoji_id: None,
            emoji_animated: false,
        };
        assert!(found.valid());
        found.label = " ".into();
        assert!(!found.valid());
        found.label = "x".repeat(MAX_TRAIT_LABEL + 1);
        assert!(!found.valid());
        found.label = "línea\nnueva".into();
        assert!(!found.valid());
        found.label = "Arte".into();
        found.emoji = Some("demasiado largo".into());
        assert!(!found.valid());
        // The name of one of the server's own may be longer.
        found.emoji_id = Some(EmojiId(5));
        assert!(found.valid());
    }

    #[test]
    fn a_community_needs_its_channels_verification_and_everyone_scanned() {
        let mut options = ServerOptions {
            rules_channel: Some(ChannelId(1)),
            updates_channel: Some(ChannelId(2)),
            verification: Verification::Low,
            media_scan: MediaScan::Everyone,
            ..ServerOptions::default()
        };
        assert!(options.community_ready());
        options.media_scan = MediaScan::WithoutRoles;
        assert!(!options.community_ready());
        options.media_scan = MediaScan::Everyone;
        options.updates_channel = None;
        assert!(!options.community_ready());
    }
}
