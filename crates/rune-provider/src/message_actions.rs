//! What message actions and voice moderation carry (P4): a new poll, a
//! change to someone's voice, and the marker of a message that notifies
//! nobody.

use rune_i18n::t;
use rune_model::ChannelId;

/// A poll to start, as [`crate::Command::CreatePoll`] carries it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewPoll {
    pub question: String,
    pub answers: Vec<NewPollAnswer>,
    /// How long voting stays open, one of [`POLL_HOURS`].
    pub hours: u32,
    pub multiselect: bool,
}

/// One answer of a [`NewPoll`]: text, an emoji, or both.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewPollAnswer {
    pub text: String,
    /// A Unicode emoji before its text.
    pub emoji: Option<String>,
}

/// The most answers a poll has, and the fewest.
pub const POLL_ANSWERS: std::ops::RangeInclusive<usize> = 2..=10;
/// The longest question and answer, in characters.
pub const POLL_QUESTION_MAX: usize = 300;
pub const POLL_ANSWER_MAX: usize = 55;
/// How long a poll may stay open, in hours: an hour to a week.
pub const POLL_HOURS: [u32; 6] = [1, 4, 8, 24, 72, 168];

impl NewPoll {
    /// Why the service would not take it, as the interface says it; `None`
    /// when it would.
    pub fn problem(&self) -> Option<&'static str> {
        if self.question.trim().is_empty() {
            Some(t("Escribí una pregunta"))
        } else if self.question.chars().count() > POLL_QUESTION_MAX {
            Some(t("La pregunta es demasiado larga"))
        } else if !POLL_ANSWERS.contains(&self.answers.len()) {
            Some(t("Una encuesta tiene entre 2 y 10 respuestas"))
        } else if self
            .answers
            .iter()
            .any(|answer| answer.text.trim().is_empty() && answer.emoji.is_none())
        {
            Some(t("Cada respuesta necesita texto o un emoji"))
        } else if self
            .answers
            .iter()
            .any(|answer| answer.text.chars().count() > POLL_ANSWER_MAX)
        {
            Some(t("Una respuesta es demasiado larga"))
        } else if !POLL_HOURS.contains(&self.hours) {
            Some(t("Elegí cuánto dura"))
        } else {
            None
        }
    }
}

/// A change to someone's voice in a server, made by its moderation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VoiceModeration {
    /// Keep their microphone off in every voice channel of the server, or
    /// let it be.
    Mute(bool),
    /// Keep their sound off, or let it be.
    Deafen(bool),
    /// Take them to another voice channel of the server.
    Move(ChannelId),
    /// Take them out of voice.
    Disconnect,
}

/// The most conversations a message is forwarded to at once.
pub const MAX_FORWARDS: usize = 5;

/// What a message starts with to notify nobody, with
/// [`crate::Capabilities::silent_messages`]: the marker and one space or
/// line break after it go, and the rest is sent.
pub const SILENT: &str = "@silent";

/// The text of `content` without the [`SILENT`] marker, when it starts with it.
pub fn without_silent(content: &str) -> Option<&str> {
    let rest = content.strip_prefix(SILENT)?;
    let mut chars = rest.chars();
    match chars.next() {
        None => Some(""),
        Some(space) if space.is_whitespace() => Some(chars.as_str()),
        Some(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_silent_marker_starts_the_text_and_takes_one_space() {
        assert_eq!(without_silent("@silent hola"), Some("hola"));
        assert_eq!(without_silent("@silent  hola"), Some(" hola"));
        assert_eq!(without_silent("@silent\nhola"), Some("hola"));
        assert_eq!(without_silent("@silent"), Some(""));
        assert_eq!(without_silent("@silently hola"), None);
        assert_eq!(without_silent("hola @silent"), None);
        assert_eq!(without_silent("@Silent hola"), None);
    }

    #[test]
    fn a_poll_needs_a_question_two_answers_and_a_known_length() {
        let answer = |text: &str| NewPollAnswer {
            text: text.into(),
            emoji: None,
        };
        let mut poll = NewPoll {
            question: "¿Pizza o empanadas?".into(),
            answers: vec![answer("Pizza"), answer("Empanadas")],
            hours: 24,
            multiselect: false,
        };
        assert_eq!(poll.problem(), None);
        poll.hours = 5;
        assert!(poll.problem().is_some());
        poll.hours = 1;
        poll.answers.push(answer(""));
        assert!(poll.problem().is_some());
        poll.answers.last_mut().unwrap().emoji = Some("🍕".into());
        assert_eq!(poll.problem(), None);
        poll.answers.truncate(1);
        assert!(poll.problem().is_some());
        poll.answers = vec![answer(&"a".repeat(56)), answer("b")];
        assert!(poll.problem().is_some());
    }
}
