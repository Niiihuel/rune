//! What a server's administration lists and changes beyond roles and
//! members: its invitations, its expressions, its webhooks and
//! integrations, and how its audit log is queried a page at a time.

use serde::{Deserialize, Serialize};

use crate::{
    AuditAction, AuditEntry, ChannelId, EmojiId, IntegrationId, RemoteImage, StickerId, Timestamp,
    UserId, WebhookId,
};

/// An invitation to a server, as its administration lists it. What the
/// provider does not report stays unknown (`None`), never zero.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Invite {
    /// Opaque to the interface: it names the invitation, nothing more.
    pub code: String,
    pub channel: Option<ChannelId>,
    pub inviter: Option<UserId>,
    pub uses: Option<u32>,
    /// `None`: no limit, or not reported.
    pub max_uses: Option<u32>,
    /// `None`: it never expires, or it was not reported.
    pub expires: Option<Timestamp>,
    /// Who enters with it leaves when they disconnect, unless given a role.
    #[serde(default)]
    pub temporary: bool,
}

/// One custom expression of a server.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExpressionId {
    Emoji(EmojiId),
    Sticker(StickerId),
}

/// The kind of an audit entry, to filter the log by it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum AuditActionKind {
    ServerUpdated,
    ChannelCreated,
    ChannelUpdated,
    ChannelDeleted,
    PermissionsChanged,
    RoleCreated,
    RoleUpdated,
    RoleDeleted,
    MemberRolesChanged,
    NickChanged,
    MemberKicked,
    MemberBanned,
    MemberUnbanned,
    InviteDeleted,
    EmojiUpdated,
    EmojiDeleted,
    StickerUpdated,
    StickerDeleted,
    // Administration (19).
    MemberTimedOut,
    InviteCreated,
    EmojiCreated,
    StickerCreated,
    /// Whatever RUNE does not describe on its own; never a filter.
    Other,
}

impl AuditAction {
    pub fn kind(&self) -> AuditActionKind {
        use AuditActionKind as Kind;
        match self {
            Self::ServerUpdated => Kind::ServerUpdated,
            Self::ChannelCreated { .. } => Kind::ChannelCreated,
            Self::ChannelUpdated { .. } => Kind::ChannelUpdated,
            Self::ChannelDeleted { .. } => Kind::ChannelDeleted,
            Self::PermissionsChanged { .. } => Kind::PermissionsChanged,
            Self::RoleCreated { .. } => Kind::RoleCreated,
            Self::RoleUpdated { .. } => Kind::RoleUpdated,
            Self::RoleDeleted { .. } => Kind::RoleDeleted,
            Self::MemberRolesChanged { .. } => Kind::MemberRolesChanged,
            Self::NickChanged { .. } => Kind::NickChanged,
            Self::MemberKicked { .. } => Kind::MemberKicked,
            Self::MemberBanned { .. } => Kind::MemberBanned,
            Self::MemberUnbanned { .. } => Kind::MemberUnbanned,
            Self::InviteDeleted { .. } => Kind::InviteDeleted,
            Self::EmojiUpdated { .. } => Kind::EmojiUpdated,
            Self::EmojiDeleted { .. } => Kind::EmojiDeleted,
            Self::StickerUpdated { .. } => Kind::StickerUpdated,
            Self::StickerDeleted { .. } => Kind::StickerDeleted,
            Self::MemberTimedOut { .. } => Kind::MemberTimedOut,
            Self::InviteCreated { .. } => Kind::InviteCreated,
            Self::EmojiCreated { .. } => Kind::EmojiCreated,
            Self::StickerCreated { .. } => Kind::StickerCreated,
            Self::Other { .. } => Kind::Other,
        }
    }
}

/// Which entries of an audit log are asked for. The provider filters, so a
/// page holds only matching entries and paging never skips any.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct AuditQuery {
    pub actor: Option<UserId>,
    pub action: Option<AuditActionKind>,
}

impl AuditQuery {
    pub fn matches(&self, entry: &AuditEntry) -> bool {
        self.actor.is_none_or(|actor| entry.actor == actor)
            && self
                .action
                .is_none_or(|action| entry.action.kind() == action)
    }
}

/// Whether `name` can name a custom emoji: 2 to 32 ASCII letters, digits or
/// underscores, as chat services usually require.
pub fn valid_emoji_name(name: &str) -> bool {
    (2..=32).contains(&name.len())
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// Whether `name` can name a sticker: 2 to 30 characters, none of them control.
pub fn valid_sticker_name(name: &str) -> bool {
    (2..=30).contains(&name.chars().count()) && !name.chars().any(char::is_control)
}

/// Whether `name` can name a webhook: 1 to 80 characters, none of them
/// control, and not taken by the service ("discord", "clyde"), as Serein checks.
pub fn valid_webhook_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    (1..=80).contains(&name.chars().count())
        && !name.chars().any(char::is_control)
        && !lower.contains("discord")
        && !lower.contains("clyde")
}

/// What a webhook is: one made in the server, one following another
/// server's announcements, or one an application made for itself. Only the
/// first can be renamed or moved here.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WebhookKind {
    Incoming,
    Follower,
    Application,
}

/// An address that posts into one channel of a server.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Webhook {
    pub id: WebhookId,
    pub channel: ChannelId,
    pub name: String,
    pub kind: WebhookKind,
    /// Who made it, when reported.
    pub creator: Option<UserId>,
    pub avatar: Option<RemoteImage>,
}

/// Something connected to a server: an application with its bot, or an
/// outside service that gives roles to its subscribers.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Integration {
    pub id: IntegrationId,
    pub name: String,
    /// What it connects, as the provider names it: "Aplicación", "Twitch".
    pub service: String,
    pub enabled: bool,
    /// Who added it, when reported.
    pub user: Option<UserId>,
    /// The application's account in the server, when it has one.
    pub bot: Option<UserId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_filters_by_actor_and_kind() {
        let entry = AuditEntry {
            id: 1,
            actor: UserId(7),
            action: AuditAction::EmojiDeleted {
                name: "ferris".into(),
            },
            when: Timestamp(0),
            reason: None,
        };
        assert!(AuditQuery::default().matches(&entry));
        assert!(AuditQuery {
            actor: Some(UserId(7)),
            action: Some(AuditActionKind::EmojiDeleted),
        }
        .matches(&entry));
        assert!(!AuditQuery {
            actor: Some(UserId(8)),
            action: None,
        }
        .matches(&entry));
        assert!(!AuditQuery {
            actor: None,
            action: Some(AuditActionKind::EmojiUpdated),
        }
        .matches(&entry));
    }

    #[test]
    fn expression_names_follow_the_usual_rules() {
        assert!(valid_emoji_name("ferris_2"));
        assert!(!valid_emoji_name("a"));
        assert!(!valid_emoji_name("con espacio"));
        assert!(!valid_emoji_name("ñandú"));
        assert!(valid_sticker_name("Cangrejo feliz"));
        assert!(!valid_sticker_name("x"));
        assert!(!valid_sticker_name("tab\there"));
        assert!(valid_webhook_name("Avisos de CI"));
        assert!(!valid_webhook_name(""));
        assert!(!valid_webhook_name("Mi Discord"));
        assert!(!valid_webhook_name("clyde"));
    }
}
