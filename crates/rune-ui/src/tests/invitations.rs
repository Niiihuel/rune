use super::*;
use rune_model::ServerId;
use rune_provider::{
    AuthenticationState, ConnectionState, Failure, InvitationError, InvitationJoinError,
    InvitationPreview, InvitationResult, InvitationTarget,
};

fn target(key: &str) -> InvitationTarget {
    InvitationTarget {
        key: key.into(),
        url: format!("https://example.invalid/invite/{key}"),
    }
}

#[test]
fn reviewing_an_invitation_from_the_server_picker_requires_an_explicit_submit() {
    use rune_components::server_menu::ids;

    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = true;
    host.click(ids::SERVER_OPEN);
    assert!(host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "invitation:input-open"));
    host.click("invitation:input-open");
    host.frame();
    assert!(host.ui.server_menu.dialog.is_none());
    assert_eq!(host.tree.focused_id(), Some("invitation:input"));
    assert!(update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "invitation:input-submit".into(),
        },
        host.now
    )
    .is_empty());
    let sent = host.sent.len();
    host.type_text("https://example.invalid/invitation");
    assert_eq!(host.sent.len(), sent);
    assert!(host.ui.invitation.is_none());
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "invitation:input-submit".into(),
        },
        host.now,
    );
    assert_eq!(intents.len(), 1, "only local recognition is requested");
    assert_eq!(
        intents,
        vec![Intent::InspectInvitation(
            "https://example.invalid/invitation".into()
        )]
    );
    assert!(!intents.iter().any(|intent| matches!(
        intent,
        Intent::OpenUrl(_) | Intent::Command(_) | Intent::PreviewInvitation { .. }
    )));
    host.key(Key::Escape, false, false);
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "invitation:input"));
}

#[test]
fn pasted_links_need_adapter_recognition_and_never_fall_back_to_a_browser() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = true;
    host.click(rune_components::server_menu::ids::SERVER_OPEN);
    host.click("invitation:input-open");
    let input = "https://example.invalid/invite/first";
    host.type_text(input);
    let intents = invitation_input_result(&mut host.ui, &host.store, input, None);
    assert!(intents.is_empty());
    assert!(host.ui.invitation.is_none());
    assert!(host.painted().join(" ").contains("Pega el enlace completo"));
    assert!(host.ui.invitation_input.is_some());
    let intents = invitation_input_result(
        &mut host.ui,
        &host.store,
        "an older input",
        Some(target("stale")),
    );
    assert!(intents.is_empty());
    assert!(host.ui.invitation.is_none());

    let target = target("first");
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = false;
    assert!(
        invitation_input_result(&mut host.ui, &host.store, input, Some(target.clone())).is_empty()
    );
    assert!(host.ui.invitation_input.is_some());
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = true;
    let intents = invitation_input_result(&mut host.ui, &host.store, input, Some(target.clone()));
    assert!(host.ui.invitation_input.is_none());
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PreviewInvitation {
                request,
                target: recognized,
            } if recognized == &target => Some(*request),
            _ => None,
        })
        .unwrap();
    assert!(!intents
        .iter()
        .any(|intent| matches!(intent, Intent::OpenUrl(_) | Intent::Command(_))));
    host.run(intents);
    // A connection that cannot accept invitations offers no way to.
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_accept = false;
    complete(&mut host, request, target, ServerId(u64::MAX - 123));
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "invitation:accept"));
}

#[test]
fn invitation_input_is_bounded_closes_with_the_account_and_blocks_navigation() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = true;
    host.click(rune_components::server_menu::ids::SERVER_OPEN);
    host.click("invitation:input-open");
    let channel = host.ui.channel();
    assert!(navigate(&mut host.ui, &mut host.store, true).is_empty());
    assert_eq!(host.ui.channel(), channel);
    for key in [Key::K, Key::F] {
        host.key(key, false, true);
        assert!(host.ui.switcher.is_none() && host.ui.search.is_none());
    }
    host.type_text("https://example.invalid/invite/bounded");
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::InputChanged {
            id: "invitation:input".into(),
            value: "x".repeat(2049),
        },
        host.now,
    );
    assert_eq!(
        intents,
        vec![Intent::SetInput {
            id: "invitation:input".into(),
            value: "https://example.invalid/invite/bounded".into(),
        }]
    );
    assert!(!format!("{:?}", host.ui.invitation_input).contains("bounded"));
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    assert!(host.ui.invitation_input.is_none());
    assert!(invitation_input_result(
        &mut host.ui,
        &host.store,
        "https://example.invalid/invite/bounded",
        Some(target("late"))
    )
    .is_empty());
    assert!(host.ui.invitation.is_none());
}

#[test]
fn invitation_input_controls_fit_narrow_and_wide_windows() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = true;
    host.click(rune_components::server_menu::ids::SERVER_OPEN);
    host.click("invitation:input-open");
    host.type_text("https://example.invalid/invite/layout");
    for window in [Size::new(320.0, 480.0), Size::new(768.0, 600.0), WINDOW] {
        host.tree.set_root(view(
            &host.ui,
            &host.store,
            &ViewContext {
                window,
                ..host.context()
            },
        ));
        let frame = host.tree.layout(window);
        for id in [
            "invitation:input",
            "invitation:input-submit",
            "invitation:input-close",
        ] {
            let rect = frame
                .regions
                .iter()
                .find(|region| region.id == id)
                .unwrap()
                .rect;
            assert!(
                rect.x >= 0.0
                    && rect.y >= 0.0
                    && rect.x + rect.width <= window.width
                    && rect.y + rect.height <= window.height,
                "{id}: {rect:?} in {window:?}"
            );
        }
    }
}

#[test]
fn server_creation_denial_is_visible_and_revalidated_when_submitting() {
    use rune_components::server_menu::{ids, ServerForm};

    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .manage_servers = false;
    host.click(ids::SERVER_OPEN);
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == ids::SERVER_OWN));
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: ids::SERVER_OWN.into(),
        },
        host.now,
    );
    assert!(intents.is_empty());
    assert!(matches!(
        host.ui.server_menu.dialog,
        Some(Dialog::Server(None))
    ));

    host.ui.server_menu.dialog = Some(Dialog::Server(Some(ServerForm {
        name: "Servidor preparado antes de perder la capacidad".into(),
        accent: rune_model::Rgb(0),
    })));
    for action in [
        UiAction::Clicked {
            id: ids::SERVER_CREATE.into(),
        },
        UiAction::Submitted {
            id: ids::SERVER_NAME.into(),
            value: "Servidor obsoleto".into(),
        },
    ] {
        let intents = update(&mut host.ui, &mut host.store, &action, host.now);
        assert!(intents.is_empty());
        assert!(host.ui.server_menu.dialog.is_some());
    }
}

#[test]
fn a_stale_invitation_entry_cannot_replace_a_server_name_or_reopen_the_input() {
    use rune_components::server_menu::ids;

    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = true;
    host.click(ids::SERVER_OPEN);
    host.click(ids::SERVER_OWN);
    host.type_text(" preparado");
    let before = host.ui.server_menu.dialog.clone();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: ids::SERVER_REVIEW_INVITATION.into(),
        },
        host.now,
    );
    assert!(intents.is_empty());
    assert_eq!(host.ui.server_menu.dialog, before);
    assert!(host.ui.invitation_input.is_none());

    host.click(ids::SERVER_BACK);
    host.click(ids::SERVER_REVIEW_INVITATION);
    host.type_text("https://example.invalid/invite/kept");
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: ids::SERVER_REVIEW_INVITATION.into(),
        },
        host.now,
    );
    assert!(intents.is_empty());
    assert_eq!(
        host.tree.input_value("invitation:input"),
        Some("https://example.invalid/invite/kept")
    );
}

fn begin(host: &mut Host, key: &str) -> (u64, InvitationTarget) {
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = true;
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_accept = false;
    let target = target(key);
    let intents = open_invitation(&mut host.ui, &host.store, target.clone());
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PreviewInvitation { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap();
    assert!(!intents
        .iter()
        .any(|intent| matches!(intent, Intent::OpenUrl(_) | Intent::Command(_))));
    host.run(intents);
    host.frame();
    (request, target)
}

fn complete(host: &mut Host, request: u64, target: InvitationTarget, server_id: ServerId) {
    let intents = invitation_result(
        &mut host.ui,
        &host.store,
        InvitationResult {
            request,
            target,
            result: Ok(InvitationPreview {
                destination: rune_provider::InvitationDestination::Server(server_id),
                name: "Destino de la invitación".into(),
                description: None,
                image: None,
                members: Some(42),
                online: Some(3),
                expires_at: None,
                event: None,
            }),
        },
    );
    host.run(intents);
    host.frame();
}

#[test]
fn preview_shows_current_account_and_never_joins_or_opens_the_browser() {
    let mut host = Host::new();
    host.open_channel("general");
    let account = host.store.me_user().unwrap().name.clone();
    let servers = host.store.servers().len();
    let (request, target) = begin(&mut host, "first");
    complete(&mut host, request, target.clone(), ServerId(u64::MAX - 100));
    let selected = host.ui.channel();
    assert!(navigate(&mut host.ui, &mut host.store, false).is_empty());
    assert_eq!(host.ui.channel(), selected);
    let painted = host.painted().join(" ");
    assert!(painted.contains(&format!("@{account}")), "{painted}");
    assert!(painted.contains("Destino de la invitación"));
    assert!(painted.contains("Esta conexión no permite aceptar invitaciones"));
    assert_eq!(host.store.servers().len(), servers);
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "invitation:open" || region.id == "invitation:accept"));
    let copy = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "invitation:copy".into(),
        },
        host.now,
    );
    assert_eq!(copy, vec![Intent::Copy(target.url)]);
    let stale = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "invitation:accept".into(),
        },
        host.now,
    );
    assert!(stale.is_empty());
}

#[test]
fn existing_server_opens_in_rune_without_joining() {
    let mut host = Host::new();
    let server = host.store.servers()[1].id;
    let (request, target) = begin(&mut host, "existing");
    complete(&mut host, request, target, server);
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "invitation:open".into(),
        },
        host.now,
    );
    assert!(!intents
        .iter()
        .any(|intent| matches!(intent, Intent::OpenUrl(_) | Intent::Command(_))));
    assert_eq!(host.ui.server, Some(server));
    assert!(host.ui.invitation.is_none());
}

#[test]
fn closed_replaced_and_account_stale_results_cannot_reopen_a_preview() {
    let mut host = Host::new();
    host.open_channel("general");
    let (old, old_target) = begin(&mut host, "old");
    let (current, current_target) = begin(&mut host, "current");
    complete(&mut host, old, old_target, ServerId(999));
    assert!(!host
        .painted()
        .join(" ")
        .contains("Destino de la invitación"));
    host.key(Key::Escape, false, false);
    assert!(host.ui.invitation.is_none());
    complete(&mut host, current, current_target, ServerId(999));
    assert!(host.ui.invitation.is_none());
    let (request, target) = begin(&mut host, "account");
    let mut snapshot = host.provider.snapshot();
    let mut other = snapshot.me.take().unwrap();
    other.id = rune_model::UserId(u64::MAX - 1);
    other.name = "otra_cuenta_sintetica".into();
    snapshot.me = Some(other);
    host.store.apply(Event::Ready(Box::new(snapshot)), host.now);
    complete(&mut host, request, target, ServerId(999));
    assert!(host.ui.invitation.is_none());
}

#[test]
fn unsupported_and_rate_limited_previews_do_not_offer_a_write_or_retry_loop() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_preview = false;
    let intents = open_invitation(&mut host.ui, &host.store, target("unsupported"));
    assert!(!intents.iter().any(|intent| matches!(
        intent,
        Intent::PreviewInvitation { .. } | Intent::OpenUrl(_)
    )));
    let (request, target) = begin(&mut host, "limited");
    invitation_result(
        &mut host.ui,
        &host.store,
        InvitationResult {
            request,
            target,
            result: Err(InvitationError::RateLimited {
                retry_after_secs: 60,
            }),
        },
    );
    let frame = host.frame();
    assert!(!frame
        .regions
        .iter()
        .any(|region| region.id == "invitation:retry"));
    let retry = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "invitation:retry".into(),
        },
        host.now,
    );
    assert!(retry.is_empty());
}

#[test]
fn preview_cooldown_expires_into_one_explicit_retry_without_joining() {
    let mut host = Host::new();
    let (request, target) = begin(&mut host, "retry-after");
    invitation_result(
        &mut host.ui,
        &host.store,
        InvitationResult {
            request,
            target: target.clone(),
            result: Err(InvitationError::RateLimited {
                retry_after_secs: 60,
            }),
        },
    );
    assert!(host
        .painted()
        .join(" ")
        .contains("Puedes volver a consultar en"));
    assert!(crate::invitations::next_expiry(&host.ui, host.now).is_some());
    let action = UiAction::Clicked {
        id: "invitation:retry".into(),
    };
    assert!(update(&mut host.ui, &mut host.store, &action, host.now).is_empty());
    host.now += Duration::from_secs(61);
    host.ui.expire(host.now);
    assert!(host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "invitation:retry"));
    let intents = update(&mut host.ui, &mut host.store, &action, host.now);
    assert_eq!(intents.iter().filter(|intent| matches!(intent, Intent::PreviewInvitation { request: next, target: next_target } if *next != request && *next_target == target)).count(), 1);
    assert!(intents
        .iter()
        .all(|intent| !matches!(intent, Intent::Command(_) | Intent::OpenUrl(_))));
    assert!(update(&mut host.ui, &mut host.store, &action, host.now).is_empty());
}

#[test]
fn invitation_expiration_is_explicit_and_a_stale_accept_cannot_join() {
    for expires_at in [None, Some(Timestamp(1)), Some(Timestamp(1_893_546_245_000))] {
        let mut host = Host::new();
        let (request, target) = begin(&mut host, "expiration");
        invitation_result(
            &mut host.ui,
            &host.store,
            InvitationResult {
                request,
                target: target.clone(),
                result: Ok(InvitationPreview {
                    destination: rune_provider::InvitationDestination::Server(ServerId(
                        u64::MAX - 100,
                    )),
                    name: "Servidor de prueba".into(),
                    description: None,
                    image: None,
                    members: None,
                    online: None,
                    expires_at,
                    event: None,
                }),
            },
        );
        host.store
            .provider
            .as_mut()
            .unwrap()
            .capabilities
            .invitation_accept = true;
        let painted = host.painted().join(" ");
        match expires_at {
            None => assert!(
                painted.contains("Caducidad no informada por el servicio."),
                "{painted}"
            ),
            Some(Timestamp(1)) => {
                assert!(painted.contains("Esta invitación venció."), "{painted}");
                assert!(action(&mut host, "invitation:accept").is_empty());
                let retry = action(&mut host, "invitation:retry");
                assert!(retry.iter().any(|intent| matches!(intent, Intent::PreviewInvitation { target: next, .. } if *next == target)));
                assert!(retry
                    .iter()
                    .all(|intent| !matches!(intent, Intent::Command(_) | Intent::OpenUrl(_))));
            }
            Some(_) => {
                assert!(painted.contains("Vence: "), "{painted}");
                assert!(crate::invitations::next_expiry(&host.ui, host.now).is_some());
            }
        }
    }
}

#[test]
fn invitation_acceptance_deadline_cannot_outlive_the_reported_expiration() {
    for remaining in [5_000, 120_000, i64::MAX] {
        let mut host = Host::new();
        let (request, target) = begin(&mut host, "short-expiration");
        let wall = crate::server_menu::wall_now();
        let expires_at = Timestamp(wall.0.saturating_add(remaining));
        invitation_result(
            &mut host.ui,
            &host.store,
            InvitationResult {
                request,
                target,
                result: Ok(InvitationPreview {
                    destination: rune_provider::InvitationDestination::Server(ServerId(
                        u64::MAX - 100,
                    )),
                    name: "Destino sintético".into(),
                    description: None,
                    image: None,
                    members: None,
                    online: None,
                    expires_at: Some(expires_at),
                    event: None,
                }),
            },
        );
        host.store
            .provider
            .as_mut()
            .unwrap()
            .capabilities
            .invitation_accept = true;
        let intents = action(&mut host, "invitation:accept");
        let [Intent::Command(Command::AcceptInvitation { expires_at, .. })] = intents.as_slice()
        else {
            panic!("one explicit synthetic acceptance expected");
        };
        let delay = expires_at.saturating_duration_since(host.now);
        assert!(delay > Duration::ZERO);
        assert!(
            delay <= Duration::from_millis(remaining as u64).min(Duration::from_secs(60)),
            "the queue deadline must respect the preview expiration: {delay:?}"
        );
    }
}

#[test]
fn approximate_counts_are_explained_without_a_symbol_or_large_empty_body() {
    let mut host = Host::new();
    let (request, target) = begin(&mut host, "counts");
    complete(&mut host, request, target, ServerId(u64::MAX - 100));
    let frame = host.frame();
    let counts = frame
        .access
        .iter()
        .find(|node| node.id == "invitation:counts")
        .unwrap();
    assert_eq!(counts.name, "42 miembros · 3 en línea");
    assert_eq!(
        counts.description.as_deref(),
        Some("Cantidades aproximadas")
    );
    assert!(!host.painted().join(" ").contains('≈'));
    let dialog = frame
        .access
        .iter()
        .find(|node| node.id == "invitation:close:dialog")
        .unwrap();
    assert!(dialog.rect.height < 360.0, "{:?}", dialog.rect);
    let copy = frame
        .regions
        .iter()
        .find(|r| r.id == "invitation:copy")
        .unwrap();
    let close = frame
        .regions
        .iter()
        .rev()
        .find(|r| r.id == "invitation:close")
        .unwrap();
    assert!((copy.rect.y - close.rect.y).abs() < 0.1);
    assert!(copy.rect.x + copy.rect.width <= close.rect.x);
    assert!(copy.rect.y > counts.rect.y + counts.rect.height);
}

#[test]
fn invitation_footer_stays_visible_on_short_and_narrow_windows() {
    for size in [Size::new(320.0, 300.0), Size::new(480.0, 280.0), WINDOW] {
        for member in [false, true] {
            let mut host = Host::new();
            let server_id = if member {
                host.store.servers()[0].id
            } else {
                ServerId(u64::MAX - 100)
            };
            let (request, target) = begin(&mut host, "long-description");
            invitation_result(
                &mut host.ui,
                &host.store,
                InvitationResult {
                    request,
                    target,
                    result: Ok(InvitationPreview {
                        destination: rune_provider::InvitationDestination::Server(server_id),
                        name: "Un servidor con un nombre bastante largo".into(),
                        description: Some("Descripción extensa del servidor. ".repeat(100)),
                        image: None,
                        members: Some(42),
                        online: Some(3),
                        expires_at: None,
                        event: None,
                    }),
                },
            );
            let mut context = host.context();
            context.window = size;
            host.tree.set_root(view(&host.ui, &host.store, &context));
            let frame = host.tree.layout(size);
            for id in ["invitation:copy", "invitation:close"]
                .into_iter()
                .chain(member.then_some("invitation:open"))
            {
                let rect = frame
                    .regions
                    .iter()
                    .rev()
                    .find(|r| r.id == id)
                    .unwrap()
                    .rect;
                assert!(rect.x >= 0.0 && rect.y >= 0.0, "{id}: {rect:?} in {size:?}");
                assert!(
                    rect.x + rect.width <= size.width,
                    "{id}: {rect:?} in {size:?}"
                );
                assert!(
                    rect.y + rect.height <= size.height,
                    "{id}: {rect:?} in {size:?}"
                );
                assert!(rect.height >= 28.0, "{id}: {rect:?} in {size:?}");
            }
        }
    }
}

#[test]
fn server_removed_after_preview_cannot_be_opened_by_a_stale_action() {
    let mut host = Host::new();
    let server = host.store.servers()[1].id;
    let (request, target) = begin(&mut host, "removed");
    complete(&mut host, request, target, server);
    host.event(Event::ServerRemoved {
        server,
        reason: None,
    });
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "invitation:open".into(),
        },
        host.now,
    );
    assert!(intents.is_empty());
    assert!(host.ui.invitation.is_some());
    assert!(!host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "invitation:open"));
}

fn joinable(host: &mut Host, key: &str, server: ServerId) -> InvitationTarget {
    let (request, target) = begin(host, key);
    complete(host, request, target.clone(), server);
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_accept = true;
    target
}

fn action(host: &mut Host, id: &str) -> Vec<Intent> {
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked { id: id.into() },
        host.now,
    )
}

fn accept(host: &mut Host, target: &InvitationTarget, server: ServerId) -> u64 {
    let intents = action(host, "invitation:accept");
    let [Intent::Command(Command::AcceptInvitation {
        request,
        target: sent,
        destination: to,
        account,
        expires_at,
    })] = intents.as_slice()
    else {
        panic!("one explicit acceptance expected: {intents:?}");
    };
    assert_eq!(sent, target);
    assert_eq!(*to, rune_provider::InvitationDestination::Server(server));
    assert_eq!(Some(*account), host.store.me());
    assert_eq!(*expires_at, host.now + Duration::from_secs(60));
    *request
}

fn acknowledge(
    host: &mut Host,
    request: u64,
    server: ServerId,
    result: Result<(), InvitationJoinError>,
) {
    host.event(Event::InvitationAcceptance {
        request,
        destination: rune_provider::InvitationDestination::Server(server),
        result,
    });
}

fn deadline(host: &mut Host, request: u64, server: ServerId) {
    host.event(Event::Failed {
        failure: Failure::Invitation {
            request,
            destination: rune_provider::InvitationDestination::Server(server),
        },
        reason: "No se recibió una respuesta a tiempo".into(),
        ambiguous: true,
    });
}

fn membership(host: &mut Host, server_id: ServerId, channel_id: ChannelId) {
    let mut server = host.store.servers()[0].clone();
    server.id = server_id;
    server.name = "Servidor sintético aceptado".into();
    let mut channel = host
        .store
        .channel(host.channel_named("general"))
        .unwrap()
        .clone();
    channel.id = channel_id;
    channel.server = Some(server_id);
    channel.parent = None;
    host.event(Event::ServerChanged(server));
    host.event(Event::ChannelChanged(channel));
}

#[test]
fn explicit_accept_is_single_account_bound_and_waits_for_real_membership_in_either_order() {
    for gateway_first in [false, true] {
        let mut host = Host::new();
        let server = ServerId(90_001);
        let channel = ChannelId(90_002);
        let before = host.ui.server;
        let target = joinable(&mut host, "join", server);
        let request = accept(&mut host, &target, server);
        assert!(
            action(&mut host, "invitation:accept").is_empty(),
            "double activation cannot send twice"
        );
        assert!(host.painted().join(" ").contains("Entrando"));
        assert!(
            host.store.server(server).is_none(),
            "submission never invents membership"
        );
        if gateway_first {
            membership(&mut host, server, channel);
            assert_eq!(
                host.ui.server, before,
                "membership alone does not hijack navigation"
            );
            acknowledge(&mut host, request, server, Ok(()));
        } else {
            acknowledge(&mut host, request, server, Ok(()));
            assert!(
                host.store.server(server).is_none(),
                "ACK never inserts a server"
            );
            assert_eq!(host.ui.server, before);
            assert!(host.painted().join(" ").contains("Esperando acceso"));
            membership(&mut host, server, channel);
        }
        assert_eq!(host.ui.server, Some(server));
        assert_eq!(host.ui.channel(), Some(channel));
        assert!(host.ui.invitation.is_none());
        host.ui.server = before;
        acknowledge(&mut host, request, server, Ok(()));
        assert_eq!(
            host.ui.server, before,
            "duplicate ACK cannot navigate again"
        );
    }
}

/// The modal of `key`, its preview leading to `destination`, with accepting allowed.
fn joinable_to(
    host: &mut Host,
    key: &str,
    destination: rune_provider::InvitationDestination,
) -> InvitationTarget {
    let (request, target) = begin(host, key);
    let intents = invitation_result(
        &mut host.ui,
        &host.store,
        InvitationResult {
            request,
            target: target.clone(),
            result: Ok(InvitationPreview {
                destination,
                name: String::new(),
                description: None,
                image: None,
                members: Some(3),
                online: None,
                expires_at: None,
                event: None,
            }),
        },
    );
    host.run(intents);
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .invitation_accept = true;
    target
}

#[test]
fn a_group_and_a_friendship_are_accepted_once_and_opened_when_they_arrive() {
    use rune_provider::InvitationDestination::{Friend, Group, Guest};
    // A group: the ACK waits for the group itself, then opens it.
    let mut host = Host::new();
    let group = ChannelId(90_010);
    let target = joinable_to(&mut host, "group", Group(group));
    let painted = host.painted().join(" ");
    assert!(painted.contains("Invitación a un grupo") && painted.contains("Grupo sin nombre"));
    let intents = action(&mut host, "invitation:accept");
    let [Intent::Command(Command::AcceptInvitation {
        request,
        destination,
        ..
    })] = intents.as_slice()
    else {
        panic!("one explicit acceptance expected: {intents:?}");
    };
    let request = *request;
    assert_eq!(*destination, Group(group));
    assert!(action(&mut host, "invitation:accept").is_empty());
    host.event(Event::InvitationAcceptance {
        request,
        destination: Group(group),
        result: Ok(()),
    });
    assert!(host.painted().join(" ").contains("Esperando el grupo"));
    let mut channel = host
        .store
        .directs()
        .iter()
        .find(|channel| channel.kind == rune_model::ChannelKind::Group)
        .map(|channel| (*channel).clone())
        .unwrap();
    channel.id = group;
    host.event(Event::ChannelChanged(channel));
    assert_eq!(host.ui.channel(), Some(group));
    assert!(host.ui.invitation.is_none());
    let _ = target;

    // A friendship: once it arrives, the modal says so and offers the
    // conversation; opening one is never automatic.
    let mut host = Host::new();
    let stranger = rune_model::UserId(10 + 5);
    assert!(host.store.relationship(stranger).is_none());
    joinable_to(&mut host, "friend", Friend(stranger));
    assert!(host.painted().join(" ").contains("Invitación de amistad"));
    let intents = action(&mut host, "invitation:accept");
    let [Intent::Command(Command::AcceptInvitation { request, .. })] = intents.as_slice() else {
        panic!("one explicit acceptance expected: {intents:?}");
    };
    host.event(Event::InvitationAcceptance {
        request: *request,
        destination: Friend(stranger),
        result: Ok(()),
    });
    host.event(Event::RelationshipChanged(rune_model::Relationship {
        user: stranger,
        kind: rune_model::RelationshipKind::Friend,
        since: None,
        nickname: None,
        spam: false,
        ignored: false,
    }));
    let painted = host.painted().join(" ");
    assert!(painted.contains("Ya son amigos.") && painted.contains("Enviar mensaje"));
    assert!(host.ui.invitation.is_some());
    let open = action(&mut host, "invitation:open");
    assert!(
        open.iter()
            .any(|intent| matches!(intent, Intent::Command(Command::OpenDirect { user, .. }) if *user == stranger)),
        "{open:?}"
    );

    // A guest: the modal says what a guest gets; without its capability,
    // it says why it cannot be accepted here.
    let mut host = Host::new();
    let guest = Guest {
        server: ServerId(90_020),
        channel: ChannelId(90_021),
    };
    joinable_to(&mut host, "guest", guest);
    let painted = host.painted().join(" ");
    assert!(painted.contains("Invitación como invitado"));
    assert!(painted.contains("sólo a un canal de voz"));
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .guest_invitations = false;
    assert!(action(&mut host, "invitation:accept").is_empty());
    assert!(host
        .painted()
        .join(" ")
        .contains("Esta conexión no permite entrar como invitado"));
}

#[test]
fn accept_rechecks_capability_connection_cache_and_session_at_activation() {
    for denied in 0..4 {
        let mut host = Host::new();
        joinable(&mut host, "guarded", ServerId(90_001));
        match denied {
            0 => {
                host.store
                    .provider
                    .as_mut()
                    .unwrap()
                    .capabilities
                    .invitation_accept = false
            }
            1 => {
                host.store.connection = ConnectionState::Disconnected {
                    reason: "Prueba".into(),
                }
            }
            2 => host.store.from_cache = true,
            _ => {
                host.store
                    .provider
                    .as_mut()
                    .unwrap()
                    .capabilities
                    .authentication = true;
                host.store.authentication = AuthenticationState::SignedOut;
            }
        }
        assert!(action(&mut host, "invitation:accept").is_empty());
    }
}

#[test]
fn closing_and_reopening_keeps_pending_write_but_never_cancels_or_renavigates_it() {
    let mut host = Host::new();
    let server = ServerId(90_001);
    let before = host.ui.server;
    let target = joinable(&mut host, "pending", server);
    let request = accept(&mut host, &target, server);
    let close = action(&mut host, "invitation:close");
    assert!(
        !close
            .iter()
            .any(|intent| matches!(intent, Intent::Command(_))),
        "closing cannot cancel an accepted write"
    );
    joinable(&mut host, "pending", server);
    assert!(action(&mut host, "invitation:accept").is_empty());
    acknowledge(&mut host, request, server, Ok(()));
    membership(&mut host, server, ChannelId(90_002));
    assert_eq!(
        host.ui.server, before,
        "closing disarmed navigation even after reopening"
    );
    assert!(host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "invitation:open"));
    let open = action(&mut host, "invitation:open");
    assert!(!open
        .iter()
        .any(|intent| matches!(intent, Intent::Command(Command::AcceptInvitation { .. }))));
    assert_eq!(host.ui.server, Some(server));
}

#[test]
fn definitive_failure_requires_an_explicit_retry_and_ambiguous_or_verification_errors_never_resend()
{
    let mut host = Host::new();
    let server = ServerId(90_001);
    let target = joinable(&mut host, "retry", server);
    let first = accept(&mut host, &target, server);
    host.event(Event::Failed {
        failure: Failure::Invitation {
            request: first,
            destination: rune_provider::InvitationDestination::Server(server),
        },
        reason: "Cola llena".into(),
        ambiguous: false,
    });
    assert!(host.ui.notices.is_empty(), "the modal owns this failure");
    assert!(host.painted().join(" ").contains("No se pudo enviar"));
    let second = accept(&mut host, &target, server);
    assert_ne!(first, second);
    acknowledge(&mut host, first, server, Ok(()));
    assert!(
        host.painted().join(" ").contains("Entrando"),
        "old ACK ignored"
    );
    deadline(&mut host, second, server);
    assert!(action(&mut host, "invitation:accept").is_empty());
    joinable(&mut host, "retry", server);
    assert!(
        action(&mut host, "invitation:accept").is_empty(),
        "reopening cannot reset uncertainty"
    );
    // A definitive response may still resolve a timed-out request.
    acknowledge(
        &mut host,
        second,
        server,
        Err(InvitationJoinError::VerificationRequired),
    );
    assert!(host
        .painted()
        .join(" ")
        .contains("requiere una verificación"));
    assert!(action(&mut host, "invitation:accept").is_empty());
}

fn verify(host: &mut Host, request: u64, server: ServerId) {
    let intents = action(host, "invitation:verify");
    let [Intent::Command(Command::VerifyInvitation {
        request: sent,
        destination: to,
        account,
        expires_at,
    })] = intents.as_slice()
    else {
        panic!("one explicit verification expected: {intents:?}");
    };
    assert_eq!(
        (*sent, *to),
        (
            request,
            rune_provider::InvitationDestination::Server(server)
        ),
        "the same attempt, never a new one"
    );
    assert_eq!(Some(*account), host.store.me());
    assert_eq!(*expires_at, host.now + Duration::from_secs(360));
}

#[test]
fn a_challenge_is_verified_only_by_an_explicit_click_on_the_same_attempt() {
    let mut host = Host::new();
    let server = ServerId(90_001);
    let target = joinable(&mut host, "challenge", server);
    let request = accept(&mut host, &target, server);
    assert!(
        action(&mut host, "invitation:verify").is_empty(),
        "nothing to verify before the service asks"
    );
    acknowledge(
        &mut host,
        request,
        server,
        Err(InvitationJoinError::ChallengeRequired),
    );
    let service = host.store.provider.as_ref().unwrap().name.clone();
    let painted = host.painted().join(" ");
    assert!(
        painted.contains(&format!("{service} pide una verificación para entrar.")),
        "{painted}"
    );
    assert!(painted.contains("Cerrarla no envía nada"));
    assert!(host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "invitation:verify"));
    assert!(
        action(&mut host, "invitation:accept").is_empty(),
        "a challenge is never answered by a plain repeat"
    );
    verify(&mut host, request, server);
    assert!(
        action(&mut host, "invitation:verify").is_empty(),
        "one window at a time"
    );
    assert!(host
        .painted()
        .join(" ")
        .contains("Completa la verificación"));
    // Solving takes longer than the usual wait without becoming uncertain.
    host.now += Duration::from_secs(120);
    host.ui.expire(host.now);
    assert!(host
        .painted()
        .join(" ")
        .contains("Completa la verificación"));
    acknowledge(&mut host, request, server, Ok(()));
    assert!(host.painted().join(" ").contains("Esperando acceso"));
    membership(&mut host, server, ChannelId(90_002));
    assert_eq!(host.ui.server, Some(server));
}

#[test]
fn closing_the_challenge_window_sends_nothing_and_only_offers_a_new_attempt() {
    let mut host = Host::new();
    let server = ServerId(90_001);
    let target = joinable(&mut host, "closed-challenge", server);
    let request = accept(&mut host, &target, server);
    acknowledge(
        &mut host,
        request,
        server,
        Err(InvitationJoinError::ChallengeRequired),
    );
    verify(&mut host, request, server);
    acknowledge(
        &mut host,
        request,
        server,
        Err(InvitationJoinError::Unavailable),
    );
    assert!(host.painted().join(" ").contains("No se pudo enviar"));
    assert!(action(&mut host, "invitation:verify").is_empty());
    assert_ne!(accept(&mut host, &target, server), request);
    // A window left open past its limit becomes uncertain, never resent.
    let mut host = Host::new();
    let target = joinable(&mut host, "slow-challenge", server);
    let request = accept(&mut host, &target, server);
    acknowledge(
        &mut host,
        request,
        server,
        Err(InvitationJoinError::ChallengeRequired),
    );
    verify(&mut host, request, server);
    assert_eq!(
        host.ui.next_expiry(host.now),
        Some(Duration::from_secs(360))
    );
    host.now += Duration::from_secs(360);
    assert!(host.ui.expire(host.now));
    assert!(action(&mut host, "invitation:accept").is_empty());
    assert!(action(&mut host, "invitation:verify").is_empty());
}

#[test]
fn rate_limit_survives_reopen_and_expiry_only_enables_an_explicit_attempt() {
    let mut host = Host::new();
    let server = ServerId(90_001);
    let target = joinable(&mut host, "limited-join", server);
    let request = accept(&mut host, &target, server);
    acknowledge(
        &mut host,
        request,
        server,
        Err(InvitationJoinError::RateLimited {
            retry_after_secs: 10,
        }),
    );
    action(&mut host, "invitation:close");
    joinable(&mut host, "limited-join", server);
    assert!(action(&mut host, "invitation:accept").is_empty());
    assert_eq!(host.ui.next_expiry(host.now), Some(Duration::from_secs(10)));
    host.now += Duration::from_secs(10);
    assert!(host.ui.expire(host.now));
    assert_ne!(accept(&mut host, &target, server), request);
}

#[test]
fn membership_wait_expires_without_reposting_or_late_navigation() {
    let mut host = Host::new();
    let before = host.ui.server;
    let server = ServerId(90_001);
    let target = joinable(&mut host, "slow-membership", server);
    let request = accept(&mut host, &target, server);
    acknowledge(&mut host, request, server, Ok(()));
    assert_eq!(host.ui.next_expiry(host.now), Some(Duration::from_secs(60)));
    host.now += Duration::from_secs(60);
    assert!(host.ui.expire(host.now));
    assert!(action(&mut host, "invitation:accept").is_empty());
    assert!(host.painted().join(" ").contains("No se pudo confirmar"));
    membership(&mut host, server, ChannelId(90_002));
    assert_eq!(host.ui.server, before);
    assert!(host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == "invitation:open"));
}

#[test]
fn accepted_server_without_visible_channels_can_be_opened_without_inventing_one() {
    let mut host = Host::new();
    let before = host.ui.server;
    let server_id = ServerId(90_001);
    let target = joinable(&mut host, "empty", server_id);
    let request = accept(&mut host, &target, server_id);
    acknowledge(&mut host, request, server_id, Ok(()));
    let mut server = host.store.servers()[0].clone();
    server.id = server_id;
    host.event(Event::ServerChanged(server));
    assert_eq!(host.ui.server, before);
    assert!(host
        .painted()
        .join(" ")
        .contains("Todavía no hay canales visibles"));
    action(&mut host, "invitation:open");
    assert_eq!(host.ui.server, Some(server_id));
    assert_eq!(host.ui.channel(), None);
}

#[test]
fn logout_and_same_account_relogin_cannot_reuse_an_old_acceptance() {
    let mut host = Host::new();
    let server = ServerId(90_001);
    let target = joinable(&mut host, "session", server);
    let old = accept(&mut host, &target, server);
    host.event(Event::Authentication(AuthenticationState::SignedOut));
    assert!(host.ui.invitation.is_none());
    host.event(Event::Ready(Box::new(host.provider.snapshot())));
    host.event(Event::Authentication(AuthenticationState::SignedIn {
        scopes: vec![],
    }));
    joinable(&mut host, "session", server);
    let current = accept(&mut host, &target, server);
    assert_ne!(
        old, current,
        "request IDs are not recycled across local sessions"
    );
    acknowledge(&mut host, old, server, Ok(()));
    assert!(host.painted().join(" ").contains("Entrando"));
    acknowledge(&mut host, current, server, Ok(()));
    assert!(host.painted().join(" ").contains("Esperando acceso"));
}

#[test]
fn membership_resolves_an_ambiguous_join_without_blocking_the_next_server() {
    let mut host = Host::new();
    let first = ServerId(90_001);
    let target = joinable(&mut host, "uncertain", first);
    let request = accept(&mut host, &target, first);
    deadline(&mut host, request, first);
    assert!(action(&mut host, "invitation:accept").is_empty());
    membership(&mut host, first, ChannelId(90_002));
    assert!(
        action(&mut host, "invitation:accept").is_empty(),
        "membership does not permit a second acceptance"
    );
    let second = ServerId(91_001);
    let target = joinable(&mut host, "another", second);
    assert_ne!(accept(&mut host, &target, second), request);
}

#[test]
fn stale_server_result_and_removal_cannot_navigate_a_confirmed_dialog() {
    let mut host = Host::new();
    let before = host.ui.server;
    let server_id = ServerId(90_001);
    let target = joinable(&mut host, "bound-server", server_id);
    let request = accept(&mut host, &target, server_id);
    acknowledge(&mut host, request, ServerId(90_003), Ok(()));
    assert!(host.painted().join(" ").contains("Entrando"));
    acknowledge(&mut host, request, server_id, Ok(()));
    let mut server = host.store.servers()[0].clone();
    server.id = server_id;
    host.event(Event::ServerChanged(server));
    host.event(Event::ServerRemoved {
        server: server_id,
        reason: None,
    });
    membership(&mut host, server_id, ChannelId(90_002));
    assert_eq!(
        host.ui.server, before,
        "removal invalidated pending navigation"
    );
}

#[test]
fn acceptance_footer_stays_inside_short_and_narrow_windows() {
    for size in [Size::new(320.0, 300.0), Size::new(480.0, 280.0), WINDOW] {
        let mut host = Host::new();
        joinable(&mut host, "compact-join", ServerId(90_001));
        let mut context = host.context();
        context.window = size;
        host.tree.set_root(view(&host.ui, &host.store, &context));
        let frame = host.tree.layout(size);
        for id in ["invitation:accept", "invitation:copy", "invitation:close"] {
            let rect = frame
                .regions
                .iter()
                .rev()
                .find(|r| r.id == id)
                .unwrap()
                .rect;
            assert!(rect.x >= 0.0 && rect.y >= 0.0, "{id}: {rect:?} in {size:?}");
            assert!(
                rect.x + rect.width <= size.width,
                "{id}: {rect:?} in {size:?}"
            );
            assert!(
                rect.y + rect.height <= size.height,
                "{id}: {rect:?} in {size:?}"
            );
            assert!(rect.height >= 28.0, "{id}: {rect:?}");
        }
    }
}

#[test]
fn terminal_ambiguity_blocks_the_same_invite_but_does_not_lock_other_destinations() {
    let mut host = Host::new();
    let server = ServerId(90_001);
    let target = joinable(&mut host, "terminal", server);
    let request = accept(&mut host, &target, server);
    acknowledge(
        &mut host,
        request,
        server,
        Err(InvitationJoinError::Ambiguous),
    );
    assert!(action(&mut host, "invitation:accept").is_empty());
    joinable(&mut host, "terminal", server);
    assert!(
        action(&mut host, "invitation:accept").is_empty(),
        "reopening cannot repeat the uncertain write"
    );
    let other = ServerId(91_001);
    let target = joinable(&mut host, "terminal-other", other);
    assert_ne!(accept(&mut host, &target, other), request);
}

#[test]
fn core_deadline_keeps_the_pending_gate_until_the_adapter_finishes() {
    let mut host = Host::new();
    let server = ServerId(90_001);
    let target = joinable(&mut host, "deadline", server);
    let request = accept(&mut host, &target, server);
    deadline(&mut host, request, server);
    let other = ServerId(91_001);
    let target = joinable(&mut host, "deadline-other", other);
    assert!(
        action(&mut host, "invitation:accept").is_empty(),
        "HTTP can still be in flight after a local timeout"
    );
    acknowledge(
        &mut host,
        request,
        server,
        Err(InvitationJoinError::Ambiguous),
    );
    assert_ne!(accept(&mut host, &target, other), request);
}

fn shown(host: &mut Host, id: &str) -> bool {
    host.frame().regions.iter().any(|region| region.id == id)
}

#[test]
fn a_refusal_says_why_offers_the_browser_and_never_a_retry() {
    for (refusal, said) in [
        (InvitationJoinError::Banned, "Te banearon de este servidor."),
        (
            InvitationJoinError::ServerLimit,
            "Llegaste al máximo de servidores",
        ),
        (
            InvitationJoinError::Denied,
            "No puedes aceptar esta invitación con esta cuenta.",
        ),
        (
            InvitationJoinError::Invalid,
            "La invitación ya no es válida.",
        ),
    ] {
        let mut host = Host::new();
        let server = ServerId(90_001);
        let target = joinable(&mut host, "refused", server);
        // A simulated or official provider shows the way out only when needed.
        assert!(!shown(&mut host, "invitation:browser"));
        let request = accept(&mut host, &target, server);
        acknowledge(&mut host, request, server, Err(refusal));
        assert!(host.painted().join(" ").contains(said), "{refusal:?}");
        assert!(!shown(&mut host, "invitation:accept"), "{refusal:?}");
        assert!(action(&mut host, "invitation:accept").is_empty());
        assert!(shown(&mut host, "invitation:browser"), "{refusal:?}");
        assert_eq!(
            action(&mut host, "invitation:browser"),
            vec![Intent::OpenInBrowser(target.url.clone())]
        );
        assert!(host.ui.invitation.is_some(), "the dialog stays");
    }
    // A failure that may pass keeps its explicit attempt.
    let mut host = Host::new();
    let server = ServerId(90_001);
    let target = joinable(&mut host, "unsent", server);
    let request = accept(&mut host, &target, server);
    acknowledge(
        &mut host,
        request,
        server,
        Err(InvitationJoinError::Unavailable),
    );
    assert!(host.painted().join(" ").contains("Volver a intentar"));
    assert_ne!(accept(&mut host, &target, server), request);
}

#[test]
fn already_joined_is_no_refusal_and_offers_to_open_what_it_gave() {
    let mut host = Host::new();
    let server = ServerId(90_001);
    let target = joinable(&mut host, "member", server);
    let request = accept(&mut host, &target, server);
    acknowledge(
        &mut host,
        request,
        server,
        Err(InvitationJoinError::AlreadyJoined),
    );
    let painted = host.painted().join(" ");
    assert!(painted.contains("Ya eres miembro de este servidor."));
    assert!(!painted.contains("No puedes aceptar"));
    assert!(!shown(&mut host, "invitation:accept"));
    assert!(!shown(&mut host, "invitation:browser"));
    // Once the account's events bring it, the dialog opens it.
    membership(&mut host, server, ChannelId(90_002));
    assert!(shown(&mut host, "invitation:open"));
    action(&mut host, "invitation:open");
    assert_eq!(host.ui.server, Some(server));
}

#[test]
fn an_unofficial_provider_always_offers_its_invitation_in_the_browser() {
    let mut host = Host::new();
    host.store.provider.as_mut().unwrap().kind = rune_provider::ProviderKind::Unofficial;
    let target = joinable(&mut host, "browser", ServerId(90_001));
    assert!(shown(&mut host, "invitation:accept"));
    assert!(shown(&mut host, "invitation:browser"));
    assert_eq!(
        action(&mut host, "invitation:browser"),
        vec![Intent::OpenInBrowser(target.url)]
    );
    // Two actions stack above «Copiar enlace» and «Cerrar», inside the window.
    for size in [Size::new(320.0, 480.0), WINDOW] {
        let mut context = host.context();
        context.window = size;
        host.tree.set_root(view(&host.ui, &host.store, &context));
        let frame = host.tree.layout(size);
        for id in [
            "invitation:accept",
            "invitation:browser",
            "invitation:copy",
            "invitation:close",
        ] {
            let rect = frame
                .regions
                .iter()
                .rev()
                .find(|region| region.id == id)
                .unwrap()
                .rect;
            assert!(
                rect.x >= 0.0 && rect.x + rect.width <= size.width,
                "{id}: {rect:?}"
            );
            assert!(
                rect.y >= 0.0 && rect.y + rect.height <= size.height,
                "{id}: {rect:?}"
            );
            assert!(rect.width >= 120.0, "{id}: {rect:?} in {size:?}");
        }
    }
    // Even before its preview answers.
    begin(&mut host, "loading");
    assert!(shown(&mut host, "invitation:browser"));
}
