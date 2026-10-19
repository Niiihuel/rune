//! A server's AutoMod rules, read, made, changed and deleted (see
//! `crate::automod` for the routes and where they come from). Each write
//! goes once; a challenge on one is offered like any other write's.
//! Fixtures only.

use reqwest::Method;
use rune_provider::{Command, Event};

use super::{Http, RestError};
use crate::{automod as wire_automod, wire, SessionSecret};

impl Http {
    pub(super) async fn automod(
        &self,
        secret: &SessionSecret,
        command: &Command,
    ) -> Result<Vec<Event>, RestError> {
        // A write's answer that cannot be read may have changed the rule.
        let unreadable = |mutating: bool| move |reason| RestError::response(reason, mutating);
        let rules = |server| format!("/guilds/{server}/auto-moderation/rules");
        match command {
            Command::LoadAutoModRules { request, server } => {
                let value = self
                    .request(
                        secret,
                        Method::GET,
                        &rules(server),
                        None,
                        wire_automod::MAX_RULES_BODY,
                    )
                    .await?;
                Ok(vec![Event::AutoModRules {
                    request: *request,
                    server: *server,
                    rules: wire_automod::rules(&value).map_err(unreadable(false))?,
                }])
            }
            Command::SaveAutoModRule {
                request,
                server,
                rule,
            } => {
                let (method, path) = match rule.id {
                    None => (Method::POST, rules(server)),
                    Some(id) => (Method::PATCH, format!("{}/{id}", rules(server))),
                };
                let value = self
                    .request_answer(
                        secret,
                        method,
                        &path,
                        Some(wire_automod::body(rule)),
                        wire_automod::MAX_RULES_BODY,
                        true,
                    )
                    .await?;
                Ok(vec![Event::AutoModRuleChanged {
                    server: *server,
                    rule: wire_automod::rule(&value).map_err(unreadable(true))?,
                    request: Some(*request),
                }])
            }
            Command::SetAutoModRuleEnabled {
                server,
                rule,
                enabled,
            } => {
                let value = self
                    .request(
                        secret,
                        Method::PATCH,
                        &format!("{}/{rule}", rules(server)),
                        Some(serde_json::json!({ "enabled": enabled })),
                        wire_automod::MAX_RULES_BODY,
                    )
                    .await?;
                Ok(vec![Event::AutoModRuleChanged {
                    server: *server,
                    rule: wire_automod::rule(&value).map_err(unreadable(true))?,
                    request: None,
                }])
            }
            Command::DeleteAutoModRule { server, rule } => {
                self.request(
                    secret,
                    Method::DELETE,
                    &format!("{}/{rule}", rules(server)),
                    None,
                    wire_automod::MAX_RULES_BODY,
                )
                .await?;
                Ok(vec![Event::AutoModRuleRemoved {
                    server: *server,
                    rule: *rule,
                }])
            }
            _ => Err(RestError::response(wire::invalid(), false)),
        }
    }
}
