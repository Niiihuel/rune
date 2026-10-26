//! AutoMod in the simulation (automod): the owned server has three rules
//! the account edits, and «Taller de Rust» one that blocks invitations to
//! other servers, which the account, a plain member there, runs into.
//! Messages are matched by their words as the service documents it; the
//! regular expressions, the service's lists, spam and mentions are kept
//! and shown, never matched. Whoever may manage a server is exempt, as in
//! the service. A reply someone writes in a server the account manages
//! may set a rule off: then it is not posted, and the account is told what
//! AutoMod did. Nothing else changes for anyone.

use std::collections::HashMap;

use rune_i18n::{t, tr};
use rune_model::{
    permissions, AuditAction, AutoModAction, AutoModExecution, AutoModRule, AutoModRuleId,
    AutoModTrigger, ChannelId, Permissions, ServerId, UserId, WordFilter,
};

use super::{World, ME};
use crate::{Command, Event, RequestId};

type Outcome = Result<Vec<Event>, String>;

/// The server the account owns, and the first one, where it is a member.
const OWNED: ServerId = ServerId(2);
const WORKSHOP: ServerId = ServerId(1);

/// The rules of each server.
#[derive(Debug, Default)]
pub(super) struct AutoModSim {
    rules: HashMap<ServerId, Vec<AutoModRule>>,
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

/// The first of `keywords` in `content`, as the service matches them: case
/// aside, a `*` at either end lets the word go on that way, and without
/// one it is a whole word. A word among `allowed` never counts.
fn matched(keywords: &[String], allowed: &[String], content: &str) -> Option<String> {
    let text = content.to_lowercase();
    let in_word = |c: char| c.is_alphanumeric();
    for keyword in keywords {
        let keyword = keyword.to_lowercase();
        let core = keyword.trim_matches('*');
        if core.is_empty() {
            continue;
        }
        for (at, _) in text.match_indices(core) {
            let end = at + core.len();
            let before = text[..at].chars().next_back();
            let after = text[end..].chars().next();
            if (!keyword.starts_with('*') && before.is_some_and(in_word))
                || (!keyword.ends_with('*') && after.is_some_and(in_word))
            {
                continue;
            }
            // The whole word around it, to compare with the exceptions.
            let start = text[..at]
                .char_indices()
                .rev()
                .take_while(|(_, c)| in_word(*c))
                .last()
                .map_or(at, |(index, _)| index);
            let stop = text[end..]
                .char_indices()
                .find(|(_, c)| !in_word(*c))
                .map_or(text.len(), |(index, _)| end + index);
            let word = &text[start..stop.max(end)];
            if allowed.iter().any(|allowed| allowed.to_lowercase() == word) {
                continue;
            }
            return Some(text[at..end].to_owned());
        }
    }
    None
}

impl World {
    /// Give the owned server its rules, and the first server its own.
    /// Call once their channels exist.
    pub(super) fn add_automod(&mut self) {
        let channel = |world: &World, name: &str| {
            world
                .channels
                .iter()
                .find(|found| found.server == Some(OWNED) && found.name == name)
                .map(|found| found.id)
        };
        let (Some(core), Some(components)) =
            (channel(self, "limn-core"), channel(self, "componentes"))
        else {
            return;
        };
        let rule = |id, name: &str, trigger, actions| AutoModRule {
            id: Some(AutoModRuleId(id)),
            name: name.into(),
            trigger,
            actions,
            enabled: true,
            exempt_roles: Vec::new(),
            exempt_channels: Vec::new(),
            creator: Some(ME),
        };
        self.automod.rules.insert(
            OWNED,
            vec![
                AutoModRule {
                    exempt_channels: vec![components],
                    ..rule(
                        900_001,
                        "Sin spoilers",
                        AutoModTrigger::Keywords(WordFilter {
                            keywords: strings(&["spoiler*", "*final de temporada*"]),
                            patterns: strings(&[r"\bfiltr(ado|ación)\b"]),
                            allowed: strings(&["spoilers-ok"]),
                        }),
                        vec![
                            AutoModAction::Block {
                                notice: Some("Guarda los spoilers para #componentes.".into()),
                            },
                            AutoModAction::Alert { channel: core },
                        ],
                    )
                },
                rule(
                    900_002,
                    "Spam de menciones",
                    AutoModTrigger::Mentions {
                        limit: 8,
                        raids: true,
                    },
                    vec![
                        AutoModAction::Block { notice: None },
                        AutoModAction::Timeout { seconds: 600 },
                    ],
                ),
                AutoModRule {
                    enabled: false,
                    ..rule(
                        900_003,
                        "Lo que parece spam",
                        AutoModTrigger::Spam,
                        vec![AutoModAction::Block { notice: None }],
                    )
                },
            ],
        );
        let owner = self
            .servers
            .iter()
            .find(|server| server.id == WORKSHOP)
            .and_then(|server| server.owner);
        self.automod.rules.insert(
            WORKSHOP,
            vec![AutoModRule {
                creator: owner,
                ..rule(
                    900_010,
                    "Sin invitaciones",
                    AutoModTrigger::Keywords(WordFilter {
                        keywords: strings(&["*discord.gg/*", "*discord.com/invite*"]),
                        ..WordFilter::default()
                    }),
                    vec![AutoModAction::Block {
                        notice: Some("No compartas invitaciones a otros servidores.".into()),
                    }],
                )
            }],
        );
    }

    /// The AutoMod commands; `None` for any other.
    pub(super) fn automod_command(&mut self, command: &Command) -> Option<Outcome> {
        Some(match command {
            Command::LoadAutoModRules { request, server } => self.load_automod(*request, *server),
            Command::SaveAutoModRule {
                request,
                server,
                rule,
            } => self.save_automod(*request, *server, rule.clone()),
            Command::SetAutoModRuleEnabled {
                server,
                rule,
                enabled,
            } => self.change_automod(*server, *rule, Some(*enabled)),
            Command::DeleteAutoModRule { server, rule } => {
                self.change_automod(*server, *rule, None)
            }
            _ => return None,
        })
    }

    fn load_automod(&mut self, request: RequestId, server: ServerId) -> Outcome {
        self.require(server, Permissions::MANAGE_SERVER)?;
        Ok(vec![Event::AutoModRules {
            request,
            server,
            rules: self.automod.rules.get(&server).cloned().unwrap_or_default(),
        }])
    }

    fn save_automod(
        &mut self,
        request: RequestId,
        server: ServerId,
        mut rule: AutoModRule,
    ) -> Outcome {
        self.require(server, Permissions::MANAGE_SERVER)?;
        if let Some(problem) = rule.problem() {
            return Err(problem);
        }
        let kind = rule.trigger.kind();
        for action in &rule.actions {
            match action {
                AutoModAction::Alert { channel }
                    if self
                        .channel(*channel)
                        .is_none_or(|found| found.server != Some(server)) =>
                {
                    return Err(t("el canal de la alerta no es de este servidor").into());
                }
                AutoModAction::Timeout { .. } => {
                    self.require(server, Permissions::TIMEOUT_MEMBERS)?;
                }
                _ => {}
            }
        }
        rule.name = rule.name.trim().to_owned();
        let list = self.automod.rules.entry(server).or_default();
        let what = match rule.id {
            None => {
                let same = list
                    .iter()
                    .filter(|known| known.trigger.kind() == kind)
                    .count();
                if kind.is_some_and(|kind| same >= kind.max_rules()) {
                    return Err(t("el servidor ya tiene todas las reglas de ese tipo").into());
                }
                self.next_id += 1;
                rule.id = Some(AutoModRuleId(self.next_id));
                rule.creator = Some(ME);
                let list = self.automod.rules.entry(server).or_default();
                list.push(rule.clone());
                tr!("creó la regla de AutoMod {name}", name = rule.name)
            }
            Some(id) => {
                let found = list
                    .iter_mut()
                    .find(|known| known.id == Some(id))
                    .ok_or(t("esa regla ya no existe"))?;
                if found.trigger.kind() != kind {
                    return Err(t("el tipo de una regla no cambia").into());
                }
                rule.creator = found.creator;
                *found = rule.clone();
                tr!("actualizó la regla de AutoMod {name}", name = rule.name)
            }
        };
        self.log_created(server, AuditAction::Other { what });
        Ok(vec![Event::AutoModRuleChanged {
            server,
            rule,
            request: Some(request),
        }])
    }

    /// Turn a rule on or off, or with `None` delete it.
    fn change_automod(
        &mut self,
        server: ServerId,
        id: AutoModRuleId,
        enabled: Option<bool>,
    ) -> Outcome {
        self.require(server, Permissions::MANAGE_SERVER)?;
        let list = self.automod.rules.entry(server).or_default();
        let index = list
            .iter()
            .position(|known| known.id == Some(id))
            .ok_or(t("esa regla ya no existe"))?;
        let (what, event) = match enabled {
            Some(enabled) => {
                let found = &mut list[index];
                found.enabled = enabled;
                (
                    tr!("actualizó la regla de AutoMod {name}", name = found.name),
                    Event::AutoModRuleChanged {
                        server,
                        rule: found.clone(),
                        request: None,
                    },
                )
            }
            None => {
                let gone = list.remove(index);
                (
                    tr!("eliminó la regla de AutoMod {name}", name = gone.name),
                    Event::AutoModRuleRemoved { server, rule: id },
                )
            }
        };
        self.log_created(server, AuditAction::Other { what });
        Ok(vec![event])
    }

    /// The rule `author`'s `content` in `channel` sets off, and what of it
    /// matched: the first one on, of words, that does not leave them alone.
    fn automod_hit(
        &self,
        channel: ChannelId,
        author: UserId,
        content: &str,
    ) -> Option<(ServerId, &AutoModRule, String)> {
        let channel = self.channel(channel)?;
        let server = self.server(channel.server?).ok()?;
        let member = self.member(server.id, author)?;
        if permissions::in_server(server, member).contains(Permissions::MANAGE_SERVER) {
            return None;
        }
        let exempt = |rule: &AutoModRule| {
            member
                .roles
                .iter()
                .any(|role| rule.exempt_roles.contains(role))
                || rule.exempt_channels.contains(&channel.id)
                || channel
                    .parent
                    .is_some_and(|parent| rule.exempt_channels.contains(&parent))
        };
        self.automod
            .rules
            .get(&server.id)?
            .iter()
            .filter(|rule| rule.enabled && !exempt(rule))
            .find_map(|rule| match &rule.trigger {
                AutoModTrigger::Keywords(filter) => {
                    matched(&filter.keywords, &filter.allowed, content)
                        .map(|word| (server.id, rule, word))
                }
                _ => None,
            })
    }

    /// What the account is told when a rule blocks what it sends: `Some`
    /// with the rule's message, if it has one, when one does.
    pub(super) fn automod_blocks(
        &self,
        channel: ChannelId,
        author: UserId,
        content: &str,
    ) -> Option<Option<String>> {
        let (_, rule, _) = self.automod_hit(channel, author, content)?;
        rule.actions.iter().find_map(|action| match action {
            AutoModAction::Block { notice } => Some(notice.clone()),
            _ => None,
        })
    }

    /// What AutoMod does to a message someone else writes: `None` when no
    /// rule cares; else whether it is still posted, and what the account,
    /// when it may manage the server, is told AutoMod did.
    pub(super) fn automod_acts(
        &self,
        channel: ChannelId,
        author: UserId,
        content: &str,
    ) -> Option<(bool, Vec<Event>)> {
        let (server, rule, word) = self.automod_hit(channel, author, content)?;
        let posted = !rule
            .actions
            .iter()
            .any(|action| matches!(action, AutoModAction::Block { .. }));
        let told = self
            .mine(server)
            .is_ok_and(|mine| mine.contains(Permissions::MANAGE_SERVER));
        let events = rule
            .actions
            .iter()
            .filter(|_| told)
            .map(|action| {
                Event::AutoModExecuted(AutoModExecution {
                    server,
                    rule: rule.id.unwrap_or_default(),
                    action: action.clone(),
                    user: author,
                    channel: Some(channel),
                    matched: Some(word.clone()),
                })
            })
            .collect();
        Some((posted, events))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mock::MockConfig;
    use rune_model::{AutoModKind, Timestamp};

    fn world() -> World {
        World::build(&MockConfig::default())
    }

    fn named(world: &World, server: ServerId, name: &str) -> ChannelId {
        world
            .channels
            .iter()
            .find(|found| found.server == Some(server) && found.name == name)
            .map(|found| found.id)
            .expect("the channel exists")
    }

    fn someone(world: &World, server: ServerId) -> UserId {
        world
            .members
            .iter()
            .find(|member| {
                member.server == server
                    && member.user != ME
                    && member.roles.is_empty()
                    && world
                        .server(server)
                        .is_ok_and(|found| found.owner != Some(member.user))
            })
            .map(|member| member.user)
            .expect("a plain member")
    }

    #[test]
    fn keywords_match_as_the_service_documents() {
        let words = strings(&["spoiler*", "*gg/*", "gato", "*final de temporada*"]);
        let none: Vec<String> = Vec::new();
        assert_eq!(
            matched(&words, &none, "Ojo: SPOILERS adelante"),
            Some("spoiler".into())
        );
        assert_eq!(
            matched(&words, &none, "unirse a discord.gg/rust"),
            Some("gg/".into())
        );
        assert_eq!(
            matched(&words, &none, "el gato duerme"),
            Some("gato".into())
        );
        assert_eq!(matched(&words, &none, "los gatos duermen"), None);
        assert_eq!(matched(&words, &none, "nospoiler"), None);
        assert!(matched(&words, &none, "¡el FINAL DE TEMPORADA!").is_some());
        let allowed = strings(&["spoilers-ok", "spoilerfree"]);
        assert_eq!(matched(&words, &allowed, "esto es spoilerfree"), None);
    }

    #[test]
    fn the_account_runs_into_a_rule_where_it_is_a_plain_member() {
        let world = world();
        let general = named(&world, WORKSHOP, "general");
        assert_eq!(
            world.automod_blocks(general, ME, "Vengan a discord.gg/rustaceos"),
            Some(Some("No compartas invitaciones a otros servidores.".into()))
        );
        assert_eq!(world.automod_blocks(general, ME, "hola a todos"), None);
        // Whoever manages the server is exempt: the account owns the lab.
        let core = named(&world, OWNED, "limn-core");
        assert_eq!(world.automod_blocks(core, ME, "spoiler del capítulo"), None);
    }

    #[test]
    fn someone_elses_reply_is_held_and_the_owner_told() {
        let mut world = world();
        let core = named(&world, OWNED, "limn-core");
        let components = named(&world, OWNED, "componentes");
        let author = someone(&world, OWNED);
        let (posted, events) = world
            .automod_acts(core, author, "¡Spoiler: gana el cangrejo!")
            .expect("the rule matches");
        assert!(!posted, "a rule that blocks keeps it from being posted");
        assert_eq!(events.len(), 2, "one per action: block and alert");
        assert!(matches!(&events[0], Event::AutoModExecuted(done)
            if done.user == author && done.matched.as_deref() == Some("spoiler")
                && done.rule == AutoModRuleId(900_001)));
        // An exempt channel, and a rule turned off, leave it alone.
        assert!(world.automod_acts(components, author, "spoiler").is_none());
        world
            .change_automod(OWNED, AutoModRuleId(900_001), Some(false))
            .unwrap();
        assert!(world.automod_acts(core, author, "spoiler").is_none());
    }

    #[test]
    fn rules_are_listed_made_changed_and_deleted_by_whoever_manages() {
        let mut world = world();
        world.now = Timestamp(1);
        let Event::AutoModRules { rules, .. } = &world.load_automod(1, OWNED).unwrap()[0] else {
            panic!("the rules");
        };
        assert_eq!(rules.len(), 3);
        assert!(world.load_automod(2, WORKSHOP).is_err(), "a plain member");
        let mut fresh = AutoModRule::fresh(AutoModKind::Keywords);
        assert!(
            world.save_automod(3, OWNED, fresh.clone()).is_err(),
            "no words"
        );
        fresh.trigger = AutoModTrigger::Keywords(WordFilter {
            keywords: strings(&["jaja*"]),
            ..WordFilter::default()
        });
        let saved = world.save_automod(4, OWNED, fresh.clone()).unwrap();
        let Event::AutoModRuleChanged {
            rule,
            request: Some(4),
            ..
        } = &saved[0]
        else {
            panic!("the saved rule");
        };
        let id = rule.id.expect("an id");
        assert_eq!(rule.creator, Some(ME));
        // Only one rule of mentions.
        let mentions = AutoModRule::fresh(AutoModKind::Mentions);
        assert!(world.save_automod(5, OWNED, mentions).is_err());
        let mut renamed = rule.clone();
        renamed.name = "  Risas  ".into();
        world.save_automod(6, OWNED, renamed).unwrap();
        assert_eq!(world.automod.rules[&OWNED][3].name, "Risas");
        let mut kind = rule.clone();
        kind.trigger = AutoModTrigger::Spam;
        assert!(world.save_automod(7, OWNED, kind).is_err(), "kinds stay");
        assert!(matches!(
            world.change_automod(OWNED, id, None).unwrap()[0],
            Event::AutoModRuleRemoved { rule, .. } if rule == id
        ));
        assert!(world.change_automod(OWNED, id, Some(true)).is_err());
        assert!(world.audit[&OWNED].iter().any(|entry| matches!(
            &entry.action,
            AuditAction::Other { what } if what.contains("Risas")
        )));
    }
}
