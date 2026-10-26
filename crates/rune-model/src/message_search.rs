//! Literal searches and explicit local context, independent of a provider.

use rune_i18n::t;

use crate::{ChannelId, Message, MessageId, Timestamp, UserId};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchQuery {
    pub channel: ChannelId,
    pub text: String,
    pub author: Option<UserId>,
    pub files: bool,
    pub pinned: bool,
    /// Inclusive message creation time, in UTC.
    pub from: Option<Timestamp>,
    /// Exclusive message creation time, in UTC.
    pub until: Option<Timestamp>,
}

impl SearchQuery {
    pub fn is_empty(&self) -> bool {
        self.text.trim().is_empty()
            && self.author.is_none()
            && !self.files
            && !self.pinned
            && self.from.is_none()
            && self.until.is_none()
    }

    pub fn validate(&self) -> Result<(), LocalReadError> {
        if self.channel.0 == 0
            || self.author.is_some_and(|author| author.0 == 0)
            || self.text.len() > 1024
            || self.text.chars().count() > 256
            || self.is_empty()
            || self
                .from
                .zip(self.until)
                .is_some_and(|(from, until)| from >= until)
        {
            return Err(LocalReadError::InvalidQuery);
        }
        Ok(())
    }

    /// Prepare once per scan. Matching preserves Rust's Unicode lowercase semantics.
    pub fn matcher(&self) -> impl Fn(&Message) -> bool + '_ {
        let needle = self.text.trim().to_lowercase();
        move |message| {
            message.channel == self.channel
                && self.author.is_none_or(|author| message.author == author)
                && (!self.files || !message.attachments.is_empty())
                && (!self.pinned || message.pinned)
                && self.from.is_none_or(|from| message.timestamp >= from)
                && self.until.is_none_or(|until| message.timestamp < until)
                && (needle.is_empty()
                    || std::iter::once(message.content.as_str())
                        .chain(card_words(message))
                        .any(|text| text.to_lowercase().contains(&needle)))
        }
    }
}

/// Opaque outside the local-read coordinator; never a raw SQL offset.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SearchCursor(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchHit {
    pub channel: ChannelId,
    pub id: MessageId,
    pub author: UserId,
    pub timestamp: Timestamp,
    pub snippet: String,
    pub files: bool,
    pub pinned: bool,
}

/// The words of a message's cards and of the copy it forwards, which a
/// search finds as it finds its text.
pub fn card_words(message: &Message) -> impl Iterator<Item = &str> {
    let copy = message.forwarded.as_ref();
    message
        .embeds
        .iter()
        .chain(copy.into_iter().flat_map(|copy| &copy.embeds))
        .flat_map(crate::Embed::words)
        .chain(copy.map(|copy| copy.content.as_str()))
}

impl SearchHit {
    /// Plain-text projection; no remote assets or full message payloads are retained.
    /// A message without text quotes its cards' words. A server's emoji
    /// reads `:name:`, never its token (2026-10-18).
    pub fn from_message(message: &Message) -> Self {
        let said = if message.content.trim().is_empty() {
            card_words(message).collect::<Vec<_>>().join(" ")
        } else {
            message
                .text_references
                .iter()
                .filter(|reference| !reference.source.is_empty())
                .fold(
                    message.content.clone(),
                    |said, reference| match &reference.target {
                        crate::TextTarget::Emoji { name, .. } => {
                            said.replace(&reference.source, &format!(":{name}:"))
                        }
                        _ => said,
                    },
                )
        };
        Self {
            channel: message.channel,
            id: message.id,
            author: message.author,
            timestamp: message.timestamp,
            snippet: said
                .chars()
                .take(160)
                .map(|character| {
                    if character.is_whitespace() {
                        ' '
                    } else {
                        character
                    }
                })
                .collect(),
            files: !message.attachments.is_empty(),
            pinned: message.pinned,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub continuation: Option<SearchCursor>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CachedContext {
    pub anchor: MessageId,
    /// Cached neighbors do not establish contiguous provider history.
    pub messages: Vec<Message>,
    pub has_older_cached: bool,
    pub has_newer_cached: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistorySource {
    Cached,
    Provider,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalReadError {
    Unavailable,
    InvalidQuery,
    NotFoundLocal,
    TooLarge,
    StaleCursor,
    Cancelled,
    Failed,
}

impl LocalReadError {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unavailable => t("El historial guardado no está disponible para esta cuenta."),
            Self::InvalidQuery => {
                t("Revisa la consulta: hasta 256 caracteres y un intervalo de fechas válido.")
            }
            Self::NotFoundLocal => t("Ese mensaje ya no está guardado en este equipo."),
            Self::TooLarge => t("Los datos superan el límite de esta lectura local."),
            Self::StaleCursor => t("El historial guardado cambió. Actualiza la búsqueda."),
            Self::Cancelled => t("La lectura local se canceló."),
            Self::Failed => t("No se pudo leer el historial guardado. Vuelve a intentarlo."),
        }
    }
}

impl std::fmt::Display for LocalReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

impl std::error::Error for LocalReadError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Attachment, AttachmentId, Delivery, MessageKind};

    fn message(content: &str) -> Message {
        Message {
            id: MessageId(1),
            channel: ChannelId(2),
            author: UserId(3),
            content: content.into(),
            timestamp: Timestamp(100),
            edited: Some(Timestamp(200)),
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

    fn query(text: &str) -> SearchQuery {
        SearchQuery {
            channel: ChannelId(2),
            text: text.into(),
            ..Default::default()
        }
    }

    #[test]
    fn search_validation_rejects_empty_invalid_scope_dates_and_oversized_text() {
        assert_eq!(query("").validate(), Err(LocalReadError::InvalidQuery));
        assert!(query(" ").is_empty());
        assert!(query("hola").validate().is_ok());
        assert!(query(&"🦀".repeat(256)).validate().is_ok());
        assert!(query(&"x".repeat(257)).validate().is_err());
        assert!(SearchQuery {
            channel: ChannelId(0),
            ..query("x")
        }
        .validate()
        .is_err());
        assert!(SearchQuery {
            author: Some(UserId(0)),
            ..query("x")
        }
        .validate()
        .is_err());
        for until in [99, 100] {
            assert!(SearchQuery {
                from: Some(Timestamp(100)),
                until: Some(Timestamp(until)),
                ..query("x")
            }
            .validate()
            .is_err());
        }
        assert!(SearchQuery {
            pinned: true,
            ..query("")
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn search_literal_unicode_matches_existing_loaded_semantics() {
        for (needle, content, matches) in [
            (" ÚNICO ", "mensaje único", true),
            ("from:me", "from:me literal", true),
            ("from:me", "mi mensaje", false),
            ("%_", "literal %_", true),
            ("%_", "cualquier cosa", false),
            ("ΟΣ", "ος", true),
            ("İ", "i\u{307}", true),
            ("unico", "único", false),
        ] {
            assert_eq!(
                query(needle).matcher()(&message(content)),
                matches,
                "{needle}"
            );
        }
    }

    #[test]
    fn search_filters_are_anded_and_dates_use_creation_time_with_exclusive_end() {
        let mut message = message("archivo");
        message.pinned = true;
        message.attachments.push(Attachment {
            id: AttachmentId(1),
            filename: "a".into(),
            size_bytes: 1,
            content_type: None,
            dimensions: None,
            preview: None,
            download: None,
            spoiler: false,
            voice: None,
            explicit: false,
            placeholder: None,
        });
        let search = SearchQuery {
            author: Some(UserId(3)),
            files: true,
            pinned: true,
            from: Some(Timestamp(100)),
            until: Some(Timestamp(101)),
            ..query("ARCHIVO")
        };
        assert!(search.matcher()(&message));
        for mismatched in [
            SearchQuery {
                author: Some(UserId(4)),
                ..search.clone()
            },
            SearchQuery {
                from: Some(Timestamp(101)),
                ..search.clone()
            },
            SearchQuery {
                until: Some(Timestamp(100)),
                ..search.clone()
            },
            SearchQuery {
                channel: ChannelId(4),
                ..search.clone()
            },
        ] {
            assert!(!mismatched.matcher()(&message));
        }
        message.attachments.clear();
        assert!(!search.matcher()(&message));
    }

    #[test]
    fn search_hits_are_bounded_plain_text_projections() {
        let hit = SearchHit::from_message(&message(&format!("uno\ndos {}", "🦀".repeat(200))));
        assert_eq!(hit.snippet.chars().count(), 160);
        assert!(hit.snippet.starts_with("uno dos "));
        assert!(!hit.files);
        // A server's emoji by its name, as a line of plain text shows it.
        let mut said = message("hola <:wave:10> y <a:baile:11>");
        said.text_references = ["<:wave:10>", "<a:baile:11>"]
            .into_iter()
            .map(|source| crate::TextReference {
                source: source.into(),
                target: crate::TextTarget::Emoji {
                    name: source.split(':').nth(1).unwrap().into(),
                    image: None,
                    jumbo: None,
                    id: None,
                },
            })
            .collect();
        assert_eq!(
            SearchHit::from_message(&said).snippet,
            "hola :wave: y :baile:"
        );
    }

    #[test]
    fn a_card_or_a_forward_is_found_and_quoted_by_its_words() {
        let mut card = message("");
        card.embeds.push(crate::Embed {
            title: Some("Compilación verde".into()),
            fields: vec![crate::EmbedField {
                name: "Rama".into(),
                value: "principal".into(),
                inline: true,
            }],
            ..Default::default()
        });
        assert!(query("VERDE").matcher()(&card));
        assert!(query("principal").matcher()(&card));
        assert!(!query("roja").matcher()(&card));
        assert_eq!(
            SearchHit::from_message(&card).snippet,
            "Compilación verde Rama principal"
        );
        // A poll result's fields are data, not words.
        card.embeds[0].kind = crate::EmbedKind::PollResult;
        assert!(!query("principal").matcher()(&card));
        let mut forward = message("");
        forward.forwarded = Some(crate::Forwarded {
            content: "nos vemos el jueves".into(),
            ..Default::default()
        });
        assert!(query("jueves").matcher()(&forward));
    }
}
