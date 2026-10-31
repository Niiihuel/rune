//! RUNE's domain: users, servers, channels, messages, members, presence,
//! voice, attachments and permissions. Every other crate speaks these types;
//! none of them knows a protocol. A provider translates its wire format into
//! them, and the interface draws them.

use serde::{Deserialize, Serialize};

mod admin;
pub mod components;
mod message_patch;
mod message_search;
pub mod permissions;
mod profiles;
mod read_mark;
mod relationships;
mod remote_search;
mod rich;
mod stage;
mod threads;
mod uploads;
// The account's synced settings (P1).
pub mod account;
// Commands and interactions (P3).
pub mod interactions;
// Message actions and moderation (P4).
mod message_actions;
// Desktop, language and local voice (P6).
mod desktop;
// Presence, identity and expressions (P5).
pub mod identity;
// Server administration, rest (P7).
mod server_admin;
// Reactions, pins and search (2026-10-18).
mod tallies;

pub use account::{
    ChannelNotifications, ContentFilter, FriendSources, MessagingPrivacy, NotificationChange,
    Notifications, NotifyLevel, PrivacyChange, ServerFolder, SpamFilter, MUTED_FOREVER,
};
pub use admin::{
    valid_emoji_name, valid_sticker_name, valid_webhook_name, AuditActionKind, AuditQuery,
    ExpressionId, Integration, Invite, Webhook, WebhookKind,
};
pub use components::{ButtonStyle, Component, Components};
pub use message_patch::MessagePatch;
pub use message_search::{
    card_words, CachedContext, HistorySource, LocalReadError, SearchCursor, SearchHit, SearchPage,
    SearchQuery,
};
pub use permissions::{Overwrite, OverwriteTarget, Permissions};
pub use profiles::{
    ListGroup, ListRow, MemberList, MemberRestrictions, Restriction, UserProfile, LIST_GROUPS_MAX,
    LIST_ROWS_MAX, MAX_MUTUALS,
};
pub use read_mark::ReadMark;
pub use relationships::{Relationship, RelationshipKind};
pub use remote_search::{
    newer, AuthorKind, Has, RemotePage, RemoteSearch, RemoteSearchError, MAX_REMOTE_FILTERS,
    MAX_REMOTE_OFFSET, REMOTE_PAGE,
};
pub use rich::{
    card_groups, card_pictures, has_spoiler, spoiled_link, Embed, EmbedField, EmbedImage,
    EmbedKind, Forwarded, Poll, PollAnswer, GALLERY,
};
pub use stage::StageSeat;
pub use threads::{
    ForumDefaults, ForumTag, TagId, Thread, MAX_APPLIED_TAGS, MAX_TAGS, MAX_TAG_NAME,
    MAX_THREAD_NAME,
};
// Threads, forums and stages (2026-10-18).
pub use threads::{PostStarter, STARTER_EXCERPT, STARTER_REACTIONS};
// Server administration: forum archive (2026-10-18).
pub use threads::ARCHIVE_AFTER;
pub use uploads::{PendingAttachment, UploadToken};
// Message actions and moderation (P4).
pub use message_actions::TimeStyle;
// Desktop, language and local voice (P6).
pub use desktop::{
    ActivitySource, ActivityVisibility, DetectableGame, LocalActivity, LocalActivityKind,
};
// Presence, identity and expressions (P5).
pub use identity::{
    Badge, Connection, EffectLayer, Identity, Nameplate, Perks, Platforms, ProfileEffect,
    RoleStyle, ServerTag, StickerPack, TextPicture, MAX_ACTIVITIES, MAX_TEXT_PICTURES,
};
// Server administration, rest (P7).
pub use server_admin::{
    ChannelMove, JoinMethod, JoinedBy, MediaScan, MemberOrder, MemberQuery, ServerOptions,
    ServerTrait, SystemMessages, Verification, AFK_TIMEOUTS, MAX_TRAITS, MAX_TRAIT_LABEL,
};
// Private calls.
mod private_calls;
pub use private_calls::PrivateCall;
// Reactions, pins and search (2026-10-18).
pub use remote_search::{link_host, RemoteOrder};
pub use tallies::{Tally, MAX_BURST_COLORS};
// Rail and notifications.
mod voice_activities;
pub use voice_activities::VoiceActivity;
// Video and documents (2026-10-18).
mod documents;
pub use documents::{PdfMark, PdfWord};
// Rules and onboarding (2026-10-18).
pub mod onboarding;
pub use onboarding::{
    ChoiceId, JoinOutcome, Onboarding, OnboardingMode, Prompt, PromptChoice, PromptEmoji, PromptId,
    PromptStyle, RulesAnswer, RulesField, RulesFieldKind, ServerRules,
};
// Scheduled events (eventos-programados).
pub mod scheduled_events;
pub use scheduled_events::{EventDraft, EventPlace, EventStatus, ScheduledEvent};
// AutoMod (automod).
pub mod automod;
pub use automod::{
    AutoModAction, AutoModExecution, AutoModKind, AutoModPreset, AutoModRule, AutoModRuleId,
    AutoModTrigger, WordFilter,
};

macro_rules! id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
        pub struct $name(pub u64);

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

id!(
    /// A person or a bot account.
    UserId
);
id!(
    /// A server: a community with its channels, members and roles.
    ServerId
);
id!(
    /// A channel of a server, a category of channels, or a direct conversation.
    ChannelId
);
id!(
    /// A message. Ids grow with time, so ordering by id is ordering by date.
    MessageId
);
id!(
    /// A role of a server.
    RoleId
);
id!(
    /// A file attached to a message.
    AttachmentId
);
id!(
    /// A reusable sticker supplied by a provider.
    StickerId
);

/// A sticker and its optional static preview. Availability is provider-owned.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Sticker {
    pub id: StickerId,
    pub name: String,
    pub image: Option<RemoteImage>,
    pub available: bool,
}

/// A provider token that presentation resolves without parsing a wire protocol.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextReference {
    pub source: String,
    pub target: TextTarget,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum TextTarget {
    User(UserId),
    Role(RoleId),
    Channel(ChannelId),
    Emoji {
        name: String,
        /// Its picture, moving when it does; `None` shows `:name:`
        /// (2026-10-17: a server's emoji showed as `:name:` always).
        #[serde(default)]
        image: Option<RemoteImage>,
        /// The same picture larger, for a message of emoji alone; `image`
        /// stands for it when absent (2026-10-18: a jumbo was blurred on a
        /// screen scaled past 1).
        #[serde(default)]
        jumbo: Option<RemoteImage>,
        /// Which emoji of a catalog it is, when its provider says: its
        /// token keeps the name it had when written (2026-10-18).
        #[serde(default)]
        id: Option<EmojiId>,
    },
    /// A complete invitation URL validated by its adapter; presentation never parses it.
    Invitation,
    // Message actions and moderation (P4).
    /// A link to a conversation of the provider, or to one of its messages,
    /// written as a bare address: it opens in RUNE. `server` is `None`
    /// outside servers.
    Link {
        server: Option<ServerId>,
        channel: ChannelId,
        message: Option<MessageId>,
    },
    /// A moment written for each reader's own clock and calendar.
    Time {
        at: Timestamp,
        style: TimeStyle,
    },
}
impl TextTarget {
    /// A server emoji's picture: the larger one in a message of emoji
    /// alone (`jumbo`) when its provider gives one, else the line's.
    pub fn emoji_picture(&self, jumbo: bool) -> Option<&RemoteImage> {
        match self {
            Self::Emoji {
                image,
                jumbo: large,
                ..
            } => large.as_ref().filter(|_| jumbo).or(image.as_ref()),
            _ => None,
        }
    }
}

id!(
    /// A custom expression supplied by a community.
    EmojiId
);
id!(
    /// A server's webhook: an address that posts into one of its channels.
    WebhookId
);
// Scheduled events (eventos-programados).
id!(
    /// Something a server planned for a time.
    ScheduledEventId
);
id!(
    /// Something connected to a server: an application or an outside service.
    IntegrationId
);

/// A provider-supplied custom emoji. The insertion text is opaque to the UI;
/// its provider chooses how this expression is represented in message content.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CustomEmoji {
    pub id: EmojiId,
    pub server: ServerId,
    pub name: String,
    pub insert: String,
    pub animated: bool,
    pub available: bool,
    pub roles: Vec<RoleId>,
    pub image: Option<RemoteImage>,
}

/// Milliseconds since the Unix epoch, in UTC.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// The time `minutes` later.
    pub fn plus_minutes(self, minutes: i64) -> Self {
        Self(self.0 + minutes * 60_000)
    }

    /// Whole minutes from `earlier` to this one; negative when it is later.
    pub fn minutes_since(self, earlier: Self) -> i64 {
        (self.0 - earlier.0) / 60_000
    }
}

/// A color as 0xRRGGBB.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Rgb(pub u32);

impl Rgb {
    /// The red, green and blue channels.
    pub fn channels(self) -> (u8, u8, u8) {
        ((self.0 >> 16) as u8, (self.0 >> 8) as u8, self.0 as u8)
    }
}

/// An image served without account headers by an origin declared by the provider.
/// URLs may contain expiring access signatures, so Debug never includes them.
/// The asset service validates the origin again before requesting pixels.
#[derive(Clone, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RemoteImage {
    pub url: String,
    #[serde(default)]
    pub kind: ImageKind,
}

/// Selects the decode and output budget, independently of any provider.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ImageKind {
    #[default]
    Artwork,
    /// Wide profile artwork, the still cover of an activity and a stream's
    /// picture (a JPEG or PNG under 4 MiB); normalized without square
    /// padding.
    Banner,
    AttachmentPreview,
    /// A short silent looping video, such as a card's GIF: decoded into
    /// frames under the same budgets as an animated preview.
    Clip,
    /// A preview's picture at its own size, for the viewer only: kept as
    /// its file is, one at a time.
    Original,
    // Message content (contenido-de-mensajes).
    /// A ThumbHash (version 1), in base64 in place of an address: decoded
    /// on this machine into the small blurry picture shown while the real
    /// one loads. Nothing is fetched.
    ThumbHash,
    /// An attachment preview blurred past recognition on this machine, for
    /// a picture the account's explicit content filter hides until it asks
    /// to see it. Fetched as [`Self::AttachmentPreview`].
    Obscured,
}

impl RemoteImage {
    /// The same picture blurred, as [`ImageKind::Obscured`] draws it.
    pub fn obscured(&self) -> Self {
        Self {
            url: self.url.clone(),
            kind: ImageKind::Obscured,
        }
    }
}

impl std::fmt::Debug for RemoteImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteImage")
            .field("url", &"[redacted]")
            .field("kind", &self.kind)
            .finish()
    }
}

/// A provider-supplied file URL, used only after an explicit download action.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RemoteFile {
    pub url: String,
}

impl std::fmt::Debug for RemoteFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteFile")
            .field("url", &"[redacted]")
            .finish()
    }
}

/// Artwork knowledge carried by a partial account object. Unknown must not
/// erase previously received artwork; Absent is an explicit removal.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProfileBanner {
    #[default]
    Unknown,
    Absent,
    Image(RemoteImage),
}

impl ProfileBanner {
    /// The public artwork source when one has been received.
    pub fn image(&self) -> Option<&RemoteImage> {
        match self {
            Self::Image(image) => Some(image),
            Self::Unknown | Self::Absent => None,
        }
    }

    /// Enrich a partial account update without undoing explicit removals.
    pub fn inherit_unknown(&mut self, previous: &Self) {
        if matches!(self, Self::Unknown) {
            *self = previous.clone();
        }
    }
}

/// An account.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct User {
    pub id: UserId,
    /// The unique handle, without a leading @.
    pub name: String,
    /// The name shown, when it differs from the handle.
    pub display_name: Option<String>,
    /// The color drawn behind the initial while there is no picture.
    pub accent: Rgb,
    #[serde(default)]
    pub avatar: Option<RemoteImage>,
    /// Artwork knowledge supplied with the account, not a complete profile fetch.
    #[serde(default)]
    pub banner: ProfileBanner,
    /// An automated account. The interface always marks it.
    pub bot: bool,
    /// How they want to be referred to, shown on their profile.
    #[serde(default)]
    pub pronouns: Option<String>,
    /// A few lines about themselves, shown on their profile.
    #[serde(default)]
    pub about: Option<String>,
    /// The marks around their name and picture.
    #[serde(default)]
    pub identity: Identity,
    /// The picture their service shows while they have none of their own;
    /// without it, their initial.
    #[serde(default)]
    pub default_avatar: Option<RemoteImage>,
}

impl User {
    /// The name to show: the display name, else the handle.
    pub fn shown_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.name)
    }

    /// The picture to show: their own, else their service's default.
    pub fn picture(&self) -> Option<&RemoteImage> {
        self.avatar.as_ref().or(self.default_avatar.as_ref())
    }
}

/// Whether someone is around.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Status {
    Online,
    Idle,
    DoNotDisturb,
    /// Around, but shown to others as offline. Only the account's own
    /// status can be this; a provider reports invisible people as offline.
    Invisible,
    #[default]
    Offline,
}

impl Status {
    /// Whether others see the person as around.
    pub fn is_around(self) -> bool {
        matches!(self, Self::Online | Self::Idle | Self::DoNotDisturb)
    }
}

/// What someone is doing, as a service reports it.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ActivityKind {
    #[default]
    Playing,
    Listening,
    Watching,
    Streaming,
    Competing,
}

/// An activity: its kind, its name, two lines about it, its pictures and
/// its times.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Activity {
    pub kind: ActivityKind,
    pub name: String,
    /// What they do in it: a track, a level.
    pub detail: Option<String>,
    /// A second line: a track's artists, a match's state.
    #[serde(default)]
    pub state: Option<String>,
    /// Its picture, and the small one over its corner.
    #[serde(default)]
    pub image: Option<RemoteImage>,
    #[serde(default)]
    pub small_image: Option<RemoteImage>,
    /// When it started, and when it ends if it has an end.
    #[serde(default)]
    pub started: Option<Timestamp>,
    #[serde(default)]
    pub ends: Option<Timestamp>,
    /// The application it belongs to, by its provider's id: what lets the
    /// provider give one without pictures its application's (2026-10-17).
    #[serde(default)]
    pub application: Option<u64>,
    /// The group it is played in; the card shows its size, as «3 de 5».
    #[serde(default)]
    pub party: Option<Party>,
    /// The labels of its buttons, at most two; their links come from the
    /// provider on demand.
    #[serde(default)]
    pub buttons: Vec<String>,
    /// The text of its large picture: a Spotify album's name, on hover.
    #[serde(default)]
    pub large_text: Option<String>,
    /// The text of its small picture, on hover.
    #[serde(default)]
    pub small_text: Option<String>,
    /// A Spotify track's id: the card links the title to the track.
    #[serde(default)]
    pub sync_id: Option<String>,
    /// The presence session it belongs to; asking for its buttons' links needs it.
    #[serde(default)]
    pub session: Option<String>,
    /// A stream's page, for a `Streaming` activity (Twitch or YouTube).
    #[serde(default)]
    pub url: Option<String>,
    /// When the service first saw it: a timer without `started` counts from here.
    #[serde(default)]
    pub created: Option<Timestamp>,
}

/// The group an activity is played in, as its service reports it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Party {
    /// The service's id for the group; Spotify's starts with `spotify:`.
    #[serde(default)]
    pub id: Option<String>,
    /// Players now and the most allowed; a maximum of 0 means no maximum.
    #[serde(default)]
    pub size: Option<(u32, u32)>,
}

/// Someone's status, the line of text they set, what they are doing and
/// where they are connected from.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Presence {
    pub status: Status,
    pub custom: Option<String>,
    /// A server's emoji before `custom`, as a picture; a Unicode one is
    /// already in its text (2026-10-17: it was left out).
    #[serde(default)]
    pub custom_emoji: Option<TextPicture>,
    /// The first is the one shown in lists; at most [`MAX_ACTIVITIES`].
    #[serde(default)]
    pub activities: Vec<Activity>,
    #[serde(default)]
    pub platforms: Platforms,
}

impl Presence {
    /// What they do first, as lists show it.
    pub fn activity(&self) -> Option<&Activity> {
        self.activities.first()
    }
}

/// What a channel is for.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum ChannelKind {
    /// Messages, in a server.
    Text,
    /// Talking, in a server.
    Voice,
    /// A heading that groups the channels under it.
    Category,
    /// A conversation between two people.
    Direct,
    /// A conversation between a few people, outside any server.
    Group,
    /// Messages a server publishes, in a server; written like text.
    Announcement,
    /// A list of posts, each a thread with a title. It holds no messages itself.
    Forum,
    /// A conversation branched from a channel, or a forum's post; see
    /// [`Channel::thread`]. Its `parent` is that channel or forum.
    Thread,
    /// Talking to an audience, in a server: speakers are invited up and
    /// everyone else listens.
    Stage,
}

impl ChannelKind {
    /// Whether it holds messages.
    pub fn has_messages(self) -> bool {
        matches!(
            self,
            Self::Text | Self::Direct | Self::Group | Self::Announcement | Self::Thread
        )
    }

    /// Whether people talk in it: listed with who is in it.
    pub fn is_voice(self) -> bool {
        matches!(self, Self::Voice | Self::Stage)
    }
}

/// A channel, a category or a direct conversation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub id: ChannelId,
    /// `None` for a direct conversation.
    pub server: Option<ServerId>,
    pub kind: ChannelKind,
    pub name: String,
    pub topic: Option<String>,
    /// The pictures `topic` names (a server's emoji), drawn in place of
    /// their tokens (2026-10-17).
    #[serde(default)]
    pub topic_pictures: Vec<TextPicture>,
    /// The category it sits under.
    pub parent: Option<ChannelId>,
    /// Order among its siblings, smallest first.
    pub position: i32,
    /// The other people of a direct conversation.
    pub recipients: Vec<UserId>,
    /// Who owns a group conversation: only they take others out of it.
    #[serde(default)]
    pub owner: Option<UserId>,
    /// What the current account may do here.
    pub permissions: Permissions,
    /// What this channel grants or takes away from roles and members.
    #[serde(default)]
    pub overwrites: Vec<Overwrite>,
    /// Seconds a member waits between two messages; 0 for no wait.
    #[serde(default)]
    pub slowmode_secs: u32,
    /// Only for adults: the interface asks before showing it.
    #[serde(default)]
    pub nsfw: bool,
    /// What a thread knows of itself; `None` for any other kind.
    #[serde(default)]
    pub thread: Option<Thread>,
    /// The tags a forum offers its posts.
    #[serde(default)]
    pub tags: Vec<ForumTag>,
    /// How a forum lists its posts by default.
    #[serde(default)]
    pub forum: ForumDefaults,
    /// A group conversation's own picture, when it has one.
    #[serde(default)]
    pub icon: Option<RemoteImage>,
}

impl Channel {
    /// What a provider names a conversation outside servers with no name
    /// and nobody known in it: interface text, kept in Spanish and
    /// translated where shown (`Store::channel_title`).
    pub const UNNAMED: &'static str = rune_i18n::es("Conversación");

    /// Whether it is a thread or a forum's post.
    pub fn is_thread(&self) -> bool {
        self.kind == ChannelKind::Thread
    }
}

/// A role: a name, a color, what it grants, and whether its members are
/// listed apart.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Role {
    pub id: RoleId,
    pub name: String,
    pub color: Option<Rgb>,
    /// Higher shows first and outranks the lower ones.
    pub position: i32,
    /// Its members get a section of their own in the member list.
    pub hoisted: bool,
    /// Anyone may @mention it.
    #[serde(default)]
    pub mentionable: bool,
    /// What it grants across the server.
    #[serde(default)]
    pub permissions: Permissions,
    /// Its picture or emoji beside names, and its gradient.
    #[serde(default)]
    pub style: RoleStyle,
}

impl Role {
    /// The role every member has, `@everyone`, granting `permissions`.
    pub fn everyone(server: ServerId, permissions: Permissions) -> Self {
        Self {
            id: RoleId(server.0),
            name: "@everyone".into(),
            color: None,
            position: 0,
            hoisted: false,
            mentionable: false,
            permissions,
            style: RoleStyle::default(),
        }
    }
}

/// A server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Server {
    /// Custom expressions supplied by this community; old snapshots have none.
    #[serde(default)]
    pub emojis: Vec<CustomEmoji>,
    pub id: ServerId,
    pub name: String,
    /// The color of its mark while there is no picture.
    pub accent: Rgb,
    #[serde(default)]
    pub icon: Option<RemoteImage>,
    /// Its roles, `@everyone` among them when the provider reports it.
    pub roles: Vec<Role>,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
    /// Who owns it: they may do everything.
    #[serde(default)]
    pub owner: Option<UserId>,
    /// What it is about, shown on its profile.
    #[serde(default)]
    pub description: Option<String>,
    // Server administration, rest (P7).
    /// What it sets for everyone beyond its name and look.
    #[serde(default)]
    pub options: ServerOptions,
}

impl Server {
    /// The id of `@everyone`: the server's own.
    pub fn everyone_id(&self) -> RoleId {
        RoleId(self.id.0)
    }

    /// The role every member has, when the provider reports it.
    pub fn everyone(&self) -> Option<&Role> {
        let id = self.everyone_id();
        self.roles.iter().find(|role| role.id == id)
    }

    /// The roles a member can be given, highest first: all but `@everyone`.
    pub fn assignable_roles(&self) -> Vec<&Role> {
        let everyone = self.everyone_id();
        let mut roles: Vec<&Role> = self
            .roles
            .iter()
            .filter(|role| role.id != everyone)
            .collect();
        roles.sort_by_key(|role| std::cmp::Reverse(role.position));
        roles
    }

    /// What a provider names a server whose name has not arrived (one
    /// unavailable in an outage): interface text, kept in Spanish and
    /// translated where shown ([`Self::shown_name`]).
    pub const UNNAMED: &'static str = rune_i18n::es("Servidor no disponible");

    /// Its name as shown: a provider's placeholder in the interface's
    /// language, any other name as it is.
    pub fn shown_name(&self) -> &str {
        if self.name == Self::UNNAMED {
            rune_i18n::t(Self::UNNAMED)
        } else {
            &self.name
        }
    }

    /// Up to two initials of the name, for its mark.
    pub fn initials(&self) -> String {
        self.shown_name()
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .take(2)
            .flat_map(char::to_uppercase)
            .collect()
    }
}

/// A person in a server. Everything here is scoped to `server`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Member {
    pub user: UserId,
    pub server: ServerId,
    pub nick: Option<String>,
    pub roles: Vec<RoleId>,
    /// What keeps them from taking part here, as last reported.
    #[serde(default)]
    pub restrictions: MemberRestrictions,
    /// When they joined this server, when reported.
    #[serde(default)]
    pub joined: Option<Timestamp>,
    /// The picture they show in this server instead of their account's.
    #[serde(default)]
    pub avatar: Option<RemoteImage>,
    /// The marks they show in this server over their account's.
    #[serde(default)]
    pub identity: Identity,
}

/// One change made to a server, as its audit log keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: u64,
    /// Who made the change.
    pub actor: UserId,
    pub action: AuditAction,
    pub when: Timestamp,
    pub reason: Option<String>,
}

/// What an audit log entry records. Names are kept as they were then, so an
/// entry still reads right after what it names is renamed or deleted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AuditAction {
    ServerUpdated,
    ChannelCreated {
        channel: ChannelId,
        name: String,
    },
    ChannelUpdated {
        channel: ChannelId,
        name: String,
    },
    ChannelDeleted {
        name: String,
    },
    PermissionsChanged {
        channel: ChannelId,
        name: String,
    },
    RoleCreated {
        role: RoleId,
        name: String,
    },
    RoleUpdated {
        role: RoleId,
        name: String,
    },
    RoleDeleted {
        name: String,
    },
    MemberRolesChanged {
        user: UserId,
        added: Vec<RoleId>,
        removed: Vec<RoleId>,
    },
    NickChanged {
        user: UserId,
        nick: Option<String>,
    },
    MemberKicked {
        user: UserId,
    },
    MemberBanned {
        user: UserId,
    },
    MemberUnbanned {
        user: UserId,
    },
    InviteDeleted {
        code: String,
    },
    /// Renamed: `name` is the new name.
    EmojiUpdated {
        name: String,
    },
    EmojiDeleted {
        name: String,
    },
    /// Renamed: `name` is the new name.
    StickerUpdated {
        name: String,
    },
    StickerDeleted {
        name: String,
    },
    // Administration (19).
    /// Timed out until `until`; `None`: the timeout was lifted.
    MemberTimedOut {
        user: UserId,
        until: Option<Timestamp>,
    },
    InviteCreated {
        code: String,
    },
    EmojiCreated {
        name: String,
    },
    StickerCreated {
        name: String,
    },
    /// A change RUNE does not describe on its own, as its provider words it:
    /// "creó un webhook".
    Other {
        what: String,
    },
}

/// Someone banned from a server, and why.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ban {
    pub user: UserId,
    pub reason: Option<String>,
}

/// One emoji's reactions on a message.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Reaction {
    /// A Unicode emoji.
    pub emoji: String,
    /// Its normal reactions; super reactions are counted in `burst`.
    pub count: u32,
    /// The current account reacted with it.
    pub me: bool,
    // Reactions, pins and search (2026-10-18).
    /// Its super reactions, counted apart as services do.
    #[serde(default)]
    pub burst: u32,
    /// The current account super-reacted with it.
    #[serde(default)]
    pub me_burst: bool,
    /// The colors the service gave its super reactions.
    #[serde(default)]
    pub burst_colors: Vec<Rgb>,
}

/// A file attached to a message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Attachment {
    pub id: AttachmentId,
    pub filename: String,
    pub size_bytes: u64,
    pub content_type: Option<String>,
    /// Pixels, for an image, so the timeline can reserve its room before it loads.
    pub dimensions: Option<(u32, u32)>,
    /// A bounded raster preview; absence keeps the file card without a request.
    #[serde(default)]
    pub preview: Option<RemoteImage>,
    /// The original file; never requested automatically with the preview.
    #[serde(default)]
    pub download: Option<RemoteFile>,
    /// Marked as a spoiler: no preview is requested until the account reveals it.
    #[serde(default)]
    pub spoiler: bool,
    /// A recorded voice message: drawn as a player instead of a file card.
    #[serde(default)]
    pub voice: Option<VoiceClip>,
    // Message content (contenido-de-mensajes).
    /// What stands in for its preview while it loads: an
    /// [`ImageKind::ThumbHash`].
    #[serde(default)]
    pub placeholder: Option<RemoteImage>,
    /// The service found it explicit: the account's filter may blur it.
    #[serde(default)]
    pub explicit: bool,
}

impl Attachment {
    /// The largest audio or video file RUNE fetches to play: the whole file
    /// stays in memory while it plays.
    pub const MAX_PLAYED_BYTES: u64 = 50 * 1024 * 1024;
}

/// What a voice message's player shows before its audio is fetched.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VoiceClip {
    /// Its length as the service announced it, above 0 and at most [`VoiceClip::MAX_SECS`].
    pub duration_secs: f32,
    /// Its loudness along the clip, 0 to 255, at most [`VoiceClip::MAX_WAVEFORM`]
    /// values; empty when unknown, and the player draws it flat.
    pub waveform: Vec<u8>,
}

impl VoiceClip {
    pub const MAX_WAVEFORM: usize = 256;
    pub const MAX_SECS: f32 = 3600.0;
}

/// What kind of event a message records.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum MessageKind {
    #[default]
    Default,
    /// Someone joined the server.
    Join,
    /// The author added someone to a group: the first of its mentions.
    RecipientAdd,
    /// The author took someone out of a group, or left it when that is them.
    RecipientRemove,
    /// The author started a call; [`Message::call`] says who joined and
    /// how long it lasted.
    Call,
    /// The author renamed the conversation; its content is the new name.
    ChannelRename,
    /// The author changed the group's picture.
    ChannelIcon,
    /// The author pinned a message: the one it answers.
    Pinned,
    /// The author boosted the server; `tier` is the level it reached, or 0.
    Boost { tier: u8 },
    /// The author started a thread; its content is the thread's name.
    ThreadCreated,
    /// A poll closed; its card holds the winning answer.
    PollResult,
    /// A kind RUNE does not draw yet, by the service's number. Its text,
    /// cards and files are still shown.
    Other(u16),
}

impl MessageKind {
    /// Written by someone, not recorded by the service.
    pub fn is_said(self) -> bool {
        matches!(self, Self::Default | Self::Other(_))
    }
}

/// Where a message is on its way to the service.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum Delivery {
    /// The service has it.
    #[default]
    Sent,
    /// Sent from here and not confirmed yet.
    Sending,
    /// It did not get there, for `reason`. Sending it again is safe.
    Failed { reason: String },
    /// It left but no answer came back (the connection dropped, or the wait
    /// ran out): it may or may not have arrived. Sending it again may repeat it.
    Unknown { reason: String },
}

impl Delivery {
    /// Shown here only: the service has not confirmed it.
    pub fn is_local(&self) -> bool {
        !matches!(self, Self::Sent)
    }
}

/// A message.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub channel: ChannelId,
    pub author: UserId,
    /// Markdown source.
    pub content: String,
    pub timestamp: Timestamp,
    pub edited: Option<Timestamp>,
    pub kind: MessageKind,
    pub reply_to: Option<MessageId>,
    pub mentions: Vec<UserId>,
    pub reactions: Vec<Reaction>,
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub stickers: Vec<Sticker>,
    #[serde(default)]
    pub text_references: Vec<TextReference>,
    /// Where it is on its way; `Sent` for everything the service reported.
    #[serde(default)]
    pub delivery: Delivery,
    /// Kept in its channel's list of pinned messages.
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    /// What it forwards; it then says nothing of its own.
    #[serde(default)]
    pub forwarded: Option<Forwarded>,
    #[serde(default)]
    pub poll: Option<Poll>,
    /// What an app laid out with it: buttons, menus, or the whole message.
    #[serde(default)]
    pub components: Components,
    // Message content (contenido-de-mensajes).
    /// Who took part in the call a [`MessageKind::Call`] records, and when
    /// it ended, as far as the provider reported them.
    #[serde(default)]
    pub call: Option<MessageCall>,
    // Rail and notifications.
    /// The roles it tells, as the provider says: a role written in the text
    /// that its author may not mention tells nobody.
    #[serde(default)]
    pub mention_roles: Vec<RoleId>,
    /// It tells everyone (`@everyone` or `@here`), as the provider says: the
    /// words alone do not when its author may not mention everyone.
    #[serde(default)]
    pub mention_everyone: bool,
}

/// The call a call's message records.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MessageCall {
    /// Everyone who joined it; someone else's call the account missed
    /// leaves the account out.
    #[serde(default)]
    pub participants: Vec<UserId>,
    /// `None` while it goes on.
    #[serde(default)]
    pub ended: Option<Timestamp>,
}

impl Message {
    /// Whether it mentions `user`.
    pub fn mentions(&self, user: UserId) -> bool {
        self.mentions.contains(&user)
    }

    /// Its files and those of the copy it forwards, which show and open as its own.
    pub fn files(&self) -> impl Iterator<Item = &Attachment> {
        let copy = self.forwarded.iter().flat_map(|copy| &copy.attachments);
        self.attachments.iter().chain(copy)
    }

    /// A voice message: it cannot be edited, and previews name it when it has no text.
    pub fn is_voice(&self) -> bool {
        self.attachments
            .iter()
            .any(|attachment| attachment.voice.is_some())
    }

    /// Add or remove the current account's `emoji`, keeping the counts right.
    pub fn toggle_reaction(&mut self, emoji: &str) {
        match self
            .reactions
            .iter()
            .position(|reaction| reaction.emoji == emoji)
        {
            Some(index) => {
                let reaction = &mut self.reactions[index];
                if reaction.me {
                    reaction.me = false;
                    reaction.count = reaction.count.saturating_sub(1);
                    if reaction.count == 0 && reaction.burst == 0 {
                        self.reactions.remove(index);
                    }
                } else {
                    reaction.me = true;
                    reaction.count += 1;
                }
            }
            None => self.reactions.push(Reaction {
                emoji: emoji.to_owned(),
                count: 1,
                me: true,
                ..Default::default()
            }),
        }
    }

    /// Make the current account's normal `emoji` reaction be there or not,
    /// whatever it was. Returns whether that changed anything.
    pub fn set_reaction(&mut self, emoji: &str, on: bool) -> bool {
        let reacted = self
            .reactions
            .iter()
            .any(|reaction| reaction.emoji == emoji && reaction.me);
        if reacted != on {
            self.toggle_reaction(emoji);
        }
        reacted != on
    }

    /// One account added or took back `emoji`. The account's own change
    /// applies only once, so it agrees with a toggle already shown; others'
    /// never take away the account's. Returns whether anything changed.
    pub fn count_reaction(&mut self, emoji: &str, me: bool, added: bool) -> bool {
        let Some(index) = self
            .reactions
            .iter()
            .position(|reaction| reaction.emoji == emoji)
        else {
            if added {
                self.reactions.push(Reaction {
                    emoji: emoji.to_owned(),
                    count: 1,
                    me,
                    ..Default::default()
                });
            }
            return added;
        };
        let reaction = &mut self.reactions[index];
        if me {
            if reaction.me == added {
                return false;
            }
            reaction.me = added;
        } else if !added && reaction.count <= u32::from(reaction.me) {
            return false;
        }
        if added {
            reaction.count = reaction.count.saturating_add(1);
        } else {
            reaction.count = reaction.count.saturating_sub(1);
            if reaction.count == 0 && reaction.burst == 0 {
                self.reactions.remove(index);
            }
        }
        true
    }

    /// Take away every reaction, or only `emoji`. Returns whether any went.
    pub fn clear_reactions(&mut self, emoji: Option<&str>) -> bool {
        let before = self.reactions.len();
        self.reactions
            .retain(|reaction| emoji.is_some_and(|emoji| reaction.emoji != emoji));
        self.reactions.len() != before
    }
}

/// How far the account's call got.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CallPhase {
    /// Asked to join; not connected yet.
    Joining,
    /// Connected; agreeing on end-to-end encryption with the others.
    Securing,
    Connected,
    /// Connected, and nobody else is in the call yet (Serein's `Waiting`).
    Waiting,
    /// The connection dropped; trying again.
    Reconnecting {
        attempt: u32,
    },
    /// It could not connect or was lost for good, for `reason`.
    Failed {
        reason: String,
    },
}

impl CallPhase {
    /// Connected, with others or alone: the call's media runs.
    pub fn is_connected(&self) -> bool {
        matches!(self, Self::Connected | Self::Waiting)
    }
}

/// What someone's video in a call shows: their shared screen or their camera.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum VideoKind {
    Screen,
    Camera,
}

/// How the account's own screen is sent: its height, 16:9, and frames a
/// second. RUNE offers 480p, 720p and 1080p at 15, 30 or 60 with no tier
/// check, as the owner decided (docs/ARCHITECTURE.md).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct StreamQuality {
    pub height: u32,
    pub fps: u32,
}

impl Default for StreamQuality {
    fn default() -> Self {
        Self {
            height: 720,
            fps: 30,
        }
    }
}

impl StreamQuality {
    pub const HEIGHTS: [u32; 3] = [480, 720, 1080];
    pub const RATES: [u32; 3] = [15, 30, 60];

    /// One of the offered heights and rates.
    pub fn valid(self) -> bool {
        Self::HEIGHTS.contains(&self.height) && Self::RATES.contains(&self.fps)
    }

    /// The largest picture sent, in pixels: 854×480, 1280×720, 1920×1080.
    pub fn size(self) -> (u32, u32) {
        ((self.height * 16).div_ceil(9) & !1, self.height)
    }

    /// The most bits a second sent: 3.5 Mbit/s, 9 for 720p60 and 1080p,
    /// as the official client allows Go Live (docs/ARCHITECTURE.md, «Calidades»).
    pub fn max_bitrate(self) -> u32 {
        if self.height > 720 || (self.height == 720 && self.fps > 30) {
            9_000_000
        } else {
            3_500_000
        }
    }
}

/// How far someone's screen stream the account watches got, or the
/// account's own.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StreamPhase {
    /// Asked to watch; its connection is not ready yet.
    Connecting,
    /// Connected: its frames may arrive.
    Live,
    /// The person paused it.
    Paused,
    /// Its connection dropped; trying again.
    Reconnecting,
    /// Its owner stopped streaming but is still in the call: it comes back
    /// by itself when they stream again.
    Waiting,
    /// Could not be reached after its retries; why. Asked again by hand.
    Failed(String),
    /// Over, with the reason when it was not the account's doing.
    Ended(Option<String>),
}

/// How the call's connection is doing, as the provider measures it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CallQuality {
    /// Round trip to the media server, in milliseconds.
    pub rtt_ms: u32,
    /// Packets lost, per thousand.
    pub loss_permille: u16,
}

/// The account's call: where, how far it got, and what the account set.
/// The other people in it are `VoiceState`s of the same channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Call {
    pub channel: ChannelId,
    pub phase: CallPhase,
    pub muted: bool,
    pub deafened: bool,
    pub quality: Option<CallQuality>,
}

/// Someone in a voice channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VoiceState {
    pub user: UserId,
    pub channel: ChannelId,
    /// Their own choice: the microphone off.
    pub muted: bool,
    /// Their own choice: the sound off.
    pub deafened: bool,
    pub speaking: bool,
    /// Sharing a screen (going live) in that channel.
    #[serde(default)]
    pub streaming: bool,
    /// The server keeps their microphone off (moderation, or not invited to
    /// speak); their own unmute does not lift it.
    #[serde(default)]
    pub server_muted: bool,
    /// The server keeps their sound off.
    #[serde(default)]
    pub server_deafened: bool,
    /// Their camera is on in that channel.
    #[serde(default)]
    pub camera: bool,
    // Stages: see [`StageSeat`].
    /// On a stage, the server holds them in the audience.
    #[serde(default)]
    pub suppressed: bool,
    /// On a stage, when they raised their hand or were invited to speak.
    #[serde(default)]
    pub speak_request: Option<Timestamp>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn stream_qualities_are_the_offered_sizes_and_rates() {
        use super::StreamQuality;
        let quality = |height, fps| StreamQuality { height, fps };
        assert_eq!(StreamQuality::default(), quality(720, 30));
        assert_eq!(quality(480, 60).size(), (854, 480));
        assert_eq!(quality(720, 30).size(), (1280, 720));
        assert_eq!(quality(1080, 15).size(), (1920, 1080));
        assert_eq!(quality(480, 60).max_bitrate(), 3_500_000);
        assert_eq!(quality(720, 30).max_bitrate(), 3_500_000);
        assert_eq!(quality(720, 60).max_bitrate(), 9_000_000);
        assert_eq!(quality(1080, 15).max_bitrate(), 9_000_000);
        assert!(quality(1080, 60).valid());
        assert!(!quality(1440, 30).valid() && !quality(720, 5).valid());
    }

    #[test]
    fn saved_voice_states_read_without_server_marks_and_stages_round_trip() {
        let state: super::VoiceState = serde_json::from_value(serde_json::json!({
            "user": 1, "channel": 2, "muted": true, "deafened": false, "speaking": false,
        }))
        .unwrap();
        assert!(state.muted && !state.server_muted && !state.server_deafened);
        let stage = serde_json::to_value(super::ChannelKind::Stage).unwrap();
        assert_eq!(
            serde_json::from_value::<super::ChannelKind>(stage).unwrap(),
            super::ChannelKind::Stage
        );
        assert!(super::ChannelKind::Stage.is_voice() && !super::ChannelKind::Stage.has_messages());
    }

    #[test]
    fn legacy_attachments_and_images_use_safe_defaults_and_urls_are_redacted() {
        let attachment: super::Attachment = serde_json::from_value(serde_json::json!({
            "id": 1,"filename":"image.png","size_bytes":10,
            "content_type":"image/png","dimensions":[20,30]
        }))
        .unwrap();
        assert!(attachment.preview.is_none() && attachment.download.is_none());
        assert!(
            attachment.voice.is_none(),
            "rows before voice messages read as files"
        );
        let source: super::RemoteImage = serde_json::from_value(serde_json::json!({
            "url":"https://cdn.example.test/image.png?hm=private-signature"
        }))
        .unwrap();
        assert_eq!(source.kind, super::ImageKind::Artwork);
        let file = super::RemoteFile {
            url: source.url.clone(),
        };
        assert!(!format!("{source:?} {file:?}").contains("private-signature"));
        assert!(!format!("{source:?} {file:?}").contains("cdn.example.test"));
        assert_eq!(
            serde_json::from_str::<super::RemoteFile>(&serde_json::to_string(&file).unwrap())
                .unwrap(),
            file
        );
    }

    #[test]
    fn legacy_users_without_artwork_remain_readable() {
        let user: super::User = serde_json::from_value(serde_json::json!({
            "id":42,"name":"Legacy","display_name":null,"accent":0,"bot":false
        }))
        .unwrap();
        assert!(user.avatar.is_none());
        assert_eq!(user.banner, ProfileBanner::Unknown);
    }
    #[test]
    fn profile_banner_knowledge_survives_storage_roundtrips() {
        use super::{ImageKind, ProfileBanner, RemoteImage, User};
        for banner in [
            ProfileBanner::Unknown,
            ProfileBanner::Absent,
            ProfileBanner::Image(RemoteImage {
                url: "https://cdn.example.test/banner.png".into(),
                kind: ImageKind::Banner,
            }),
        ] {
            let mut user: User = serde_json::from_value(serde_json::json!({
                "id":42,"name":"Legacy","display_name":null,"accent":0,"bot":false
            }))
            .unwrap();
            user.banner = banner;
            let restored: User =
                serde_json::from_str(&serde_json::to_string(&user).unwrap()).unwrap();
            assert_eq!(restored, user);
        }
    }
    use super::*;

    fn message() -> Message {
        Message {
            id: MessageId(1),
            channel: ChannelId(1),
            author: UserId(1),
            content: String::new(),
            timestamp: Timestamp(0),
            edited: None,
            kind: MessageKind::Default,
            reply_to: None,
            mentions: vec![UserId(7)],
            reactions: vec![Reaction {
                emoji: "👍".into(),
                count: 2,
                me: false,
                ..Default::default()
            }],
            attachments: Vec::new(),
            stickers: Vec::new(),
            text_references: Vec::new(),
            delivery: Delivery::Sent,
            pinned: false,
            embeds: Vec::new(),
            forwarded: None,
            poll: None,
            components: Default::default(),
            call: None,
            mention_roles: Vec::new(),
            mention_everyone: false,
        }
    }

    #[test]
    fn counted_reactions_apply_the_accounts_own_change_once() {
        let mut message = message();
        assert!(message.count_reaction("👍", false, true));
        assert_eq!(message.reactions[0].count, 3);
        assert!(message.count_reaction("👍", true, true));
        assert!(!message.count_reaction("👍", true, true), "already mine");
        assert_eq!(
            (message.reactions[0].count, message.reactions[0].me),
            (4, true)
        );
        for _ in 0..3 {
            message.count_reaction("👍", false, false);
        }
        assert!(
            !message.count_reaction("👍", false, false),
            "others never take the account's away"
        );
        assert_eq!(message.reactions[0].count, 1);
        assert!(message.count_reaction("👍", true, false));
        assert!(message.reactions.is_empty());
        assert!(!message.count_reaction("🎉", false, false));
        assert!(message.count_reaction("🎉", true, true));
        assert!(message.count_reaction("🔥", false, true));
        assert!(message.clear_reactions(Some("🎉")));
        assert_eq!(message.reactions.len(), 1);
        assert!(message.clear_reactions(None));
        assert!(!message.clear_reactions(None));
    }

    #[test]
    fn poll_votes_replace_the_accounts_choice_and_count_others() {
        let answer = |id, votes, me| PollAnswer {
            id,
            text: id.to_string(),
            votes,
            me,
            ..Default::default()
        };
        let mut poll = Poll {
            question: "?".into(),
            answers: vec![answer(1, 2, true), answer(2, 0, false), answer(3, 1, false)],
            results_known: true,
            ..Default::default()
        };
        assert!(poll.valid_choice(&[2]) && poll.valid_choice(&[]));
        assert!(!poll.valid_choice(&[2, 3]) && !poll.valid_choice(&[9]));
        poll.vote(&[2]);
        assert_eq!(
            poll.answers
                .iter()
                .map(|a| (a.votes, a.me))
                .collect::<Vec<_>>(),
            [(1, false), (1, true), (1, false)]
        );
        assert!(!poll.count_vote(2, true, true), "already mine");
        assert!(poll.count_vote(3, false, true));
        assert!(poll.count_vote(1, false, false));
        assert!(
            !poll.count_vote(2, false, false),
            "only the account's is left"
        );
        assert!(!poll.count_vote(9, false, true));
        poll.multiselect = true;
        assert!(poll.valid_choice(&[1, 3]) && !poll.valid_choice(&[1, 1]));
        poll.vote(&[]);
        assert!(!poll.voted());
        poll.results_known = false;
        assert!(
            !poll.count_vote(1, false, true),
            "unknown counts stay unknown"
        );
        assert!(poll.count_vote(1, true, true));
        assert_eq!((poll.answers[0].votes, poll.answers[0].me), (0, true));
    }

    #[test]
    fn toggling_a_reaction_counts_the_current_account_once() {
        let mut message = message();
        message.toggle_reaction("👍");
        assert_eq!(
            message.reactions[0],
            Reaction {
                emoji: "👍".into(),
                count: 3,
                me: true,
                ..Default::default()
            }
        );
        message.toggle_reaction("👍");
        assert_eq!(
            message.reactions[0],
            Reaction {
                emoji: "👍".into(),
                count: 2,
                me: false,
                ..Default::default()
            }
        );
        message.toggle_reaction("🎉");
        message.toggle_reaction("🎉");
        assert_eq!(
            message.reactions.len(),
            1,
            "a reaction nobody keeps is removed"
        );
    }

    #[test]
    fn initials_take_the_first_letter_of_two_words() {
        let server = Server {
            stickers: Vec::new(),
            emojis: Vec::new(),
            icon: None,
            id: ServerId(1),
            name: "taller de rust".into(),
            accent: Rgb(0),
            roles: Vec::new(),
            owner: None,
            description: None,
            options: Default::default(),
        };
        assert_eq!(server.initials(), "TD");
        let one = Server {
            name: "Ñandú".into(),
            ..server
        };
        assert_eq!(one.initials(), "Ñ");
    }

    #[test]
    fn a_message_kept_before_delivery_existed_reads_as_sent() {
        let mut stored = serde_json::to_value(message()).unwrap();
        let fields = stored.as_object_mut().unwrap();
        fields.remove("delivery");
        fields.remove("pinned");
        fields.remove("stickers");
        fields.remove("text_references");
        fields.remove("components");
        fields.insert("pending".into(), false.into());
        // A card kept before cards had parts or a channel.
        fields.insert("embeds".into(), serde_json::json!([{"title": "Viejo"}]));
        let read: Message = serde_json::from_value(stored).unwrap();
        assert!(read.components.is_empty() && read.embeds[0].channel.is_none());
        assert_eq!(read.delivery, Delivery::Sent);
        assert!(!read.pinned, "kept before pins existed: not pinned");
        assert!(read.stickers.is_empty() && read.text_references.is_empty());
    }

    #[test]
    fn permissions_contain_their_parts() {
        assert!(Permissions::MEMBER.contains(Permissions::SEND | Permissions::VIEW));
        assert!(!Permissions::MEMBER.contains(Permissions::MANAGE_MESSAGES));
        assert!(message().mentions(UserId(7)));
    }

    #[test]
    fn server_snapshots_before_expression_catalogs_keep_reading() {
        let server: Server = serde_json::from_value(serde_json::json!({
            "id": 1, "name": "Legacy", "accent": 0, "roles": [],
            "owner": null, "description": null
        }))
        .unwrap();
        assert!(server.emojis.is_empty() && server.stickers.is_empty());
        assert!(server.icon.is_none());
    }
}
