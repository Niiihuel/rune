//! A server's automatic moderation (AutoMod, 2026-10-19): rules that look
//! at what members write, or at their names and profiles, and block it,
//! alert a channel or time the member out. The limits are those the
//! service documents and the official client's bundle holds (docs/REA.md);
//! none was confirmed against a service.

use rune_i18n::{t, tr};

use crate::{ChannelId, RoleId, ServerId, UserId};

/// Characters of a rule's name, at most.
pub const MAX_NAME: usize = 100;
/// Words of a rule, and characters of each, at most.
pub const MAX_KEYWORDS: usize = 1000;
pub const MAX_KEYWORD_CHARS: usize = 60;
/// Regular expressions of a rule, and characters of each, at most.
pub const MAX_PATTERNS: usize = 10;
pub const MAX_PATTERN_CHARS: usize = 260;
/// Exceptions of a rule of words or profiles, and of one of the service's
/// lists, at most; each as long as a word.
pub const MAX_ALLOWED: usize = 100;
pub const MAX_ALLOWED_PRESETS: usize = 1000;
/// Mentions in one message a rule may allow, at most.
pub const MAX_MENTIONS: u32 = 50;
/// Characters of the message shown to whoever is blocked, at most.
pub const MAX_NOTICE: usize = 150;
/// The longest a rule times a member out: four weeks.
pub const MAX_TIMEOUT_SECS: u32 = 4 * 7 * 24 * 60 * 60;
/// Roles and channels a rule leaves alone, at most.
pub const MAX_EXEMPT_ROLES: usize = 20;
pub const MAX_EXEMPT_CHANNELS: usize = 50;

/// An AutoMod rule of a server.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AutoModRuleId(pub u64);

impl std::fmt::Display for AutoModRuleId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

/// What a rule looks for, as a kind: what the editor offers to create.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AutoModKind {
    Keywords,
    Spam,
    Presets,
    Mentions,
    Profile,
}

impl AutoModKind {
    pub const ALL: [Self; 5] = [
        Self::Keywords,
        Self::Mentions,
        Self::Spam,
        Self::Presets,
        Self::Profile,
    ];

    /// How many rules of this kind a server may have, as the service documents.
    pub fn max_rules(self) -> usize {
        match self {
            Self::Keywords => 6,
            _ => 1,
        }
    }

    /// The name a new rule of this kind starts with.
    pub fn new_name(self) -> &'static str {
        match self {
            Self::Keywords => t("Bloquear palabras propias"),
            Self::Spam => t("Bloquear lo que parece spam"),
            Self::Presets => t("Bloquear palabras de las listas"),
            Self::Mentions => t("Bloquear el spam de menciones"),
            Self::Profile => t("Bloquear palabras en los perfiles"),
        }
    }
}

/// Words, patterns and exceptions a rule looks for in messages or profiles.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WordFilter {
    /// Words; `*` at either end matches the rest of a word.
    pub keywords: Vec<String>,
    /// Regular expressions, as the service reads them (Rust's syntax).
    pub patterns: Vec<String>,
    /// What never counts, even when it matches.
    pub allowed: Vec<String>,
}

/// One of the service's own lists of words.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AutoModPreset {
    Profanity,
    Sexual,
    Slurs,
}

impl AutoModPreset {
    pub const ALL: [Self; 3] = [Self::Profanity, Self::Sexual, Self::Slurs];
}

/// What a rule looks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutoModTrigger {
    /// The server's own words in messages.
    Keywords(WordFilter),
    /// Messages the service takes for spam.
    Spam,
    /// The service's lists of words, with the server's exceptions.
    Presets {
        lists: Vec<AutoModPreset>,
        allowed: Vec<String>,
    },
    /// More than `limit` different people and roles mentioned in one
    /// message; `raids` also watches for many members doing it at once.
    Mentions { limit: u32, raids: bool },
    /// The server's own words in members' names and profiles.
    Profile(WordFilter),
    /// A kind RUNE does not know, by the provider's number: shown, turned
    /// on and off and deleted, never edited.
    Other(u32),
}

impl AutoModTrigger {
    pub fn kind(&self) -> Option<AutoModKind> {
        Some(match self {
            Self::Keywords(_) => AutoModKind::Keywords,
            Self::Spam => AutoModKind::Spam,
            Self::Presets { .. } => AutoModKind::Presets,
            Self::Mentions { .. } => AutoModKind::Mentions,
            Self::Profile(_) => AutoModKind::Profile,
            Self::Other(_) => return None,
        })
    }

    /// An empty one of `kind`, as a new rule starts.
    pub fn fresh(kind: AutoModKind) -> Self {
        match kind {
            AutoModKind::Keywords => Self::Keywords(WordFilter::default()),
            AutoModKind::Spam => Self::Spam,
            AutoModKind::Presets => Self::Presets {
                lists: Vec::new(),
                allowed: Vec::new(),
            },
            AutoModKind::Mentions => Self::Mentions {
                limit: 20,
                raids: true,
            },
            AutoModKind::Profile => Self::Profile(WordFilter::default()),
        }
    }
}

/// What a rule does when it matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutoModAction {
    /// The message is not posted; `notice` is what its author is told.
    Block { notice: Option<String> },
    /// A note of what matched goes to `channel`.
    Alert { channel: ChannelId },
    /// The author cannot take part for `seconds`.
    Timeout { seconds: u32 },
    /// The member cannot take part until their profile changes.
    Quarantine,
    /// An action RUNE does not know, by the provider's number.
    Other(u32),
}

/// An AutoMod rule of a server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoModRule {
    /// `None` for one not saved yet.
    pub id: Option<AutoModRuleId>,
    pub name: String,
    pub trigger: AutoModTrigger,
    pub actions: Vec<AutoModAction>,
    pub enabled: bool,
    /// Roles and channels it leaves alone.
    pub exempt_roles: Vec<RoleId>,
    pub exempt_channels: Vec<ChannelId>,
    pub creator: Option<UserId>,
}

impl AutoModRule {
    /// A new rule of `kind` that blocks, on, named as the kind.
    pub fn fresh(kind: AutoModKind) -> Self {
        Self {
            id: None,
            name: kind.new_name().to_owned(),
            trigger: AutoModTrigger::fresh(kind),
            actions: vec![if kind == AutoModKind::Profile {
                AutoModAction::Quarantine
            } else {
                AutoModAction::Block { notice: None }
            }],
            enabled: true,
            exempt_roles: Vec::new(),
            exempt_channels: Vec::new(),
            creator: None,
        }
    }

    /// Whether RUNE can change it without losing what it does not know.
    pub fn editable(&self) -> bool {
        self.trigger.kind().is_some()
            && !self
                .actions
                .iter()
                .any(|action| matches!(action, AutoModAction::Other(_)))
    }

    /// Why it cannot be saved as it is, if it cannot: what the service
    /// would refuse, checked before anything is sent.
    pub fn problem(&self) -> Option<String> {
        let Some(kind) = self.trigger.kind().filter(|_| self.editable()) else {
            return Some(t("RUNE no sabe cambiar este tipo de regla").into());
        };
        let name = self.name.trim();
        if name.is_empty() {
            return Some(t("Poné un nombre a la regla").into());
        }
        if name.chars().count() > MAX_NAME {
            return Some(tr!(
                "El nombre de la regla tiene más de {max} caracteres",
                max = MAX_NAME
            ));
        }
        let problem = match &self.trigger {
            AutoModTrigger::Keywords(filter) | AutoModTrigger::Profile(filter) => {
                filter_problem(filter)
            }
            AutoModTrigger::Presets { lists, .. } if lists.is_empty() => {
                Some(t("Elegí al menos una lista").into())
            }
            AutoModTrigger::Presets { allowed, .. } => {
                words_problem(allowed, MAX_ALLOWED_PRESETS, true)
            }
            AutoModTrigger::Mentions { limit, .. } => (!(1..=MAX_MENTIONS).contains(limit))
                .then(|| tr!("El límite de menciones va de 1 a {max}", max = MAX_MENTIONS)),
            AutoModTrigger::Spam | AutoModTrigger::Other(_) => None,
        };
        if problem.is_some() {
            return problem;
        }
        if self.actions.is_empty() {
            return Some(t("Elegí al menos una acción").into());
        }
        for action in &self.actions {
            let problem = match action {
                AutoModAction::Block { .. } if kind == AutoModKind::Profile => {
                    Some(t("Una regla de perfiles no bloquea mensajes"))
                }
                AutoModAction::Block {
                    notice: Some(notice),
                } if notice.chars().count() > MAX_NOTICE => {
                    return Some(tr!(
                        "El mensaje para quien escribió tiene más de {max} caracteres",
                        max = MAX_NOTICE
                    ))
                }
                AutoModAction::Alert { channel } if channel.0 == 0 => {
                    Some(t("Elegí el canal de la alerta"))
                }
                AutoModAction::Timeout { .. }
                    if !matches!(kind, AutoModKind::Keywords | AutoModKind::Mentions) =>
                {
                    Some(t("Sólo una regla de palabras o de menciones aísla"))
                }
                AutoModAction::Timeout { seconds } if !(1..=MAX_TIMEOUT_SECS).contains(seconds) => {
                    Some(t("El aislamiento dura hasta cuatro semanas"))
                }
                AutoModAction::Quarantine if kind != AutoModKind::Profile => {
                    Some(t("Sólo una regla de perfiles bloquea la participación"))
                }
                _ => None,
            };
            if let Some(problem) = problem {
                return Some(problem.into());
            }
        }
        if self.exempt_roles.len() > MAX_EXEMPT_ROLES {
            return Some(tr!("Hasta {max} roles exentos", max = MAX_EXEMPT_ROLES));
        }
        if self.exempt_channels.len() > MAX_EXEMPT_CHANNELS {
            return Some(tr!(
                "Hasta {max} canales exentos",
                max = MAX_EXEMPT_CHANNELS
            ));
        }
        None
    }
}

/// Why `filter` cannot be saved, if it cannot.
fn filter_problem(filter: &WordFilter) -> Option<String> {
    if filter.keywords.is_empty() && filter.patterns.is_empty() {
        return Some(t("Agregá al menos una palabra o una expresión regular").into());
    }
    if let Some(problem) = words_problem(&filter.keywords, MAX_KEYWORDS, false) {
        return Some(problem);
    }
    if filter.patterns.len() > MAX_PATTERNS {
        return Some(tr!("Hasta {max} expresiones regulares", max = MAX_PATTERNS));
    }
    for pattern in &filter.patterns {
        if pattern.chars().count() > MAX_PATTERN_CHARS {
            return Some(tr!(
                "«{pattern}» tiene más de {max} caracteres",
                pattern,
                max = MAX_PATTERN_CHARS
            ));
        }
        if regex_syntax::Parser::new().parse(pattern).is_err() {
            return Some(tr!(
                "«{pattern}» no es una expresión regular válida",
                pattern
            ));
        }
    }
    words_problem(&filter.allowed, MAX_ALLOWED, true)
}

/// Why `words` cannot be saved, if they cannot: too many, or one too long.
fn words_problem(words: &[String], max: usize, allowed: bool) -> Option<String> {
    if words.len() > max {
        return Some(if allowed {
            tr!("Hasta {max} excepciones", max)
        } else {
            tr!("Hasta {max} palabras", max)
        });
    }
    words
        .iter()
        .find(|word| word.chars().count() > MAX_KEYWORD_CHARS)
        .map(|word| {
            tr!(
                "«{word}» tiene más de {max} caracteres",
                word,
                max = MAX_KEYWORD_CHARS
            )
        })
}

/// The words of `text`, separated by commas or lines, as the editor takes
/// words and exceptions.
pub fn words(text: &str) -> Vec<String> {
    text.split([',', '\n'])
        .map(str::trim)
        .filter(|word| !word.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The regular expressions of `text`, one per line: spaces in them count.
pub fn patterns(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(str::to_owned)
        .collect()
}

/// Something AutoMod did in a server, as the provider reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AutoModExecution {
    pub server: ServerId,
    pub rule: AutoModRuleId,
    pub action: AutoModAction,
    /// Whose message or profile matched.
    pub user: UserId,
    pub channel: Option<ChannelId>,
    /// The word or text that matched, when the provider says.
    pub matched: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(trigger: AutoModTrigger, actions: Vec<AutoModAction>) -> AutoModRule {
        AutoModRule {
            trigger,
            actions,
            ..AutoModRule::fresh(AutoModKind::Keywords)
        }
    }

    fn keywords(keywords: &[&str], patterns: &[&str]) -> AutoModTrigger {
        AutoModTrigger::Keywords(WordFilter {
            keywords: keywords.iter().map(|word| word.to_string()).collect(),
            patterns: patterns.iter().map(|pattern| pattern.to_string()).collect(),
            allowed: Vec::new(),
        })
    }

    #[test]
    fn the_editor_splits_words_by_commas_and_patterns_by_lines() {
        assert_eq!(
            words(" spoiler*, final ,\n*gratis*,,"),
            ["spoiler*", "final", "*gratis*"]
        );
        assert_eq!(patterns("a b\n\n  \n^x$\r\n"), ["a b", "^x$"]);
    }

    #[test]
    fn a_new_rule_of_each_kind_is_almost_ready() {
        for kind in AutoModKind::ALL {
            let fresh = AutoModRule::fresh(kind);
            assert_eq!(fresh.trigger.kind(), Some(kind));
            assert!(fresh.editable());
            let ready = match kind {
                AutoModKind::Keywords | AutoModKind::Profile | AutoModKind::Presets => {
                    fresh.problem().is_some()
                }
                AutoModKind::Spam | AutoModKind::Mentions => fresh.problem().is_none(),
            };
            assert!(ready, "{kind:?}: {:?}", fresh.problem());
        }
    }

    #[test]
    fn words_and_patterns_keep_to_the_service_limits() {
        let block = || vec![AutoModAction::Block { notice: None }];
        assert!(rule(keywords(&["hola", "*gg*"], &[r"\bnitro\b"]), block())
            .problem()
            .is_none());
        assert!(rule(keywords(&[], &[]), block()).problem().is_some());
        let long = "x".repeat(MAX_KEYWORD_CHARS + 1);
        assert!(rule(keywords(&[&long], &[]), block())
            .problem()
            .unwrap()
            .contains(&long));
        assert!(rule(keywords(&[], &["(abierto"]), block())
            .problem()
            .unwrap()
            .contains("(abierto"));
        let many: Vec<&str> = vec!["a"; MAX_PATTERNS + 1];
        assert!(rule(keywords(&[], &many), block()).problem().is_some());
        let presets = AutoModTrigger::Presets {
            lists: Vec::new(),
            allowed: Vec::new(),
        };
        assert!(rule(presets, block()).problem().is_some());
        let mentions = |limit| AutoModTrigger::Mentions {
            limit,
            raids: false,
        };
        assert!(rule(mentions(0), block()).problem().is_some());
        assert!(rule(mentions(MAX_MENTIONS + 1), block())
            .problem()
            .is_some());
        assert!(rule(mentions(MAX_MENTIONS), block()).problem().is_none());
    }

    #[test]
    fn each_kind_takes_only_its_actions() {
        let words = || keywords(&["hola"], &[]);
        assert!(rule(words(), Vec::new()).problem().is_some());
        assert!(rule(words(), vec![AutoModAction::Quarantine])
            .problem()
            .is_some());
        assert!(rule(
            words(),
            vec![AutoModAction::Alert {
                channel: ChannelId(0)
            }]
        )
        .problem()
        .is_some());
        assert!(rule(words(), vec![AutoModAction::Timeout { seconds: 60 }])
            .problem()
            .is_none());
        assert!(rule(
            words(),
            vec![AutoModAction::Timeout {
                seconds: MAX_TIMEOUT_SECS + 1
            }]
        )
        .problem()
        .is_some());
        assert!(rule(
            AutoModTrigger::Spam,
            vec![AutoModAction::Timeout { seconds: 60 }]
        )
        .problem()
        .is_some());
        let profile = AutoModTrigger::Profile(WordFilter {
            keywords: vec!["hola".into()],
            ..WordFilter::default()
        });
        assert!(
            rule(profile.clone(), vec![AutoModAction::Block { notice: None }])
                .problem()
                .is_some()
        );
        assert!(rule(profile, vec![AutoModAction::Quarantine])
            .problem()
            .is_none());
        let long = Some("x".repeat(MAX_NOTICE + 1));
        assert!(rule(words(), vec![AutoModAction::Block { notice: long }])
            .problem()
            .is_some());
    }

    #[test]
    fn what_rune_does_not_know_is_never_edited() {
        let unknown = rule(
            AutoModTrigger::Other(9),
            vec![AutoModAction::Block { notice: None }],
        );
        assert!(!unknown.editable() && unknown.problem().is_some());
        let action = rule(keywords(&["hola"], &[]), vec![AutoModAction::Other(7)]);
        assert!(!action.editable() && action.problem().is_some());
    }
}
