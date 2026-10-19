//! Live reactions and poll votes. Both are changes, not totals, so each one
//! must reach the state once. The Gateway already drops a dispatch whose
//! sequence it has seen (a resume replays only what follows); this adds a
//! per-account record of the last change forwarded for each reaction or
//! answer, so a repeated dispatch is dropped even without that guarantee.

use std::collections::{HashMap, VecDeque};

use rune_model::{ChannelId, MessageId, UserId};
use rune_provider::{Event, Snapshot};
use serde_json::Value;

use crate::wire;

// shortcut: the oldest records are forgotten past this many; a repeat of a
// forgotten change would count again. Upgrade to per-message bounds if seen.
const LIMIT: usize = 4096;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
enum Choice {
    /// Its emoji, and whether it is a burst reaction, kept apart from the
    /// account's normal one as the service does.
    Reaction(String, bool),
    Answer(u32),
}

type Key = (ChannelId, MessageId, UserId, Choice);

/// The last change forwarded per account, reaction or answer: `true` added.
/// A new session (READY) starts it empty with the rest of the adapter cache.
#[derive(Default)]
pub(crate) struct Tallies {
    last: HashMap<Key, bool>,
    order: VecDeque<Key>,
}

impl Tallies {
    /// Map one dispatch of `channel`; `None` for other kinds. A malformed
    /// or custom-emoji change is dropped: it is decoration, not a reason to
    /// lose the connection, and custom reactions are not shown either.
    pub fn dispatch(
        &mut self,
        snapshot: &Snapshot,
        kind: &str,
        data: &Value,
        channel: ChannelId,
    ) -> Option<Vec<Event>> {
        let added = match kind {
            "MESSAGE_REACTION_ADD" | "MESSAGE_POLL_VOTE_ADD" => true,
            "MESSAGE_REACTION_REMOVE" | "MESSAGE_POLL_VOTE_REMOVE" => false,
            "MESSAGE_REACTION_REMOVE_ALL" | "MESSAGE_REACTION_REMOVE_EMOJI" => {
                return Some(self.cleared(kind, data, channel).into_iter().collect())
            }
            _ => return None,
        };
        Some(
            self.changed(snapshot, kind, data, channel, added)
                .into_iter()
                .collect(),
        )
    }

    fn changed(
        &mut self,
        snapshot: &Snapshot,
        kind: &str,
        data: &Value,
        channel: ChannelId,
        added: bool,
    ) -> Option<Event> {
        let message = MessageId(wire::id(&data["message_id"]).ok()?);
        let user = UserId(wire::id(&data["user_id"]).ok()?);
        let mine = snapshot.me.as_ref().is_some_and(|me| me.id == user);
        let (choice, event) = if kind.starts_with("MESSAGE_POLL_VOTE") {
            let answer = u32::try_from(data["answer_id"].as_u64()?).ok()?;
            (
                Choice::Answer(answer),
                Event::PollVoteChanged {
                    channel,
                    message,
                    answer,
                    me: mine,
                    added,
                },
            )
        } else {
            let emoji = wire::reaction_emoji(&data["emoji"]).ok()??;
            // `type` is 0 (normal) or 1 (super); older events only say
            // `burst`, the oldest neither. An unknown type, or one `burst`
            // contradicts, is dropped rather than counted as either kind.
            let field = |name| data.get(name).filter(|value| !value.is_null());
            let burst = match (
                field("type").map(Value::as_u64),
                field("burst").map(Value::as_bool),
            ) {
                (Some(Some(kind @ (0 | 1))), None) => kind == 1,
                (Some(Some(kind @ (0 | 1))), Some(Some(burst))) if burst == (kind == 1) => burst,
                (None, Some(Some(burst))) => burst,
                (None, None) => false,
                _ => return None,
            };
            let event = if burst {
                Event::BurstReactionChanged {
                    channel,
                    message,
                    emoji: emoji.clone(),
                    me: mine,
                    added,
                    colors: wire::burst_colors(&data["burst_colors"]),
                }
            } else {
                Event::ReactionChanged {
                    channel,
                    message,
                    emoji: emoji.clone(),
                    me: mine,
                    added,
                }
            };
            (Choice::Reaction(emoji, burst), event)
        };
        self.admit((channel, message, user, choice), added)
            .then_some(event)
    }

    fn cleared(&mut self, kind: &str, data: &Value, channel: ChannelId) -> Option<Event> {
        let message = MessageId(wire::id(&data["message_id"]).ok()?);
        let emoji = if kind == "MESSAGE_REACTION_REMOVE_EMOJI" {
            Some(wire::reaction_emoji(&data["emoji"]).ok()??)
        } else {
            None
        };
        self.last.retain(|(at, id, _, choice), _| {
            !(*at == channel
                && *id == message
                && matches!(choice, Choice::Reaction(shown, _)
                    if emoji.as_ref().is_none_or(|emoji| emoji == shown)))
        });
        let last = &self.last;
        self.order.retain(|key| last.contains_key(key));
        Some(Event::ReactionsCleared {
            channel,
            message,
            emoji,
        })
    }

    /// Record a confirmed change of `user`'s normal reaction; whether it is new.
    pub fn own_reaction(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        user: UserId,
        emoji: &str,
        added: bool,
    ) -> bool {
        self.admit(
            (
                channel,
                message,
                user,
                Choice::Reaction(emoji.to_owned(), false),
            ),
            added,
        )
    }

    /// Whether this change differs from the last one forwarded for its key.
    fn admit(&mut self, key: Key, added: bool) -> bool {
        match self.last.insert(key.clone(), added) {
            Some(previous) => previous != added,
            None => {
                self.order.push_back(key);
                if self.order.len() > LIMIT {
                    if let Some(oldest) = self.order.pop_front() {
                        self.last.remove(&oldest);
                    }
                }
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{Rgb, User};
    use serde_json::json;

    fn account() -> Snapshot {
        Snapshot {
            me: Some(User {
                identity: Default::default(),
                avatar: None,
                banner: Default::default(),
                id: UserId(42),
                name: "Owner".into(),
                display_name: None,
                accent: Rgb(0),
                bot: false,
                pronouns: None,
                about: None,
                default_avatar: None,
            }),
            ..Default::default()
        }
    }

    fn map(tallies: &mut Tallies, kind: &str, data: Value) -> Vec<Event> {
        tallies
            .dispatch(&account(), kind, &data, ChannelId(20))
            .unwrap()
    }

    fn reaction(user: u64, name: &str) -> Value {
        json!({"channel_id":"20","message_id":"100","user_id":user.to_string(),
            "emoji":{"id":null,"name":name}})
    }

    #[test]
    fn repeated_changes_are_forwarded_once_and_me_is_only_the_account() {
        let mut tallies = Tallies::default();
        let added = map(&mut tallies, "MESSAGE_REACTION_ADD", reaction(43, "🦀"));
        assert_eq!(
            added,
            [Event::ReactionChanged {
                channel: ChannelId(20),
                message: MessageId(100),
                emoji: "🦀".into(),
                me: false,
                added: true,
            }]
        );
        assert!(
            map(&mut tallies, "MESSAGE_REACTION_ADD", reaction(43, "🦀")).is_empty(),
            "a replayed dispatch"
        );
        let mine = map(&mut tallies, "MESSAGE_REACTION_ADD", reaction(42, "🦀"));
        assert!(matches!(
            &mine[..],
            [Event::ReactionChanged { me: true, .. }]
        ));
        let mut burst = reaction(42, "🦀");
        burst["burst"] = json!(true);
        burst["burst_colors"] = json!(["#ff8800", "nada", "00ff00"]);
        let burst = map(&mut tallies, "MESSAGE_REACTION_ADD", burst);
        assert_eq!(
            burst,
            [Event::BurstReactionChanged {
                channel: ChannelId(20),
                message: MessageId(100),
                emoji: "🦀".into(),
                me: true,
                added: true,
                colors: vec![Rgb(0xff8800), Rgb(0x00ff00)],
            }],
            "a super reaction is counted apart from the account's own"
        );
        // `type` says which kind it is; an unknown one, or one `burst`
        // contradicts, is dropped.
        for (kind, burst) in [
            (json!(0), json!(true)),
            (json!(1), json!(false)),
            (json!(2), json!(null)),
        ] {
            let mut odd = reaction(44, "🦀");
            odd["type"] = kind;
            odd["burst"] = burst;
            assert!(map(&mut tallies, "MESSAGE_REACTION_ADD", odd).is_empty());
        }
        let mut normal = reaction(44, "🦀");
        normal["type"] = json!(0);
        normal["burst"] = json!(false);
        assert!(matches!(
            &map(&mut tallies, "MESSAGE_REACTION_ADD", normal)[..],
            [Event::ReactionChanged { .. }]
        ));
        let mut typed = reaction(44, "🦀");
        typed["type"] = json!(1);
        assert!(matches!(
            &map(&mut tallies, "MESSAGE_REACTION_REMOVE", typed)[..],
            [Event::BurstReactionChanged { added: false, .. }]
        ));
        assert_eq!(
            map(&mut tallies, "MESSAGE_REACTION_REMOVE", reaction(43, "🦀")).len(),
            1
        );
        assert!(map(&mut tallies, "MESSAGE_REACTION_REMOVE", reaction(43, "🦀")).is_empty());
        assert_eq!(
            map(&mut tallies, "MESSAGE_REACTION_ADD", reaction(43, "🦀")).len(),
            1,
            "added again after taking it back"
        );
        let vote = |user: u64| json!({"channel_id":"20","message_id":"100","user_id":user.to_string(),"answer_id":2});
        let voted = map(&mut tallies, "MESSAGE_POLL_VOTE_ADD", vote(42));
        assert_eq!(
            voted,
            [Event::PollVoteChanged {
                channel: ChannelId(20),
                message: MessageId(100),
                answer: 2,
                me: true,
                added: true,
            }]
        );
        assert!(map(&mut tallies, "MESSAGE_POLL_VOTE_ADD", vote(42)).is_empty());
        assert_eq!(
            map(&mut tallies, "MESSAGE_POLL_VOTE_ADD", vote(44)).len(),
            1
        );
        assert_eq!(
            map(&mut tallies, "MESSAGE_POLL_VOTE_REMOVE", vote(42)).len(),
            1
        );
    }

    #[test]
    fn clearing_forgets_the_record_and_custom_or_malformed_changes_are_dropped() {
        let mut tallies = Tallies::default();
        map(&mut tallies, "MESSAGE_REACTION_ADD", reaction(43, "🦀"));
        map(&mut tallies, "MESSAGE_REACTION_ADD", reaction(43, "🎉"));
        let cleared = map(
            &mut tallies,
            "MESSAGE_REACTION_REMOVE_EMOJI",
            json!({"channel_id":"20","message_id":"100","emoji":{"id":null,"name":"🦀"}}),
        );
        assert_eq!(
            cleared,
            [Event::ReactionsCleared {
                channel: ChannelId(20),
                message: MessageId(100),
                emoji: Some("🦀".into()),
            }]
        );
        assert_eq!(
            map(&mut tallies, "MESSAGE_REACTION_ADD", reaction(43, "🦀")).len(),
            1
        );
        assert!(map(&mut tallies, "MESSAGE_REACTION_ADD", reaction(43, "🎉")).is_empty());
        map(
            &mut tallies,
            "MESSAGE_REACTION_REMOVE_ALL",
            json!({"channel_id":"20","message_id":"100"}),
        );
        assert_eq!(
            map(&mut tallies, "MESSAGE_REACTION_ADD", reaction(43, "🎉")).len(),
            1
        );
        let mut custom = reaction(43, "propio");
        custom["emoji"]["id"] = json!("9");
        assert!(map(&mut tallies, "MESSAGE_REACTION_ADD", custom).is_empty());
        for broken in [
            json!({"channel_id":"20","message_id":"100","user_id":"43"}),
            json!({"channel_id":"20","message_id":"x","user_id":"43","answer_id":1}),
            json!({"channel_id":"20","message_id":"100","user_id":"43","answer_id":-1}),
        ] {
            assert!(map(&mut tallies, "MESSAGE_POLL_VOTE_ADD", broken.clone()).is_empty());
            assert!(map(&mut tallies, "MESSAGE_REACTION_ADD", broken).is_empty());
        }
        assert!(tallies
            .dispatch(&account(), "MESSAGE_CREATE", &json!({}), ChannelId(20))
            .is_none());
    }

    #[test]
    fn the_record_stays_bounded() {
        let mut tallies = Tallies::default();
        for user in 0..LIMIT as u64 + 10 {
            map(
                &mut tallies,
                "MESSAGE_REACTION_ADD",
                reaction(1000 + user, "🦀"),
            );
        }
        assert_eq!(tallies.last.len(), LIMIT);
        assert_eq!(tallies.order.len(), LIMIT);
    }
}
