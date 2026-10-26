//! The connection's pill over the conversation: it shows only once the
//! connection has been missing for [`DELAY`], so a short cut leaves the
//! screen as it is, and says «Conectado» for a moment when it is back. It
//! lies over the conversation and takes no room, so nothing below it moves.

use std::time::{Duration, Instant};

use limn::Element;
use rune_components::chrome::{self, PillSign};
use rune_components::look::status_label;
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::Status;
use rune_provider::ConnectionState;
use rune_state::Store;

use crate::{splash, Ui};

/// How long the connection must be missing before the pill shows.
pub const DELAY: Duration = Duration::from_secs(2);
/// How long «Conectado» stays once a shown pill is gone.
const BACK_FOR: Duration = Duration::from_secs(2);
/// From this much retry wait, the pill says in how many seconds.
const COUNTDOWN_FROM_MS: u64 = 3_000;

/// The pill's state, kept between events.
#[derive(Debug, Default)]
pub struct Pill {
    /// When the connection left `Connected`, while it is not there.
    since: Option<Instant>,
    /// Whether this spell is a reconnection: «Reconectando…» stays between
    /// attempts until the connection is back or given up.
    reconnecting: bool,
    /// Until when «Conectado» shows, after a pill that was shown.
    back_until: Option<Instant>,
}

impl Pill {
    /// Whether the connection has been missing for [`DELAY`] at `now`.
    fn shown(&self, now: Instant) -> bool {
        self.since
            .is_some_and(|since| now.saturating_duration_since(since) >= DELAY)
    }

    /// When an idle host must wake for the pill to show or «Conectado» to go.
    pub fn next_expiry(&self, now: Instant) -> Option<Duration> {
        let show = self
            .since
            .map(|since| (since + DELAY).saturating_duration_since(now))
            .filter(|wait| !wait.is_zero());
        let back = self
            .back_until
            .map(|until| until.saturating_duration_since(now));
        show.into_iter().chain(back).min()
    }
}

/// After a connection event of the provider: when it went or came back.
pub fn on_applied(ui: &mut Ui, store: &Store, now: Instant) {
    let pill = &mut ui.connection;
    match store.connection {
        ConnectionState::Connected => {
            if let Some(since) = pill.since.take() {
                pill.back_until =
                    (now.saturating_duration_since(since) >= DELAY).then(|| now + BACK_FOR);
                pill.reconnecting = false;
            }
        }
        ConnectionState::Reconnecting { .. } => {
            pill.reconnecting = true;
            pill.since.get_or_insert(now);
        }
        ConnectionState::Disconnected { .. } => {
            pill.reconnecting = false;
            pill.since.get_or_insert(now);
        }
        ConnectionState::Connecting => {
            pill.since.get_or_insert(now);
        }
    }
}

/// Let the pill's timers pass at `now`. True when what it shows may have
/// changed, so the window is drawn again.
pub fn expire(ui: &mut Ui, now: Instant) -> bool {
    let pill = &mut ui.connection;
    if pill.back_until.is_some_and(|until| now >= until) {
        pill.back_until = None;
        return true;
    }
    pill.shown(now)
}

/// The pill over the conversation, while it says something.
pub fn view(ui: &Ui, store: &Store, look: &Look, now: Instant) -> Option<Element> {
    let pill = &ui.connection;
    match &store.connection {
        ConnectionState::Connected => pill
            .back_until
            .filter(|until| now < *until)
            .map(|_| chrome::connection_pill(t("Conectado"), PillSign::Back, None, look)),
        _ if !pill.shown(now) => None,
        ConnectionState::Connecting => {
            let sentence = if pill.reconnecting {
                t("Reconectando…")
            } else {
                t("Conectando…")
            };
            Some(chrome::connection_pill(
                sentence,
                PillSign::Working,
                None,
                look,
            ))
        }
        ConnectionState::Reconnecting { retry_in_ms, .. } => {
            let sentence = if *retry_in_ms >= COUNTDOWN_FROM_MS {
                tr!(
                    "Reconectando en {seconds} s",
                    seconds = retry_in_ms.div_ceil(1000)
                )
            } else {
                t("Reconectando…").to_owned()
            };
            Some(chrome::connection_pill(
                &sentence,
                PillSign::Working,
                splash::retry_now(store, look),
                look,
            ))
        }
        // The reason, which the pill keeps short, shows over it.
        ConnectionState::Disconnected { reason } => {
            Some(limn::Component::render(limn::Tooltip::new(
                chrome::connection_pill(
                    t("Sin conexión"),
                    PillSign::Offline,
                    splash::reconnect(ui, store, look),
                    look,
                ),
                "connection:reason",
                t(reason),
                look.menu_palette(),
            )))
        }
    }
}

/// The person's own status in the footer. While the connection is not there
/// and no status is known (the provider sent none), it says the connection's
/// state, not «Desconectado».
pub fn own_status(store: &Store, status: Status) -> String {
    match store.connection {
        ConnectionState::Connected => status_label(status).to_owned(),
        _ if status != Status::Offline => status_label(status).to_owned(),
        ConnectionState::Disconnected { .. } => t("Sin conexión").to_owned(),
        _ => t("Conectando…").to_owned(),
    }
}

/// For `--capture-connection`: the screen as a connection `state` shows it,
/// the pill already past its delay.
pub fn capture(ui: &mut Ui, store: &mut Store, state: ConnectionState, now: Instant) {
    let back = state == ConnectionState::Connected;
    ui.connection.reconnecting = matches!(state, ConnectionState::Reconnecting { .. });
    store.connection = state;
    if back {
        ui.connection.back_until = Some(now + BACK_FOR);
    } else {
        ui.connection.since = Some(now.checked_sub(DELAY).unwrap_or(now));
    }
}
