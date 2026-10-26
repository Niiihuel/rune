//! Choosing the provider from the window: its origin always said, the
//! unofficial one warned about, and leaving the current one confirmed.

use super::*;
use rune_provider::{AuthenticationState, Capabilities, ProviderInfo, ProviderKind};

fn options() -> Vec<ProviderOption> {
    vec![
        ProviderOption {
            key: "local",
            name: "Simulación de prueba".into(),
            kind: ProviderKind::Simulated,
            notice: "Datos inventados.".into(),
            unavailable: None,
        },
        ProviderOption {
            key: "official",
            name: "Servicio oficial".into(),
            kind: ProviderKind::Official,
            notice: "Con autorización.".into(),
            unavailable: Some("Falta configurarlo.".into()),
        },
        ProviderOption {
            key: "personal",
            name: "Sesión personal".into(),
            kind: ProviderKind::Unofficial,
            notice: "Experimento.".into(),
            unavailable: None,
        },
    ]
}

fn clicked(host: &mut Host, id: &str) -> Vec<Intent> {
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked { id: id.into() },
        host.now,
    )
}

#[test]
fn the_sign_in_page_says_where_it_comes_from_and_offers_the_others() {
    let mut host = Host::new();
    host.ui.providers = Providers {
        options: options(),
        current: Some("personal"),
        ..Default::default()
    };
    host.store = Store::new();
    host.store.provider = Some(ProviderInfo {
        name: "Sesión personal".into(),
        kind: ProviderKind::Unofficial,
        capabilities: Capabilities {
            authentication: true,
            ..Capabilities::default()
        },
        notice: "Experimento.".into(),
    });
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    // The choices survive the page's own reset of the window.
    assert_eq!(host.ui.providers.options.len(), 3);
    let text = host.painted().join(" ");
    assert!(text.contains("Sesión personal · no oficial"));
    assert!(text.contains("Experimento personal no oficial"));
    host.click("provider:list");
    let text = host.painted().join(" ");
    assert!(text.contains("Simulación de prueba · sin conexión real"));
    assert!(text.contains("Falta configurarlo."));
    assert!(text.contains("En uso"));
    // What cannot start is not offered; the one in use is not chosen again.
    assert!(clicked(&mut host, "provider:use=official").is_empty());
    assert_eq!(host.ui.providers.confirming, None);
    clicked(&mut host, "provider:use=personal");
    assert_eq!(host.ui.providers.confirming, None);
    // Another one asks first, and says what goes.
    host.click("provider:use=local");
    let text = host.painted().join(" ");
    assert!(text.contains("¿Cambiar a Simulación de prueba?"), "{text}");
    host.click("provider:cancel");
    assert_eq!(host.ui.providers.confirming, None);
    host.click("provider:use=local");
    assert_eq!(
        clicked(&mut host, "provider:confirm"),
        vec![Intent::UseProvider("local")]
    );
}

#[test]
fn a_new_provider_clears_the_account_state_and_keeps_the_window_settings() {
    let mut host = Host::new();
    host.ui.providers = Providers {
        options: options(),
        current: Some("local"),
        ..Default::default()
    };
    let channel = host.store.read_channels().next().unwrap().id;
    host.ui.drafts.insert(channel, "borrador".into());
    host.ui.prefs.developer = true;
    host.ui.theme = Theme::Light;
    let intents = provider_changed(&mut host.ui, "personal");
    assert!(intents.contains(&Intent::StopMeter));
    assert!(host.ui.drafts.is_empty() && host.ui.channel().is_none());
    assert!(host.ui.prefs.developer && host.ui.theme == Theme::Light);
    assert_eq!(host.ui.providers.current, Some("personal"));
    assert_eq!(host.ui.providers.options.len(), 3);
}

#[test]
fn the_connection_settings_offer_the_providers_and_the_column_says_the_origin() {
    let mut host = Host::new();
    let text = host.painted().join(" ");
    assert!(
        text.contains("Simulación local · sin conexión real"),
        "{text}"
    );
    host.ui.providers = Providers {
        options: options(),
        current: Some("local"),
        ..Default::default()
    };
    host.click("me:settings");
    host.click("us:page=connection");
    let text = host.painted().join(" ");
    assert!(text.contains("Cambiar de proveedor"), "{text}");
    assert!(text.contains("Sesión personal · no oficial"));
    host.click("provider:use=personal");
    assert_eq!(host.ui.providers.confirming, Some("personal"));
}

#[test]
fn only_simulated_data_names_its_origin_under_the_channels() {
    let mut host = Host::new();
    let simulated = host.painted().join(" ");
    let origin = crate::providers::origin_of(&host.store);
    assert!(origin
        .as_deref()
        .is_some_and(|origin| simulated.contains(origin)));
    host.store.provider.as_mut().unwrap().kind = ProviderKind::Unofficial;
    assert_eq!(crate::providers::origin_of(&host.store), None);
    assert!(!host.painted().join(" ").contains("no oficial"));
}
