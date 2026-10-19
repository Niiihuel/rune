//! Stages: voice channels where speakers talk and an audience listens. The
//! place of each person comes from two marks of their voice state, read as
//! the official client reads them: whether the server holds them in the
//! audience, and when they asked to speak or were invited to.

use crate::VoiceState;

/// Where someone is on a stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageSeat {
    /// Listening.
    Audience,
    /// Listening, with a hand raised to speak.
    HandRaised,
    /// Invited to speak by moderation; speaking once they accept.
    Invited,
    /// Speaking.
    Speaker,
}

impl VoiceState {
    /// Where they are, were this a stage.
    pub fn stage_seat(&self) -> StageSeat {
        match (self.suppressed, self.speak_request.is_some()) {
            (true, false) => StageSeat::Audience,
            (true, true) => StageSeat::HandRaised,
            (false, true) => StageSeat::Invited,
            (false, false) => StageSeat::Speaker,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StageSeat;
    use crate::{ChannelId, Timestamp, UserId, VoiceState};

    #[test]
    fn a_seat_reads_from_the_hold_and_the_request_and_old_states_still_read() {
        let state: VoiceState = serde_json::from_value(serde_json::json!({
            "user": 1, "channel": 2, "muted": false, "deafened": false, "speaking": false,
        }))
        .unwrap();
        assert_eq!(state.stage_seat(), StageSeat::Speaker);
        let with = |suppressed: bool, request: Option<i64>| VoiceState {
            suppressed,
            speak_request: request.map(Timestamp),
            ..state.clone()
        };
        assert_eq!(with(true, None).stage_seat(), StageSeat::Audience);
        assert_eq!(with(true, Some(5)).stage_seat(), StageSeat::HandRaised);
        assert_eq!(with(false, Some(5)).stage_seat(), StageSeat::Invited);
        assert_eq!(state.user, UserId(1));
        assert_eq!(state.channel, ChannelId(2));
    }
}
