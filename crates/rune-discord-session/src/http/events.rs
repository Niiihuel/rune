//! A server's scheduled events, read and written (see `crate::events` for
//! the routes and where they come from). Each write goes once; a challenge
//! on one is offered like any other write's. Fixtures only.

use reqwest::Method;
use rune_model::UserId;
use rune_provider::{Command, Event};
use serde_json::json;

use super::{Http, RestError};
use crate::{events as wire_events, wire, SessionSecret};

impl Http {
    pub(super) async fn scheduled_events(
        &self,
        secret: &SessionSecret,
        command: &Command,
    ) -> Result<Vec<Event>, RestError> {
        let invalid = |mutating: bool| move |reason| RestError::response(reason, mutating);
        let changed = |request, value: &serde_json::Value, server| {
            wire_events::event(value, Some(server))
                .map_err(invalid(true))?
                .map(|event| Event::ScheduledEventChanged {
                    request: Some(request),
                    event,
                })
                .ok_or_else(|| RestError::response(wire::invalid(), true))
        };
        match command {
            Command::LoadScheduledEvents { request, server } => {
                let value = self
                    .request(
                        secret,
                        Method::GET,
                        &format!("/guilds/{server}/scheduled-events?with_user_count=true"),
                        None,
                        wire_events::MAX_LIST,
                    )
                    .await?;
                let mut events = wire_events::events(&value, *server).map_err(invalid(false))?;
                let mine = self
                    .request(
                        secret,
                        Method::GET,
                        &format!("/users/@me/scheduled-events?guild_ids={server}"),
                        None,
                        wire_events::MAX_LIST,
                    )
                    .await?;
                let mine = wire_events::interests(&mine).map_err(invalid(false))?;
                for event in &mut events {
                    event.account_interested = mine.contains(&event.id);
                }
                Ok(vec![Event::ScheduledEvents {
                    request: *request,
                    server: *server,
                    events,
                }])
            }
            Command::LoadEventInterested {
                request,
                server,
                event,
            } => {
                let value = self
                    .request(
                        secret,
                        Method::GET,
                        &format!(
                            "/guilds/{server}/scheduled-events/{event}/users?limit={}&with_member=true",
                            wire_events::INTERESTED_PAGE
                        ),
                        None,
                        wire_events::MAX_LIST,
                    )
                    .await?;
                Ok(vec![Event::EventInterested {
                    request: *request,
                    server: *server,
                    event: *event,
                    users: wire_events::interested(&value).map_err(invalid(false))?,
                }])
            }
            Command::SetEventInterest {
                request,
                server,
                event,
                interested,
            } => {
                let path = format!("/guilds/{server}/scheduled-events/{event}/users/@me");
                if *interested {
                    let body = json!({ "response": wire_events::INTERESTED });
                    self.request_answer(secret, Method::PUT, &path, Some(body), 64 * 1024, true)
                        .await?;
                } else {
                    self.request_answer(secret, Method::DELETE, &path, None, 64 * 1024, true)
                        .await?;
                }
                // The account is named by `events::confirmed`, which knows it.
                Ok(vec![Event::EventInterest {
                    request: Some(*request),
                    server: *server,
                    event: *event,
                    user: UserId(0),
                    interested: *interested,
                }])
            }
            Command::CreateScheduledEvent {
                request,
                server,
                draft,
            } => {
                let body = wire_events::body(draft).map_err(invalid(false))?;
                let value = self
                    .request(
                        secret,
                        Method::POST,
                        &format!("/guilds/{server}/scheduled-events"),
                        Some(body),
                        256 * 1024,
                    )
                    .await?;
                Ok(vec![changed(*request, &value, *server)?])
            }
            Command::EditScheduledEvent {
                request,
                server,
                event,
                draft,
            } => {
                let body = wire_events::body(draft).map_err(invalid(false))?;
                let value = self
                    .request(
                        secret,
                        Method::PATCH,
                        &format!("/guilds/{server}/scheduled-events/{event}"),
                        Some(body),
                        256 * 1024,
                    )
                    .await?;
                Ok(vec![changed(*request, &value, *server)?])
            }
            Command::SetScheduledEventStatus {
                request,
                server,
                event,
                status,
            } => {
                let value = self
                    .request(
                        secret,
                        Method::PATCH,
                        &format!("/guilds/{server}/scheduled-events/{event}"),
                        Some(json!({ "status": wire_events::status(*status) })),
                        256 * 1024,
                    )
                    .await?;
                Ok(vec![changed(*request, &value, *server)?])
            }
            Command::DeleteScheduledEvent {
                request,
                server,
                event,
            } => {
                self.request_answer(
                    secret,
                    Method::DELETE,
                    &format!("/guilds/{server}/scheduled-events/{event}"),
                    None,
                    64 * 1024,
                    true,
                )
                .await?;
                Ok(vec![Event::ScheduledEventDeleted {
                    request: Some(*request),
                    server: *server,
                    event: *event,
                }])
            }
            _ => Err(RestError::response(wire::invalid(), false)),
        }
    }
}
