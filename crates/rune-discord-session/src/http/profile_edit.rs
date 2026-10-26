//! Changes to the account's own profile. The routes and their fields are the
//! ones the official web client calls (REA of its public bundle, 2026-10-17;
//! see docs/DISCORD.md), Serein's `PATCH /users/@me` among them:
//!
//! - `PATCH /users/@me` `{global_name, avatar}`: the account's name and picture.
//! - `PATCH /users/@me/profile` `{bio, pronouns, accent_color, banner}`.
//! - `PATCH /guilds/{g}/members/@me` `{nick, avatar}`: its membership there.
//! - `PATCH /guilds/{g}/profile/@me` `{bio, banner}`: its profile there.
//!
//! Pictures travel as data URIs: a PNG, or a GIF for a moving one, as the
//! official client sends a moving avatar. Each write is one attempt, never
//! repeated; as Serein does, the profile is read back afterwards, and what
//! it says is what is reported. Nothing that needs the password (username,
//! email) is ever sent. Fixtures only: none was tried against Discord.

use base64::Engine;
use reqwest::Method;
use rune_i18n::{es, t};
use rune_model::{ServerId, User, UserProfile};
use rune_provider::{Event, Failure, ProfileEdit, ProfileImage, ServerProfileEdit};
use serde_json::{json, Map, Value};

use super::{Http, RestError};
use crate::{bootstrap, wire, SessionSecret};

/// Lengths the service takes, in characters, as Serein bounds them.
const NAME_MAX: usize = 32;
const ABOUT_MAX: usize = 190;
const PRONOUNS_MAX: usize = 40;

fn data_uri(image: &ProfileImage) -> String {
    format!(
        "data:{};base64,{}",
        image.mime(),
        base64::engine::general_purpose::STANDARD.encode(image.bytes())
    )
}

/// `text` within `max` characters, without control characters but line
/// breaks where `lines` allows them.
fn bounded(text: &str, max: usize, lines: bool) -> Result<(), RestError> {
    let fits = text.chars().count() <= max
        && text
            .chars()
            .all(|c| !c.is_control() || (lines && matches!(c, '\n' | '\r' | '\t')));
    if fits {
        Ok(())
    } else {
        Err(RestError::rejected(es(
            "El perfil tiene un texto demasiado largo o con caracteres no permitidos.",
        )))
    }
}

/// A text field as the routes take it: the text, or empty to take it away.
fn text(field: &Option<String>, max: usize, lines: bool) -> Result<Value, RestError> {
    let text = field.as_deref().map(str::trim).unwrap_or_default();
    bounded(text, max, lines)?;
    Ok(json!(text))
}

/// A picture field: the new one, or `null` to take it away.
fn picture(field: &Option<ProfileImage>) -> Value {
    field
        .as_ref()
        .map_or(Value::Null, |image| json!(data_uri(image)))
}

impl Http {
    /// Change the account's profile, then read it back. `me` is the account
    /// as the session knows it, so what one route does not report stays.
    pub(crate) async fn edit_profile(
        &self,
        secret: &SessionSecret,
        me: &User,
        edit: &ProfileEdit,
    ) -> Result<Vec<Event>, RestError> {
        let mut account = Map::new();
        if let Some(name) = &edit.display_name {
            let name = name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty());
            if let Some(name) = name {
                bounded(name, NAME_MAX, false)?;
            }
            account.insert("global_name".into(), json!(name));
        }
        if let Some(avatar) = &edit.avatar {
            account.insert("avatar".into(), picture(avatar));
        }
        let mut profile = Map::new();
        if let Some(about) = &edit.about {
            profile.insert("bio".into(), text(about, ABOUT_MAX, true)?);
        }
        if let Some(pronouns) = &edit.pronouns {
            profile.insert("pronouns".into(), text(pronouns, PRONOUNS_MAX, false)?);
        }
        if let Some(accent) = edit.accent {
            profile.insert("accent_color".into(), json!(accent.0 & 0xFF_FFFF));
        }
        if let Some(banner) = &edit.banner {
            profile.insert("banner".into(), picture(banner));
        }
        if account.is_empty() && profile.is_empty() {
            return Ok(Vec::new());
        }
        let mut user = me.clone();
        if !account.is_empty() {
            let value = self
                .request(
                    secret,
                    Method::PATCH,
                    "/users/@me",
                    Some(Value::Object(account)),
                    1024 * 1024,
                )
                .await?;
            let saved = wire::user(&value).map_err(|reason| RestError::response(reason, true))?;
            if saved.id != me.id || saved.bot {
                return Err(RestError::response(wire::invalid(), true));
            }
            user = User {
                pronouns: me.pronouns.clone(),
                about: me.about.clone(),
                ..saved
            };
            user.banner.inherit_unknown(&me.banner);
        }
        if !profile.is_empty() {
            self.request(
                secret,
                Method::PATCH,
                "/users/@me/profile",
                Some(Value::Object(profile)),
                crate::profiles::MAX_RESPONSE,
            )
            .await?;
        }
        let mut events = Vec::new();
        match self.read_back(secret, me.id, None).await {
            Ok(saved) => {
                user.about = saved.about.clone();
                user.pronouns = saved.pronouns.clone();
                if let Some(accent) = saved.accent {
                    user.accent = accent;
                }
                if saved.banner != rune_model::ProfileBanner::Unknown {
                    user.banner = saved.banner.clone();
                }
                events.push(Event::UserChanged(user));
                events.push(Event::ProfileChanged(saved));
            }
            Err(error) if error.ends_session => return Err(error),
            Err(_) => {
                events.push(Event::UserChanged(user));
                events.push(unconfirmed(Failure::Profile));
            }
        }
        Ok(events)
    }

    /// Change the account's profile in `server`, then read it back there.
    pub(crate) async fn edit_server_profile(
        &self,
        secret: &SessionSecret,
        me: rune_model::UserId,
        server: ServerId,
        edit: &ServerProfileEdit,
    ) -> Result<Vec<Event>, RestError> {
        let mut member = Map::new();
        if let Some(nick) = &edit.nick {
            let nick = nick
                .as_deref()
                .map(str::trim)
                .filter(|nick| !nick.is_empty());
            if let Some(nick) = nick {
                bounded(nick, NAME_MAX, false)?;
            }
            member.insert("nick".into(), json!(nick));
        }
        if let Some(avatar) = &edit.avatar {
            member.insert("avatar".into(), picture(avatar));
        }
        let mut profile = Map::new();
        if let Some(about) = &edit.about {
            profile.insert("bio".into(), text(about, ABOUT_MAX, true)?);
        }
        if let Some(banner) = &edit.banner {
            profile.insert("banner".into(), picture(banner));
        }
        if member.is_empty() && profile.is_empty() {
            return Ok(Vec::new());
        }
        let mut events = Vec::new();
        if !member.is_empty() {
            let value = self
                .request(
                    secret,
                    Method::PATCH,
                    &format!("/guilds/{server}/members/@me"),
                    Some(Value::Object(member)),
                    1024 * 1024,
                )
                .await?;
            let saved = bootstrap::list_member(&value, server)
                .map_err(|reason| RestError::response(reason, true))?;
            if saved.user != me {
                return Err(RestError::response(wire::invalid(), true));
            }
            events.push(Event::MemberChanged(saved));
        }
        if !profile.is_empty() {
            self.request(
                secret,
                Method::PATCH,
                &format!("/guilds/{server}/profile/@me"),
                Some(Value::Object(profile)),
                crate::profiles::MAX_RESPONSE,
            )
            .await?;
        }
        match self.read_back(secret, me, Some(server)).await {
            Ok(saved) => events.push(Event::ProfileChanged(saved)),
            Err(error) if error.ends_session => return Err(error),
            Err(_) => events.push(unconfirmed(Failure::ServerProfile { server })),
        }
        Ok(events)
    }

    /// The account's profile as the service has it now, in `server`.
    async fn read_back(
        &self,
        secret: &SessionSecret,
        me: rune_model::UserId,
        server: Option<ServerId>,
    ) -> Result<UserProfile, RestError> {
        // What was edited only: its effect is not read (`profile_changed`
        // takes nothing of it).
        let events = self.profile(secret, 0, me, server, false, false).await?;
        events
            .into_iter()
            .find_map(|event| match event {
                Event::ProfileLoaded { profile, .. } => Some(profile),
                _ => None,
            })
            .ok_or_else(|| RestError::response(wire::invalid(), false))
    }
}

/// The change was taken but could not be read back: it may differ from what
/// was asked, so the interface says so instead of showing it as confirmed.
fn unconfirmed(failure: Failure) -> Event {
    Event::Failed {
        failure,
        reason: t("Se guardó, pero no se pudo confirmar cómo quedó. Abre el perfil otra vez antes de volver a cambiarlo.").into(),
        ambiguous: true,
    }
}
