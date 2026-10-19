//! The standard sticker packs and Discord's own GIF search (P5), read by
//! the routes of Serein and the official web client; see
//! [`crate::identity`]. Reads only. Fixtures only.

use reqwest::Method;
use rune_i18n::es;
use rune_provider::{Command, Event, GifError, GifResult};
use serde_json::Value;

use super::{Http, RestError};
use crate::{identity, SessionSecret};

impl Http {
    /// The REST calls of a P5 read. A GIF search that fails answers with
    /// its error; a suggestion that fails answers nothing.
    pub(crate) async fn expressions(
        &self,
        secret: &SessionSecret,
        command: &Command,
    ) -> Result<Vec<Event>, RestError> {
        let paths = identity::gif_paths(command);
        match command {
            Command::LoadStickerPacks => {
                let value = self
                    .request(
                        secret,
                        Method::GET,
                        "/sticker-packs",
                        None,
                        identity::MAX_PACKS,
                    )
                    .await?;
                let packs = identity::sticker_packs(&value).map_err(|_| {
                    RestError::rejected(es(
                        "Discord devolvió paquetes de stickers que RUNE no lee.",
                    ))
                })?;
                Ok(vec![Event::StickerPacks(packs)])
            }
            Command::SearchGifs {
                request,
                query,
                offset,
            } => {
                let gifs = self.gif_read(secret, &paths[0]).await;
                let result = match gifs {
                    Err(error) if error.ends_session => return Err(error),
                    Err(_) => Err(GifError::Unavailable),
                    Ok(gifs) => {
                        // Trending's categories are a second read; without
                        // them the GIFs still show.
                        let categories = match paths.get(1) {
                            Some(path) => match self.gif_read(secret, path).await {
                                Err(error) if error.ends_session => return Err(error),
                                found => found.ok(),
                            },
                            None => None,
                        };
                        Ok(identity::gif_page(&gifs, categories.as_ref(), *offset))
                    }
                };
                Ok(vec![Event::GifResults(GifResult {
                    request: *request,
                    query: query.clone(),
                    offset: *offset,
                    result,
                })])
            }
            Command::SuggestGifs { request, query } => {
                match self.gif_read(secret, &paths[0]).await {
                    Err(error) if error.ends_session => Err(error),
                    Err(_) => Ok(Vec::new()),
                    Ok(value) => Ok(vec![Event::GifSuggestions {
                        request: *request,
                        query: query.clone(),
                        terms: identity::suggestions(&value),
                    }]),
                }
            }
            _ => Err(RestError::rejected(es(
                "Esta acción todavía no está disponible.",
            ))),
        }
    }

    /// The icon of `application`, for an activity without pictures; any
    /// failure is no icon, never one the interface reports.
    pub(crate) async fn application_icon(
        &self,
        secret: &SessionSecret,
        application: u64,
    ) -> Option<rune_model::RemoteImage> {
        let value = self
            .request(
                secret,
                Method::GET,
                &format!("/applications/{application}/public"),
                None,
                crate::app_icons::MAX_RESPONSE,
            )
            .await
            .ok()?;
        crate::app_icons::icon(application, &value)
    }

    async fn gif_read(&self, secret: &SessionSecret, path: &str) -> Result<Value, RestError> {
        self.request(secret, Method::GET, path, None, identity::MAX_GIFS)
            .await
    }
}
