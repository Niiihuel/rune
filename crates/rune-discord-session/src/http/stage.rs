//! A stage's requests, as the official web client sends them (REA of its
//! public bundle, 2026-10-17): `PATCH /guilds/{g}/voice-states/@me` to raise
//! or lower a hand, accept an invitation or step down, and
//! `PATCH /guilds/{g}/voice-states/{user}` for moderation to invite someone
//! (`suppress: false` with a request time, which they accept) or send them
//! to the audience. One attempt each; the Gateway confirms. Fixtures only.

use reqwest::Method;
use rune_i18n::es;
use rune_model::{ServerId, UserId};
use rune_provider::{Command, Event};
use serde_json::{json, Value};

use super::{Http, RestError};
use crate::SessionSecret;

impl Http {
    pub(crate) async fn stage(
        &self,
        secret: &SessionSecret,
        server: ServerId,
        me: UserId,
        command: &Command,
    ) -> Result<Vec<Event>, RestError> {
        let now = || chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let (who, body) = match command {
            Command::RaiseHand { channel, raised } => (
                "@me".to_owned(),
                json!({
                    "channel_id": channel.to_string(),
                    "request_to_speak_timestamp": if *raised { Value::String(now()) } else { Value::Null },
                }),
            ),
            Command::SetSpeaker {
                channel,
                user,
                speaker,
            } if *user == me => (
                "@me".to_owned(),
                json!({
                    "channel_id": channel.to_string(),
                    "suppress": !speaker,
                    "request_to_speak_timestamp": null,
                }),
            ),
            Command::SetSpeaker {
                channel,
                user,
                speaker: true,
            } => (
                user.to_string(),
                json!({
                    "channel_id": channel.to_string(),
                    "suppress": false,
                    "request_to_speak_timestamp": now(),
                }),
            ),
            Command::SetSpeaker {
                channel,
                user,
                speaker: false,
            } => (
                user.to_string(),
                json!({"channel_id": channel.to_string(), "suppress": true}),
            ),
            _ => {
                return Err(RestError::rejected(es(
                    "Esta acción todavía no está disponible.",
                )))
            }
        };
        self.request_answer(
            secret,
            Method::PATCH,
            &format!("/guilds/{server}/voice-states/{who}"),
            Some(body),
            64 * 1024,
            true,
        )
        .await?;
        Ok(Vec::new())
    }
}
