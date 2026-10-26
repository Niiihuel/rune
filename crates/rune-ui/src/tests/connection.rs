//! The connection's pill over the conversation: it waits, keeps one wording
//! through a reconnection, says «Conectado» once, and never repeats itself
//! in a toast.

use super::*;
use rune_provider::ConnectionState;

fn reconnecting(retry_in_ms: u64) -> Event {
    Event::Connection(ConnectionState::Reconnecting {
        attempt: 1,
        retry_in_ms,
    })
}

fn shows(host: &mut Host, text: &str) -> bool {
    host.painted().iter().any(|line| line == text)
}

#[test]
fn a_short_cut_shows_nothing_and_says_nothing_after() {
    let mut host = Host::new();
    host.event(reconnecting(1000));
    host.now += Duration::from_millis(1500);
    host.event(Event::Connection(ConnectionState::Connected));
    host.now += Duration::from_secs(5);
    assert!(!shows(&mut host, "Reconectando…"));
    assert!(!shows(&mut host, "Conectado"));
    assert!(host.ui.notices.is_empty());
}

#[test]
fn a_lost_connection_shows_after_the_delay_with_a_way_out_and_no_toast() {
    let mut host = Host::new();
    host.ui.providers.current = Some("mock");
    host.event(Event::Connection(ConnectionState::Disconnected {
        reason: "sin red".into(),
    }));
    assert!(!shows(&mut host, "Sin conexión"));
    assert_eq!(
        host.ui.next_expiry(host.now),
        Some(crate::connection::DELAY)
    );
    assert!(host.ui.notices.is_empty(), "the pill says it, not a toast");
    host.now += crate::connection::DELAY;
    assert!(shows(&mut host, "Sin conexión"));
    assert!(host.painted().iter().any(|line| line == "Reintentar"));
    let frame = host.frame();
    assert!(!frame.commands.is_empty());
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: crate::splash::ids::RECONNECT.into(),
        },
        host.now,
    );
    assert_eq!(
        intents,
        vec![Intent::UseProvider(host.ui.providers.current.unwrap())]
    );
}

#[test]
fn a_reconnection_keeps_its_wording_between_attempts() {
    let mut host = Host::new();
    host.event(reconnecting(5000));
    host.now += crate::connection::DELAY;
    assert!(shows(&mut host, "Reconectando en 5 s"));
    host.event(Event::Connection(ConnectionState::Connecting));
    assert!(shows(&mut host, "Reconectando…"));
    assert!(!shows(&mut host, "Conectando…"));
    host.event(reconnecting(1000));
    assert!(shows(&mut host, "Reconectando…"));
}

#[test]
fn back_after_a_shown_pill_says_connected_for_a_moment() {
    let mut host = Host::new();
    host.event(reconnecting(1000));
    host.now += crate::connection::DELAY;
    assert!(shows(&mut host, "Reconectando…"));
    host.event(Event::Connection(ConnectionState::Connected));
    assert!(shows(&mut host, "Conectado"));
    host.now += Duration::from_secs(3);
    assert!(!shows(&mut host, "Conectado"));
}

#[test]
fn back_after_a_short_cut_says_nothing() {
    let mut host = Host::new();
    host.event(reconnecting(1000));
    host.now += Duration::from_millis(500);
    host.event(Event::Connection(ConnectionState::Connected));
    assert!(!shows(&mut host, "Conectado"));
}

#[test]
fn the_footer_says_the_connection_not_offline_while_it_waits() {
    let mut host = Host::new();
    if let Some(info) = host.store.provider.as_mut() {
        info.capabilities.presence = true;
    }
    host.store.connection = ConnectionState::Connecting;
    // The account's last known status stays, as it does when connected.
    let me = host.store.me().expect("the session is loaded");
    let known = rune_components::look::status_label(host.store.presence(me).status);
    assert!(shows(&mut host, known), "{known}");
    assert!(!shows(&mut host, "Desconectado"));
}
