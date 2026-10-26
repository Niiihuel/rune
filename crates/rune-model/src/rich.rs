//! Rich content a message carries besides its text: cards and forwards.

use serde::{Deserialize, Serialize};

use crate::{Attachment, ChannelId, Component, Components, RemoteImage, TextReference, Timestamp};

/// A card under a message: a link's preview or one an app built. Every part
/// is optional; a card with nothing to show is not kept.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Embed {
    #[serde(default)]
    pub kind: EmbedKind,
    /// The site that made the preview, such as "YouTube".
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    /// Where the title leads: only `http` or `https`.
    #[serde(default)]
    pub url: Option<String>,
    /// Markdown source.
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub fields: Vec<EmbedField>,
    #[serde(default)]
    pub footer: Option<String>,
    #[serde(default)]
    pub timestamp: Option<Timestamp>,
    /// The stripe down its left edge, as `0xRRGGBB`.
    #[serde(default)]
    pub color: Option<u32>,
    /// The large picture under its text.
    #[serde(default)]
    pub image: Option<EmbedImage>,
    /// The small picture beside its text; a video's poster.
    #[serde(default)]
    pub thumbnail: Option<EmbedImage>,
    /// Mentions and emoji found in its description, field values and parts.
    #[serde(default)]
    pub text_references: Vec<TextReference>,
    /// What a card of parts lays out, as a message's parts do.
    #[serde(default)]
    pub components: Vec<Component>,
    /// The voice channel a voice card is about, when the provider knows it.
    #[serde(default)]
    pub channel: Option<ChannelId>,
    /// A short looping clip to play in place of its still picture, for a
    /// `Gifv` card: a video through the same proxy as its pictures.
    #[serde(default)]
    pub video: Option<EmbedImage>,
    // Message content (contenido-de-mensajes).
    /// The service found its pictures explicit: the account's filter may
    /// blur them.
    #[serde(default)]
    pub explicit: bool,
}

/// What a card previews, which decides how it is drawn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum EmbedKind {
    /// Text an app or a page wrote: drawn as a card.
    #[default]
    Rich,
    /// A picture alone: drawn as the picture, without a card.
    Image,
    /// A short looping clip, as a picture of its first frame.
    Gifv,
    /// A video: its poster, opening the page.
    Video,
    /// A notice the service itself sends about the account: its header as
    /// `title`, its body as `description`, its date, and `url` as the page
    /// that explains it. Its machine fields are not kept.
    SystemNotice,
    /// The result of a closed poll: its machine fields, read with
    /// [`Embed::field`], hold the question, the winner and the counts; its
    /// `thumbnail`, when present, is the picture of the winner's own emoji.
    PollResult,
    /// A voice channel to join: its title and text, and [`Embed::channel`].
    VoiceChannel,
    /// A card made of [`Embed::components`].
    Components,
    /// A kind this client does not draw yet: drawn as a card, with a note
    /// that part of it may be missing.
    Other,
}

/// A picture of a card, through the service's own image proxy.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EmbedImage {
    pub source: RemoteImage,
    /// Pixels, so the timeline reserves its room before it loads.
    pub dimensions: (u32, u32),
}

impl Embed {
    /// Whether it has anything to show as text.
    pub fn is_empty(&self) -> bool {
        self.provider.is_none()
            && self.author.is_none()
            && self.title.is_none()
            && self.description.is_none()
            && self.fields.is_empty()
            && self.footer.is_none()
            && self.image.is_none()
            && self.thumbnail.is_none()
            && self.components.is_empty()
    }

    /// The value of the field called `name`, as machine cards keep their data.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|field| field.name == name)
            .map(|field| field.value.as_str())
    }

    /// Drawn as its picture alone, without a card.
    pub fn is_media(&self) -> bool {
        matches!(self.kind, EmbedKind::Image | EmbedKind::Gifv) && self.picture().is_some()
    }

    /// The picture that stands for it: the large one, else the small one.
    pub fn picture(&self) -> Option<&EmbedImage> {
        self.image.as_ref().or(self.thumbnail.as_ref())
    }

    /// Whether a message saying `content` wrote its link behind a spoiler,
    /// which hides the whole card until revealed, as the reference client does.
    pub fn spoiled(&self, content: &str) -> bool {
        self.url
            .as_deref()
            .is_some_and(|url| spoiled_link(content, url))
    }

    /// Whether its pictures wait behind a spoiler in a message saying
    /// `content`: the card is spoiled, or its own text hides something.
    pub fn hides_pictures(&self, content: &str) -> bool {
        self.spoiled(content) || self.words().any(has_spoiler)
    }

    /// Whether `other`, after it, only adds a picture to it as a gallery: the
    /// same link, a large picture and no clip, and nothing of its own that
    /// differs, as the reference clients merge them.
    fn takes(&self, other: &Embed) -> bool {
        let same =
            |mine: &Option<String>, theirs: &Option<String>| theirs.is_none() || theirs == mine;
        self.url.is_some()
            && self.image.is_some()
            && other.url == self.url
            && other.image.is_some()
            && other.video.is_none()
            && matches!(other.kind, EmbedKind::Rich | EmbedKind::Image)
            && same(&self.title, &other.title)
            && same(&self.description, &other.description)
            && same(&self.author, &other.author)
            && same(&self.provider, &other.provider)
            && same(&self.footer, &other.footer)
            && other.fields.is_empty()
            && other.components.is_empty()
    }

    /// What it says to a reader, in reading order: to search and to quote.
    /// A poll result's fields are machine data, not words.
    pub fn words(&self) -> impl Iterator<Item = &str> {
        let fields = self
            .fields
            .iter()
            .filter(|_| self.kind != EmbedKind::PollResult)
            .flat_map(|field| [field.name.as_str(), field.value.as_str()]);
        [&self.provider, &self.author, &self.title, &self.description]
            .into_iter()
            .flatten()
            .map(String::as_str)
            .chain(fields)
            .chain(self.footer.as_deref())
    }
}

/// Whether `text` hides something between two `||`.
pub fn has_spoiler(text: &str) -> bool {
    text.split_once("||")
        .and_then(|(_, rest)| rest.split_once("||"))
        .is_some_and(|(inside, _)| !inside.is_empty())
}

/// The cards drawn as one, as `(first, count)`: each card with the ones
/// after it that only add their picture to it, a gallery of at most
/// [`GALLERY`] pictures.
pub fn card_groups(embeds: &[Embed]) -> Vec<(usize, usize)> {
    let mut groups: Vec<(usize, usize)> = Vec::new();
    for (index, embed) in embeds.iter().enumerate() {
        match groups.last_mut() {
            Some((first, count)) if *count < GALLERY && embeds[*first].takes(embed) => *count += 1,
            _ => groups.push((index, 1)),
        }
    }
    groups
}

/// The most pictures a gallery shows, as the reference client lays them.
pub const GALLERY: usize = 4;

/// The pictures a viewer can show of the cards `embeds[first..first + count]`:
/// a picture alone, else the gallery's large pictures and then the small
/// one beside the first card's text. A video's poster is not one: it opens
/// its page.
pub fn card_pictures(embeds: &[Embed], first: usize, count: usize) -> Vec<&EmbedImage> {
    let Some(leader) = embeds.get(first) else {
        return Vec::new();
    };
    if leader.is_media() {
        return leader.picture().into_iter().collect();
    }
    if leader.kind != EmbedKind::Rich {
        return Vec::new();
    }
    embeds[first..(first + count).min(embeds.len())]
        .iter()
        .filter_map(|embed| embed.image.as_ref())
        .chain(leader.thumbnail.as_ref())
        .collect()
}

/// Whether a card's link appears behind a spoiler in `content`, which
/// hides the card as the reference client does.
pub fn spoiled_link(content: &str, url: &str) -> bool {
    let mut rest = content;
    while let Some((_, after)) = rest.split_once("||") {
        let Some((inside, next)) = after.split_once("||") else {
            return false;
        };
        if inside.contains(url) {
            return true;
        }
        rest = next;
    }
    false
}

/// A name and its value inside a card.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EmbedField {
    pub name: String,
    /// Markdown source.
    pub value: String,
    /// Sits beside its inline neighbours, up to three a row.
    #[serde(default)]
    pub inline: bool,
}

/// A copy of a message someone forwarded, as it was when they did.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Forwarded {
    /// Markdown source.
    #[serde(default)]
    pub content: String,
    pub timestamp: Timestamp,
    #[serde(default)]
    pub edited: Option<Timestamp>,
    /// File cards only: a copy never fetches the original's files.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    /// What an app laid out with the original, without pictures of its
    /// files, for the same reason as `attachments`.
    #[serde(default)]
    pub components: Components,
    // Message content (contenido-de-mensajes).
    /// The mentions, channels and emoji its `content` names, as a message's.
    #[serde(default)]
    pub text_references: Vec<TextReference>,
}

/// A question with answers to vote on.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Poll {
    pub question: String,
    pub answers: Vec<PollAnswer>,
    /// When voting closes; `None` keeps it open.
    #[serde(default)]
    pub expiry: Option<Timestamp>,
    #[serde(default)]
    pub multiselect: bool,
    /// The counts were reported. Without them a vote count of 0 means unknown.
    #[serde(default)]
    pub results_known: bool,
    /// The service counted the last vote.
    #[serde(default)]
    pub finalized: bool,
}

impl Poll {
    /// Whether voting is over at `now`.
    pub fn ended(&self, now: Timestamp) -> bool {
        self.finalized || self.expiry.is_some_and(|expiry| expiry <= now)
    }

    pub fn votes(&self) -> u64 {
        self.answers
            .iter()
            .map(|answer| u64::from(answer.votes))
            .sum()
    }

    /// Whether the account voted for any answer.
    pub fn voted(&self) -> bool {
        self.answers.iter().any(|answer| answer.me)
    }

    /// Whether `answers` is a vote this poll takes: known, distinct answers,
    /// at most one unless it is multiselect. None takes a vote back.
    pub fn valid_choice(&self, answers: &[u32]) -> bool {
        (self.multiselect || answers.len() <= 1)
            && answers.iter().enumerate().all(|(index, id)| {
                !answers[..index].contains(id) && self.answers.iter().any(|known| known.id == *id)
            })
    }

    /// Make exactly `answers` the account's vote, as the service replaces it.
    /// Unknown counts stay unknown; only the marks change then.
    pub fn vote(&mut self, answers: &[u32]) {
        for answer in &mut self.answers {
            let mine = answers.contains(&answer.id);
            if mine != answer.me && self.results_known {
                answer.votes = if mine {
                    answer.votes.saturating_add(1)
                } else {
                    answer.votes.saturating_sub(1)
                };
            }
            answer.me = mine;
        }
    }

    /// One account added or took back its vote for `answer`. The account's
    /// own change applies only once; others' change counts only when the
    /// counts are known. Returns whether anything changed.
    pub fn count_vote(&mut self, answer: u32, me: bool, added: bool) -> bool {
        let known = self.results_known;
        let Some(answer) = self.answers.iter_mut().find(|found| found.id == answer) else {
            return false;
        };
        if me {
            if answer.me == added {
                return false;
            }
            answer.me = added;
        } else if !known || !added && answer.votes <= u32::from(answer.me) {
            return false;
        }
        if known {
            answer.votes = if added {
                answer.votes.saturating_add(1)
            } else {
                answer.votes.saturating_sub(1)
            };
        }
        true
    }

    /// An update without counts keeps the ones already known for the same
    /// question and answers, instead of showing every answer at zero.
    pub fn retain_results(&mut self, previous: &Self) {
        let same = self.question == previous.question
            && self.answers.len() == previous.answers.len()
            && self
                .answers
                .iter()
                .zip(&previous.answers)
                .all(|(new, old)| new.id == old.id && new.text == old.text);
        if !self.results_known && previous.results_known && same {
            for (new, old) in self.answers.iter_mut().zip(&previous.answers) {
                new.votes = old.votes;
                new.me = old.me;
            }
            self.results_known = true;
            self.finalized |= previous.finalized;
        }
    }
}

/// One answer of a poll.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PollAnswer {
    pub id: u32,
    pub text: String,
    /// A Unicode emoji before its text.
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(default)]
    pub votes: u32,
    /// The current account voted for it.
    #[serde(default)]
    pub me: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImageKind;

    fn image(url: &str) -> Option<EmbedImage> {
        Some(EmbedImage {
            source: RemoteImage {
                url: url.into(),
                kind: ImageKind::AttachmentPreview,
            },
            dimensions: (4, 3),
        })
    }

    fn card(link: &str, picture: &str) -> Embed {
        Embed {
            url: Some(link.into()),
            image: image(picture),
            ..Embed::default()
        }
    }

    #[test]
    fn cards_with_the_same_link_and_only_a_picture_join_the_first_as_a_gallery() {
        let mut first = card("https://a.test/post", "1");
        first.title = Some("Una publicación".into());
        let mut repeated = card("https://a.test/post", "3");
        repeated.title = first.title.clone();
        let mut other = card("https://b.test", "5");
        other.description = Some("Otra".into());
        let embeds = vec![
            first,
            card("https://a.test/post", "2"),
            repeated,
            card("https://a.test/post", "4"),
            card("https://a.test/post", "extra"),
            other,
        ];
        // At most four pictures a gallery; the fifth starts its own card.
        assert_eq!(card_groups(&embeds), [(0, 4), (4, 1), (5, 1)]);
        let urls: Vec<_> = card_pictures(&embeds, 0, 4)
            .iter()
            .map(|picture| picture.source.url.as_str())
            .collect();
        assert_eq!(urls, ["1", "2", "3", "4"]);
        // A card with words of its own, or a clip, stays apart.
        let mut texted = card("https://a.test/post", "2");
        texted.description = Some("Otro texto".into());
        let mut clip = card("https://a.test/post", "2");
        clip.video = image("v");
        for second in [texted, clip] {
            assert_eq!(card_groups(&[embeds[0].clone(), second]).len(), 2);
        }
        let mut without_link = card("https://a.test/post", "1");
        without_link.url = None;
        assert_eq!(
            card_groups(&[without_link.clone(), without_link]).len(),
            2,
            "no link, no gallery"
        );
    }

    #[test]
    fn a_viewer_shows_a_picture_alone_or_a_cards_pictures_but_not_a_poster() {
        let mut alone = card("https://a.test", "only");
        alone.kind = EmbedKind::Image;
        assert_eq!(card_pictures(std::slice::from_ref(&alone), 0, 1).len(), 1);
        let mut rich = card("https://a.test", "large");
        rich.thumbnail = image("small");
        let urls: Vec<_> = card_pictures(std::slice::from_ref(&rich), 0, 1)
            .iter()
            .map(|picture| picture.source.url.as_str())
            .collect();
        assert_eq!(urls, ["large", "small"]);
        rich.kind = EmbedKind::Video;
        assert!(card_pictures(&[rich], 0, 1).is_empty());
        assert!(card_pictures(&[alone], 3, 1).is_empty());
    }

    #[test]
    fn spoilers_are_found_in_text_and_around_links() {
        assert!(has_spoiler("el final: ||muere||"));
        assert!(!has_spoiler("a || b"));
        assert!(!has_spoiler("||||"));
        let url = "https://a.test/x.png";
        assert!(spoiled_link(&format!("mira ||{url}|| ojo"), url));
        assert!(spoiled_link(&format!("||nada|| y ||ojo {url}||"), url));
        assert!(!spoiled_link(&format!("||nada|| {url}"), url));
        assert!(!spoiled_link(&format!("||{url}"), url));
        let mut embed = card(url, "1");
        assert!(!embed.hides_pictures("hola"));
        assert!(embed.spoiled(&format!("||{url}||")));
        assert!(embed.hides_pictures(&format!("||{url}||")));
        embed.description = Some("||sorpresa||".into());
        assert!(embed.hides_pictures("hola") && !embed.spoiled("hola"));
    }
}
