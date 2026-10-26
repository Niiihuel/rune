use super::*;
use rune_provider::{AuthenticationState, Capabilities, ProviderInfo, ProviderKind, Snapshot};

fn signed_out() -> Host {
    let mut host = Host::new();
    host.store = Store::new();
    host.store.provider = Some(ProviderInfo {
        name: "Servicio autorizado de prueba".into(),
        kind: ProviderKind::Official,
        capabilities: Capabilities {
            authentication: true,
            account_servers: true,
            ..Capabilities::default()
        },
        notice: "Identidad y servidores; sin conversaciones.".into(),
    });
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    host.sent.clear();
    host
}

#[test]
fn an_unofficial_login_page_opens_once_at_start_then_waits_for_a_click() {
    let mut host = Host::new();
    host.store = Store::new();
    host.store.provider = Some(ProviderInfo {
        name: "Discord".into(),
        kind: ProviderKind::Unofficial,
        capabilities: Capabilities {
            authentication: true,
            ..Capabilities::default()
        },
        notice: "Sesión personal de prueba.".into(),
    });
    host.sent.clear();
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    assert_eq!(
        host.sent,
        vec![Command::BeginSignIn],
        "no click needed at start"
    );
    host.event(Event::Authentication(AuthenticationState::AwaitingExternal));
    host.click("auth:cancel");
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    assert_eq!(
        host.sent,
        vec![Command::BeginSignIn, Command::CancelSignIn],
        "a cancel never reopens the window on its own"
    );
    host.click("auth:sign-in");
    assert_eq!(host.sent.last(), Some(&Command::BeginSignIn));
}

#[test]
fn sign_in_is_explicit_and_browser_launch_requires_a_click() {
    let mut host = signed_out();
    assert!(host.sent.is_empty());
    host.click("auth:sign-in");
    assert_eq!(host.sent, vec![Command::BeginSignIn]);
    let url = "https://example.org/authorize?state=opaque".to_owned();
    host.event(Event::Authentication(
        AuthenticationState::AwaitingBrowser { url: url.clone() },
    ));
    assert_eq!(host.sent, vec![Command::BeginSignIn]);
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "auth:browser".into(),
        },
        host.now,
    );
    assert_eq!(intents, vec![Intent::OpenUrl(url)]);
    host.click("auth:cancel");
    assert_eq!(host.sent.last(), Some(&Command::CancelSignIn));
}

#[test]
fn signed_in_without_conversations_opens_the_workspace_and_can_sign_out() {
    let mut host = signed_out();
    let user = host.provider.snapshot().me.unwrap();
    let name = user.shown_name().to_owned();
    host.event(Event::Ready(Box::new(Snapshot {
        me: Some(user.clone()),
        users: vec![user],
        ..Snapshot::default()
    })));
    host.event(Event::Authentication(AuthenticationState::SignedIn {
        scopes: vec!["identity.read".into(), "groups.read".into()],
    }));
    let text = host.painted().join("\n");
    assert!(text.contains(&name));
    assert!(text.contains("Tus conversaciones"));
    assert!(text.contains("No hay conversaciones cargadas"));
    assert!(!text.contains("Identidad verificada") && !text.contains("Permisos concedidos"));
    assert!(!text.contains("Conectando") && !text.contains("Discord"));
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id.starts_with("composer:")
            || region.id == "me:mute"
            || region.id.starts_with("auth:")));
    assert_eq!(host.tree.focused_id(), Some("home"));
    host.click("home");
    host.click("me:status");
    assert!(host.ui.status_menu);
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "pp:status-menu"));
    host.key(Key::Escape, false, false);
    host.click("me:settings");
    assert_eq!(host.ui.settings, Some(SettingsPage::Account));
    host.click("us:page=connection");
    let details = host.painted().join(" ");
    assert!(details.contains("identity.read") && details.contains("groups.read"));
    host.click("us:page=account");
    host.frame();
    host.tree.scroll_to_end(&SettingsPage::Account.scroll_id());
    host.click("us:sign-out");
    assert_eq!(host.sent.last(), Some(&Command::SignOut));
    // The provider owns the transition: opening settings or clicking logout
    // does not pretend that the account has already disconnected.
    assert!(matches!(
        host.store.authentication,
        AuthenticationState::SignedIn { .. }
    ));
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    assert!(host.store.me().is_none() && host.ui.settings.is_none());
    let frame = host.frame();
    assert!(frame
        .regions
        .iter()
        .any(|region| region.id == "auth:sign-in"));
    assert!(!frame
        .regions
        .iter()
        .any(|region| region.id == "me:settings"));
}

#[test]
fn sign_out_and_failed_authorization_clear_previous_account_ui() {
    let mut host = signed_out();
    for state in [
        AuthenticationState::SignedOut,
        AuthenticationState::Failed {
            reason: "Autorización cancelada".into(),
        },
    ] {
        host.ui
            .drafts
            .insert(ChannelId(123), "private draft".into());
        host.ui.channels.insert(None, ChannelId(123));
        host.ui.switcher = Some(Switcher::default());
        host.ui.profile = Some(rune_model::UserId(5));
        host.ui.status_menu = true;
        host.ui.theme = Theme::Light;
        let request_before_logout = host.ui.fresh_request();
        host.event(Event::Authentication(state));
        assert!(host.ui.fresh_request() > request_before_logout);
        assert!(host.ui.drafts.is_empty());
        assert!(host.ui.channels.is_empty());
        assert!(host.ui.switcher.is_none() && host.ui.profile.is_none());
        assert!(!host.ui.status_menu);
        assert_eq!(host.ui.theme, Theme::Light);
    }
    assert!(host.painted().join("\n").contains("Autorización cancelada"));
    host.click("auth:sign-in");
    assert_eq!(host.sent.last(), Some(&Command::BeginSignIn));
}

#[test]
fn auth_page_does_not_open_chat_shortcuts_and_mock_keeps_its_ui() {
    let mut host = signed_out();
    host.key(Key::K, false, true);
    host.key(Key::F, false, true);
    assert!(host.ui.switcher.is_none() && host.ui.search.is_none());
    let mut mock = Host::new();
    assert!(mock
        .frame()
        .regions
        .iter()
        .any(|region| region.id.starts_with("composer:")));
    assert!(!mock
        .frame()
        .regions
        .iter()
        .any(|region| region.id.starts_with("auth:")));
    assert!(mock.painted().join(" ").contains("sin conexión real"));
    assert!(update(
        &mut mock.ui,
        &mut mock.store,
        &UiAction::Clicked {
            id: "us:sign-out".into()
        },
        mock.now
    )
    .is_empty());
}

#[test]
fn signed_in_servers_are_navigable_without_fabricated_channels() {
    let mut host = signed_out();
    let sample = host.provider.snapshot();
    let servers = sample.servers.into_iter().take(2).collect::<Vec<_>>();
    let ids = servers.iter().map(|server| server.id).collect::<Vec<_>>();
    let user = sample.me.unwrap();
    host.event(Event::Ready(Box::new(Snapshot {
        me: Some(user.clone()),
        users: vec![user],
        servers,
        ..Snapshot::default()
    })));
    host.event(Event::Authentication(AuthenticationState::SignedIn {
        scopes: vec!["groups.read".into()],
    }));
    for id in ids {
        host.click(&format!("server:{id}"));
        assert_eq!(host.ui.server, Some(id));
        assert!(host.ui.channel().is_none());
        assert!(host.painted().join(" ").contains("Sin canales cargados"));
    }
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id.starts_with("channel:")));
    for id in [
        "auth:sign-in",
        "auth:browser",
        "auth:cancel",
        "auth:sign-out",
        "me:mute",
        "me:deafen",
        "pp:status=online",
    ] {
        let intents = update(
            &mut host.ui,
            &mut host.store,
            &UiAction::Clicked { id: id.into() },
            host.now,
        );
        assert!(intents.is_empty(), "stale action {id}");
    }
    assert!(!host.ui.muted && !host.ui.deafened && host.ui.profiles.timed.is_none());
}

#[test]
fn escape_cancels_pending_authorization_and_sign_in_fits_the_small_window() {
    let mut host = signed_out();
    let window = Size::new(720.0, 480.0);
    let mut context = host.context();
    context.window = window;
    host.tree.set_root(view(&host.ui, &host.store, &context));
    let frame = host.tree.layout(window);
    let sign_in = frame
        .regions
        .iter()
        .find(|region| region.id == "auth:sign-in")
        .unwrap();
    assert!(sign_in.rect.y >= 0.0 && sign_in.rect.y + sign_in.rect.height <= window.height);
    host.event(Event::Authentication(AuthenticationState::Exchanging));
    host.key(Key::Escape, false, false);
    assert_eq!(host.sent.last(), Some(&Command::CancelSignIn));
}

#[test]
fn signed_out_page_explains_unconfirmed_revocation_and_session_expiry() {
    let mut host = signed_out();
    host.event(Event::Failed {
        failure: rune_provider::Failure::Other,
        reason: "No se confirmó la revocación remota".into(),
        ambiguous: true,
    });
    assert!(host
        .painted()
        .join("\n")
        .contains("No se confirmó la revocación remota"));
    host.event(Event::Connection(ConnectionState::Disconnected {
        reason: "La autorización venció; inicia sesión de nuevo".into(),
    }));
    // The expiry remains explained after its temporary toast disappears.
    host.ui.notices.clear();
    assert!(host.painted().join("\n").contains("La autorización venció"));
}

#[test]
fn external_login_focuses_cancel_and_never_sends_credentials_or_browser_urls() {
    let mut host = signed_out();
    host.store.provider.as_mut().unwrap().kind = ProviderKind::Unofficial;
    host.event(Event::Authentication(AuthenticationState::AwaitingExternal));
    let text = host.painted().join("\n");
    assert!(text.contains("ventana de inicio de sesión"));
    assert!(!text.contains("Integración no oficial"));
    assert_eq!(host.tree.focused_id(), Some("auth:cancel"));
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "auth:browser" || region.kind == limn::HitKind::TextInput));
    host.click("auth:cancel");
    assert_eq!(host.sent.last(), Some(&Command::CancelSignIn));
    host.sent.clear();
    host.key(Key::Escape, false, false);
    assert_eq!(host.sent, vec![Command::CancelSignIn]);
}

#[test]
fn account_details_stay_in_settings_without_repeated_notices_in_the_workspace() {
    let mut host = signed_out();
    let provider = host.store.provider.as_mut().unwrap();
    provider.kind = ProviderKind::Unofficial;
    provider.capabilities.account_servers = false;
    let user = host.provider.snapshot().me.unwrap();
    host.event(Event::Ready(Box::new(Snapshot {
        me: Some(user.clone()),
        users: vec![user],
        ..Snapshot::default()
    })));
    host.event(Event::Authentication(AuthenticationState::SignedIn {
        scopes: Vec::new(),
    }));
    let text = host.painted().join("\n");
    assert!(!text.contains("Integración no oficial"));
    assert!(!text.contains("sin OAuth2"));
    assert!(!text.contains("Permisos concedidos"));
    assert!(!text.contains("Servidores · 0"));
    assert!(!text.contains("No hay servidores disponibles"));
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "auth:servers"));
    let notice = host.store.provider.as_ref().unwrap().notice.clone();
    assert!(!text.contains(&notice));
    host.click("me:settings");
    host.click("us:page=connection");
    let text = host.painted().join(" ");
    assert!(text.contains(&notice));
    assert!(text.contains("Servicio autorizado de prueba"));
}

#[test]
fn compact_auth_actions_fit_small_and_desktop_windows_without_repeated_notices() {
    for window in [Size::new(320.0, 640.0), Size::new(720.0, 480.0), WINDOW] {
        // Checking the account shows the opening screen instead (`splash`).
        for state in [
            AuthenticationState::SignedOut,
            AuthenticationState::AwaitingExternal,
        ] {
            let mut host = signed_out();
            host.store.provider.as_mut().unwrap().kind = ProviderKind::Unofficial;
            host.event(Event::Authentication(state.clone()));
            let mut context = host.context();
            context.window = window;
            host.tree.set_root(view(&host.ui, &host.store, &context));
            let frame = host.tree.layout(window);
            let action = if state == AuthenticationState::SignedOut {
                "auth:sign-in"
            } else {
                "auth:cancel"
            };
            let control = frame
                .regions
                .iter()
                .find(|region| region.id == action)
                .unwrap();
            assert!(control.rect.width >= (window.width - 96.0).min(432.0) - 2.0);
            assert!(control.rect.height >= 44.0);
            assert!(control.rect.x >= 0.0 && control.rect.x + control.rect.width <= window.width);
            assert!(
                control.rect.y >= 0.0 && control.rect.y + control.rect.height <= window.height,
                "{state:?}, {window:?}: {:?}",
                control.rect
            );
            for omitted in [
                "Integración no oficial",
                "Chat y llamadas aún no disponibles",
                "Detalles de conexión",
            ] {
                let text = frame
                    .commands
                    .iter()
                    .filter_map(|command| match command {
                        limn::DrawCommand::Text { text, .. } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                assert!(
                    !text.contains(omitted),
                    "unexpected {omitted}: {state:?}, {window:?}: {text}"
                );
            }
        }
    }
}

#[test]
fn authorization_has_accessible_hierarchy_details_and_keyboard_cancellation() {
    let mut host = signed_out();
    let notice = host.store.provider.as_ref().unwrap().notice.clone();
    let frame = host.frame();
    assert!(
        !frame.commands.iter().any(|command| matches!(command,
        limn::DrawCommand::Text { text, .. } if text == &notice)),
        "provider details must not cover the login card merely because its action received focus"
    );
    assert!(frame
        .access
        .iter()
        .any(|node| node.role == limn::Role::Heading));
    let sign_in = frame
        .access
        .iter()
        .find(|node| node.id == "auth:sign-in")
        .unwrap();
    assert_eq!(sign_in.description, None);
    host.event(Event::Authentication(
        AuthenticationState::AwaitingBrowser {
            url: "https://example.org/authorize".into(),
        },
    ));
    host.frame();
    assert_eq!(host.tree.focused_id(), Some("auth:browser"));
    host.key(Key::Tab, false, false);
    assert_eq!(host.tree.focused_id(), Some("auth:cancel"));
    host.key(Key::Enter, false, false);
    assert_eq!(host.sent, vec![Command::CancelSignIn]);
}

#[test]
fn a_long_sign_in_error_can_be_read_and_scrolled_to_retry() {
    let mut host = signed_out();
    let reason =
        "No pudimos confirmar el acceso. Revisa la conexión y vuelve a intentarlo. ".repeat(12);
    host.event(Event::Authentication(AuthenticationState::Failed {
        reason: reason.clone(),
    }));
    let window = Size::new(720.0, 480.0);
    let mut context = host.context();
    context.window = window;
    host.tree.set_root(view(&host.ui, &host.store, &context));
    let frame = host.tree.layout(window);
    let error = frame
        .access
        .iter()
        .find(|node| node.id == "auth:error")
        .unwrap();
    assert_eq!(error.role, limn::Role::Alert);
    assert_eq!(error.name, reason);
    host.tree.scroll_to_end("auth:page");
    let frame = host.tree.layout(window);
    let retry = frame
        .regions
        .iter()
        .find(|region| region.id == "auth:sign-in")
        .unwrap();
    assert!(retry.rect.y >= 0.0 && retry.rect.y + retry.rect.height <= window.height);
    assert!(frame
        .access
        .iter()
        .any(|node| node.id == "auth:sign-in" && node.name == "Volver a intentar"));
}

/// Match the host's focus handoff while exercising a real, constrained viewport.
fn auth_frame(host: &mut Host, window: Size) -> Frame {
    let mut context = host.context();
    context.window = window;
    host.tree.set_root(view(&host.ui, &host.store, &context));
    let mut frame = host.tree.layout(window);
    if let Some((id, _)) = host.focus.take() {
        host.tree.focus(&id);
        frame = host.tree.layout(window);
    }
    frame
}

#[track_caller]
fn fully_visible(rect: limn::Rect, window: Size) {
    assert!(
        rect.x >= 0.0
            && rect.y >= 0.0
            && rect.x + rect.width <= window.width + 0.5
            && rect.y + rect.height <= window.height + 0.5,
        "{rect:?} escapes {window:?}"
    );
}

#[test]
fn welcome_and_pending_states_keep_their_heading_and_actions_centered_in_every_theme() {
    for theme in [Theme::Onyx, Theme::Dark, Theme::Light] {
        for window in [Size::new(320.0, 640.0), Size::new(720.0, 480.0), WINDOW] {
            for state in [
                AuthenticationState::SignedOut,
                AuthenticationState::Failed {
                    reason: "La conexión se interrumpió.".into(),
                },
                AuthenticationState::AwaitingBrowser {
                    url: "https://example.org/authorize".into(),
                },
                // Exchanging and Unavailable show the opening screen.
                AuthenticationState::AwaitingExternal,
            ] {
                let mut host = signed_out();
                host.ui.theme = theme;
                host.event(Event::Authentication(state.clone()));
                let frame = auth_frame(&mut host, window);
                let title = frame
                    .access
                    .iter()
                    .find(|node| node.id == "auth:title" && node.role == limn::Role::Heading)
                    .expect("every authorization stage has a heading");
                fully_visible(title.rect, window);
                // Check the painted lines, not merely a full-width accessibility box:
                // a left-aligned title in that box would otherwise falsely pass.
                let title_lines = frame
                    .commands
                    .iter()
                    .filter_map(|command| match command {
                        limn::DrawCommand::Text {
                            position,
                            text,
                            font_size,
                            ..
                        } if (*font_size - 24.0).abs() < 0.1
                            && position.y >= title.rect.y
                            && position.y < title.rect.y + title.rect.height =>
                        {
                            Some((position, text, font_size))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert!(
                    !title_lines.is_empty(),
                    "missing title: {theme:?}, {state:?}"
                );
                for (position, text, font_size) in title_lines {
                    // Host::new installs this deterministic text measurer.
                    let width = text.chars().count() as f32 * font_size * 0.5;
                    assert!(
                        (position.x + width / 2.0 - window.width / 2.0).abs() <= 1.0,
                        "off-center heading {text:?} at {position:?}: {theme:?}, {state:?}, {window:?}"
                    );
                    assert!(position.x >= 24.0 && position.x + width <= window.width - 24.0);
                }
                let actions = frame
                    .regions
                    .iter()
                    .filter(|region| {
                        matches!(
                            region.id.as_str(),
                            "auth:sign-in" | "auth:browser" | "auth:cancel"
                        )
                    })
                    .collect::<Vec<_>>();
                let expected = match state {
                    AuthenticationState::Unavailable => 0,
                    AuthenticationState::AwaitingBrowser { .. } => 2,
                    _ => 1,
                };
                assert_eq!(actions.len(), expected, "{theme:?}, {state:?}, {window:?}");
                for action in actions {
                    let rect = action.rect;
                    fully_visible(rect, window);
                    assert!((rect.height - 44.0).abs() <= 1.0, "{rect:?}");
                    assert!((rect.width - (window.width - 96.0).min(432.0)).abs() <= 1.0);
                    assert!((rect.x + rect.width / 2.0 - window.width / 2.0).abs() <= 1.0);
                }
                if let AuthenticationState::Failed { reason } = &state {
                    let alert = frame
                        .access
                        .iter()
                        .find(|node| node.id == "auth:error")
                        .unwrap();
                    assert_eq!(alert.role, limn::Role::Alert);
                    assert_eq!(&alert.name, reason);
                    fully_visible(alert.rect, window);
                }
                if matches!(
                    state,
                    AuthenticationState::Unavailable | AuthenticationState::Exchanging
                ) {
                    assert!(frame.access.iter().any(|node| {
                        node.id == "auth:progress" && node.role == limn::Role::ProgressIndicator
                    }));
                }
                assert!(!frame
                    .regions
                    .iter()
                    .any(|region| region.kind == limn::HitKind::TextInput));
                assert!(
                    host.sent.is_empty(),
                    "layout must never start authorization"
                );
            }
        }
    }
}

#[test]
fn long_authorization_errors_keep_the_full_alert_and_scroll_to_a_complete_retry_button() {
    let reason =
        "No pudimos confirmar el acceso. Revisa la conexión y vuelve a intentarlo. ".repeat(24);
    for theme in [Theme::Onyx, Theme::Dark, Theme::Light] {
        for window in [Size::new(320.0, 640.0), Size::new(720.0, 480.0), WINDOW] {
            let mut host = signed_out();
            host.ui.theme = theme;
            host.event(Event::Authentication(AuthenticationState::Failed {
                reason: reason.clone(),
            }));
            let frame = auth_frame(&mut host, window);
            let alert = frame
                .access
                .iter()
                .find(|node| node.id == "auth:error")
                .unwrap();
            assert_eq!(
                alert.name, reason,
                "the accessible explanation must not be truncated"
            );
            assert_eq!(alert.role, limn::Role::Alert);
            host.tree.scroll_to_end("auth:page");
            let frame = host.tree.layout(window);
            let retry = frame
                .regions
                .iter()
                .find(|region| region.id == "auth:sign-in")
                .unwrap();
            fully_visible(retry.rect, window);
            assert!(
                (retry.rect.height - 44.0).abs() <= 1.0,
                "retry is clipped: {theme:?}, {window:?}"
            );
            assert!(frame.access.iter().any(|node| {
                node.id == "auth:sign-in" && node.name == "Volver a intentar" && node.actionable
            }));
            let point = Point::new(
                retry.rect.x + retry.rect.width / 2.0,
                retry.rect.y + retry.rect.height / 2.0,
            );
            assert_eq!(frame.hit_test(point), Some("auth:sign-in"));
        }
    }
}

#[test]
fn redesigned_welcome_preserves_keyboard_entry_and_pending_cancellation() {
    for theme in [Theme::Onyx, Theme::Dark, Theme::Light] {
        let mut host = signed_out();
        host.ui.theme = theme;
        host.frame();
        assert_eq!(host.tree.focused_id(), Some("auth:sign-in"));
        host.key(Key::Enter, false, false);
        assert_eq!(host.sent, vec![Command::BeginSignIn]);
        for state in [
            AuthenticationState::AwaitingExternal,
            AuthenticationState::Exchanging,
        ] {
            host.sent.clear();
            host.event(Event::Authentication(state));
            host.frame();
            assert_eq!(host.tree.focused_id(), Some("auth:cancel"));
            host.key(Key::Enter, false, false);
            assert_eq!(host.sent, vec![Command::CancelSignIn]);
            host.sent.clear();
            host.key(Key::Escape, false, false);
            assert_eq!(host.sent, vec![Command::CancelSignIn]);
        }
    }
}

fn unofficial() -> Host {
    let mut host = Host::new();
    host.store = Store::new();
    host.store.provider = Some(ProviderInfo {
        name: "Discord".into(),
        kind: ProviderKind::Unofficial,
        capabilities: Capabilities {
            authentication: true,
            ..Capabilities::default()
        },
        notice: "Sesión personal de prueba.".into(),
    });
    host.sent.clear();
    host
}

#[test]
fn restoring_a_saved_session_does_not_open_the_login_window() {
    let mut host = unofficial();
    host.event(Event::Authentication(AuthenticationState::Exchanging));
    assert!(host.sent.is_empty());
    // The account is checked behind RUNE's opening screen.
    assert!(host.painted().join(" ").contains("Iniciando sesión"));
    // A revoked saved session falls back to the login page, opened once.
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    assert_eq!(host.sent, vec![Command::BeginSignIn]);
}

#[test]
fn connection_settings_show_a_remembered_session_and_can_forget_it() {
    let mut host = unofficial();
    let user = host.provider.snapshot().me.unwrap();
    host.event(Event::Ready(Box::new(Snapshot {
        me: Some(user.clone()),
        users: vec![user],
        ..Snapshot::default()
    })));
    host.event(Event::Authentication(AuthenticationState::SignedIn {
        scopes: Vec::new(),
    }));
    host.click("me:settings");
    host.click("us:page=connection");
    let text = host.painted().join(" ");
    assert!(!text.contains("Sesión recordada") && !text.contains("Sesión no recordada"));
    host.event(Event::SessionRemembered(true));
    assert!(host.painted().join(" ").contains("Sesión recordada"));
    host.sent.clear();
    host.click("us:forget-session");
    assert_eq!(host.sent, vec![Command::ForgetSession]);
    host.event(Event::SessionRemembered(false));
    assert!(host.painted().join(" ").contains("Sesión no recordada"));
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "us:forget-session"));
    // Signing out forgets what the provider said about this account.
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    assert_eq!(host.store.remembered, None);
}

fn with_accounts(names: &[(u64, &str)]) -> Host {
    let mut host = unofficial();
    if let Some(info) = host.store.provider.as_mut() {
        info.capabilities.saved_accounts = true;
    }
    host.event(Event::RememberedAccounts(
        names
            .iter()
            .map(|(id, name)| rune_provider::RememberedAccount {
                id: rune_model::UserId(*id),
                name: (*name).into(),
                avatar: None,
            })
            .collect(),
    ));
    host
}

#[test]
fn remembered_accounts_are_listed_instead_of_opening_the_login_window() {
    let mut host = with_accounts(&[(42, "Rune Test"), (7, "")]);
    // A revoked last account falls back to the rest, not to the login window.
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    assert!(host.sent.is_empty());
    host.frame();
    assert_eq!(host.tree.focused_id(), Some("auth:account=42"));
    let text = host.painted().join(" ");
    assert!(text.contains("Continuar como Rune Test"));
    assert!(text.contains("Continuar como Cuenta guardada"));
    assert!(text.contains("Usar otra cuenta") && !text.contains("Iniciar sesión"));
    host.click("auth:account=7");
    assert_eq!(
        host.sent,
        vec![Command::UseAccount(Some(rune_model::UserId(7)))]
    );
    host.sent.clear();
    host.click("auth:other");
    assert_eq!(host.sent, vec![Command::UseAccount(None)]);
    // Forgetting asks first; cancelling sends nothing.
    host.sent.clear();
    host.click("auth:forget=42");
    assert!(host.painted().join(" ").contains("¿Olvidar a Rune Test"));
    host.click("auth:forget-no");
    assert!(host.sent.is_empty());
    assert!(!host.painted().join(" ").contains("¿Olvidar"));
    host.click("auth:forget=42");
    host.click("auth:forget-yes");
    assert_eq!(
        host.sent,
        vec![Command::ForgetAccount(rune_model::UserId(42))]
    );
    // An account that is not listed is never entered.
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "auth:account=99".into(),
        },
        host.now,
    );
    assert!(intents.is_empty());
    // Without accounts left, the usual sign-in comes back.
    host.event(Event::RememberedAccounts(Vec::new()));
    assert!(host.painted().join(" ").contains("Iniciar sesión"));
}

#[test]
fn a_saved_session_that_failed_offers_retry_and_the_other_accounts() {
    let mut host = with_accounts(&[(42, "Rune Test"), (7, "Otra")]);
    host.event(Event::Authentication(AuthenticationState::Failed {
        reason: "Discord no respondió.".into(),
    }));
    let text = host.painted().join(" ");
    assert!(text.contains("Volver a intentar") && text.contains("Continuar como Otra"));
    host.click("auth:sign-in");
    assert_eq!(host.sent, vec![Command::BeginSignIn]);
}

fn signed_in_with(host: &mut Host) -> rune_model::User {
    let user = host.provider.snapshot().me.unwrap();
    host.event(Event::Ready(Box::new(Snapshot {
        me: Some(user.clone()),
        users: vec![user.clone()],
        ..Snapshot::default()
    })));
    host.event(Event::Authentication(AuthenticationState::SignedIn {
        scopes: Vec::new(),
    }));
    host.click("me:settings");
    host.frame();
    host.tree.scroll_to_end(&SettingsPage::Account.scroll_id());
    user
}

#[test]
fn switching_from_settings_clears_the_account_and_keeps_the_list() {
    let mut host = with_accounts(&[]);
    let me = host.provider.snapshot().me.unwrap();
    host.event(Event::RememberedAccounts(vec![
        rune_provider::RememberedAccount {
            id: me.id,
            name: me.shown_name().into(),
            avatar: None,
        },
        rune_provider::RememberedAccount {
            id: rune_model::UserId(7),
            name: "Otra".into(),
            avatar: None,
        },
    ]));
    host.ui
        .drafts
        .insert(ChannelId(123), "private draft".into());
    let user = signed_in_with(&mut host);
    let text = host.painted().join(" ");
    assert!(text.contains("Cambiar de cuenta") && text.contains("Continuar como Otra"));
    assert!(!text.contains(&format!("Continuar como {}", user.shown_name())));
    host.sent.clear();
    host.click("us:account=7");
    assert_eq!(
        host.sent,
        vec![Command::UseAccount(Some(rune_model::UserId(7)))]
    );
    // The provider ends the session first: the usual account boundary.
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    assert!(host.ui.drafts.is_empty() && host.ui.settings.is_none());
    assert!(host.store.me().is_none());
    assert_eq!(host.store.accounts.len(), 2);
    assert_eq!(
        host.sent,
        vec![Command::UseAccount(Some(rune_model::UserId(7)))],
        "no login window on top of the switch"
    );
    host.event(Event::Authentication(AuthenticationState::Exchanging));
    // The account is checked behind RUNE's opening screen.
    assert!(host.painted().join(" ").contains("Iniciando sesión"));
}

#[test]
fn the_user_panel_offers_switching_accounts_through_settings() {
    let mut host = with_accounts(&[]);
    host.event(Event::RememberedAccounts(vec![
        rune_provider::RememberedAccount {
            id: rune_model::UserId(7),
            name: "Otra".into(),
            avatar: None,
        },
    ]));
    signed_in_with(&mut host);
    host.ui.settings = None;
    host.click("me:status");
    host.click("pp:switch-account");
    assert!(host.ui.settings.is_some() && !host.ui.status_menu);
    host.frame();
    host.tree.scroll_to_end(&SettingsPage::Account.scroll_id());
    assert!(host.painted().join(" ").contains("Continuar como Otra"));
}

#[test]
fn adding_an_account_from_settings_asks_for_a_new_login() {
    let mut host = with_accounts(&[]);
    signed_in_with(&mut host);
    host.sent.clear();
    host.click("us:add-account");
    assert_eq!(host.sent, vec![Command::UseAccount(None)]);
    // Not even the first sign-out of this run opens a second login window.
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    assert_eq!(host.sent, vec![Command::UseAccount(None)]);
}

#[test]
fn providers_without_saved_accounts_show_no_switcher() {
    let mut host = signed_out();
    signed_in_with(&mut host);
    assert!(!host.painted().join(" ").contains("Cambiar de cuenta"));
}

#[test]
fn a_remembered_account_shows_its_picture_once_it_arrives_else_its_initial() {
    let mut host = with_accounts(&[(42, "Rune Test")]);
    let picture = rune_model::RemoteImage {
        url: "https://cdn.example.test/avatars/42/a.png".into(),
        kind: Default::default(),
    };
    host.store.accounts[0].avatar = Some(picture.clone());
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    host.frame();
    // Asked for where the accounts are listed, like any other picture.
    assert!(wanted_images(&host.ui, &host.store, &host.tree).contains(&picture));
    let image = limn::RasterImage::new(
        "e".repeat(64),
        limn::RasterImageFormat::Png,
        3,
        std::sync::Arc::from("AAAA"),
        "Avatar",
    )
    .unwrap();
    host.ui.images.insert(picture, image);
    let frame = host.frame();
    assert!(frame
        .commands
        .iter()
        .any(|command| matches!(command, limn::DrawCommand::RasterImage { image, .. } if image.sha256() == "e".repeat(64))));
}
