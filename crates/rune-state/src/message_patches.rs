//! Partial confirmed messages reconcile retained domain data and optimistic undo.

use std::sync::Arc;

use rune_model::{ChannelId, Message, MessageId, MessagePatch};

use crate::{Applied, Store};

impl Store {
    pub(super) fn patched(&mut self, patch: MessagePatch) -> Applied {
        let (channel, id) = (patch.channel, patch.id);
        if id.0 == 0
            || !self
                .channel(channel)
                .is_some_and(|channel| Self::visible(channel) && channel.kind.has_messages())
            || self
                .timelines
                .get(&channel)
                .is_some_and(|timeline| timeline.is_deleted(id))
        {
            return Applied::Nothing;
        }
        let visible = self
            .timelines
            .get(&channel)
            .and_then(|timeline| timeline.get(id))
            .cloned();
        let pinned = self.retained_pin(channel, id).cloned();
        if visible.is_none() && pinned.is_none() {
            self.unknown_pin_patch(&patch);
            return Applied::Nothing;
        }
        let undo = self.before_change.get(&(channel, id)).cloned();
        let baseline = undo
            .as_deref()
            .or(visible.as_deref())
            .into_iter()
            .chain(pinned.as_ref())
            .max_by_key(|message| message.edited)
            .unwrap();
        if visible
            .as_deref()
            .into_iter()
            .chain(pinned.as_ref())
            .chain(undo.as_deref())
            .any(|message| {
                message.channel != baseline.channel
                    || message.id != baseline.id
                    || message.author != baseline.author
                    || message.delivery.is_local()
            })
        {
            return Applied::Nothing;
        }
        let mut confirmed = baseline.clone();
        if !patch.apply(&mut confirmed) {
            return Applied::Nothing;
        }
        let pin_after = pinned.as_ref().map(|previous| {
            let mut message = confirmed.clone();
            if patch.pinned.is_none() {
                message.pinned = previous.pinned;
            }
            message
        });
        let mut shown = visible.as_deref().map(|message| match undo.as_deref() {
            Some(original) => rebase_local(baseline, original, message),
            None => baseline.clone(),
        });
        if let Some(message) = &mut shown {
            // Local optimism does not establish a newer server edit version.
            let local_edited = message.edited;
            if undo.is_some() {
                message.edited = baseline.edited;
            }
            if !patch.apply(message) {
                return Applied::Nothing;
            }
            if undo.is_some() && patch.edited.is_none() {
                message.edited = local_edited;
            }
        }
        let changed = shown.as_ref() != visible.as_deref() || pin_after != pinned;
        if undo.is_some() {
            if shown.as_ref() == Some(&confirmed) {
                self.before_change.remove(&(channel, id));
            } else {
                self.before_change
                    .insert((channel, id), Arc::new(confirmed.clone()));
            }
        }
        let canonical = pin_after.as_ref().unwrap_or(&confirmed);
        if patch.pinned.is_some() || changed && canonical.pinned {
            self.patch_pin(canonical, patch.pinned.is_some());
        }
        if let Some(message) = shown {
            // Restore changes only an existing row; it never inserts a baseline.
            if let Some(timeline) = self.timelines.get_mut(&channel) {
                timeline.restore(Arc::new(message));
            }
        }
        if changed {
            Applied::Messages { channel, added: 0 }
        } else {
            Applied::Nothing
        }
    }
}

impl Store {
    /// One account's reaction or vote changed: apply it to the confirmed
    /// message, then show what the account changed here meanwhile on top, so
    /// the account's own confirmation never counts twice.
    pub(super) fn tallied(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        change: impl Fn(&mut Message) -> bool,
    ) -> Applied {
        if id.0 == 0
            || !self
                .channel(channel)
                .is_some_and(|channel| Self::visible(channel) && channel.kind.has_messages())
            || self
                .timelines
                .get(&channel)
                .is_some_and(|timeline| timeline.is_deleted(id))
        {
            return Applied::Nothing;
        }
        let mut changed = false;
        let pinned = self.retained_pin(channel, id).cloned();
        if let Some(mut pinned) = pinned.clone() {
            if change(&mut pinned) {
                self.patch_pin(&pinned, false);
                changed = true;
            }
        }
        let visible = self
            .timelines
            .get(&channel)
            .and_then(|timeline| timeline.get(id))
            .cloned();
        let Some(visible) = visible else {
            if pinned.is_none() {
                // Without a baseline a pins read in flight may hold either count.
                self.unknown_pin_patch(&MessagePatch {
                    channel,
                    id,
                    reactions: Some(Vec::new()),
                    ..Default::default()
                });
            }
            return if changed {
                Applied::Messages { channel, added: 0 }
            } else {
                Applied::Nothing
            };
        };
        let shown = match self.before_change.get(&(channel, id)).cloned() {
            Some(original) => {
                let mut confirmed = Message::clone(&original);
                if !change(&mut confirmed) {
                    None
                } else {
                    let shown = rebase_local(&confirmed, &original, &visible);
                    if shown == confirmed {
                        self.before_change.remove(&(channel, id));
                    } else {
                        self.before_change
                            .insert((channel, id), Arc::new(confirmed));
                    }
                    Some(shown)
                }
            }
            None => {
                let mut shown = Message::clone(&visible);
                change(&mut shown).then_some(shown)
            }
        };
        if let Some(shown) = shown.filter(|shown| shown != visible.as_ref()) {
            if let Some(timeline) = self.timelines.get_mut(&channel) {
                timeline.restore(Arc::new(shown));
            }
            changed = true;
        }
        if changed {
            Applied::Messages { channel, added: 0 }
        } else {
            Applied::Nothing
        }
    }
}

pub(super) fn rebase_local(confirmed: &Message, original: &Message, visible: &Message) -> Message {
    let mut message = confirmed.clone();
    if visible.content != original.content {
        message.content.clone_from(&visible.content);
        // Canonical references describe the canonical text, not this local edit.
        message.text_references.clear();
    }
    // Only the account's toggle is optimistic. Counts from other accounts
    // belong to the latest confirmed baseline and must not be copied back.
    for reaction in &original.reactions {
        if reaction.me
            && !visible
                .reactions
                .iter()
                .any(|shown| shown.emoji == reaction.emoji && shown.me)
            && message
                .reactions
                .iter()
                .any(|current| current.emoji == reaction.emoji && current.me)
        {
            message.toggle_reaction(&reaction.emoji);
        }
    }
    for reaction in &visible.reactions {
        if reaction.me
            && !original
                .reactions
                .iter()
                .any(|before| before.emoji == reaction.emoji && before.me)
            && !message
                .reactions
                .iter()
                .any(|current| current.emoji == reaction.emoji && current.me)
        {
            message.toggle_reaction(&reaction.emoji);
        }
    }
    // A local vote replaces the account's choice, as the service does.
    if let (Some(before), Some(chosen), Some(poll)) =
        (&original.poll, &visible.poll, &mut message.poll)
    {
        let mine = |poll: &rune_model::Poll| -> Vec<u32> {
            poll.answers
                .iter()
                .filter(|answer| answer.me)
                .map(|answer| answer.id)
                .collect()
        };
        if mine(before) != mine(chosen) {
            poll.vote(&mine(chosen));
        }
    }
    if visible.edited != original.edited {
        message.edited = visible.edited;
    }
    message
}
