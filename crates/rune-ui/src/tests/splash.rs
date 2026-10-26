//! The screen until the session is loaded: its stages, why it waits, the
//! ways out, and «Reconectando» left for a session already loaded.

use super::*;
use rune_provider::{
    AuthenticationState, Capabilities, ConnectionState, ProviderInfo, ProviderKind,
};

/// A host whose provider has said nothing yet: no session, no cache.
fn starting(authentication: bool) -> Host {
    let mut host = Host::new();
    host.store = Store::new();
    host.store.provider = Some(ProviderInfo {
        name: "Prueba".into(),
        kind: if authentication {
            ProviderKind::Unofficial
        } else {
            ProviderKind::Simulated
        },
        capabilities: Capabilities {
            authentication,
            saved_accounts: authentication,
            ..Capabilities::default()
        },
        notice: "Proveedor de prueba.".into(),
    });
    host.ui = Ui::new();
    host.ui.providers.current = Some("mock");
    host.sent.clear();
    host
}

fn click(host: &mut Host, id: &str) -> Vec<Intent> {
    host.frame();
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked { id: id.into() },
        host.now,
    )
}

#[test]
fn it_says_each_stage_until_the_session_is_ready_then_gives_way() {
    let mut host = starting(true);
    host.event(Event::Connection(ConnectionState::Connecting));
    let text = host.painted().join(" ");
    assert!(
        text.contains("Conectando…") && text.contains("Proveedor de prueba."),
        "{text}"
    );
    assert!(!host.painted().iter().any(|line| line == "Servidores"));
    host.event(Event::Authentication(AuthenticationState::Exchanging));
    assert!(host.painted().join(" ").contains("Iniciando sesión…"));
    host.event(Event::Authentication(AuthenticationState::SignedIn {
        scopes: Vec::new(),
    }));
    assert!(host.painted().join(" ").contains("Cargando servidores…"));
    assert!(host.ui.splash_since.is_some());
    host.event(Event::Ready(Box::new(host.provider.snapshot())));
    assert!(!crate::splash::visible(&host.store));
    assert_eq!(host.ui.splash_since, None);
    assert!(!host.painted().join(" ").contains("Cargando servidores…"));
    // A connection lost now is the pill's, not the opening screen's.
    host.event(Event::Connection(ConnectionState::Reconnecting {
        attempt: 1,
        retry_in_ms: 2000,
    }));
    host.now += crate::connection::DELAY;
    let text = host.painted().join(" ");
    assert!(text.contains("Reconectando…"), "{text}");
    assert!(!text.contains("Cargando servidores…"));
}

#[test]
fn after_ten_seconds_it_says_why_and_offers_the_ways_out() {
    let mut host = starting(true);
    host.event(Event::Authentication(AuthenticationState::SignedIn {
        scopes: Vec::new(),
    }));
    host.event(Event::Connection(ConnectionState::Reconnecting {
        attempt: 2,
        retry_in_ms: 4000,
    }));
    let early = host.painted().join(" ");
    assert!(
        !early.contains("Reintentar") && !early.contains("intento 2"),
        "{early}"
    );
    let since = host.ui.splash_since.unwrap();
    assert_eq!(
        host.ui.next_expiry(since + Duration::from_secs(4)),
        Some(Duration::from_secs(6)),
        "an idle window wakes to say it"
    );
    host.now = since + crate::splash::TROUBLE_AFTER;
    let text = host.painted().join(" ");
    assert!(
        text.contains("Reintentando (intento 2) en 4.0 s…"),
        "{text}"
    );
    for label in ["Reintentar", "Cambiar de cuenta", "Cerrar sesión"] {
        assert!(text.contains(label), "{label}: {text}");
    }
    // Without a way to cut the wait short, only a new start.
    assert!(!text.contains("Reintentar ya"), "{text}");
    assert_eq!(
        click(&mut host, crate::splash::ids::RETRY),
        vec![Intent::UseProvider("mock")]
    );
}

#[test]
fn switching_lists_the_remembered_accounts_first_and_signing_out_asks() {
    use crate::splash::ids;
    let mut host = starting(true);
    host.event(Event::RememberedAccounts(
        [(42, "Rune Test"), (7, "Otra")]
            .into_iter()
            .map(|(id, name)| rune_provider::RememberedAccount {
                id: rune_model::UserId(id),
                name: name.into(),
                avatar: None,
            })
            .collect(),
    ));
    host.event(Event::Authentication(AuthenticationState::Exchanging));
    host.now = host.ui.splash_since.unwrap() + crate::splash::TROUBLE_AFTER;
    // The accounts before any login page, «Agregar cuenta» among them.
    assert_eq!(
        click(&mut host, ids::SWITCH),
        vec![Intent::Focus("splash:account=42".into())]
    );
    let text = host.painted().join(" ");
    for label in [
        "Continuar como Rune Test",
        "Continuar como Otra",
        "Agregar cuenta",
    ] {
        assert!(text.contains(label), "{label}: {text}");
    }
    assert!(!text.contains("Cerrar sesión"), "{text}");
    assert!(click(&mut host, ids::BACK).is_empty());
    assert!(host.painted().join(" ").contains("Cerrar sesión"));
    click(&mut host, ids::SWITCH);
    assert_eq!(
        click(&mut host, "splash:account=7"),
        vec![Intent::Command(Command::UseAccount(Some(
            rune_model::UserId(7)
        )))]
    );
    // Gone with the choice: the ways out again if it still waits.
    assert_eq!(host.ui.splash_asking, None);
    click(&mut host, ids::SWITCH);
    assert_eq!(
        click(&mut host, ids::ADD_ACCOUNT),
        vec![Intent::Command(Command::UseAccount(None))]
    );
    // Signing out forgets the account: asked first, «Cancelar» focused.
    assert_eq!(
        click(&mut host, ids::SIGN_OUT),
        vec![Intent::Focus(ids::BACK.into())]
    );
    let text = host.painted().join(" ");
    assert!(text.contains("RUNE olvida esta cuenta"), "{text}");
    assert!(click(&mut host, ids::BACK).is_empty());
    assert!(!host.painted().join(" ").contains("RUNE olvida esta cuenta"));
    click(&mut host, ids::SIGN_OUT);
    assert_eq!(
        click(&mut host, ids::SIGN_OUT_YES),
        vec![Intent::Command(Command::SignOut)]
    );
    // The screen gone, the question goes with it.
    click(&mut host, ids::SIGN_OUT);
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    assert_eq!(host.ui.splash_asking, None);
}

#[test]
fn inside_the_gateways_wait_it_can_try_again_at_once() {
    let mut host = starting(true);
    if let Some(info) = host.store.provider.as_mut() {
        info.capabilities.reconnect_now = true;
    }
    // The session provider's order: the account checked (Ready with only
    // the account), then signed in, then its Gateway connects and waits.
    let me = host.provider.snapshot().me;
    host.event(Event::Ready(Box::new(rune_provider::Snapshot {
        users: me.clone().into_iter().collect(),
        me,
        ..Default::default()
    })));
    host.event(Event::Authentication(AuthenticationState::SignedIn {
        scopes: Vec::new(),
    }));
    host.event(Event::Connection(ConnectionState::Connecting));
    host.event(Event::Connection(ConnectionState::Reconnecting {
        attempt: 3,
        retry_in_ms: 8000,
    }));
    // The opening screen is gone: the pill over the conversation offers it,
    // once the connection has been missing for a moment.
    assert!(!crate::splash::visible(&host.store));
    host.now += crate::connection::DELAY;
    let text = host.painted().join(" ");
    assert!(
        text.contains("Reconectando en 8 s") && text.contains("Reintentar ya"),
        "{text}"
    );
    assert_eq!(
        click(&mut host, crate::splash::ids::RETRY_NOW),
        vec![Intent::Command(Command::ReconnectNow)]
    );
    // Given up, nothing waits: no hurry left to offer.
    host.event(Event::Connection(ConnectionState::Disconnected {
        reason: "sin red".into(),
    }));
    let text = host.painted().join(" ");
    assert!(!text.contains("Reintentar ya"), "{text}");
    // But the provider can start again from the same line, without a restart.
    assert!(text.contains("Reintentar"), "{text}");
    let current = host.ui.providers.current.unwrap();
    assert_eq!(
        click(&mut host, crate::splash::ids::RECONNECT),
        vec![Intent::UseProvider(current)]
    );
}

#[test]
fn a_provider_that_waits_before_it_knows_the_account_offers_it_here() {
    let mut host = starting(true);
    if let Some(info) = host.store.provider.as_mut() {
        info.capabilities.reconnect_now = true;
    }
    host.event(Event::Connection(ConnectionState::Reconnecting {
        attempt: 1,
        retry_in_ms: 2000,
    }));
    host.now = host.ui.splash_since.unwrap() + crate::splash::TROUBLE_AFTER;
    assert!(crate::splash::visible(&host.store));
    assert!(host.painted().join(" ").contains("Reintentar ya"));
}

#[test]
fn a_connection_given_up_says_so_at_once_and_a_simulation_can_only_retry() {
    let mut host = starting(false);
    host.event(Event::Connection(ConnectionState::Disconnected {
        reason: "sin red".into(),
    }));
    let text = host.painted().join(" ");
    assert!(
        text.contains("Sin conexión: sin red") && text.contains("Reintentar"),
        "{text}"
    );
    assert!(!text.contains("Cerrar sesión") && !text.contains("Cambiar de cuenta"));
}

#[test]
fn signing_in_shows_the_login_page_not_the_opening_screen() {
    let mut host = starting(true);
    for state in [
        AuthenticationState::SignedOut,
        AuthenticationState::AwaitingExternal,
    ] {
        host.event(Event::Authentication(state));
        assert!(!crate::splash::visible(&host.store));
    }
}
