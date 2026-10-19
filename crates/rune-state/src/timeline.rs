//! One conversation's loaded messages: a contiguous run, oldest first, that
//! ends at the newest once `latest` is set, plus the account's messages not
//! confirmed yet, which always sort last.
//! An explicit context window is partial; cached neighbors make no coverage claim.
//!
//! A page from the provider is the truth for the range it covers: messages
//! of that range the page does not hold were deleted while nobody was
//! looking and leave; a newest page that does not reach the messages already
//! held starts the run again rather than leave a gap that would never fill.

use std::collections::VecDeque;
use std::sync::Arc;

use rune_model::{Message, MessageId};

/// Deleted messages remembered, so a page fetched before a deletion does not
/// bring the message back.
const TOMBSTONES: usize = 256;

#[derive(Clone, Debug, Default)]
pub struct Timeline {
    messages: Arc<Vec<Arc<Message>>>,
    /// The first message of the conversation is loaded.
    pub reached_start: bool,
    /// The newest messages are loaded, so new ones can be added at the end.
    pub latest: bool,
    context: Option<rune_model::HistorySource>,
    deleted: VecDeque<MessageId>,
}

impl Timeline {
    /// A partial navigation window, distinct from a latest history run.
    pub fn context_source(&self) -> Option<rune_model::HistorySource> {
        self.context
    }

    pub(crate) fn clear_context(&mut self) {
        if self.context.take().is_some() {
            Arc::make_mut(&mut self.messages).retain(|message| message.delivery.is_local());
            self.latest = false;
            self.reached_start = false;
        }
    }

    pub(crate) fn apply_context(&mut self, page: Vec<Message>, source: rune_model::HistorySource) {
        Arc::make_mut(&mut self.messages).retain(|message| message.delivery.is_local());
        for message in page {
            self.upsert(message);
        }
        self.latest = false;
        self.reached_start = false;
        self.context = Some(source);
    }

    pub(crate) fn is_deleted(&self, id: MessageId) -> bool {
        self.deleted.contains(&id)
    }

    /// The loaded messages, oldest first, shared: cheap to keep for a frame.
    pub fn messages(&self) -> Arc<Vec<Arc<Message>>> {
        self.messages.clone()
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    /// The oldest message the service confirmed: where an older page starts.
    pub fn oldest(&self) -> Option<MessageId> {
        self.confirmed().next().map(|message| message.id)
    }

    /// The newest message the service confirmed.
    pub fn newest(&self) -> Option<&Message> {
        self.confirmed().next_back().map(Arc::as_ref)
    }

    fn confirmed(&self) -> impl DoubleEndedIterator<Item = &Arc<Message>> {
        self.messages
            .iter()
            .filter(|message| !message.delivery.is_local())
    }

    pub fn get(&self, id: MessageId) -> Option<&Arc<Message>> {
        self.position(id).ok().map(|index| &self.messages[index])
    }

    fn position(&self, id: MessageId) -> Result<usize, usize> {
        self.messages
            .binary_search_by_key(&id, |message| message.id)
    }

    /// Add `message`, or replace the one it updates, keeping the order. An
    /// older edit than the one held, or a message deleted lately, is ignored.
    pub(crate) fn upsert(&mut self, message: Message) -> bool {
        if self.deleted.contains(&message.id) {
            return false;
        }
        match self.position(message.id) {
            Ok(index) => {
                let held = &self.messages[index];
                if held.edited > message.edited {
                    return false;
                }
                Arc::make_mut(&mut self.messages)[index] = Arc::new(message);
            }
            Err(index) => Arc::make_mut(&mut self.messages).insert(index, Arc::new(message)),
        }
        true
    }

    /// Put `message` back exactly as it was, edit date or not: undoing a
    /// change made here that the service refused.
    pub(crate) fn restore(&mut self, message: Arc<Message>) {
        if let Ok(index) = self.position(message.id) {
            Arc::make_mut(&mut self.messages)[index] = message;
        }
    }

    /// Take `id` out and remember it was deleted.
    pub(crate) fn delete(&mut self, id: MessageId) -> Option<Arc<Message>> {
        self.deleted.push_back(id);
        if self.deleted.len() > TOMBSTONES {
            self.deleted.pop_front();
        }
        self.remove(id)
    }

    /// Take `id` out, as when a message shown here is replaced.
    pub(crate) fn remove(&mut self, id: MessageId) -> Option<Arc<Message>> {
        let index = self.position(id).ok()?;
        Some(Arc::make_mut(&mut self.messages).remove(index))
    }

    /// Apply a page of history: `before` it, or the newest when `None`.
    pub(crate) fn apply_page(
        &mut self,
        page: Vec<Message>,
        before: Option<MessageId>,
        reached_start: bool,
    ) {
        if before.is_none() {
            self.clear_context();
        } else if self.context.is_some() {
            return;
        }
        let end = before.unwrap_or(MessageId(u64::MAX));
        let start = match page.first() {
            Some(first) => first.id,
            // An empty page at the start: nothing is left before `end`.
            None if reached_start => MessageId(0),
            None => end,
        };
        let messages = Arc::make_mut(&mut self.messages);
        if before.is_none() {
            // The newest page reaches what was held only if what was held
            // goes as far as its first message. If not, a run of older
            // messages would sit before a gap nothing would fill: drop it.
            let held_newest = messages
                .iter()
                .rev()
                .find(|message| !message.delivery.is_local())
                .map(|message| message.id);
            if held_newest.is_some_and(|newest| newest < start) {
                messages.retain(|message| message.delivery.is_local());
                self.reached_start = false;
            }
        }
        let in_page: std::collections::HashSet<MessageId> =
            page.iter().map(|message| message.id).collect();
        // What the range held that the page does not was deleted meanwhile.
        messages.retain(|message| {
            message.delivery.is_local()
                || message.id < start
                || message.id >= end
                || in_page.contains(&message.id)
        });
        for message in page {
            self.upsert(message);
        }
        if before.is_none() {
            self.latest = true;
        }
        if reached_start {
            self.reached_start = true;
        }
    }

    /// Keep only the newest `keep` messages.
    pub(crate) fn trim(&mut self, keep: usize) {
        if self.messages.len() > keep {
            let messages = Arc::make_mut(&mut self.messages);
            messages.drain(..messages.len() - keep);
            self.reached_start = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{ChannelId, Delivery, MessageKind, Timestamp, UserId};

    fn message(id: u64) -> Message {
        Message {
            id: MessageId(id),
            channel: ChannelId(1),
            author: UserId(2),
            content: format!("{id}"),
            timestamp: Timestamp(id as i64),
            edited: None,
            kind: MessageKind::Default,
            reply_to: None,
            mentions: Vec::new(),
            reactions: Vec::new(),
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

    fn page(ids: impl IntoIterator<Item = u64>) -> Vec<Message> {
        ids.into_iter().map(message).collect()
    }

    fn ids(timeline: &Timeline) -> Vec<u64> {
        timeline
            .messages()
            .iter()
            .map(|message| message.id.0)
            .collect()
    }

    #[test]
    fn a_newest_page_that_does_not_reach_what_was_held_starts_again() {
        let mut timeline = Timeline::default();
        timeline.apply_page(page(1..=10), None, true);
        let mut sending = message(u64::MAX - 1);
        sending.delivery = Delivery::Sending;
        timeline.upsert(sending);
        // Eighty messages later the newest page starts at 81: 11..81 are missing.
        timeline.apply_page(page(81..=90), None, false);
        assert_eq!(
            ids(&timeline),
            (81..=90).chain([u64::MAX - 1]).collect::<Vec<_>>()
        );
        assert!(
            !timeline.reached_start,
            "what was dropped can be fetched again"
        );
    }

    #[test]
    fn a_page_removes_what_was_deleted_in_its_range_and_keeps_the_rest() {
        let mut timeline = Timeline::default();
        timeline.apply_page(page(1..=20), None, true);
        // The newest page again, without 15 and 18: they were deleted meanwhile.
        timeline.apply_page(
            page((11..=21).filter(|id| *id != 15 && *id != 18)),
            None,
            false,
        );
        assert_eq!(
            ids(&timeline),
            (1..=21)
                .filter(|id| *id != 15 && *id != 18)
                .collect::<Vec<_>>()
        );
        // An older page: 3 is gone too.
        timeline.apply_page(
            page((1..=10).filter(|id| *id != 3)),
            Some(MessageId(11)),
            true,
        );
        assert!(!ids(&timeline).contains(&3));
    }

    #[test]
    fn a_message_deleted_lately_is_not_brought_back_by_a_page() {
        let mut timeline = Timeline::default();
        timeline.apply_page(page(1..=5), None, true);
        timeline.delete(MessageId(4));
        timeline.apply_page(page(1..=5), None, true);
        assert_eq!(ids(&timeline), vec![1, 2, 3, 5]);
    }

    #[test]
    fn an_older_edit_does_not_undo_a_newer_one() {
        let mut timeline = Timeline::default();
        let mut newer = message(1);
        newer.content = "nuevo".into();
        newer.edited = Some(Timestamp(50));
        timeline.upsert(newer);
        let mut older = message(1);
        older.edited = Some(Timestamp(10));
        assert!(!timeline.upsert(older));
        assert_eq!(timeline.get(MessageId(1)).unwrap().content, "nuevo");
    }
}
