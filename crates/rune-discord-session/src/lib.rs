//! Experimental normal-account communication, separate from OAuth2.
//!
//! The host supplies an explicit owner-operated login surface. This adapter
//! never reads credentials from another application or exposes them to the
//! UI state. Every transport presents the web client in a WebKit browser
//! (owner decision of 2026-10-19, `browser`). Transport stays behind the
//! provider boundary.

use rune_i18n::{es, t};
use std::fmt;

use rune_provider::BoxFuture;
use zeroize::Zeroizing;

mod admin;
mod app_icons;
mod bootstrap;
mod challenges;
mod components;
mod emoji;
mod gateway;
mod http;
mod invitations;
mod member_search;
mod members;
mod messages;
mod profiles;
mod provider;
mod reads;
mod rich;
mod social;
mod stage;
mod stickers;
mod tallies;
mod text;
mod uploads;
mod voice;
mod wire;
// The account's synced settings (P1).
mod account;
mod settings_proto;
// Presence, identity and expressions (P5).
mod identity;
// Commands and interactions (P3).
mod interactions;
// Message actions and moderation (P4).
mod message_actions;
// Desktop, language and local voice (P6).
mod desktop;
// Rail and notifications.
mod activities;
// Rules and onboarding (2026-10-18).
mod onboarding;
// The web client's identity in a browser (2026-10-19).
mod browser;
// Scheduled events (eventos-programados).
mod events;
// AutoMod (automod).
mod automod;

pub use browser::{learnt_user_agent, user_agent, Brand, Engine};
pub use provider::{has_remembered_accounts, DiscordSessionProvider};

/// A fixed local error label, optionally carrying a numeric Gateway close code.
/// Remote reasons, response bodies and account/session identifiers are excluded.
/// The label is Spanish, marked with `es`, and shown in the current language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoginError {
    message: &'static str,
    gateway_code: Option<u16>,
}

impl LoginError {
    pub const fn new(message: &'static str) -> Self {
        Self {
            message,
            gateway_code: None,
        }
    }

    pub(crate) const fn gateway_close(message: &'static str, code: u16) -> Self {
        Self {
            message,
            gateway_code: Some(code),
        }
    }

    pub(crate) const fn gateway_close_code(self) -> Option<u16> {
        self.gateway_code
    }
}

impl fmt::Display for LoginError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(t(self.message))?;
        if let Some(code) = self.gateway_code {
            write!(f, " (Gateway {code})")?;
        }
        Ok(())
    }
}

impl std::error::Error for LoginError {}

/// An owned, redacted credential which cannot enter ordinary domain events.
/// The owned string is erased on drop; transport/TLS libraries can hold
/// temporary copies, so this is not a guarantee that all process memory is wiped.
///
/// ```compile_fail
/// use rune_discord_session::SessionSecret;
/// let secret = SessionSecret::try_from_local("SYNTHETIC_SESSION".into()).unwrap();
/// let duplicate = secret.clone();
/// ```
///
/// ```compile_fail
/// use rune_discord_session::SessionSecret;
/// let secret = SessionSecret::try_from_local("SYNTHETIC_SESSION".into()).unwrap();
/// let persisted = serde_json::to_string(&secret).unwrap();
/// ```
pub struct SessionSecret(Zeroizing<String>);

impl SessionSecret {
    pub fn try_from_local(value: String) -> Result<Self, LoginError> {
        let value = Zeroizing::new(value);
        if !(16..=2048).contains(&value.len())
            || !value.bytes().all(|byte| byte.is_ascii_graphic())
            || value.starts_with("Bot ")
            || value.starts_with("Bearer ")
        {
            return Err(LoginError::new(es(
                "La sesión local recibida no es válida.",
            )));
        }
        Ok(Self(value))
    }
}

impl fmt::Debug for SessionSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionSecret([REDACTED])")
    }
}

/// What a host needs to show one hCaptcha challenge that the service set on
/// an explicit action: never the account credential. The parts that bind the
/// answer to that action stay in this adapter.
pub struct Challenge {
    sitekey: String,
    rqdata: Option<Zeroizing<String>>,
    invisible: bool,
}

impl Challenge {
    /// `None` unless every value is bounded and printable.
    pub fn new(sitekey: String, rqdata: Option<String>, invisible: bool) -> Option<Self> {
        let rqdata = rqdata.map(Zeroizing::new);
        let valid = (1..=128).contains(&sitekey.len())
            && sitekey
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            && rqdata.as_deref().is_none_or(|data| printable(data, 4096));
        valid.then_some(Self {
            sitekey,
            rqdata,
            invisible,
        })
    }

    pub fn sitekey(&self) -> &str {
        &self.sitekey
    }

    pub fn rqdata(&self) -> Option<&str> {
        self.rqdata.as_deref().map(String::as_str)
    }

    /// The service asked for the invisible variant, started by a click.
    pub fn invisible(&self) -> bool {
        self.invisible
    }
}

impl fmt::Debug for Challenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Challenge([REDACTED])")
    }
}

/// The person's solution to a [`Challenge`], sent once with the action it
/// belongs to and erased on drop.
pub struct ChallengeAnswer(Zeroizing<String>);

impl ChallengeAnswer {
    pub fn try_from_local(value: String) -> Result<Self, LoginError> {
        let value = Zeroizing::new(value);
        if !printable(&value, 8192) {
            return Err(LoginError::new(es(
                "La verificación recibida no es válida.",
            )));
        }
        Ok(Self(value))
    }
}

impl fmt::Debug for ChallengeAnswer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ChallengeAnswer([REDACTED])")
    }
}

/// Non-empty, bounded and visible ASCII: safe as an HTTP header value.
pub(crate) fn printable(value: &str, max: usize) -> bool {
    (1..=max).contains(&value.len()) && value.bytes().all(|byte| byte.is_ascii_graphic())
}

/// A host-owned login task. Dropping its future must close the temporary
/// login surface and discard pending results, including candidate credentials.
pub trait SessionLogin: Send + Sync {
    fn acquire(&self) -> BoxFuture<'_, Result<SessionSecret, LoginError>>;

    /// Show `challenge` to the person in a separate temporary window, never
    /// solving it automatically. Closing the window is an error; dropping the
    /// future closes it. Hosts without such a window refuse.
    fn verify(&self, _challenge: Challenge) -> BoxFuture<'_, Result<ChallengeAnswer, LoginError>> {
        Box::pin(async {
            Err(LoginError::new(es(
                "Esta compilación no incluye la ventana de verificación.",
            )))
        })
    }

    /// What a page in the engine of these windows reads of its browser, so
    /// every request presents the same (windows-login: WebView2). `None`:
    /// WebKitGTK's, known beforehand. Asked before anything goes out.
    fn engine(&self) -> BoxFuture<'_, Result<Option<Engine>, LoginError>> {
        Box::pin(async { Ok(None) })
    }
}

#[cfg(test)]
mod secret_tests {
    use super::*;

    #[test]
    fn invalid_credentials_cannot_reach_an_http_header() {
        for invalid in [
            String::new(),
            "short".into(),
            "SYNTHETIC_SECRET\r\nx-injected: value".into(),
            "Bearer SYNTHETIC_SESSION".into(),
            "Bot SYNTHETIC_SESSION".into(),
            " leading_whitespace_secret".into(),
            "non_ascii_credential_é".into(),
            "x".repeat(2049),
        ] {
            assert!(SessionSecret::try_from_local(invalid).is_err());
        }
    }

    #[test]
    fn credentials_are_redacted_in_diagnostics() {
        let secret = SessionSecret::try_from_local("SYNTHETIC_SESSION_MARKER".into()).unwrap();
        assert_eq!(format!("{secret:?}"), "SessionSecret([REDACTED])");
    }
}

#[cfg(test)]
mod tests;
