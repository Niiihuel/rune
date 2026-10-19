//! Rules and onboarding (2026-10-18): what the account still owes a server
//! it is in, and what the provider sent of each server's rules and
//! onboarding. Nothing is shown ahead: an answer counts once the provider
//! says so, and then the membership says it too.

use std::collections::HashMap;

use rune_i18n::tr;
use rune_model::{JoinOutcome, Onboarding, ServerId, ServerRules};
use rune_provider::Event;

use crate::{Applied, Permit, Store};

/// What the provider sent of servers' rules and onboarding this session.
#[derive(Debug, Default)]
pub(crate) struct JoinState {
    /// By server; `None` inside when it asks nothing.
    rules: HashMap<ServerId, Option<ServerRules>>,
    onboardings: HashMap<ServerId, Option<Onboarding>>,
    /// The rules answered and what came of it, until the membership says
    /// it; kept across sessions of the same account.
    outcomes: HashMap<ServerId, JoinOutcome>,
}

/// What the account still owes a server.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct JoinSteps {
    /// Its rules, before it may take part.
    pub rules: bool,
    /// Its onboarding's questions: unanswered, only what they give is missing.
    pub onboarding: bool,
}

impl JoinSteps {
    pub fn any(self) -> bool {
        self.rules || self.onboarding
    }
}

impl Store {
    /// What the account still owes `server`, as far as the provider can
    /// ask it.
    pub fn join_steps(&self, server: ServerId) -> JoinSteps {
        let capabilities = self
            .provider
            .as_ref()
            .map(|info| info.capabilities)
            .unwrap_or_default();
        let (Some(found), Some(member)) = (
            self.server(server),
            self.me.and_then(|me| self.member_of(server, me)),
        ) else {
            return JoinSteps::default();
        };
        JoinSteps {
            rules: capabilities.server_rules && member.restrictions.pending,
            onboarding: capabilities.onboarding
                && found.options.onboarding
                && !member.restrictions.onboarded
                // A guest is let into one voice channel, never onboarded.
                && !member.restrictions.guest
                // Once read, one that asks nothing is owed nothing.
                && self
                    .join
                    .onboardings
                    .get(&server)
                    .is_none_or(|known| known.as_ref().is_some_and(Onboarding::asks)),
        }
    }

    /// `server`'s rules as the provider sent them: `None` until asked,
    /// `Some(None)` when it asks nothing.
    pub fn server_rules(&self, server: ServerId) -> Option<Option<&ServerRules>> {
        self.join.rules.get(&server).map(Option::as_ref)
    }

    /// `server`'s onboarding as the provider sent it, as [`Self::server_rules`].
    pub fn onboarding(&self, server: ServerId) -> Option<Option<&Onboarding>> {
        self.join.onboardings.get(&server).map(Option::as_ref)
    }

    /// What came of the rules the account answered in `server`, while its
    /// membership still waits.
    pub fn rules_outcome(&self, server: ServerId) -> Option<JoinOutcome> {
        self.join.outcomes.get(&server).copied().filter(|_| {
            self.me
                .and_then(|me| self.member_of(server, me))
                .is_some_and(|member| member.restrictions.pending)
        })
    }

    /// Why a pending membership keeps the account out of `name` once it
    /// answered the rules: a moderator reviews them, or refused them.
    /// `None` while the rules wait.
    pub(crate) fn answered_reason(&self, server: ServerId, name: &str) -> Option<Permit> {
        let reason = match self.rules_outcome(server)? {
            JoinOutcome::InReview => tr!(
                "La moderación de {server} está revisando tus respuestas: vas a poder participar cuando las apruebe",
                server = name
            ),
            JoinOutcome::Rejected => {
                tr!("La moderación de {server} rechazó tu solicitud", server = name)
            }
            JoinOutcome::Approved => return None,
        };
        Some(Permit::Denied(reason))
    }

    /// A new session reads them again; what came of answered rules stays
    /// for the same account, so rules a moderator reviews are not offered
    /// again on reconnecting. shortcut: forgotten when RUNE closes; keep it
    /// with the account's data if a provider cannot say it again.
    pub(crate) fn join_ready(&mut self, same_account: bool) {
        let outcomes = std::mem::take(&mut self.join.outcomes);
        self.join = JoinState::default();
        if same_account {
            self.join.outcomes = outcomes;
        }
    }

    pub(crate) fn apply_join(&mut self, event: Event) -> Applied {
        match event {
            Event::ServerRules {
                request,
                server,
                rules,
            } => {
                if self.server(server).is_none() {
                    return Applied::Nothing;
                }
                self.join
                    .rules
                    .insert(server, rules.filter(ServerRules::bounded));
                Applied::ServerRules { request, server }
            }
            Event::RulesAnswered {
                request,
                server,
                outcome,
            } => {
                // Approved, the account takes part now, as the provider says.
                if outcome == JoinOutcome::Approved {
                    self.set_mine(server, |member| member.restrictions.pending = false);
                }
                self.join.outcomes.insert(server, outcome);
                Applied::RulesAnswered {
                    request,
                    server,
                    outcome,
                }
            }
            Event::OnboardingLoaded {
                request,
                server,
                onboarding,
            } => {
                if self.server(server).is_none() {
                    return Applied::Nothing;
                }
                self.join.onboardings.insert(server, onboarding);
                Applied::Onboarding { request, server }
            }
            Event::OnboardingAnswered {
                request,
                server,
                chosen,
            } => {
                self.set_mine(server, |member| member.restrictions.onboarded = true);
                if let Some(Some(onboarding)) = self.join.onboardings.get_mut(&server) {
                    onboarding.chosen = chosen;
                }
                Applied::OnboardingAnswered { request, server }
            }
            _ => Applied::Nothing,
        }
    }

    /// Change the account's membership of `server`, if known.
    fn set_mine(&mut self, server: ServerId, change: impl FnOnce(&mut rune_model::Member)) {
        let Some(me) = self.me else {
            return;
        };
        if let Some(index) = self.member_index.get(&(server, me)).copied() {
            if let Some(member) = self
                .members
                .get_mut(&server)
                .and_then(|members| members.get_mut(index))
            {
                change(member);
            }
        }
    }
}
