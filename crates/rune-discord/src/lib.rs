//! Scoped Discord OAuth2 identity, with no bot or personal-token input.
//!
//! Public-client S256 is advertised by Discord's discovery document. Direct
//! public-client HTTP exchange remains experimental until tested with the
//! owner's registered application. Nothing here enables messaging or voice.

mod auth;
mod http;
mod provider;

pub use provider::{ConfigError, DiscordProvider};

/// Register this exact URI in the application's OAuth2 redirect allowlist.
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:48731/oauth/callback";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AuthError(&'static str);

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(rune_i18n::t(self.0))
    }
}

#[cfg(test)]
mod tests;
