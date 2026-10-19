//! Live changes of a message's reactions and poll votes (2026-10-18). Each is
//! a step, not a total, applied alike to what the interface shows and to what
//! the local store keeps. Super reactions are counted apart from normal ones,
//! as services count them; RUNE shows them but never sends one.

use crate::{Message, Reaction, Rgb};

/// The most colors a super reaction keeps.
pub const MAX_BURST_COLORS: usize = 8;

/// One live change of a message's reactions or poll.
#[derive(Clone, Debug, PartialEq)]
pub enum Tally {
    /// Someone, the account when `me`, added or took back a reaction.
    Reaction {
        emoji: String,
        me: bool,
        added: bool,
    },
    /// The same with a super reaction, and the colors the service gave it.
    Burst {
        emoji: String,
        me: bool,
        added: bool,
        colors: Vec<Rgb>,
    },
    /// Every reaction, or only `emoji`'s, was taken away.
    Cleared { emoji: Option<String> },
    /// Someone added or took back a vote for `answer`.
    Vote { answer: u32, me: bool, added: bool },
}

impl Tally {
    /// Apply it to `message`; whether anything changed.
    pub fn apply(&self, message: &mut Message) -> bool {
        match self {
            Self::Reaction { emoji, me, added } => message.count_reaction(emoji, *me, *added),
            Self::Burst {
                emoji,
                me,
                added,
                colors,
            } => message.count_burst(emoji, *me, *added, colors),
            Self::Cleared { emoji } => message.clear_reactions(emoji.as_deref()),
            Self::Vote { answer, me, added } => message
                .poll
                .as_mut()
                .is_some_and(|poll| poll.count_vote(*answer, *me, *added)),
        }
    }
}

impl Message {
    /// One account added or took back a super reaction with `emoji`, as
    /// [`count_reaction`](Self::count_reaction) does with a normal one. The
    /// first colors given stay, as the service's client keeps them.
    pub fn count_burst(&mut self, emoji: &str, me: bool, added: bool, colors: &[Rgb]) -> bool {
        let colors = &colors[..colors.len().min(MAX_BURST_COLORS)];
        let Some(index) = self
            .reactions
            .iter()
            .position(|reaction| reaction.emoji == emoji)
        else {
            if added {
                self.reactions.push(Reaction {
                    emoji: emoji.to_owned(),
                    burst: 1,
                    me_burst: me,
                    burst_colors: colors.to_vec(),
                    ..Default::default()
                });
            }
            return added;
        };
        let reaction = &mut self.reactions[index];
        if me {
            if reaction.me_burst == added {
                return false;
            }
            reaction.me_burst = added;
        } else if !added && reaction.burst <= u32::from(reaction.me_burst) {
            return false;
        }
        if added {
            reaction.burst = reaction.burst.saturating_add(1);
            if reaction.burst_colors.is_empty() {
                reaction.burst_colors = colors.to_vec();
            }
        } else {
            reaction.burst = reaction.burst.saturating_sub(1);
            if reaction.count == 0 && reaction.burst == 0 {
                self.reactions.remove(index);
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ChannelId, Delivery, MessageId, MessageKind, Timestamp, UserId};

    fn message() -> Message {
        Message {
            id: MessageId(1),
            channel: ChannelId(1),
            author: UserId(1),
            content: String::new(),
            timestamp: Timestamp(0),
            edited: None,
            kind: MessageKind::Default,
            reply_to: None,
            mentions: Vec::new(),
            reactions: vec![Reaction {
                emoji: "👍".into(),
                count: 2,
                ..Default::default()
            }],
            attachments: Vec::new(),
            stickers: Vec::new(),
            text_references: Vec::new(),
            delivery: Delivery::Sent,
            pinned: false,
            embeds: Vec::new(),
            forwarded: None,
            poll: None,
            components: Default::default(),
            call: None,
            mention_roles: Vec::new(),
            mention_everyone: false,
        }
    }

    fn burst(emoji: &str, me: bool, added: bool) -> Tally {
        Tally::Burst {
            emoji: emoji.into(),
            me,
            added,
            colors: vec![Rgb(0xff_00_00), Rgb(0x00_ff_00)],
        }
    }

    #[test]
    fn super_reactions_count_apart_from_normal_ones() {
        let mut message = message();
        assert!(burst("👍", true, true).apply(&mut message));
        assert!(!burst("👍", true, true).apply(&mut message), "once");
        assert!(burst("👍", false, true).apply(&mut message));
        let shown = &message.reactions[0];
        assert_eq!((shown.count, shown.me), (2, false));
        assert_eq!((shown.burst, shown.me_burst), (2, true));
        assert_eq!(shown.burst_colors, [Rgb(0xff_00_00), Rgb(0x00_ff_00)]);
        // The account's normal reaction is its own, beside its super one.
        assert!(message.set_reaction("👍", true));
        assert!(!message.set_reaction("👍", true));
        assert_eq!(
            (message.reactions[0].count, message.reactions[0].me),
            (3, true)
        );
        // Others never take away the account's super reaction.
        assert!(burst("👍", false, false).apply(&mut message));
        assert!(!burst("👍", false, false).apply(&mut message));
        assert_eq!(message.reactions[0].burst, 1);
        // Gone when neither kind is left.
        for change in [
            burst("👍", true, false),
            Tally::Reaction {
                emoji: "👍".into(),
                me: false,
                added: false,
            },
            Tally::Reaction {
                emoji: "👍".into(),
                me: false,
                added: false,
            },
        ] {
            assert!(change.apply(&mut message));
        }
        assert_eq!(message.reactions[0].count, 1, "the account's own stays");
        assert!(message.set_reaction("👍", false));
        assert!(message.reactions.is_empty());
    }

    #[test]
    fn a_super_reaction_alone_survives_normal_toggles_and_keeps_few_colors() {
        let mut message = message();
        message.reactions.clear();
        let many = Tally::Burst {
            emoji: "🔥".into(),
            me: false,
            added: true,
            colors: vec![Rgb(1); 20],
        };
        assert!(many.apply(&mut message));
        assert_eq!(message.reactions[0].burst_colors.len(), MAX_BURST_COLORS);
        message.toggle_reaction("🔥");
        message.toggle_reaction("🔥");
        assert_eq!(
            (message.reactions[0].count, message.reactions[0].burst),
            (0, 1),
            "a normal toggle back leaves the super reaction"
        );
        assert!(Tally::Cleared {
            emoji: Some("🔥".into())
        }
        .apply(&mut message));
        assert!(message.reactions.is_empty());
    }
}
