//! A server's rules and onboarding, read, answered and edited (see
//! `crate::onboarding` for the routes and where they come from). Each write
//! goes once; a challenge on one is offered like any other write's.
//! Fixtures only.

use reqwest::Method;
use rune_provider::{Command, Event};

use super::{Http, RestError};
use crate::{onboarding as wire_onboarding, wire, SessionSecret};

impl Http {
    pub(super) async fn onboarding(
        &self,
        secret: &SessionSecret,
        command: &Command,
    ) -> Result<Vec<Event>, RestError> {
        let invalid = |mutating: bool| move |reason| RestError::response(reason, mutating);
        match command {
            Command::LoadServerRules { request, server } => {
                let value = self
                    .request(
                        secret,
                        Method::GET,
                        &format!("/guilds/{server}/member-verification?with_guild=false"),
                        None,
                        wire_onboarding::MAX_FORM,
                    )
                    .await?;
                Ok(vec![Event::ServerRules {
                    request: *request,
                    server: *server,
                    rules: wire_onboarding::rules(&value).map_err(invalid(false))?,
                }])
            }
            Command::AnswerServerRules {
                request,
                server,
                rules,
                answers,
            } => {
                // `onboarding::allowed` already refused rules it cannot give back.
                let body = wire_onboarding::join_request(rules, answers)
                    .ok_or_else(|| RestError::rejected(wire_onboarding::INCOMPLETE))?;
                let value = self
                    .request_answer(
                        secret,
                        Method::PUT,
                        &format!("/guilds/{server}/requests/@me"),
                        Some(body),
                        wire_onboarding::MAX_FORM,
                        true,
                    )
                    .await?;
                Ok(vec![Event::RulesAnswered {
                    request: *request,
                    server: *server,
                    outcome: wire_onboarding::outcome(&value),
                }])
            }
            Command::LoadOnboarding { request, server } => {
                let value = self
                    .request(
                        secret,
                        Method::GET,
                        &format!("/guilds/{server}/onboarding"),
                        None,
                        wire_onboarding::MAX_FORM,
                    )
                    .await?;
                Ok(vec![Event::OnboardingLoaded {
                    request: *request,
                    server: *server,
                    onboarding: Some(wire_onboarding::onboarding(&value).map_err(invalid(false))?),
                }])
            }
            Command::AnswerOnboarding {
                request,
                server,
                onboarding,
                chosen,
            } => {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_millis() as u64);
                let value = self
                    .request_answer(
                        secret,
                        Method::POST,
                        &format!("/guilds/{server}/onboarding-responses"),
                        Some(wire_onboarding::responses(onboarding, chosen, now)),
                        wire_onboarding::MAX_FORM,
                        true,
                    )
                    .await?;
                // The roles it gives come as the member's update.
                Ok(vec![Event::OnboardingAnswered {
                    request: *request,
                    server: *server,
                    chosen: wire_onboarding::chosen(&value).unwrap_or_else(|| chosen.clone()),
                }])
            }
            Command::SaveOnboarding {
                request,
                server,
                onboarding,
            } => {
                let value = self
                    .request_answer(
                        secret,
                        Method::PUT,
                        &format!("/guilds/{server}/onboarding"),
                        Some(wire_onboarding::edit(onboarding)),
                        wire_onboarding::MAX_FORM,
                        true,
                    )
                    .await?;
                // A 204 saved what was sent; a body RUNE cannot read leaves
                // it unknown what Discord kept. An edit never changes what
                // the account chose.
                let mut saved = if value.is_null() {
                    onboarding.clone()
                } else {
                    wire_onboarding::onboarding(&value).map_err(invalid(true))?
                };
                saved.chosen = onboarding.chosen.clone();
                Ok(vec![Event::OnboardingLoaded {
                    request: *request,
                    server: *server,
                    onboarding: Some(saved),
                }])
            }
            _ => Err(RestError::response(wire::invalid(), false)),
        }
    }
}
