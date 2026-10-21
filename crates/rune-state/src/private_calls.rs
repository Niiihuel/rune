//! Calls in direct and group conversations (2026-10-17): which of them have
//! one and whom it rings, as the provider says, and how the account's own
//! went: whom it rang, who declined, who came. Who is in them is the voice
//! states of their channel, as in a voice channel.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rune_model::{CallPhase, ChannelId, ChannelKind, PrivateCall, UserId};

use crate::{Applied, Store};

/// Conversations whose call is kept at once, as Serein bounds them.
const CALLS_KEPT: usize = 64;
/// A ring that stopped this soon, without its person coming, was declined;
/// later, the service gave up on it. Discord stops ringing by itself well
/// after this (docs/DISCORD.md, «Llamadas privadas»).
pub const DECLINED_WITHIN: Duration = Duration::from_secs(20);

/// What the account sees of the call of one direct or group conversation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallStatus {
    /// It rings the account, which is not in it.
    Incoming,
    /// Others are in it without the account, and it does not ring it.
    InProgress,
    /// The account is in it from another of its devices.
    Elsewhere,
    /// The account's call here, its phase as it goes, with somebody else.
    Live,
    /// The account's call here rings the others; nobody else came yet.
    Ringing,
    /// The account's call here, alone: whom it rang never answered.
    Unanswered,
    /// The account's call here, alone: whom it rang declined.
    Declined,
    /// The account's call here, alone: the others were in it and left.
    HungUp,
    /// The account's call here, alone, ringing nobody.
    Alone,
}

/// The account's own call in a conversation: how it went.
#[derive(Debug, Default)]
struct Own {
    channel: ChannelId,
    /// Whom it rang, ever.
    rang: Vec<UserId>,
    /// Of them, who stopped it soon without coming.
    declined: Vec<UserId>,
    /// Who was in it with the account at some point.
    came: Vec<UserId>,
}

#[derive(Debug, Default)]
pub(crate) struct PrivateCalls {
    /// Whom each conversation's call rings, and since when each one.
    calls: HashMap<ChannelId, Vec<(UserId, Instant)>>,
    own: Option<Own>,
}

impl Store {
    /// A provider's word on a conversation's call: whom it rings now.
    pub(crate) fn private_call_changed(&mut self, call: PrivateCall, now: Instant) -> Applied {
        let PrivateCall {
            channel,
            mut ringing,
        } = call;
        if !self.channel(channel).is_some_and(is_private) {
            return Applied::Nothing;
        }
        ringing.truncate(PrivateCall::MAX_RINGING);
        ringing.dedup();
        let calls = &mut self.private_calls.calls;
        if !calls.contains_key(&channel) && calls.len() >= CALLS_KEPT {
            return Applied::Nothing;
        }
        let before = calls.remove(&channel).unwrap_or_default();
        let since = |user: UserId| {
            before
                .iter()
                .find(|(rung, _)| *rung == user)
                .map_or(now, |(_, since)| *since)
        };
        let after: Vec<(UserId, Instant)> =
            ringing.iter().map(|user| (*user, since(*user))).collect();
        let present = |user: UserId| {
            self.voice
                .iter()
                .any(|state| state.user == user && state.channel == channel)
        };
        if let Some(own) = self
            .private_calls
            .own
            .as_mut()
            .filter(|own| own.channel == channel)
        {
            for (user, since) in &before {
                let stopped = !ringing.contains(user);
                if stopped && !present(*user) && now.duration_since(*since) < DECLINED_WITHIN {
                    own.declined.push(*user);
                }
            }
            for user in &ringing {
                if Some(*user) != self.me && !own.rang.contains(user) {
                    own.rang.push(*user);
                }
            }
            // Rung again: how that ring ends is what counts.
            own.declined.retain(|user| !ringing.contains(user));
        }
        self.private_calls.calls.insert(channel, after);
        Applied::PrivateCall(channel)
    }

    /// A new session tells its calls again; the account's own goes on.
    pub(crate) fn private_calls_ready(&mut self) {
        self.private_calls.calls.clear();
    }

    /// The call of `channel` is over for everyone.
    pub(crate) fn private_call_ended(&mut self, channel: ChannelId) -> Applied {
        match self.private_calls.calls.remove(&channel) {
            Some(_) => Applied::PrivateCall(channel),
            None => Applied::Nothing,
        }
    }

    /// Someone is in `channel` now: of the account's call there, they came.
    pub(crate) fn private_call_joined(&mut self, user: UserId, channel: ChannelId) {
        if let Some(own) = self
            .private_calls
            .own
            .as_mut()
            .filter(|own| own.channel == channel && Some(user) != self.me)
        {
            if !own.came.contains(&user) {
                own.came.push(user);
            }
        }
    }

    /// The account starts or answers the call of `channel`: who is already
    /// in it came.
    pub(crate) fn private_call_begun(&mut self, channel: ChannelId) {
        self.private_calls.own =
            self.channel(channel)
                .filter(|found| is_private(found))
                .map(|_| Own {
                    channel,
                    came: self
                        .voice
                        .iter()
                        .filter(|state| state.channel == channel && Some(state.user) != self.me)
                        .map(|state| state.user)
                        .collect(),
                    ..Own::default()
                });
    }

    /// Whom the call of `channel` rings, the earliest first; empty without one.
    pub fn ringing(&self, channel: ChannelId) -> Vec<UserId> {
        self.private_calls
            .calls
            .get(&channel)
            .map(|ringing| ringing.iter().map(|(user, _)| *user).collect())
            .unwrap_or_default()
    }

    /// Whether `channel` has a call, as the provider said.
    #[cfg(test)]
    pub fn has_private_call(&self, channel: ChannelId) -> bool {
        self.private_calls.calls.contains_key(&channel)
    }

    /// The conversations whose call rings the account, the oldest first.
    pub fn incoming_calls(&self) -> Vec<ChannelId> {
        let Some(me) = self.me else {
            return Vec::new();
        };
        let mut incoming: Vec<(Instant, ChannelId)> = self
            .private_calls
            .calls
            .iter()
            .filter(|(channel, _)| !self.in_call_here(**channel))
            .filter_map(|(channel, ringing)| {
                let (_, since) = ringing.iter().find(|(user, _)| *user == me)?;
                Some((*since, *channel))
            })
            .collect();
        incoming.sort();
        incoming.into_iter().map(|(_, channel)| channel).collect()
    }

    /// Decline the call of `channel` here, before the provider answers.
    pub fn decline_call(&mut self, channel: ChannelId) {
        let me = self.me;
        if let Some(ringing) = self.private_calls.calls.get_mut(&channel) {
            ringing.retain(|(user, _)| Some(*user) != me);
            self.revision += 1;
        }
    }

    /// What the account sees of the call of the direct or group
    /// conversation `channel`; `None` when there is none to see.
    pub fn call_status(&self, channel: ChannelId) -> Option<CallStatus> {
        let me = self.me?;
        if !self.channel(channel).is_some_and(is_private) {
            return None;
        }
        let others = self
            .voice
            .iter()
            .any(|state| state.channel == channel && state.user != me);
        let ringing = self.private_calls.calls.get(&channel);
        let rings = |mine: bool| {
            ringing.is_some_and(|ringing| ringing.iter().any(|(user, _)| (*user == me) == mine))
        };
        if self.in_call_here(channel) {
            if others {
                return Some(CallStatus::Live);
            }
            if rings(false) {
                return Some(CallStatus::Ringing);
            }
            let own = self
                .private_calls
                .own
                .as_ref()
                .filter(|own| own.channel == channel);
            return Some(match own {
                Some(own) if !own.came.is_empty() => CallStatus::HungUp,
                Some(own)
                    if !own.rang.is_empty()
                        && own.rang.iter().all(|user| own.declined.contains(user)) =>
                {
                    CallStatus::Declined
                }
                Some(own) if !own.rang.is_empty() => CallStatus::Unanswered,
                _ => CallStatus::Alone,
            });
        }
        if rings(true) {
            return Some(CallStatus::Incoming);
        }
        if self
            .my_voice()
            .is_some_and(|state| state.channel == channel)
        {
            return Some(CallStatus::Elsewhere);
        }
        (others || ringing.is_some()).then_some(CallStatus::InProgress)
    }

    /// The account's call is in `channel` and has not failed.
    fn in_call_here(&self, channel: ChannelId) -> bool {
        self.call.as_ref().is_some_and(|call| {
            call.channel == channel && !matches!(call.phase, CallPhase::Failed { .. })
        })
    }
}

fn is_private(channel: &rune_model::Channel) -> bool {
    matches!(channel.kind, ChannelKind::Direct | ChannelKind::Group)
}
