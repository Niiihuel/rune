//! What a server asks of someone who joins it (2026-10-18): its rules,
//! which they accept (with questions a moderator may review) before they
//! take part, and its onboarding: questions whose answers give roles and
//! open channels. The administration edits that same onboarding. The
//! limits follow the forms of the official client and of Serein
//! (docs/DISCORD.md); none was confirmed against a service.

use rune_i18n::{t, tr};

use crate::{Channel, ChannelId, ChannelKind, EmojiId, Member, Permissions, RoleId, Server};

/// Fields of a server's rules, at most.
pub const MAX_RULES_FIELDS: usize = 5;
/// Rules of the field that lists them, at most.
pub const MAX_RULES: usize = 16;
/// Characters of one rule, of a field's label or description, at most.
pub const MAX_RULE_CHARS: usize = 300;
/// Choices of a multiple-choice field, at most.
pub const MAX_RULES_CHOICES: usize = 8;
/// Characters of a short answer and of a paragraph, at most.
pub const MAX_SHORT_ANSWER: usize = 150;
pub const MAX_PARAGRAPH: usize = 1000;
/// Questions of an onboarding RUNE saves, and options of one question, at
/// most. A provider may send more questions: see [`MAX_PROMPTS_READ`].
pub const MAX_PROMPTS: usize = 15;
pub const MAX_PROMPT_OPTIONS: usize = 50;
/// Characters of a question's title, an option's title and its description.
pub const MAX_PROMPT_TITLE: usize = 100;
pub const MAX_OPTION_TITLE: usize = 50;
pub const MAX_OPTION_DESCRIPTION: usize = 100;
/// Questions of an onboarding RUNE reads, at most: a defensive bound above
/// what it saves, so a larger one still shows (Serein reads as many).
pub const MAX_PROMPTS_READ: usize = 50;
/// Characters of an option's emoji name, at most.
pub const MAX_EMOJI_NAME: usize = 64;

/// A server's rules, and what else it asks before a new member takes part.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ServerRules {
    /// What the provider calls this version, given back when accepted.
    pub version: Option<String>,
    /// What the server says before them.
    pub description: Option<String>,
    pub fields: Vec<RulesField>,
}

/// What one field of the rules asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RulesFieldKind {
    /// The rules themselves, to agree to.
    Terms,
    /// A line of text.
    ShortAnswer,
    Paragraph,
    /// One of the field's `choices`.
    MultipleChoice,
    /// A kind RUNE does not know, by the provider's name.
    Other(String),
}

/// One field of a server's rules.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RulesField {
    pub kind: RulesFieldKind,
    pub label: String,
    pub description: Option<String>,
    pub placeholder: Option<String>,
    /// The rules listed, for [`RulesFieldKind::Terms`].
    pub rules: Vec<String>,
    /// What can be chosen, for [`RulesFieldKind::MultipleChoice`].
    pub choices: Vec<String>,
    pub required: bool,
    /// The provider's own copy of the field, given back as it came when the
    /// rules are answered; opaque to everything above the provider.
    pub echo: Option<String>,
}

/// What the account answered to one field, in the order of the fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RulesAnswer {
    Agreed(bool),
    Text(String),
    Choice(Option<usize>),
}

/// What came of answering a server's rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JoinOutcome {
    /// The account may take part now.
    Approved,
    /// A moderator reviews the answers first.
    InReview,
    Rejected,
}

impl RulesFieldKind {
    /// Whether an answer to it waits on a moderator: anything but agreeing
    /// to the rules does.
    pub fn reviewed(&self) -> bool {
        !matches!(self, Self::Terms)
    }
}

impl RulesField {
    /// The answer it starts with: nothing agreed, written or chosen.
    pub fn starting(&self) -> RulesAnswer {
        match self.kind {
            RulesFieldKind::Terms => RulesAnswer::Agreed(false),
            RulesFieldKind::MultipleChoice => RulesAnswer::Choice(None),
            _ => RulesAnswer::Text(String::new()),
        }
    }

    /// The most characters a written answer takes; `None` for a field that
    /// is not written.
    pub fn max_chars(&self) -> Option<usize> {
        match self.kind {
            RulesFieldKind::ShortAnswer => Some(MAX_SHORT_ANSWER),
            RulesFieldKind::Paragraph => Some(MAX_PARAGRAPH),
            _ => None,
        }
    }

    /// Whether it lists no more rules and choices than a provider sends.
    fn within_limits(&self) -> bool {
        self.rules.len() <= MAX_RULES && self.choices.len() <= MAX_RULES_CHOICES
    }

    /// Why `answer` cannot go for this field, if it cannot.
    pub fn problem(&self, answer: &RulesAnswer) -> Option<String> {
        if let RulesFieldKind::Other(_) = self.kind {
            return self
                .required
                .then(|| t("RUNE todavía no completa este campo").into());
        }
        let given = match answer {
            RulesAnswer::Agreed(agreed) if self.kind == RulesFieldKind::Terms => *agreed,
            RulesAnswer::Choice(picked) if self.kind == RulesFieldKind::MultipleChoice => {
                picked.is_some_and(|index| index < self.choices.len())
            }
            RulesAnswer::Text(written) => match self.max_chars() {
                Some(max) if written.chars().count() > max => {
                    return Some(tr!("Escribe como mucho {max} caracteres", max));
                }
                Some(_) => !written.trim().is_empty(),
                None => false,
            },
            _ => false,
        };
        if given || !self.required {
            return None;
        }
        Some(
            match self.kind {
                RulesFieldKind::Terms => t("Acepta las reglas para seguir"),
                RulesFieldKind::MultipleChoice => t("Elige una opción"),
                _ => t("Completa este campo"),
            }
            .into(),
        )
    }
}

impl ServerRules {
    /// Whether the answers go to a moderator before the account takes part.
    pub fn reviewed(&self) -> bool {
        self.fields.iter().any(|field| field.kind.reviewed())
    }

    /// Why `answers` cannot be sent, if they cannot: the first field's
    /// problem, or a count that does not match the fields.
    pub fn problem(&self, answers: &[RulesAnswer]) -> Option<String> {
        if answers.len() != self.fields.len() {
            return Some(t("Las reglas cambiaron: ábrelas otra vez").into());
        }
        self.fields
            .iter()
            .zip(answers)
            .find_map(|(field, answer)| field.problem(answer))
    }

    /// Whether it keeps within the limits a provider sends.
    pub fn bounded(&self) -> bool {
        self.fields.len() <= MAX_RULES_FIELDS && self.fields.iter().all(RulesField::within_limits)
    }
}

/// Whether someone with no role but `@everyone` may write in `channel` of
/// `server`: an enabled onboarding needs such a channel among those it
/// counts.
pub fn everyone_writes(server: &Server, channel: &Channel) -> bool {
    let nobody = Member {
        server: server.id,
        ..Member::default()
    };
    channel.kind == ChannelKind::Text
        && channel.server == Some(server.id)
        && crate::permissions::in_channel(server, &nobody, channel)
            .contains(Permissions::VIEW | Permissions::SEND)
}

/// A question of an onboarding.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PromptId(pub u64);

/// An option of a question of an onboarding.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ChoiceId(pub u64);

/// What counts toward an onboarding's requirements.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OnboardingMode {
    /// Only the channels everyone gets.
    #[default]
    Default,
    /// Those and the channels the questions open.
    Advanced,
}

/// How a question shows its options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PromptStyle {
    #[default]
    Choices,
    Dropdown,
}

/// An option's emoji: a Unicode one, or the server's by `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptEmoji {
    pub name: String,
    pub id: Option<EmojiId>,
    pub animated: bool,
}

/// One option of a question: what choosing it gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptChoice {
    pub id: ChoiceId,
    pub title: String,
    pub description: Option<String>,
    pub emoji: Option<PromptEmoji>,
    pub roles: Vec<RoleId>,
    pub channels: Vec<ChannelId>,
}

/// One question of an onboarding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Prompt {
    pub id: PromptId,
    pub title: String,
    pub style: PromptStyle,
    /// Only one option may be chosen.
    pub single: bool,
    pub required: bool,
    /// Asked when joining; otherwise only offered later.
    pub in_onboarding: bool,
    pub options: Vec<PromptChoice>,
}

/// A server's onboarding: what it asks, and what everyone gets.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Onboarding {
    pub enabled: bool,
    pub mode: OnboardingMode,
    /// The channels every new member gets.
    pub default_channels: Vec<ChannelId>,
    pub prompts: Vec<Prompt>,
    /// What the account chose, as the provider reported it. Never saved
    /// by an edit.
    pub chosen: Vec<ChoiceId>,
}

impl Onboarding {
    /// The questions someone joining answers: those of the onboarding that
    /// have options.
    pub fn asked(&self) -> impl Iterator<Item = &Prompt> {
        self.prompts
            .iter()
            .filter(|prompt| prompt.in_onboarding && !prompt.options.is_empty())
    }

    /// Whether there is anything to answer.
    pub fn asks(&self) -> bool {
        self.enabled && self.asked().next().is_some()
    }

    /// What the account chose among the questions asked: the provider may
    /// also report options of questions asked only later, or gone.
    pub fn answered(&self) -> Vec<ChoiceId> {
        self.chosen
            .iter()
            .copied()
            .filter(|id| self.asking(*id).is_some())
            .collect()
    }

    /// The question asked that offers `choice`.
    fn asking(&self, choice: ChoiceId) -> Option<&Prompt> {
        self.asked()
            .find(|prompt| prompt.options.iter().any(|option| option.id == choice))
    }

    /// Why `chosen` cannot be sent as the answers, if it cannot: an option
    /// no question asked offers, then each question by its order.
    pub fn answer_problem(&self, chosen: &[ChoiceId]) -> Option<String> {
        let mut per_prompt: Vec<PromptId> = Vec::with_capacity(chosen.len());
        for choice in chosen {
            match self.asking(*choice) {
                Some(prompt) => per_prompt.push(prompt.id),
                None => {
                    return Some(t("Esa opción ya no está: abre las preguntas otra vez").into())
                }
            }
        }
        self.asked().find_map(|prompt| {
            let title = &prompt.title;
            let count = per_prompt.iter().filter(|id| **id == prompt.id).count();
            if count == 0 && prompt.required {
                Some(tr!("Responde «{title}»", title))
            } else if count > 1 && prompt.single {
                Some(tr!("Elige una sola opción en «{title}»", title))
            } else {
                None
            }
        })
    }

    /// The channels that count toward its requirements, by its mode.
    pub fn counted_channels(&self) -> Vec<ChannelId> {
        let mut channels = self.default_channels.clone();
        if self.mode == OnboardingMode::Advanced {
            for option in self.asked().flat_map(|prompt| &prompt.options) {
                channels.extend(option.channels.iter().copied());
            }
        }
        channels.sort();
        channels.dedup();
        channels
    }

    /// Why it cannot be saved as it is, if it cannot. `everyone_writes`
    /// says whether everyone may write in a channel: an enabled onboarding
    /// needs one such channel among those it counts.
    pub fn problem(&self, everyone_writes: impl Fn(ChannelId) -> bool) -> Option<String> {
        if self.prompts.len() > MAX_PROMPTS {
            return Some(tr!("Hasta {max} preguntas", max = MAX_PROMPTS));
        }
        for prompt in &self.prompts {
            let title = prompt.title.trim();
            if title.is_empty() {
                return Some(t("Cada pregunta necesita un título").into());
            }
            if title.chars().count() > MAX_PROMPT_TITLE {
                return Some(tr!(
                    "El título de una pregunta lleva hasta {max} caracteres",
                    max = MAX_PROMPT_TITLE
                ));
            }
            if prompt.options.is_empty() {
                return Some(tr!("«{title}» necesita al menos una opción", title));
            }
            if prompt.options.len() > MAX_PROMPT_OPTIONS {
                return Some(tr!(
                    "Hasta {max} opciones por pregunta",
                    max = MAX_PROMPT_OPTIONS
                ));
            }
            for option in &prompt.options {
                let name = option.title.trim();
                if name.is_empty() {
                    return Some(tr!("Cada opción de «{title}» necesita un título", title));
                }
                if name.chars().count() > MAX_OPTION_TITLE {
                    return Some(tr!(
                        "El título de una opción lleva hasta {max} caracteres",
                        max = MAX_OPTION_TITLE
                    ));
                }
                if option
                    .description
                    .as_ref()
                    .is_some_and(|text| text.chars().count() > MAX_OPTION_DESCRIPTION)
                {
                    return Some(tr!(
                        "La descripción de una opción lleva hasta {max} caracteres",
                        max = MAX_OPTION_DESCRIPTION
                    ));
                }
                if option.emoji.as_ref().is_some_and(|emoji| {
                    emoji.name.is_empty()
                        || emoji.name.chars().count() > MAX_EMOJI_NAME
                        || emoji
                            .name
                            .chars()
                            .any(|c| c.is_control() || c.is_whitespace())
                }) {
                    return Some(tr!("El emoji de una opción de «{title}» no vale", title));
                }
            }
        }
        if self.enabled && !self.counted_channels().into_iter().any(everyone_writes) {
            return Some(
                t("Para activarla, elige al menos un canal por defecto donde todos puedan escribir.")
                    .into(),
            );
        }
        None
    }

    /// An id for a question or an option made now, `now_ms` into the
    /// epoch, as the official client names them: the time, or one past the
    /// last id made so in the same millisecond. The provider's own ids
    /// (snowflakes, far above any such time) are not compared.
    pub fn fresh_id(&self, now_ms: u64) -> u64 {
        // A snowflake keeps its time above bit 22: any made after the first
        // 17 minutes of its epoch is above every millisecond until 2109.
        const MADE_HERE: u64 = 1 << 42;
        let latest = self
            .prompts
            .iter()
            .flat_map(|prompt| {
                std::iter::once(prompt.id.0).chain(prompt.options.iter().map(|option| option.id.0))
            })
            .filter(|id| *id < MADE_HERE)
            .max();
        latest.map_or(now_ms, |latest| now_ms.max(latest + 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A field of a photography club's form, asking `kind`.
    fn asking(kind: RulesFieldKind, label: &str, required: bool) -> RulesField {
        RulesField {
            kind,
            label: label.into(),
            description: Some("La moderación lo lee antes de aprobarte.".into()),
            placeholder: None,
            rules: Vec::new(),
            choices: vec!["Mañana".into(), "Tarde".into(), "Noche".into()],
            required,
            echo: None,
        }
    }

    /// A question about a club's channels, offering `options`.
    fn question(id: u64, title: &str, single: bool, required: bool) -> Prompt {
        Prompt {
            id: PromptId(id),
            title: title.into(),
            style: PromptStyle::Dropdown,
            single,
            required,
            in_onboarding: true,
            options: Vec::new(),
        }
    }

    fn offering(prompt: Prompt, options: &[(u64, &str, Option<u64>)]) -> Prompt {
        Prompt {
            options: options
                .iter()
                .map(|(id, title, channel)| PromptChoice {
                    id: ChoiceId(*id),
                    title: (*title).into(),
                    description: Some(format!("Abre #{title}")),
                    emoji: Some(PromptEmoji {
                        name: "📷".into(),
                        id: None,
                        animated: false,
                    }),
                    roles: vec![RoleId(*id + 7)],
                    channels: channel.iter().copied().map(ChannelId).collect(),
                })
                .collect(),
            ..prompt
        }
    }

    #[test]
    fn a_written_answer_is_bounded_by_its_kind_and_an_unknown_kind_only_blocks_when_required() {
        let story = asking(RulesFieldKind::Paragraph, "Cuéntanos de ti", true);
        let nick = asking(RulesFieldKind::ShortAnswer, "¿Cómo te dicen?", false);
        assert_eq!(story.max_chars(), Some(MAX_PARAGRAPH));
        assert_eq!(nick.max_chars(), Some(MAX_SHORT_ANSWER));
        assert_eq!(
            asking(RulesFieldKind::Terms, "Reglas", true).max_chars(),
            None
        );
        // Blank is unanswered; the limit counts characters, not bytes.
        assert!(story.problem(&RulesAnswer::Text(" \n ".into())).is_some());
        let full = "ñ".repeat(MAX_PARAGRAPH);
        assert_eq!(story.problem(&RulesAnswer::Text(full.clone())), None);
        assert!(story
            .problem(&RulesAnswer::Text(format!("{full}ñ")))
            .is_some());
        // Optional, it may stay empty, but never too long.
        assert_eq!(nick.problem(&RulesAnswer::Text(String::new())), None);
        assert!(nick
            .problem(&RulesAnswer::Text("a".repeat(MAX_SHORT_ANSWER + 1)))
            .is_some());
        // An answer of another kind answers nothing.
        assert!(story.problem(&RulesAnswer::Agreed(true)).is_some());
        let mut unknown = asking(
            RulesFieldKind::Other("VIDEO_CALL".into()),
            "Videollamada",
            false,
        );
        assert_eq!(unknown.problem(&RulesAnswer::Text("hola".into())), None);
        unknown.required = true;
        assert!(unknown.problem(&RulesAnswer::Agreed(true)).is_some());
    }

    #[test]
    fn rules_go_to_review_unless_they_only_ask_to_agree() {
        let mut terms = asking(RulesFieldKind::Terms, "Leí las reglas", true);
        terms.rules = vec!["Sin flash en la sala oscura.".into()];
        let mut rules = ServerRules {
            version: None,
            description: None,
            fields: vec![terms.clone()],
        };
        assert!(!rules.reviewed());
        assert_eq!(
            rules.problem(&[RulesAnswer::Agreed(false)]),
            Some(t("Acepta las reglas para seguir").into())
        );
        assert_eq!(rules.problem(&[RulesAnswer::Agreed(true)]), None);
        rules.fields.push(asking(
            RulesFieldKind::MultipleChoice,
            "¿Cuándo sacas?",
            true,
        ));
        assert!(rules.reviewed());
        assert!(
            rules.problem(&[RulesAnswer::Agreed(true)]).is_some(),
            "one answer per field"
        );
        let night = [RulesAnswer::Agreed(true), RulesAnswer::Choice(Some(2))];
        assert_eq!(rules.problem(&night), None);
        let beyond = [RulesAnswer::Agreed(true), RulesAnswer::Choice(Some(3))];
        assert!(
            rules.problem(&beyond).is_some(),
            "only the choices it offers"
        );
        assert!(rules.bounded());
        terms.rules = vec![String::new(); MAX_RULES + 1];
        rules.fields[0] = terms;
        assert!(!rules.bounded());
        rules.fields = vec![asking(RulesFieldKind::Terms, "Reglas", false); MAX_RULES_FIELDS + 1];
        assert!(!rules.bounded());
    }

    #[test]
    fn answers_count_only_the_options_of_the_questions_asked() {
        let gear = offering(
            question(1_100, "¿Qué cámara usas?", true, true),
            &[(1_101, "analogica", Some(40)), (1_102, "digital", None)],
        );
        let themes = offering(
            question(1_200, "¿Qué te gusta sacar?", false, false),
            &[(1_201, "retratos", None), (1_202, "paisajes", Some(41))],
        );
        let mut later = offering(
            question(1_300, "¿Quieres avisos?", false, false),
            &[(1_301, "concursos", None)],
        );
        later.in_onboarding = false;
        let onboarding = Onboarding {
            enabled: true,
            prompts: vec![gear, question(1_400, "Vacía", false, true), themes, later],
            // What the provider said was chosen: a later question's option,
            // and one that is gone.
            chosen: vec![ChoiceId(1_202), ChoiceId(1_301), ChoiceId(9)],
            ..Onboarding::default()
        };
        assert_eq!(
            onboarding
                .asked()
                .map(|prompt| prompt.id.0)
                .collect::<Vec<_>>(),
            [1_100, 1_200],
            "neither a question without options nor one asked later"
        );
        assert_eq!(onboarding.answered(), [ChoiceId(1_202)]);
        assert_eq!(
            onboarding.answer_problem(&onboarding.chosen),
            Some(t("Esa opción ya no está: abre las preguntas otra vez").into())
        );
        assert_eq!(
            onboarding.answer_problem(&onboarding.answered()),
            Some(tr!("Responde «{title}»", title = "¿Qué cámara usas?"))
        );
        let both = [ChoiceId(1_102), ChoiceId(1_101), ChoiceId(1_202)];
        assert_eq!(
            onboarding.answer_problem(&both),
            Some(tr!(
                "Elige una sola opción en «{title}»",
                title = "¿Qué cámara usas?"
            ))
        );
        assert_eq!(
            onboarding.answer_problem(&[ChoiceId(1_201), ChoiceId(1_202), ChoiceId(1_102)]),
            None
        );
        assert!(onboarding.asks());
        let off = Onboarding {
            enabled: false,
            ..onboarding
        };
        assert!(!off.asks());
    }

    #[test]
    fn an_edit_needs_titles_a_valid_emoji_and_when_on_a_channel_everyone_writes_in() {
        let open = |channel: ChannelId| channel == ChannelId(41);
        let mut onboarding = Onboarding {
            enabled: true,
            default_channels: vec![ChannelId(40)],
            prompts: vec![offering(
                question(1_200, "¿Qué te gusta sacar?", false, false),
                &[(1_202, "paisajes", Some(41))],
            )],
            ..Onboarding::default()
        };
        assert!(
            onboarding.problem(open).is_some(),
            "a default channel nobody writes in"
        );
        onboarding.mode = OnboardingMode::Advanced;
        assert_eq!(
            onboarding.counted_channels(),
            [ChannelId(40), ChannelId(41)]
        );
        assert_eq!(onboarding.problem(open), None);
        fn option(onboarding: &mut Onboarding) -> &mut PromptChoice {
            &mut onboarding.prompts[0].options[0]
        }
        option(&mut onboarding).emoji.as_mut().unwrap().name = "dos palabras".into();
        assert!(onboarding.problem(open).is_some(), "an emoji is one name");
        option(&mut onboarding).emoji.as_mut().unwrap().name = "x".repeat(MAX_EMOJI_NAME + 1);
        assert!(onboarding.problem(open).is_some());
        option(&mut onboarding).emoji = None;
        option(&mut onboarding).description = Some("d".repeat(MAX_OPTION_DESCRIPTION + 1));
        assert!(onboarding.problem(open).is_some());
        option(&mut onboarding).description = None;
        onboarding.prompts[0].title = "t".repeat(MAX_PROMPT_TITLE + 1);
        assert!(onboarding.problem(open).is_some());
        onboarding.prompts[0].title = "Paisajes".into();
        assert_eq!(onboarding.problem(open), None);
        onboarding.prompts = vec![question(5, "x", false, false); MAX_PROMPTS + 1];
        assert!(onboarding.problem(open).is_some());
        // Off, nothing has to be open.
        onboarding.prompts.clear();
        onboarding.enabled = false;
        assert_eq!(onboarding.problem(|_| false), None);
    }

    #[test]
    fn a_fresh_id_is_the_time_and_only_steps_past_the_ids_made_so() {
        let snowflake = 1_300_000_000_000_000_000;
        let mut onboarding = Onboarding {
            prompts: vec![offering(
                question(snowflake, "¿Qué cámara usas?", true, true),
                &[(snowflake + 1, "analogica", None)],
            )],
            ..Onboarding::default()
        };
        assert_eq!(onboarding.fresh_id(1_760_000_000_000), 1_760_000_000_000);
        onboarding.prompts[0].options[0].id = ChoiceId(1_760_000_000_000);
        assert_eq!(onboarding.fresh_id(1_760_000_000_000), 1_760_000_000_001);
        assert_eq!(onboarding.fresh_id(1_760_000_000_500), 1_760_000_000_500);
        onboarding.prompts[0].id = PromptId(u64::MAX);
        assert_eq!(onboarding.fresh_id(5), 1_760_000_000_001, "never overflows");
    }
}
