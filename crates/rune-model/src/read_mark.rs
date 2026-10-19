use serde::{Deserialize, Serialize};

use crate::MessageId;

/// An account-local reading choice. A reminder never rewinds a provider cursor.
/// Revisions order local choices, not provider confirmations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReadMark {
    #[serde(default)]
    pub through: Option<MessageId>,
    /// The exact message chosen by the user; it may no longer be available.
    #[serde(default)]
    pub unread_from: Option<MessageId>,
    #[serde(default)]
    pub revision: u64,
}
