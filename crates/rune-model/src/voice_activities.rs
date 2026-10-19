//! Apps launched in voice channels (the service's activities) and who uses
//! each: the rail says it in its hint, as the official client does.

use crate::{ChannelId, UserId};

/// An app launched in a voice channel, and who uses it now.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceActivity {
    /// The provider's name for this launch: a later report with the same
    /// one replaces it, and one with nobody ends it.
    pub id: String,
    pub channel: ChannelId,
    pub people: Vec<UserId>,
}
