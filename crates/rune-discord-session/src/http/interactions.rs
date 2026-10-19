//! Apps' routes (P3): the command index of a server or of a conversation
//! with a bot, and `POST /interactions`, whose answer comes by the Gateway.
//! A modal's files (comandos-de-apps) are staged by the route messages use.

use reqwest::Method;
use rune_provider::Event;

use super::{Http, RestError};
use crate::interactions::{index, Work, MAX_INDEX};
use crate::SessionSecret;

impl Http {
    pub(crate) async fn apps(
        &self,
        secret: &SessionSecret,
        work: Work,
    ) -> Result<Vec<Event>, RestError> {
        match work {
            Work::Index {
                path,
                request,
                channel,
                server,
                roles,
                cache,
            } => {
                let value = self
                    .request(secret, Method::GET, &path, None, MAX_INDEX)
                    .await?;
                let (commands, raw) = index(&value, server, &roles)
                    .map_err(|reason| RestError::response(reason, false))?;
                if let Ok(mut kept) = cache.lock() {
                    *kept = raw;
                }
                Ok(vec![Event::AppCommands {
                    request,
                    channel,
                    commands,
                }])
            }
            Work::Post {
                mut body,
                channel,
                files,
            } => {
                // A modal's files are staged first, as Serein does; nothing
                // reaches the app until the interaction, whatever happens
                // here. No progress: the interface shows none for a modal.
                if !files.is_empty() {
                    let staged = self
                        .post_files(secret, channel, &files, None)
                        .await
                        .map_err(|error| RestError {
                            ambiguous: false,
                            ..error
                        })?;
                    body["data"]["attachments"] = serde_json::json!(staged);
                }
                // 204 without a body; the app answers by the Gateway.
                self.request_answer(
                    secret,
                    Method::POST,
                    "/interactions",
                    Some(body),
                    64 * 1024,
                    true,
                )
                .await?;
                Ok(Vec::new())
            }
        }
    }
}
