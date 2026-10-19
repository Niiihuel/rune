//! The simulation's calls in direct and group conversations (2026-10-17),
//! made up and deterministic: whom the account rings answers, declines or
//! lets it ring by who they are ([`answer`]); whoever answered a direct
//! call hangs up after a while; and the host can have someone call the
//! account ([`super::MockControl::ring_me`]) and the account answer it on
//! «another device» ([`super::MockControl::answer_elsewhere`]). Nothing of
//! this carries sound. Before a direct call starts, the simulated service
//! says whether it may ring ([`World::check_call`]) by how the account and
//! the other person are related.

use std::collections::HashMap;
use std::time::Duration;

use rune_i18n::t;
use rune_model::{ChannelId, ChannelKind, PrivateCall, RelationshipKind, UserId, VoiceState};

use super::{World, ME};
use crate::{Event, Failure};

/// How long a ring lasts before the simulated service gives up on it.
pub(super) const RING_FOR: Duration = Duration::from_secs(30);
/// How long the simulated people take to answer or decline.
const ANSWER_AFTER: Duration = Duration::from_secs(3);
const DECLINE_AFTER: Duration = Duration::from_secs(2);
/// How long someone who answered a direct call talks before hanging up.
const HANG_UP_AFTER: Duration = Duration::from_secs(45);
/// How long a caller waits after the account declines before hanging up.
const GIVE_UP_AFTER: Duration = Duration::from_secs(1);

/// What one of the simulated people does when rung: by who they are, so
/// each direct conversation shows one way a call goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Answer {
    Answers,
    Declines,
    LetsItRing,
}

pub(super) fn answer(user: UserId) -> Answer {
    match user.0 % 3 {
        1 => Answer::Answers,
        2 => Answer::Declines,
        _ => Answer::LetsItRing,
    }
}

/// When `answer` happens, after the ring.
fn after(answer: Answer) -> Duration {
    match answer {
        Answer::Answers => ANSWER_AFTER,
        Answer::Declines => DECLINE_AFTER,
        Answer::LetsItRing => RING_FOR,
    }
}

/// Something the session does later about a call: who, where, which round
/// of rings (a later one makes it stale) and when.
pub(super) struct Later {
    pub(super) user: UserId,
    pub(super) channel: ChannelId,
    pub(super) round: u64,
    pub(super) after: Duration,
    pub(super) what: Step,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Step {
    /// The ring of `user` ends as they answer it, or as the service gives up.
    Rung(Answer),
    /// `user` leaves the call.
    HangUp,
}

#[derive(Debug, Default)]
pub(super) struct CallsSim {
    /// Whom each conversation's call rings.
    calls: HashMap<ChannelId, Vec<UserId>>,
    /// Rounds of rings so far, by conversation.
    rounds: HashMap<ChannelId, u64>,
}

impl World {
    fn private(&self, channel: ChannelId) -> bool {
        self.channel(channel)
            .is_some_and(|found| matches!(found.kind, ChannelKind::Direct | ChannelKind::Group))
    }

    fn call_event(&self, channel: ChannelId) -> Event {
        Event::PrivateCall(PrivateCall {
            channel,
            ringing: self.calls.calls.get(&channel).cloned().unwrap_or_default(),
        })
    }

    fn in_call(&self, user: UserId, channel: ChannelId) -> bool {
        self.voice
            .iter()
            .any(|state| state.user == user && state.channel == channel)
    }

    /// The account is in the call of `channel` now: it exists, and does not
    /// ring the account.
    pub(super) fn call_joined(&mut self, channel: ChannelId) -> Vec<Event> {
        if !self.private(channel) {
            return Vec::new();
        }
        let ringing = self.calls.calls.entry(channel).or_default();
        if ringing.contains(&ME) {
            ringing.retain(|user| *user != ME);
            // Answered: the caller's giving up is stale.
            *self.calls.rounds.entry(channel).or_default() += 1;
        }
        vec![self.call_event(channel)]
    }

    /// Whether the call of the direct conversation `channel` may be made and
    /// ring (llamadas-y-panel-de-voz): with a friend it rings; with a
    /// request on either side it starts without ringing; with anyone else
    /// it is refused, as Discord refuses calls between people who are not
    /// friends.
    pub(super) fn check_call(&self, channel: ChannelId) -> Event {
        let other = self
            .channel(channel)
            .filter(|found| found.kind == ChannelKind::Direct)
            .and_then(|found| found.recipients.first().copied());
        let ringable = match other.and_then(|user| self.kind_with(user)) {
            Some(RelationshipKind::Friend) => true,
            Some(RelationshipKind::IncomingRequest | RelationshipKind::OutgoingRequest) => false,
            _ => {
                return Event::Failed {
                    // Only a direct conversation is the service's to refuse.
                    failure: Failure::CallCheck {
                        channel,
                        refused: other.is_some(),
                    },
                    reason: t("Llamada simulada: no son amigos.").into(),
                    ambiguous: false,
                };
            }
        };
        Event::CallChecked { channel, ringable }
    }

    /// Ring `users` of `channel` to the account's call there, everyone else
    /// not in it when empty; with what each of them does, for later.
    pub(super) fn ring(
        &mut self,
        channel: ChannelId,
        users: Vec<UserId>,
    ) -> (Vec<Event>, Vec<Later>) {
        let recipients = self
            .channel(channel)
            .filter(|_| self.private(channel))
            .map(|found| found.recipients.clone())
            .unwrap_or_default();
        if recipients.is_empty() || !self.in_call(ME, channel) {
            let failed = Event::Failed {
                failure: Failure::Ring { channel },
                reason: t("Entrá a la llamada para hacerla sonar.").into(),
                ambiguous: false,
            };
            return (vec![failed], Vec::new());
        }
        let people: Vec<UserId> = recipients
            .into_iter()
            .filter(|user| {
                (users.is_empty() || users.contains(user)) && !self.in_call(*user, channel)
            })
            .collect();
        let round = self.calls.rounds.entry(channel).or_default();
        *round += 1;
        let round = *round;
        // Someone who answers a direct call hangs up after a while.
        let direct = self
            .channel(channel)
            .is_some_and(|found| found.kind == ChannelKind::Direct);
        let ringing = self.calls.calls.entry(channel).or_default();
        let mut later = Vec::new();
        for user in people {
            if !ringing.contains(&user) {
                ringing.push(user);
            }
            let what = answer(user);
            later.push(Later {
                user,
                channel,
                round,
                after: after(what),
                what: Step::Rung(what),
            });
            if what == Answer::Answers && direct {
                later.push(Later {
                    user,
                    channel,
                    round,
                    after: ANSWER_AFTER + HANG_UP_AFTER,
                    what: Step::HangUp,
                });
            }
        }
        (vec![self.call_event(channel)], later)
    }

    /// Do `later`, unless a newer round of rings made it stale.
    pub(super) fn call_step(&mut self, later: &Later) -> Vec<Event> {
        let round = self.calls.rounds.get(&later.channel).copied().unwrap_or(0);
        if round != later.round {
            return Vec::new();
        }
        match later.what {
            Step::Rung(answer) => self.rung(later.channel, later.user, answer),
            Step::HangUp => self.hang_up(later.user, later.channel),
        }
    }

    /// The ring of `user` to `channel` ends, if it still rings them.
    fn rung(&mut self, channel: ChannelId, user: UserId, answer: Answer) -> Vec<Event> {
        let Some(ringing) = self
            .calls
            .calls
            .get_mut(&channel)
            .filter(|ringing| ringing.contains(&user))
        else {
            return Vec::new();
        };
        ringing.retain(|rung| *rung != user);
        let mut events = vec![self.call_event(channel)];
        if answer == Answer::Answers {
            events.extend(self.enter_call(user, channel));
        }
        events
    }

    /// `user` comes into the call of `channel`, leaving any other.
    fn enter_call(&mut self, user: UserId, channel: ChannelId) -> Vec<Event> {
        let mut events = Vec::new();
        if let Some(at) = self.voice.iter().position(|state| state.user == user) {
            let left = self.voice.remove(at);
            events.push(Event::VoiceLeft {
                user,
                channel: left.channel,
            });
        }
        let state = VoiceState {
            user,
            channel,
            muted: false,
            deafened: false,
            speaking: false,
            streaming: false,
            server_muted: false,
            server_deafened: false,
            camera: false,
            suppressed: false,
            speak_request: None,
        };
        self.voice.push(state.clone());
        events.push(Event::VoiceChanged(state));
        events
    }

    /// `user` leaves the call of `channel`, if in it; with nobody left in
    /// it, the call is over.
    fn hang_up(&mut self, user: UserId, channel: ChannelId) -> Vec<Event> {
        if !self.in_call(user, channel) {
            return Vec::new();
        }
        self.voice
            .retain(|state| !(state.user == user && state.channel == channel));
        let mut events = vec![Event::VoiceLeft { user, channel }];
        events.extend(self.call_over_if_empty(channel));
        events
    }

    /// With nobody in it, the call of `channel` is over, and rings nobody.
    pub(super) fn call_over_if_empty(&mut self, channel: ChannelId) -> Vec<Event> {
        let anyone = self.voice.iter().any(|state| state.channel == channel);
        if anyone || self.calls.calls.remove(&channel).is_none() {
            return Vec::new();
        }
        vec![Event::PrivateCallEnded { channel }]
    }

    /// Stop ringing `users` to `channel`: the account alone declines. Then
    /// the caller of a direct conversation gives up, later.
    pub(super) fn stop_ringing(
        &mut self,
        channel: ChannelId,
        users: Vec<UserId>,
    ) -> (Vec<Event>, Vec<Later>) {
        let Some(ringing) = self.calls.calls.get_mut(&channel) else {
            return (Vec::new(), Vec::new());
        };
        ringing.retain(|user| !users.contains(user));
        let events = vec![self.call_event(channel)];
        let declined = users == [ME]
            && self
                .channel(channel)
                .is_some_and(|found| found.kind == ChannelKind::Direct);
        let round = self.calls.rounds.get(&channel).copied().unwrap_or(0);
        let later = self
            .voice
            .iter()
            .filter(|state| declined && state.channel == channel && state.user != ME)
            .map(|state| Later {
                user: state.user,
                channel,
                round,
                after: GIVE_UP_AFTER,
                what: Step::HangUp,
            })
            .collect();
        (events, later)
    }

    /// Someone calls the account: the person of its first direct
    /// conversation, who gives up if nobody answers. Nothing while that
    /// call is on already.
    pub(super) fn ring_me(&mut self) -> (Vec<Event>, Vec<Later>) {
        let channel = self.first_direct();
        let Some(caller) = self
            .channel(channel)
            .and_then(|found| found.recipients.first().copied())
        else {
            return (Vec::new(), Vec::new());
        };
        if self.calls.calls.contains_key(&channel) || self.in_call(ME, channel) {
            return (Vec::new(), Vec::new());
        }
        let round = self.calls.rounds.entry(channel).or_default();
        *round += 1;
        let round = *round;
        let mut events = self.enter_call(caller, channel);
        self.calls.calls.insert(channel, vec![ME]);
        events.push(self.call_event(channel));
        let later = vec![Later {
            user: caller,
            channel,
            round,
            after: RING_FOR,
            what: Step::HangUp,
        }];
        (events, later)
    }

    /// The account answers the call ringing it on another of its devices:
    /// its voice goes there, and a call of this device ends.
    pub(super) fn answer_elsewhere(&mut self) -> Vec<Event> {
        let Some(channel) = self
            .calls
            .calls
            .iter()
            .find(|(_, ringing)| ringing.contains(&ME))
            .map(|(channel, _)| *channel)
        else {
            return Vec::new();
        };
        let mut events = self.leave_voice();
        for event in &mut events {
            if let Event::CallEnded { reason, .. } = event {
                *reason = Some(t("Tu llamada siguió en otro dispositivo.").into());
            }
        }
        if let Some(ringing) = self.calls.calls.get_mut(&channel) {
            ringing.retain(|user| *user != ME);
        }
        // The caller talks with that device now: their giving up is stale.
        *self.calls.rounds.entry(channel).or_default() += 1;
        events.push(self.call_event(channel));
        // Shown, not kept: this device is not in that call.
        events.push(Event::VoiceChanged(VoiceState {
            user: ME,
            channel,
            muted: false,
            deafened: false,
            speaking: false,
            streaming: false,
            server_muted: false,
            server_deafened: false,
            camera: false,
            suppressed: false,
            speak_request: None,
        }));
        events
    }
}
