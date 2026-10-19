//! Threads and forums: what a branched conversation or a forum post knows of
//! itself, and the tags a forum offers its posts. A thread is a [`Channel`]
//! whose kind is [`ChannelKind::Thread`](crate::ChannelKind) and whose
//! `parent` is the channel or forum it hangs from.
//!
//! [`Channel`]: crate::Channel

use serde::{Deserialize, Serialize};

use crate::{EmojiId, RemoteImage, Timestamp, UserId};

/// A forum's tag, by the service's id.
#[derive(
    Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub struct TagId(pub u64);

/// Tags a forum offers, at most; more is malformed.
pub const MAX_TAGS: usize = 20;
/// Tags one post carries, at most.
pub const MAX_APPLIED_TAGS: usize = 5;
/// Characters of a tag's name, at most.
pub const MAX_TAG_NAME: usize = 50;
/// The minutes without activity after which a forum's new posts are put
/// away, as services offer them: an hour, a day, three days, a week.
pub const ARCHIVE_AFTER: [u32; 4] = [60, 1440, 4320, 10080];

/// A label a forum offers its posts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ForumTag {
    pub id: TagId,
    pub name: String,
    /// A Unicode emoji shown before the name.
    #[serde(default)]
    pub emoji: Option<String>,
    /// Only moderation may put it on a post.
    #[serde(default)]
    pub moderated: bool,
}

/// How a forum lists its posts until the person picks otherwise, and what
/// a new post needs. Every field defaults so a forum kept before it reads.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ForumDefaults {
    /// Cards side by side, as a gallery, instead of a list.
    #[serde(default)]
    pub gallery: bool,
    /// The newest posts first, instead of the most recently active.
    #[serde(default)]
    pub by_creation: bool,
    /// A new post must carry at least one tag.
    #[serde(default)]
    pub require_tag: bool,
    // Server administration, rest (P7).
    /// The reaction a new post shows to begin with: a Unicode emoji, as
    /// reactions are.
    #[serde(default)]
    pub reaction: Option<String>,
    // Server emoji, archive and tag filter (2026-10-18).
    /// One of the server's own emojis as that reaction, instead of `reaction`.
    #[serde(default)]
    pub reaction_emoji: Option<EmojiId>,
    /// Minutes without activity before a new post is put away: one of
    /// [`ARCHIVE_AFTER`]; `None` as the service decides.
    #[serde(default)]
    pub archive_after: Option<u32>,
    /// Filtering the posts by tags shows those with every tag chosen,
    /// instead of those with any.
    #[serde(default)]
    pub match_all_tags: bool,
}

/// Characters of a thread's or a post's name, at most.
pub const MAX_THREAD_NAME: usize = 100;

/// What a thread knows beyond a channel. Every field defaults so a thread
/// kept before a field existed still reads.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Thread {
    /// Put away: it leaves the active list and takes no replies.
    #[serde(default)]
    pub archived: bool,
    /// Only moderation may reply or reopen it.
    #[serde(default)]
    pub locked: bool,
    /// Seen only by its members and moderation.
    #[serde(default)]
    pub private: bool,
    /// The account is one of its members.
    #[serde(default)]
    pub joined: bool,
    /// Who started it.
    #[serde(default)]
    pub owner: Option<UserId>,
    /// Messages in it, as the service counts them; 0 when unknown.
    #[serde(default)]
    pub messages: u32,
    /// People in it, as the service counts them; 0 when unknown.
    #[serde(default)]
    pub members: u32,
    /// Its last message, or when it started.
    #[serde(default)]
    pub last_activity: Timestamp,
    /// The forum tags put on it, when it is a post.
    #[serde(default)]
    pub tags: Vec<TagId>,
    // Threads, forums and stages (2026-10-18).
    /// The account silenced it until then, [`MUTED_FOREVER`] for good;
    /// `None` while it does not.
    ///
    /// [`MUTED_FOREVER`]: crate::MUTED_FOREVER
    #[serde(default)]
    pub muted_until: Option<Timestamp>,
    /// What a forum's card shows of a post's first message, when the
    /// provider read it.
    #[serde(default)]
    pub starter: Option<PostStarter>,
}

impl Thread {
    /// Whether the account silenced it at `now`.
    pub fn muted(&self, now: Timestamp) -> bool {
        self.muted_until.is_some_and(|until| until > now)
    }
}

/// Characters of a post's first words a card shows, at most.
pub const STARTER_EXCERPT: usize = 200;
/// Reactions of a post's first message a card knows, at most.
pub const STARTER_REACTIONS: usize = 20;

/// What a forum's card shows of a post's first message.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PostStarter {
    /// Its first words as written, at most [`STARTER_EXCERPT`] characters;
    /// empty when it has none or hides them behind a spoiler.
    #[serde(default)]
    pub excerpt: String,
    /// Its first picture that is not behind a spoiler: a file's preview,
    /// or a card's.
    #[serde(default)]
    pub image: Option<RemoteImage>,
    /// Its reactions, a Unicode emoji and how many, the most first.
    #[serde(default)]
    pub reactions: Vec<(String, u32)>,
}

impl PostStarter {
    /// What a card shows of `message`, the first of a post, maybe nothing:
    /// a first message known shows the forum's reaction all the same. A
    /// message with a spoiler shows no words; a picture behind a spoiler, or
    /// one the service found explicit, shows none.
    pub fn of(message: &crate::Message) -> Self {
        let excerpt = if message.content.contains("||") {
            String::new()
        } else {
            let words = message
                .content
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            match words.char_indices().nth(STARTER_EXCERPT) {
                Some((end, _)) => {
                    let mut cut = &words[..end];
                    // A token cut in half (an emoji, a mention) is left out whole.
                    if let Some(open) = cut.rfind('<').filter(|open| !cut[*open..].contains('>')) {
                        cut = &cut[..open];
                    }
                    format!("{}…", cut.trim_end())
                }
                None => words,
            }
        };
        let image = message
            .attachments
            .iter()
            .filter(|file| !file.spoiler && !file.explicit && file.voice.is_none())
            .find_map(|file| file.preview.clone())
            .or_else(|| {
                (!crate::has_spoiler(&message.content))
                    .then(|| {
                        message.embeds.iter().find_map(|embed| {
                            embed
                                .image
                                .as_ref()
                                .or(embed.thumbnail.as_ref())
                                .map(|picture| picture.source.clone())
                        })
                    })
                    .flatten()
            });
        let mut reactions: Vec<(String, u32)> = message
            .reactions
            .iter()
            .map(|reaction| {
                (
                    reaction.emoji.clone(),
                    reaction.count.saturating_add(reaction.burst),
                )
            })
            .filter(|(_, count)| *count > 0)
            .collect();
        reactions.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        reactions.truncate(STARTER_REACTIONS);
        Self {
            excerpt,
            image,
            reactions,
        }
    }

    /// The reaction a card shows, as the official client chooses it: the
    /// one most given, or else the forum's own (`default`) with no count
    /// (0) to be the first.
    pub fn reaction(&self, default: Option<&str>) -> Option<(String, u32)> {
        self.reactions
            .first()
            .cloned()
            .or_else(|| default.map(|emoji| (emoji.to_owned(), 0)))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Channel, ChannelKind};

    #[test]
    fn channels_kept_before_threads_existed_still_read() {
        let channel: Channel = serde_json::from_value(serde_json::json!({
            "id": 5, "server": 1, "kind": "Text", "name": "general", "topic": null,
            "parent": null, "position": 0, "recipients": [], "permissions": 3
        }))
        .unwrap();
        assert!(channel.thread.is_none() && channel.tags.is_empty());
        assert_eq!(channel.forum, super::ForumDefaults::default());
        let thread: super::Thread = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(thread, super::Thread::default());
    }

    fn first(content: &str) -> crate::Message {
        use crate::*;
        Message {
            id: MessageId(1),
            channel: ChannelId(1),
            author: UserId(3),
            content: content.into(),
            timestamp: Timestamp(100),
            edited: None,
            kind: MessageKind::Default,
            reply_to: None,
            mentions: vec![],
            reactions: vec![],
            attachments: vec![],
            stickers: vec![],
            text_references: vec![],
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

    #[test]
    fn a_card_shows_the_first_words_picture_and_reaction_of_a_post() {
        use super::PostStarter;
        use crate::{Attachment, AttachmentId, Reaction, RemoteImage};
        assert_eq!(PostStarter::of(&first("")), PostStarter::default());
        let mut message = first("Hola   a\ntodos <@12>");
        let picture = |path: &str| RemoteImage {
            url: format!("https://example.invalid/{path}"),
            kind: Default::default(),
        };
        let file = |path: &str, spoiler: bool| Attachment {
            id: AttachmentId(path.len() as u64),
            filename: path.into(),
            size_bytes: 1,
            content_type: Some("image/png".into()),
            dimensions: Some((2, 2)),
            preview: Some(picture(path)),
            download: None,
            spoiler,
            voice: None,
            placeholder: None,
            explicit: false,
        };
        message.attachments = vec![file("oculta.png", true), file("vista.png", false)];
        let reaction = |emoji: &str, count| Reaction {
            emoji: emoji.into(),
            count,
            me: false,
            burst: 1,
            me_burst: false,
            burst_colors: Vec::new(),
        };
        message.reactions = vec![reaction("👍", 2), reaction("❤️", 5)];
        let starter = PostStarter::of(&message);
        assert_eq!(starter.excerpt, "Hola a todos <@12>");
        assert_eq!(starter.image, Some(picture("vista.png")));
        assert_eq!(starter.reactions, vec![("❤️".into(), 6), ("👍".into(), 3)]);
        // The one most given; without any, the forum's own, to be the first.
        assert_eq!(starter.reaction(Some("👍")), Some(("❤️".into(), 6)));
        let bare = PostStarter {
            reactions: Vec::new(),
            ..starter.clone()
        };
        assert_eq!(bare.reaction(Some("🎉")), Some(("🎉".into(), 0)));
        assert_eq!(bare.reaction(None), None);
        // A spoiler hides the words; a long text is cut, never inside a token.
        let hidden = PostStarter::of(&first("||final||"));
        assert_eq!(hidden, PostStarter::default());
        // Known and bare, it still shows the forum's own, with no count.
        assert_eq!(hidden.reaction(Some("🎉")), Some(("🎉".into(), 0)));
        let long = format!("{} <@123456789>", "a".repeat(super::STARTER_EXCERPT - 3));
        assert_eq!(
            PostStarter::of(&first(&long)).excerpt,
            format!("{}…", "a".repeat(super::STARTER_EXCERPT - 3))
        );
    }

    #[test]
    fn a_thread_is_muted_until_its_time() {
        let thread = super::Thread {
            muted_until: Some(crate::Timestamp(10)),
            ..Default::default()
        };
        assert!(thread.muted(crate::Timestamp(9)) && !thread.muted(crate::Timestamp(10)));
        assert!(super::Thread {
            muted_until: Some(crate::MUTED_FOREVER),
            ..Default::default()
        }
        .muted(crate::Timestamp(i64::MAX - 1)));
    }

    #[test]
    fn threads_and_announcements_hold_messages_and_forums_do_not() {
        for kind in [ChannelKind::Thread, ChannelKind::Announcement] {
            assert!(kind.has_messages(), "{kind:?}");
        }
        assert!(!ChannelKind::Forum.has_messages());
    }
}
