//! The lookups a local activity needs before it is published (P6), by the
//! routes Serein uses (`apps/desktop/src/game_activity.rs`, read
//! 2026-10-17): an application's name (`/applications/{id}/rpc`), its
//! picture keys (`/oauth2/applications/{id}/assets`) and Discord's media
//! paths for addresses (`/applications/{id}/external-assets`); and the
//! public list of games Discord recognizes. Fixtures only.

use reqwest::{Method, StatusCode};
use rune_i18n::es;
use rune_model::LocalActivity;
use rune_provider::Event;
use serde_json::json;
use tokio::time::Instant;

use super::{read_bounded, status_error, Http, RestError};
use crate::desktop::{self, LocalPresence, ASSETS_AGAIN};
use crate::SessionSecret;

const ANSWER: usize = 256 * 1024;
/// Discord's list is large; Serein caps it here too.
const DETECTABLE: usize = 24 * 1024 * 1024;

impl Http {
    /// Resolve what `activity` lacks and leave it in `local`, for the
    /// presence. A picture that does not resolve is left out; an
    /// application that cannot be named is not published.
    pub(crate) async fn local_activity(
        &self,
        secret: &SessionSecret,
        activity: LocalActivity,
        local: LocalPresence,
    ) -> Result<Vec<Event>, RestError> {
        let name = match (&activity.name, activity.application) {
            (Some(name), _) => name.clone(),
            (None, Some(application)) => self.application_name(secret, application, &local).await?,
            (None, None) => {
                return Err(RestError::rejected(es(
                    "La actividad no dice de qué aplicación es.",
                )))
            }
        };
        let mut images = Vec::new();
        for image in [&activity.large_image, &activity.small_image]
            .into_iter()
            .flatten()
        {
            match self
                .image(secret, activity.application, image, &local)
                .await
            {
                Ok(Some(resolved)) => images.push((image.clone(), resolved)),
                Err(error) if error.ends_session => return Err(error),
                _ => {}
            }
        }
        let value = desktop::build(&activity, &name, |image| {
            images
                .iter()
                .find(|(given, _)| given == image)
                .map(|(_, resolved)| resolved.clone())
        })
        .ok_or_else(|| {
            RestError::rejected(es("La actividad es demasiado larga para publicarla."))
        })?;
        local.resolved(value);
        Ok(Vec::new())
    }

    async fn application_name(
        &self,
        secret: &SessionSecret,
        application: u64,
        local: &LocalPresence,
    ) -> Result<String, RestError> {
        let known = local
            .cache
            .lock()
            .ok()
            .and_then(|cache| cache.names.get(&application).cloned());
        if let Some(name) = known {
            return Ok(name);
        }
        let answer = self
            .request(
                secret,
                Method::GET,
                &format!("/applications/{application}/rpc"),
                None,
                ANSWER,
            )
            .await?;
        let name = desktop::application_name(application, &answer).ok_or_else(|| {
            RestError::rejected(es("Discord no reconoce la aplicación de la actividad."))
        })?;
        if let Ok(mut cache) = local.cache.lock() {
            cache.names.insert(application, name.clone());
        }
        Ok(name)
    }

    /// What Discord shows for `image`: an `mp:` path as it is, an address
    /// through its media proxy, a key through the application's assets.
    async fn image(
        &self,
        secret: &SessionSecret,
        application: Option<u64>,
        image: &str,
        local: &LocalPresence,
    ) -> Result<Option<String>, RestError> {
        if let Some(path) = image.strip_prefix("mp:") {
            return Ok(desktop::media_path(path));
        }
        let Some(application) = application else {
            return Ok(None);
        };
        if image.starts_with("https://") {
            let known = local.cache.lock().ok().and_then(|cache| {
                cache
                    .external
                    .iter()
                    .find(|(url, _)| url == image)
                    .map(|(_, path)| path.clone())
            });
            if known.is_some() {
                return Ok(known);
            }
            let answer = self
                .request(
                    secret,
                    Method::POST,
                    &format!("/applications/{application}/external-assets"),
                    Some(json!({ "urls": [image] })),
                    ANSWER,
                )
                .await?;
            let path = answer
                .as_array()
                .and_then(|found| found.iter().find(|asset| asset["url"] == image))
                .and_then(|asset| asset["external_asset_path"].as_str())
                .and_then(desktop::media_path);
            if let (Some(path), Ok(mut cache)) = (&path, local.cache.lock()) {
                cache.remember_external(image.to_owned(), path.clone());
            }
            return Ok(path);
        }
        let key = image.to_lowercase();
        let find = |keys: &[(String, String)]| {
            keys.iter()
                .find(|(id, name)| *name == key || *id == key)
                .map(|(id, _)| id.clone())
        };
        let (found, fresh) = local
            .cache
            .lock()
            .ok()
            .and_then(|cache| {
                cache
                    .assets
                    .get(&application)
                    .map(|(at, keys)| (find(keys), at.elapsed() < ASSETS_AGAIN))
            })
            .unwrap_or((None, false));
        if found.is_some() || fresh {
            return Ok(found);
        }
        let answer = self
            .request(
                secret,
                Method::GET,
                &format!("/oauth2/applications/{application}/assets"),
                None,
                ANSWER,
            )
            .await?;
        let keys = desktop::asset_keys(&answer);
        let found = find(&keys);
        if let Ok(mut cache) = local.cache.lock() {
            cache.assets.insert(application, (Instant::now(), keys));
        }
        Ok(found)
    }

    /// The games Discord recognizes. The list is public: it is asked for
    /// without the account's credential, and read off the runtime.
    pub(crate) async fn detectable_games(&self) -> Result<Vec<Event>, RestError> {
        let response = self
            .api(
                Method::GET,
                format!("{}/applications/detectable", self.base),
            )
            .send()
            .await
            .map_err(|_| {
                RestError::rejected(es("No se pudo leer la lista de juegos de Discord."))
            })?;
        if response.status() != StatusCode::OK {
            return Err(status_error(response.status(), false));
        }
        let bytes = read_bounded(response, DETECTABLE)
            .await
            .map_err(|reason| RestError::response(reason, false))?;
        let games = tokio::task::spawn_blocking(move || desktop::detectable(&bytes))
            .await
            .ok()
            .flatten()
            .ok_or_else(|| {
                RestError::rejected(es("La lista de juegos de Discord no se pudo leer."))
            })?;
        Ok(vec![Event::DetectableGames(games)])
    }
}
