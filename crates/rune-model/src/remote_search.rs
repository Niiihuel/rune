//! Searches a provider runs over a conversation's or a server's whole
//! history, with the filters services offer, independent of any of them.

use rune_i18n::{t, tr};

use crate::{card_words, ChannelId, Message, ServerId, Timestamp, UserId};

/// Results in one page, as services give them.
pub const REMOTE_PAGE: usize = 25;
/// Where the last page a search reaches starts: services stop there.
pub const MAX_REMOTE_OFFSET: u32 = 9_975;
/// The most filters one search carries.
pub const MAX_REMOTE_FILTERS: usize = 16;

/// What a message has, as a filter: `has:link`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Has {
    Link,
    Embed,
    File,
    Image,
    Video,
    Sound,
    Sticker,
    // Reactions, pins and search (2026-10-18).
    Poll,
}

impl Has {
    pub const ALL: [Has; 8] = [
        Has::Link,
        Has::Embed,
        Has::File,
        Has::Image,
        Has::Video,
        Has::Sound,
        Has::Sticker,
        Has::Poll,
    ];

    /// The word a query names it by, as services take it.
    pub fn key(self) -> &'static str {
        match self {
            Self::Link => "link",
            Self::Embed => "embed",
            Self::File => "file",
            Self::Image => "image",
            Self::Video => "video",
            Self::Sound => "sound",
            Self::Sticker => "sticker",
            Self::Poll => "poll",
        }
    }

    /// What the interface calls it.
    pub fn label(self) -> &'static str {
        match self {
            Self::Link => t("enlace"),
            Self::Embed => t("tarjeta"),
            Self::File => t("archivo"),
            Self::Image => t("imagen"),
            Self::Video => "video",
            Self::Sound => t("sonido"),
            Self::Sticker => "sticker",
            Self::Poll => t("encuesta"),
        }
    }

    /// Its key or its label, in any case.
    pub fn parse(word: &str) -> Option<Self> {
        let word = word.to_lowercase();
        Self::ALL
            .into_iter()
            .find(|has| has.key() == word || has.label() == word)
    }

    fn found(self, message: &Message) -> bool {
        let typed = |kind: &str| {
            message.files().any(|file| {
                file.content_type
                    .as_deref()
                    .is_some_and(|known| known.starts_with(kind))
            })
        };
        match self {
            Self::Link => {
                message.content.contains("https://") || message.content.contains("http://")
            }
            Self::Embed => !message.embeds.is_empty(),
            Self::File => message.files().next().is_some(),
            Self::Image => typed("image/"),
            Self::Video => typed("video/"),
            Self::Sound => typed("audio/"),
            Self::Sticker => !message.stickers.is_empty(),
            Self::Poll => message.poll.is_some(),
        }
    }
}

/// Who wrote a message, as a filter: `author_type:bot`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AuthorKind {
    User,
    Bot,
    Webhook,
}

impl AuthorKind {
    pub const ALL: [AuthorKind; 3] = [AuthorKind::User, AuthorKind::Bot, AuthorKind::Webhook];

    pub fn key(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Bot => "bot",
            Self::Webhook => "webhook",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::User => t("persona"),
            Self::Bot => "bot",
            Self::Webhook => "webhook",
        }
    }

    pub fn parse(word: &str) -> Option<Self> {
        let word = word.to_lowercase();
        Self::ALL
            .into_iter()
            .find(|kind| kind.key() == word || kind.label() == word)
    }
}

/// How a provider orders what it finds (2026-10-18).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum RemoteOrder {
    /// The newest first.
    #[default]
    Newest,
    /// The best matches first, as the provider ranks them.
    Relevance,
}

/// The site `value` names, as a link filter takes it: `ejemplo.com` from
/// `https://Ejemplo.com/x` too; `None` when it is not a host name.
pub fn link_host(value: &str) -> Option<String> {
    let value = value.trim();
    let value = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .unwrap_or(value);
    let host = value
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_lowercase();
    (!host.is_empty()
        && host.len() <= 253
        && !host.starts_with(['.', '-'])
        && !host.contains("..")
        && host
            .chars()
            .all(|letter| letter.is_ascii_alphanumeric() || matches!(letter, '.' | '-')))
    .then_some(host)
}

/// Whether `message` links to `host` or to a site under it.
fn links_to(message: &Message, host: &str) -> bool {
    let embedded = message
        .embeds
        .iter()
        .filter_map(|embed| embed.url.as_deref());
    std::iter::once(message.content.as_str())
        .chain(embedded)
        .flat_map(|text| text.split("://").skip(1))
        .filter_map(|rest| {
            let end = rest
                .find(|letter: char| !(letter.is_alphanumeric() || matches!(letter, '.' | '-')))
                .unwrap_or(rest.len());
            link_host(&rest[..end])
        })
        .any(|found| found == host || found.ends_with(&format!(".{host}")))
}

/// What to look for at the provider, and where.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RemoteSearch {
    /// The conversation it was started from.
    pub channel: ChannelId,
    /// Across the whole server, the channels the account reads there; only
    /// `channel` when `None`.
    pub server: Option<ServerId>,
    pub text: String,
    /// Written by any of them: `from:`.
    pub authors: Vec<UserId>,
    /// Mentioning any of them: `mentions:`.
    pub mentions: Vec<UserId>,
    pub has: Vec<Has>,
    /// Only in these channels of `server`: `in:`.
    pub channels: Vec<ChannelId>,
    pub author_kinds: Vec<AuthorKind>,
    pub pinned: Option<bool>,
    /// Written at or after this time.
    pub after: Option<Timestamp>,
    /// Written before this time.
    pub before: Option<Timestamp>,
    // Reactions, pins and search (2026-10-18).
    /// Linking to any of these sites, as [`link_host`] writes them: `link_from:`.
    pub link_hosts: Vec<String>,
    pub order: RemoteOrder,
}

impl RemoteSearch {
    /// How many filters it carries, the text aside.
    pub fn filters(&self) -> usize {
        self.authors.len()
            + self.mentions.len()
            + self.has.len()
            + self.channels.len()
            + self.author_kinds.len()
            + usize::from(self.pinned.is_some())
            + usize::from(self.after.is_some())
            + usize::from(self.before.is_some())
            + self.link_hosts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty() && self.filters() == 0
    }

    /// Why it cannot be sent, as the interface says it.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.is_empty() {
            return Err(t("Escribí texto o elegí un filtro para buscar."));
        }
        if self.text.len() > 1024
            || self.text.chars().count() > 256
            || self.text.chars().any(char::is_control)
        {
            return Err(t("La búsqueda admite hasta 256 caracteres."));
        }
        if self.filters() > MAX_REMOTE_FILTERS {
            return Err(t("Usá como mucho 16 filtros."));
        }
        if self.channel.0 == 0
            || self.server.is_some_and(|server| server.0 == 0)
            || self
                .authors
                .iter()
                .chain(&self.mentions)
                .any(|user| user.0 == 0)
            || self.channels.iter().any(|channel| channel.0 == 0)
        {
            return Err(t("Elegí personas y canales de las sugerencias."));
        }
        if self
            .link_hosts
            .iter()
            .any(|host| link_host(host).as_ref() != Some(host))
        {
            return Err(t("Escribí el sitio como ejemplo.com."));
        }
        if self.server.is_none() && !self.channels.is_empty() {
            return Err(t("«en:» sólo busca en canales de un servidor."));
        }
        if self
            .after
            .zip(self.before)
            .is_some_and(|(after, before)| after >= before)
        {
            return Err(t("La fecha «después» tiene que ser anterior a «antes»."));
        }
        Ok(())
    }

    /// The same search over loaded messages, as literal text, when every
    /// filter can be checked on a message; `None` when one cannot (who kind
    /// of author wrote it is not on the message).
    pub fn loaded_matcher(&self) -> Option<impl Fn(&Message) -> bool + '_> {
        if !self.author_kinds.is_empty() {
            return None;
        }
        let needle = self.text.trim().to_lowercase();
        Some(move |message: &Message| {
            !message.delivery.is_local()
                && (self.channels.is_empty() || self.channels.contains(&message.channel))
                && (self.authors.is_empty() || self.authors.contains(&message.author))
                && self.mentions.iter().all(|user| message.mentions(*user))
                && self.has.iter().all(|has| has.found(message))
                && self.pinned.is_none_or(|pinned| message.pinned == pinned)
                && self.after.is_none_or(|after| message.timestamp >= after)
                && self.before.is_none_or(|before| message.timestamp < before)
                && (self.link_hosts.is_empty()
                    || self.link_hosts.iter().any(|host| links_to(message, host)))
                && (needle.is_empty()
                    || std::iter::once(message.content.as_str())
                        .chain(card_words(message))
                        .any(|text| text.to_lowercase().contains(&needle)))
        })
    }
}

/// One page of what a provider found, newest first.
#[derive(Clone, Debug, PartialEq)]
pub struct RemotePage {
    /// As the provider has them now; their authors arrive before them.
    pub messages: Vec<Message>,
    /// How many match in all, as the provider counts them.
    pub total: u64,
    /// Where this page starts among them.
    pub offset: u32,
    /// The provider is still indexing older messages: there may be more.
    pub partial: bool,
}

impl RemotePage {
    /// Whether it can be the answer to `search` from `offset`: a page at
    /// most, each message once, newest first unless ranked by relevance,
    /// and where it was asked.
    pub fn fits(&self, search: &RemoteSearch, offset: u32) -> bool {
        let mut ids: Vec<_> = self.messages.iter().map(|message| message.id).collect();
        ids.sort_unstable();
        ids.dedup();
        self.offset == offset
            && self.messages.len() <= REMOTE_PAGE
            && ids.len() == self.messages.len()
            && (search.order == RemoteOrder::Relevance
                || self
                    .messages
                    .windows(2)
                    .all(|pair| newer(&pair[0], &pair[1])))
            && self.messages.iter().all(|message| {
                message.id.0 != 0
                    && (search.server.is_some() || message.channel == search.channel)
                    && (search.channels.is_empty() || search.channels.contains(&message.channel))
            })
    }
}

/// Whether `a` was written after `b`; by id at the same time.
pub fn newer(a: &Message, b: &Message) -> bool {
    (a.timestamp, a.id) > (b.timestamp, b.id)
}

/// Why a provider's search gave no page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RemoteSearchError {
    /// The provider is still preparing its index of these messages: ask
    /// again after `retry_after_ms`.
    Indexing {
        retry_after_ms: u64,
    },
    Failed(String),
}

impl RemoteSearchError {
    pub fn label(&self) -> String {
        match self {
            Self::Indexing { retry_after_ms } => tr!(
                "El servicio todavía está preparando el índice de búsqueda. Se vuelve a intentar en {seconds} s.",
                seconds = retry_after_ms.div_ceil(1000).max(1)
            ),
            Self::Failed(reason) => reason.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Attachment, AttachmentId, Delivery, MessageId, MessageKind};

    fn message(id: u64, content: &str) -> Message {
        Message {
            id: MessageId(id),
            channel: ChannelId(2),
            author: UserId(3),
            content: content.into(),
            timestamp: Timestamp(1_000),
            edited: None,
            kind: MessageKind::Default,
            reply_to: None,
            mentions: vec![UserId(9)],
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

    fn search(text: &str) -> RemoteSearch {
        RemoteSearch {
            channel: ChannelId(2),
            text: text.into(),
            ..Default::default()
        }
    }

    #[test]
    fn validation_bounds_text_filters_scope_and_dates() {
        assert!(search("").validate().is_err());
        assert!(search("hola").validate().is_ok());
        assert!(search(&"x".repeat(257)).validate().is_err());
        assert!(RemoteSearch {
            pinned: Some(true),
            ..search("")
        }
        .validate()
        .is_ok());
        // `in:` needs a server; a zero id never leaves.
        assert!(RemoteSearch {
            channels: vec![ChannelId(5)],
            ..search("x")
        }
        .validate()
        .is_err());
        assert!(RemoteSearch {
            authors: vec![UserId(0)],
            ..search("x")
        }
        .validate()
        .is_err());
        assert!(RemoteSearch {
            authors: (1..=17).map(UserId).collect(),
            ..search("x")
        }
        .validate()
        .is_err());
        assert!(RemoteSearch {
            after: Some(Timestamp(10)),
            before: Some(Timestamp(10)),
            ..search("x")
        }
        .validate()
        .is_err());
    }

    #[test]
    fn has_and_kinds_parse_keys_and_labels() {
        assert_eq!(Has::parse("LINK"), Some(Has::Link));
        assert_eq!(Has::parse("imagen"), Some(Has::Image));
        assert_eq!(Has::parse("poll"), Some(Has::Poll));
        assert_eq!(Has::parse("encuesta"), Some(Has::Poll));
        assert_eq!(Has::parse("snapshot"), None);
        assert_eq!(AuthorKind::parse("persona"), Some(AuthorKind::User));
        assert_eq!(AuthorKind::parse("bot"), Some(AuthorKind::Bot));
    }

    #[test]
    fn the_loaded_matcher_checks_every_filter_or_declines() {
        let mut picture = message(5, "mirá https://example.invalid");
        picture.attachments.push(Attachment {
            id: AttachmentId(1),
            filename: "a.png".into(),
            size_bytes: 1,
            content_type: Some("image/png".into()),
            dimensions: None,
            preview: None,
            download: None,
            spoiler: false,
            voice: None,
            explicit: false,
            placeholder: None,
        });
        let wanted = RemoteSearch {
            authors: vec![UserId(3)],
            mentions: vec![UserId(9)],
            has: vec![Has::Link, Has::Image, Has::File],
            after: Some(Timestamp(1_000)),
            before: Some(Timestamp(1_001)),
            ..search("MIRÁ")
        };
        let matches = wanted.loaded_matcher().unwrap();
        assert!(matches(&picture));
        assert!(!matches(&message(6, "mirá")), "no link, no image");
        let video = RemoteSearch {
            has: vec![Has::Video],
            ..search("")
        };
        assert!(!video.loaded_matcher().unwrap()(&picture));
        let pending = Message {
            delivery: Delivery::Sending,
            ..picture.clone()
        };
        assert!(!matches(&pending), "only what the provider has");
        assert!(RemoteSearch {
            author_kinds: vec![AuthorKind::Bot],
            ..search("x")
        }
        .loaded_matcher()
        .is_none());
    }

    #[test]
    fn link_and_poll_filters_match_what_a_message_has() {
        assert_eq!(
            link_host(" https://Docs.Rs/limn?x=1 ").as_deref(),
            Some("docs.rs")
        );
        assert_eq!(link_host("ejemplo.com.").as_deref(), Some("ejemplo.com"));
        for bad in ["", "a b", "-x.com", "x..com", "https://", "ñandú.ar"] {
            assert_eq!(link_host(bad), None, "{bad}");
        }
        let linked = message(5, "mirá <https://www.docs.rs/limn> y http://otro.ar");
        let sites = |hosts: &[&str]| RemoteSearch {
            link_hosts: hosts.iter().map(|host| host.to_string()).collect(),
            ..search("")
        };
        let found = |hosts: &[&str]| sites(hosts).loaded_matcher().unwrap()(&linked);
        assert!(found(&["docs.rs"]), "a site under it");
        assert!(found(&["nada.com", "otro.ar"]), "any of them");
        assert!(!found(&["cs.rs"]));
        assert!(!found(&["nada.com"]));
        assert!(sites(&["docs.rs"]).validate().is_ok());
        assert!(
            sites(&["Docs.rs"]).validate().is_err(),
            "written as link_host does"
        );
        let mut poll = message(6, "");
        poll.poll = Some(Default::default());
        let polls = RemoteSearch {
            has: vec![Has::Poll],
            ..search("")
        };
        assert!(polls.loaded_matcher().unwrap()(&poll));
        assert!(!polls.loaded_matcher().unwrap()(&linked));
    }

    #[test]
    fn a_page_by_relevance_keeps_the_providers_order() {
        let page = RemotePage {
            messages: vec![message(5, "a"), message(9, "b")],
            total: 2,
            offset: 0,
            partial: false,
        };
        assert!(!page.fits(&search("a"), 0));
        let ranked = RemoteSearch {
            order: RemoteOrder::Relevance,
            ..search("a")
        };
        assert!(page.fits(&ranked, 0));
        let mut twice = page;
        twice.messages[1].id = MessageId(5);
        assert!(!twice.fits(&ranked, 0), "each message once");
    }

    #[test]
    fn a_page_fits_only_its_search_and_offset() {
        let page = RemotePage {
            messages: vec![message(9, "b"), message(5, "a")],
            total: 2,
            offset: 0,
            partial: false,
        };
        assert!(page.fits(&search("a"), 0));
        assert!(!page.fits(&search("a"), 25));
        let mut elsewhere = page.clone();
        elsewhere.messages[0].channel = ChannelId(7);
        assert!(!elsewhere.fits(&search("a"), 0));
        assert!(elsewhere.fits(
            &RemoteSearch {
                server: Some(ServerId(1)),
                ..search("a")
            },
            0
        ));
        let mut unordered = page;
        unordered.messages.reverse();
        assert!(!unordered.fits(&search("a"), 0));
    }
}
