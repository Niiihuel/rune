//! Diagnostics, always on: one stderr line per step of the account's voice
//! and stream connections and of the main Gateway's voice requests, so a
//! failed real call can be reported (docs/DISCORD.md says which
//! lines to send); and of each invitation acceptance (docs/DISCORD.md).
//!
//! A line says what happened and why, never with what: no token, session
//! id, key, MLS message, user id or the account's own public address. Hosts
//! of Discord's voice servers, opcodes, close codes, encryption modes and
//! counts are fine.

use std::fmt::Arguments;

/// `rune: {scope}: {what}` on stderr.
pub(crate) fn write(scope: &str, what: Arguments<'_>) {
    let line = format!("rune: {scope}: {what}");
    eprintln!("{line}");
    #[cfg(test)]
    captured::push(line);
}

/// [`write`] with `format!` arguments.
macro_rules! note {
    ($scope:expr, $($what:tt)*) => {
        $crate::voice::diag::write($scope, format_args!($($what)*))
    };
}
pub(crate) use note;

/// The host (and port) of a `wss://host[:port]/…` address: what a line may
/// say of where it connects.
pub(crate) fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?']).next().unwrap_or_default()
}

/// An endpoint as Discord sent it, as a line may show it: it names
/// Discord's server, not the account; printable and short.
pub(crate) fn shown_endpoint(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':'))
        .take(80)
        .collect()
}

/// A close frame's reason as a line may show it: short and printable.
pub(crate) fn reason(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_graphic() || *c == ' ')
        .take(64)
        .collect()
}

/// What kind of failure a websocket error is, without its text.
pub(crate) fn socket_error(error: &tokio_tungstenite::tungstenite::Error) -> String {
    use tokio_tungstenite::tungstenite::Error;
    match error {
        Error::ConnectionClosed => "closed".into(),
        Error::AlreadyClosed => "already closed".into(),
        Error::Io(error) => format!("i/o: {:?}", error.kind()),
        Error::Tls(_) => "TLS".into(),
        Error::Capacity(_) => "message over the size limit".into(),
        Error::Protocol(_) => "websocket protocol".into(),
        Error::WriteBufferFull(_) => "write buffer full".into(),
        Error::Utf8(_) => "invalid UTF-8".into(),
        Error::AttackAttempt => "attack attempt".into(),
        Error::Url(_) => "address".into(),
        Error::Http(response) => format!("HTTP {}", response.status().as_u16()),
        Error::HttpFormat(_) => "HTTP format".into(),
    }
}

/// Fixtures read what was written.
#[cfg(test)]
pub(crate) mod captured {
    use std::sync::Mutex;

    static LINES: Mutex<Vec<String>> = Mutex::new(Vec::new());

    pub(crate) fn push(line: String) {
        let mut lines = LINES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if lines.len() >= 100_000 {
            lines.clear();
        }
        lines.push(line);
    }

    /// Every line so far, of every test running.
    pub(crate) fn lines() -> Vec<String> {
        LINES
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}
