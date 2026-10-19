//! Calls in direct and group conversations (2026-10-17). The service says
//! which conversations have a call and whom each one rings; who is in one
//! is a `VoiceState` of its channel, as in a voice channel.

use crate::{ChannelId, UserId};

/// The call of a direct or group conversation, as its service reports it.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PrivateCall {
    pub channel: ChannelId,
    /// Who it rings and has not answered yet: the account, for a call
    /// coming in.
    pub ringing: Vec<UserId>,
}

impl PrivateCall {
    /// The most people a call rings at once, as Serein bounds it.
    pub const MAX_RINGING: usize = 64;
}
