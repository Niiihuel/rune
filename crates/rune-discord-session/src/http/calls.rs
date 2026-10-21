//! Ringing people to a conversation's call, or stopping it, as the official
//! web client does (`CALL_RING`, `CALL_STOP_RINGING`; docs/DISCORD.md): one
//! attempt each, never repeated. The Gateway's `CALL_UPDATE` says whom it
//! rings then. And, before a direct call, whether it may ring (`CALL`,
//! 2026-10-18). Fixtures only.

use reqwest::Method;
use rune_model::ChannelId;
use rune_provider::{Command, Event, Failure};

use super::{Http, RestError};
use crate::SessionSecret;

/// The answer of `GET /channels/{id}/call` is a few fields.
const CALL_ANSWER: usize = 4 * 1024;

impl Http {
    pub(super) async fn ring(
        &self,
        secret: &SessionSecret,
        command: &Command,
    ) -> Result<Vec<Event>, RestError> {
        let Some((path, body)) = crate::voice::private::request(command) else {
            return Err(RestError::rejected(rune_i18n::es(
                "Esta acción todavía no está disponible.",
            )));
        };
        self.request_answer(secret, Method::POST, &path, Some(body), 64 * 1024, true)
            .await?;
        Ok(Vec::new())
    }

    /// Whether the call of the direct conversation `channel` may ring, as
    /// the official client asks before calling: only a `true` `ringable`
    /// rings. Discord's own refusal is said as such (`refused`), apart from
    /// an answer that never came.
    pub(super) async fn check_call(
        &self,
        secret: &SessionSecret,
        channel: ChannelId,
    ) -> Result<Vec<Event>, RestError> {
        let path = format!("/channels/{channel}/call");
        let said = |what: std::fmt::Arguments<'_>| {
            crate::voice::diag::write(crate::voice::private::PRIVATE, what)
        };
        let answer = match self
            .request(secret, Method::GET, &path, None, CALL_ANSWER)
            .await
        {
            Ok(answer) => answer,
            Err(error) if error.refused => {
                said(format_args!(
                    "may it ring (GET /call)? refused: {}",
                    error.reason
                ));
                return Ok(vec![Event::Failed {
                    failure: Failure::CallCheck {
                        channel,
                        refused: true,
                    },
                    reason: error.reason.to_string(),
                    ambiguous: false,
                }]);
            }
            Err(error) => {
                said(format_args!(
                    "may it ring (GET /call)? failed: {}",
                    error.reason
                ));
                return Err(error);
            }
        };
        let ringable = answer["ringable"].as_bool() == Some(true);
        said(format_args!(
            "may it ring (GET /call)? ringable: {ringable}"
        ));
        Ok(vec![Event::CallChecked { channel, ringable }])
    }
}
