//! Partial confirmed changes to an existing message, independent of transport.

use crate::{
    Attachment, ChannelId, Components, Embed, Message, MessageCall, MessageId, Poll, Reaction,
    RoleId, Sticker, TextReference, Timestamp, UserId,
};

/// A confirmed partial update. Absence preserves a field; an empty collection
/// clears it. Nested options distinguish an absent field from an explicit null.
/// This transient type does not reconstruct messages or change their identity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MessagePatch {
    pub channel: ChannelId,
    pub id: MessageId,
    /// If supplied, asserts the existing author; never reassigns authorship.
    pub author: Option<UserId>,
    pub content: Option<String>,
    pub edited: Option<Option<Timestamp>>,
    pub reply_to: Option<Option<MessageId>>,
    pub pinned: Option<bool>,
    pub mentions: Option<Vec<UserId>>,
    pub reactions: Option<Vec<Reaction>>,
    pub attachments: Option<Vec<Attachment>>,
    pub stickers: Option<Vec<Sticker>>,
    pub embeds: Option<Vec<Embed>>,
    /// Without counts, keeps the counts already known.
    pub poll: Option<Poll>,
    pub components: Option<Components>,
    /// References for the supplied content. Content without references clears
    /// the old references so actions cannot outlive the text they describe.
    pub text_references: Option<Vec<TextReference>>,
    // Message content (contenido-de-mensajes).
    /// A call's message learns who joined and when it ended.
    pub call: Option<MessageCall>,
    // Rail and notifications.
    pub mention_roles: Option<Vec<RoleId>>,
    pub mention_everyone: Option<bool>,
}

impl MessagePatch {
    /// Apply to a retained confirmed baseline. Invalid scope, author, local
    /// delivery or an explicitly older edit returns false without any mutation.
    pub fn apply(&self, message: &mut Message) -> bool {
        if self.channel != message.channel
            || self.id != message.id
            || self.author.is_some_and(|author| author != message.author)
            || message.delivery.is_local()
            || self.edited.is_some_and(|edited| edited < message.edited)
        {
            return false;
        }
        if self.content.is_some() && self.text_references.is_none() {
            message.text_references.clear();
        }
        replace(&mut message.content, &self.content);
        replace(&mut message.edited, &self.edited);
        replace(&mut message.reply_to, &self.reply_to);
        replace(&mut message.pinned, &self.pinned);
        replace(&mut message.mentions, &self.mentions);
        replace(&mut message.reactions, &self.reactions);
        replace(&mut message.attachments, &self.attachments);
        replace(&mut message.stickers, &self.stickers);
        replace(&mut message.embeds, &self.embeds);
        if let Some(poll) = &self.poll {
            let mut poll = poll.clone();
            if let Some(previous) = &message.poll {
                poll.retain_results(previous);
            }
            message.poll = Some(poll);
        }
        replace(&mut message.components, &self.components);
        replace(&mut message.text_references, &self.text_references);
        if let Some(call) = &self.call {
            message.call = Some(call.clone());
        }
        replace(&mut message.mention_roles, &self.mention_roles);
        replace(&mut message.mention_everyone, &self.mention_everyone);
        true
    }
}

fn replace<T: Clone>(target: &mut T, value: &Option<T>) {
    if let Some(value) = value {
        target.clone_from(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AttachmentId, Delivery, MessageKind, StickerId, TextTarget};

    fn message() -> Message {
        Message {
            id: MessageId(30),
            channel: ChannelId(20),
            author: UserId(10),
            content: "before".into(),
            timestamp: Timestamp(1),
            edited: Some(Timestamp(5)),
            kind: MessageKind::Default,
            reply_to: Some(MessageId(29)),
            mentions: vec![UserId(11)],
            reactions: vec![Reaction {
                emoji: "ok".into(),
                count: 2,
                me: true,
                ..Default::default()
            }],
            attachments: vec![Attachment {
                id: AttachmentId(50),
                filename: "old.txt".into(),
                size_bytes: 1,
                content_type: None,
                dimensions: None,
                preview: None,
                download: None,
                spoiler: false,
                voice: None,
                explicit: false,
                placeholder: None,
            }],
            stickers: vec![Sticker {
                id: StickerId(60),
                name: "old".into(),
                image: None,
                available: true,
            }],
            text_references: vec![TextReference {
                source: "before".into(),
                target: TextTarget::Invitation,
            }],
            delivery: Delivery::Sent,
            pinned: true,
            embeds: Vec::new(),
            forwarded: None,
            poll: None,
            components: Default::default(),
            call: None,
            mention_roles: Vec::new(),
            mention_everyone: false,
        }
    }

    fn patch() -> MessagePatch {
        MessagePatch {
            channel: ChannelId(20),
            id: MessageId(30),
            ..Default::default()
        }
    }

    #[test]
    fn absent_fields_preserve_the_confirmed_baseline() {
        let mut current = message();
        let before = current.clone();
        assert!(patch().apply(&mut current));
        assert_eq!(current, before);
    }

    #[test]
    fn content_and_references_change_together_without_erasing_other_fields() {
        let mut current = message();
        let mut update = patch();
        update.content = Some(String::new());
        update.edited = Some(Some(Timestamp(6)));
        assert!(update.apply(&mut current));
        assert!(current.content.is_empty() && current.text_references.is_empty());
        assert_eq!(current.edited, Some(Timestamp(6)));
        assert_eq!(current.mentions, vec![UserId(11)]);
        assert_eq!(current.reply_to, Some(MessageId(29)));
        update.content = Some("new".into());
        update.text_references = Some(vec![TextReference {
            source: "new".into(),
            target: TextTarget::Invitation,
        }]);
        assert!(update.apply(&mut current));
        assert_eq!(current.text_references, update.text_references.unwrap());
    }

    #[test]
    fn explicit_empty_and_null_clear_only_the_supplied_fields() {
        let mut current = message();
        let mut update = patch();
        update.reply_to = Some(None);
        update.mentions = Some(vec![]);
        update.reactions = Some(vec![]);
        update.attachments = Some(vec![]);
        update.stickers = Some(vec![]);
        update.pinned = Some(false);
        assert!(update.apply(&mut current));
        assert!(
            current.reply_to.is_none()
                && current.mentions.is_empty()
                && current.reactions.is_empty()
        );
        assert!(current.attachments.is_empty() && current.stickers.is_empty());
        assert!(!current.pinned);
        assert_eq!(current.content, "before");
        assert_eq!(current.edited, Some(Timestamp(5)));
    }

    #[test]
    fn invalid_scope_authorship_delivery_and_older_edits_are_atomic() {
        for update in [
            MessagePatch {
                channel: ChannelId(21),
                ..patch()
            },
            MessagePatch {
                id: MessageId(31),
                ..patch()
            },
            MessagePatch {
                author: Some(UserId(11)),
                ..patch()
            },
            MessagePatch {
                edited: Some(Some(Timestamp(4))),
                ..patch()
            },
            MessagePatch {
                edited: Some(None),
                ..patch()
            },
        ] {
            let mut current = message();
            let before = current.clone();
            let update = MessagePatch {
                content: Some("must not apply".into()),
                ..update
            };
            assert!(!update.apply(&mut current));
            assert_eq!(current, before);
        }
        let mut current = message();
        current.delivery = Delivery::Sending;
        let before = current.clone();
        assert!(!patch().apply(&mut current));
        assert_eq!(current, before);
    }

    #[test]
    fn same_version_and_matching_author_allow_independent_changes() {
        let mut current = message();
        let update = MessagePatch {
            author: Some(current.author),
            edited: Some(current.edited),
            pinned: Some(false),
            ..patch()
        };
        assert!(update.apply(&mut current));
        assert!(!current.pinned);
        assert_eq!(current.author, UserId(10));
        assert_eq!(current.timestamp, Timestamp(1));
        assert_eq!(current.kind, MessageKind::Default);
        assert_eq!(current.delivery, Delivery::Sent);
    }
}
