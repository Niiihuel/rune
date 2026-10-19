//! People the account knows outside any server: friends, requests both
//! ways, and who it blocked. One relationship per person: a newer report
//! about the same person replaces the older one.

use serde::{Deserialize, Serialize};

use crate::{Timestamp, UserId};

/// Where the account stands with someone.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum RelationshipKind {
    Friend,
    /// They asked to be friends; the account has not answered.
    IncomingRequest,
    /// The account asked to be friends; they have not answered.
    OutgoingRequest,
    /// The account blocked them.
    Blocked,
}

impl RelationshipKind {
    /// A request still waiting for an answer, either way.
    pub fn is_pending(self) -> bool {
        matches!(self, Self::IncomingRequest | Self::OutgoingRequest)
    }
}

/// The account's relationship with `user`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Relationship {
    pub user: UserId,
    pub kind: RelationshipKind,
    /// When it started, when the service says.
    #[serde(default)]
    pub since: Option<Timestamp>,
    /// A private name the account gave a friend.
    #[serde(default)]
    pub nickname: Option<String>,
    /// An incoming request the service filed as likely spam: it waits apart,
    /// in its own folder, and is not counted with the others.
    #[serde(default)]
    pub spam: bool,
}

impl Relationship {
    /// The longest private name a friend can be given.
    pub const NICKNAME_MAX: usize = 32;

    /// A request still waiting for the account's answer, outside the spam folder.
    pub fn awaits_answer(&self) -> bool {
        self.kind == RelationshipKind::IncomingRequest && !self.spam
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_relationship_kept_without_its_optional_fields_still_reads() {
        let read: Relationship =
            serde_json::from_value(serde_json::json!({"user": 7, "kind": "Blocked"})).unwrap();
        assert_eq!(read.kind, RelationshipKind::Blocked);
        assert!(read.since.is_none() && read.nickname.is_none() && !read.spam);
        assert!(RelationshipKind::OutgoingRequest.is_pending());
        assert!(!RelationshipKind::Friend.is_pending());
    }
}
