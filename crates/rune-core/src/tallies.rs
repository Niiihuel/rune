//! Live reactions and votes (2026-10-18). Each change the provider reports
//! also reaches the local store, applied to the retained row, so a restart
//! shows it without reloading the history. A vote has a deadline of the
//! client's own: a provider that never answers cannot leave it shown.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use rune_i18n::{t, tr};
use rune_model::{ChannelId, MessageId, Tally};
use rune_provider::{Command, Event, Failure};
use rune_storage::MessageMutation;

/// The change a live event makes to one stored message, if it is one.
pub(crate) fn mutation(event: &Event) -> Option<MessageMutation> {
    let (channel, message, change) = match event {
        Event::ReactionChanged {
            channel,
            message,
            emoji,
            me,
            added,
        } => (
            channel,
            message,
            Tally::Reaction {
                emoji: emoji.clone(),
                me: *me,
                added: *added,
            },
        ),
        Event::BurstReactionChanged {
            channel,
            message,
            emoji,
            me,
            added,
            colors,
        } => (
            channel,
            message,
            Tally::Burst {
                emoji: emoji.clone(),
                me: *me,
                added: *added,
                colors: colors.clone(),
            },
        ),
        Event::ReactionsCleared {
            channel,
            message,
            emoji,
        } => (
            channel,
            message,
            Tally::Cleared {
                emoji: emoji.clone(),
            },
        ),
        Event::PollVoteChanged {
            channel,
            message,
            answer,
            me,
            added,
        } => (
            channel,
            message,
            Tally::Vote {
                answer: *answer,
                me: *me,
                added: *added,
            },
        ),
        _ => return None,
    };
    Some(MessageMutation::Tally {
        channel: *channel,
        message: *message,
        change,
    })
}

/// The account's votes sent and not answered yet, by message.
#[derive(Debug, Default)]
pub(crate) struct Votes {
    waiting: HashMap<(ChannelId, MessageId), Instant>,
}

impl Votes {
    /// The message a vote is for, if `command` is one.
    pub fn of(command: &Command) -> Option<(ChannelId, MessageId)> {
        match command {
            Command::VotePoll {
                channel, message, ..
            } => Some((*channel, *message)),
            _ => None,
        }
    }

    /// The vote for `key` was admitted at `now`.
    pub fn sent(&mut self, key: (ChannelId, MessageId), now: Instant) {
        self.waiting.insert(key, now);
    }

    /// The store let the client go on at `now` after holding it since `paused`.
    pub fn resume(&mut self, paused: Instant, now: Instant) {
        for since in self.waiting.values_mut() {
            *since += now.saturating_duration_since(paused.max(*since));
        }
    }

    /// What answers a vote: the account's own change, the whole message, or
    /// a failure. Something else of the message does not.
    pub fn observe(&mut self, event: &Event) {
        let key = match event {
            Event::PollVoteChanged {
                channel,
                message,
                me: true,
                ..
            }
            | Event::MessageDeleted { channel, message }
            | Event::Failed {
                failure: Failure::Vote { channel, message },
                ..
            } => (*channel, *message),
            Event::MessageUpdated(message) => (message.channel, message.id),
            _ => return,
        };
        self.waiting.remove(&key);
    }

    /// A verification asked for at `now` moves its vote's deadline past it.
    pub fn challenged(&mut self, failure: &Failure, now: Instant) {
        if let Failure::Vote { channel, message } = failure {
            if let Some(since) = self.waiting.get_mut(&(*channel, *message)) {
                *since = now + rune_provider::CHALLENGE_LIFETIME;
            }
        }
    }

    /// Fail every vote, as nobody can say whether it arrived.
    pub fn lost(&mut self, found: &mut impl Extend<Event>) {
        found.extend(
            self.waiting
                .drain()
                .map(|((channel, message), _)| Event::Failed {
                    failure: Failure::Vote { channel, message },
                    reason: t("se perdió la conexión antes de la confirmación").into(),
                    ambiguous: true,
                }),
        );
    }

    pub fn clear(&mut self) {
        self.waiting.clear();
    }

    /// Fail the votes waiting `limit` or longer at `now`.
    pub fn expire(&mut self, now: Instant, limit: Duration, found: &mut impl Extend<Event>) {
        let late: Vec<_> = self
            .waiting
            .iter()
            .filter(|(_, since)| now.saturating_duration_since(**since) >= limit)
            .map(|(key, _)| *key)
            .collect();
        for (channel, message) in late {
            self.waiting.remove(&(channel, message));
            found.extend([Event::Failed {
                failure: Failure::Vote { channel, message },
                reason: tr!("sin confirmación en {seconds} s", seconds = limit.as_secs()),
                ambiguous: true,
            }]);
        }
    }

    /// When the first vote waiting falls due.
    pub fn deadline(&self, limit: Duration) -> Option<Instant> {
        self.waiting.values().map(|since| *since + limit).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vote() -> Command {
        Command::VotePoll {
            channel: ChannelId(2),
            message: MessageId(3),
            answers: vec![1],
        }
    }

    #[test]
    fn a_vote_waits_until_answered_or_late() {
        let start = Instant::now();
        let limit = Duration::from_secs(30);
        let mut votes = Votes::default();
        let mut found = Vec::new();
        let key = Votes::of(&vote()).unwrap();
        votes.sent(key, start);
        assert_eq!(votes.deadline(limit), Some(start + limit));
        // Someone else's vote, or another message's, is no answer.
        votes.observe(&Event::PollVoteChanged {
            channel: ChannelId(2),
            message: MessageId(3),
            answer: 1,
            me: false,
            added: true,
        });
        votes.expire(start + limit / 2, limit, &mut found);
        assert!(found.is_empty());
        votes.expire(start + limit, limit, &mut found);
        assert!(matches!(
            &found[..],
            [Event::Failed {
                failure: Failure::Vote { .. },
                ambiguous: true,
                ..
            }]
        ));
        assert_eq!(votes.deadline(limit), None);

        found.clear();
        votes.sent(key, start);
        votes.observe(&Event::PollVoteChanged {
            channel: ChannelId(2),
            message: MessageId(3),
            answer: 1,
            me: true,
            added: true,
        });
        votes.expire(start + limit * 2, limit, &mut found);
        assert!(found.is_empty(), "answered");

        votes.sent(key, start);
        votes.challenged(
            &Failure::Vote {
                channel: ChannelId(2),
                message: MessageId(3),
            },
            start,
        );
        votes.expire(start + limit, limit, &mut found);
        assert!(found.is_empty(), "a person is solving a verification");
        votes.lost(&mut found);
        assert_eq!(found.len(), 1);

        // Time the store held the client does not count.
        votes.sent(key, start);
        votes.resume(start, start + limit);
        votes.expire(start + limit, limit, &mut found);
        assert_eq!(found.len(), 1);
        assert_eq!(Votes::of(&Command::LoadGifFavorites), None);
    }

    #[test]
    fn live_changes_become_stored_steps() {
        let changed = Event::BurstReactionChanged {
            channel: ChannelId(2),
            message: MessageId(3),
            emoji: "🔥".into(),
            me: false,
            added: true,
            colors: vec![rune_model::Rgb(1)],
        };
        assert!(matches!(
            mutation(&changed),
            Some(MessageMutation::Tally {
                change: Tally::Burst { .. },
                ..
            })
        ));
        assert!(mutation(&Event::ReactionsCleared {
            channel: ChannelId(2),
            message: MessageId(3),
            emoji: None,
        })
        .is_some());
        assert!(mutation(&Event::MessageDeleted {
            channel: ChannelId(2),
            message: MessageId(3),
        })
        .is_none());
    }
}
