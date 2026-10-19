//! AutoMod (automod): the rules of each server as the provider sent them,
//! and what AutoMod did lately where the account moderates. Nothing shows
//! ahead: a rule changes once the provider says so.

use std::collections::{HashMap, VecDeque};

use rune_model::{AutoModExecution, AutoModRule, ServerId};
use rune_provider::Event;

use crate::{Applied, Store};

/// What AutoMod did in a server that is kept, the newest first.
pub const RECENT_EXECUTIONS: usize = 20;

/// What the provider sent of servers' AutoMod this session.
#[derive(Debug, Default)]
pub(crate) struct AutoModState {
    rules: HashMap<ServerId, Vec<AutoModRule>>,
    executed: HashMap<ServerId, VecDeque<AutoModExecution>>,
}

impl Store {
    /// `server`'s AutoMod rules: `None` until the provider sent them.
    pub fn automod_rules(&self, server: ServerId) -> Option<&[AutoModRule]> {
        self.automod.rules.get(&server).map(Vec::as_slice)
    }

    /// What AutoMod did in `server` this session, the newest first.
    pub fn automod_executions(&self, server: ServerId) -> impl Iterator<Item = &AutoModExecution> {
        self.automod.executed.get(&server).into_iter().flatten()
    }

    /// A new session reads them again.
    pub(crate) fn automod_ready(&mut self) {
        self.automod = AutoModState::default();
    }

    pub(crate) fn apply_automod(&mut self, event: Event) -> Applied {
        match event {
            Event::AutoModRules {
                request,
                server,
                rules,
            } => {
                if self.server(server).is_none() {
                    return Applied::Nothing;
                }
                // Once per rule, as listed.
                let mut unique: Vec<AutoModRule> = Vec::with_capacity(rules.len());
                for rule in rules {
                    if rule.id.is_some() && unique.iter().all(|known| known.id != rule.id) {
                        unique.push(rule);
                    }
                }
                self.automod.rules.insert(server, unique);
                Applied::AutoModRules { request, server }
            }
            Event::AutoModRuleChanged {
                server,
                rule,
                request,
            } => {
                if self.server(server).is_none() || rule.id.is_none() {
                    return Applied::Nothing;
                }
                // Only a list already read takes it: one never read is read whole.
                if let Some(list) = self.automod.rules.get_mut(&server) {
                    match list.iter_mut().find(|known| known.id == rule.id) {
                        Some(known) => *known = rule,
                        None => list.push(rule),
                    }
                }
                Applied::AutoMod { server, request }
            }
            Event::AutoModRuleRemoved { server, rule } => {
                if let Some(list) = self.automod.rules.get_mut(&server) {
                    list.retain(|known| known.id != Some(rule));
                }
                Applied::AutoMod {
                    server,
                    request: None,
                }
            }
            Event::AutoModExecuted(execution) => {
                let server = execution.server;
                if self.server(server).is_none() {
                    return Applied::Nothing;
                }
                let recent = self.automod.executed.entry(server).or_default();
                recent.push_front(execution);
                recent.truncate(RECENT_EXECUTIONS);
                Applied::AutoMod {
                    server,
                    request: None,
                }
            }
            _ => Applied::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use rune_model::{AutoModAction, AutoModKind, AutoModRuleId, ServerId, UserId};
    use rune_provider::mock::MockProvider;
    use rune_provider::Event;

    use super::RECENT_EXECUTIONS;
    use crate::{Applied, Store};

    const LAB: ServerId = ServerId(2);

    fn ready() -> Event {
        Event::Ready(Box::new(MockProvider::new(Default::default()).snapshot()))
    }

    fn store() -> Store {
        let mut store = Store::new();
        store.apply(ready(), Instant::now());
        store
    }

    fn rule(id: u64, name: &str) -> rune_model::AutoModRule {
        rune_model::AutoModRule {
            id: Some(AutoModRuleId(id)),
            name: name.into(),
            ..rune_model::AutoModRule::fresh(AutoModKind::Spam)
        }
    }

    #[test]
    fn rules_arrive_whole_and_then_change_one_by_one() {
        let mut store = store();
        let now = Instant::now();
        assert!(store.automod_rules(LAB).is_none());
        // A change before the list was read is not a list.
        store.apply(
            Event::AutoModRuleChanged {
                server: LAB,
                rule: rule(9, "Antes"),
                request: None,
            },
            now,
        );
        assert!(store.automod_rules(LAB).is_none());
        let applied = store.apply(
            Event::AutoModRules {
                request: 4,
                server: LAB,
                rules: vec![rule(1, "Uno"), rule(2, "Dos"), rule(1, "Uno otra vez")],
            },
            now,
        );
        assert_eq!(
            applied,
            Applied::AutoModRules {
                request: 4,
                server: LAB
            }
        );
        assert_eq!(
            store.automod_rules(LAB).unwrap(),
            [rule(1, "Uno"), rule(2, "Dos")]
        );
        let applied = store.apply(
            Event::AutoModRuleChanged {
                server: LAB,
                rule: rule(2, "Dos cambiada"),
                request: Some(7),
            },
            now,
        );
        assert_eq!(
            applied,
            Applied::AutoMod {
                server: LAB,
                request: Some(7)
            }
        );
        store.apply(
            Event::AutoModRuleChanged {
                server: LAB,
                rule: rule(3, "Tres"),
                request: None,
            },
            now,
        );
        store.apply(
            Event::AutoModRuleRemoved {
                server: LAB,
                rule: AutoModRuleId(1),
            },
            now,
        );
        let names: Vec<&str> = store
            .automod_rules(LAB)
            .unwrap()
            .iter()
            .map(|rule| rule.name.as_str())
            .collect();
        assert_eq!(names, ["Dos cambiada", "Tres"]);
        // An unknown server's are not kept.
        let other = store.apply(
            Event::AutoModRules {
                request: 5,
                server: ServerId(77),
                rules: vec![rule(1, "Uno")],
            },
            now,
        );
        assert_eq!(other, Applied::Nothing);
    }

    #[test]
    fn what_automod_did_is_kept_newest_first_and_bounded() {
        let mut store = store();
        let now = Instant::now();
        for user in 0..RECENT_EXECUTIONS as u64 + 3 {
            store.apply(
                Event::AutoModExecuted(rune_model::AutoModExecution {
                    server: LAB,
                    rule: AutoModRuleId(1),
                    action: AutoModAction::Block { notice: None },
                    user: UserId(user),
                    channel: None,
                    matched: None,
                }),
                now,
            );
        }
        let users: Vec<u64> = store
            .automod_executions(LAB)
            .map(|done| done.user.0)
            .collect();
        assert_eq!(users.len(), RECENT_EXECUTIONS);
        assert_eq!(users[0], RECENT_EXECUTIONS as u64 + 2);
        // A new session forgets them, and the rules.
        store.apply(
            Event::AutoModRules {
                request: 1,
                server: LAB,
                rules: vec![rule(1, "Uno")],
            },
            now,
        );
        store.apply(ready(), now);
        assert!(store.automod_rules(LAB).is_none());
        assert_eq!(store.automod_executions(LAB).count(), 0);
    }
}
