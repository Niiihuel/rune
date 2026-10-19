use super::super::invitations::{preview, target};
use super::*;
use rune_model::ServerId;
use rune_provider::InvitationJoinError as JoinError;

fn join(request: u64) -> Command {
    Command::AcceptInvitation {
        request,
        target: target(),
        destination: rune_provider::InvitationDestination::Server(ServerId(10)),
        account: UserId(42),
        expires_at: std::time::Instant::now() + Duration::from_secs(60),
    }
}

fn result(request: u64, result: Result<(), JoinError>) -> Event {
    Event::InvitationAcceptance {
        request,
        destination: rune_provider::InvitationDestination::Server(ServerId(10)),
        result,
    }
}

fn guild() -> Value {
    json!({"id":"10","name":"Synthetic","owner_id":"42","roles":[],"channels":[],"members":[]})
}

async fn post(app: &mut Harness, request: u64) -> oneshot::Sender<String> {
    app.running.commands.send(join(request)).await.unwrap();
    let (get, reply) = app.pending().await;
    assert!(get.starts_with("GET /api/v10/invites/synthetic?with_counts=true "));
    reply.send(response(200, &preview().to_string())).unwrap();
    let (post, reply) = app.pending().await;
    assert!(post.starts_with("POST /api/v10/invites/synthetic "));
    presented_with_context(&post);
    reply
}

/// Browser identity (2026-10-19): the web client's headers and the invite
/// modal's context, from the preview just read.
fn presented_with_context(post: &str) {
    use base64::Engine as _;
    crate::browser::assert_presented(post, SECRET);
    let context = post
        .lines()
        .find_map(|line| line.strip_prefix("x-context-properties: "))
        .expect("the web client's invite context");
    let context = base64::engine::general_purpose::STANDARD
        .decode(context)
        .unwrap();
    assert_eq!(
        String::from_utf8(context).unwrap(),
        r#"{"location":"Desktop Invite Modal","location_guild_id":"10"}"#
    );
}

#[tokio::test]
async fn production_declares_acceptance_and_its_post_presents_the_browser() {
    let (login, _attempts) = login();
    let provider = DiscordSessionProvider::new(login).unwrap();
    assert!(provider.info().capabilities.invitation_accept);
    assert!(provider.info().capabilities.invitation_preview);
    let mut app = Harness::new().await;
    let reply = post(&mut app, 1).await;
    reply.send(response(200, &preview().to_string())).unwrap();
    assert_eq!(app.next_data().await, result(1, Ok(())));
}

const CHALLENGE: &str = r#"{"captcha_key":["captcha-required"],"captcha_service":"hcaptcha","captcha_sitekey":"synthetic-site-key","captcha_rqdata":"SYNTHETIC_RQDATA","captcha_rqtoken":"SYNTHETIC_RQTOKEN","captcha_session_id":"SYNTHETIC_CAPTCHA_SESSION"}"#;

fn verify(request: u64) -> Command {
    Command::VerifyInvitation {
        request,
        destination: rune_provider::InvitationDestination::Server(ServerId(10)),
        account: UserId(42),
        expires_at: std::time::Instant::now() + Duration::from_secs(360),
    }
}

async fn quiet(app: &mut Harness) {
    assert!(
        tokio::time::timeout(Duration::from_millis(30), app.requests.recv())
            .await
            .is_err(),
        "no request may leave without the person's answer"
    );
}

/// A first attempt answered with an hCaptcha, offered and not yet shown.
async fn challenged(app: &mut Harness, request: u64) {
    let reply = post(app, request).await;
    reply.send(response(400, CHALLENGE)).unwrap();
    assert_eq!(
        app.next_data().await,
        result(request, Err(JoinError::ChallengeRequired))
    );
    quiet(app).await;
    assert!(
        app.verifications.try_recv().is_err(),
        "only a click opens it"
    );
}

#[tokio::test]
async fn a_solved_hcaptcha_resends_the_same_acceptance_once_with_its_headers() {
    let mut app = Harness::new().await;
    challenged(&mut app, 2).await;
    app.running.commands.send(verify(2)).await.unwrap();
    let (challenge, answer) = app.verifications.recv().await.unwrap();
    assert_eq!(challenge.sitekey(), "synthetic-site-key");
    assert_eq!(challenge.rqdata(), Some("SYNTHETIC_RQDATA"));
    assert!(!format!("{challenge:?}").contains("SYNTHETIC"));
    quiet(&mut app).await;
    answer
        .send(ChallengeAnswer::try_from_local(
            "SYNTHETIC_HCAPTCHA_TOKEN".into(),
        ))
        .unwrap();
    // Straight to the write: no second preview and no other request first.
    let (sent, reply) = app.pending().await;
    assert!(
        sent.starts_with("POST /api/v10/invites/synthetic "),
        "{sent}"
    );
    let lower = sent.to_ascii_lowercase();
    for header in [
        "x-captcha-key: SYNTHETIC_HCAPTCHA_TOKEN\r\n",
        "x-captcha-rqtoken: SYNTHETIC_RQTOKEN\r\n",
        "x-captcha-session-id: SYNTHETIC_CAPTCHA_SESSION\r\n",
        "authorization: SYNTHETIC_SESSION_MARKER\r\n",
    ] {
        assert!(sent.contains(header), "{header}");
    }
    // The retry says the same as the attempt it repeats.
    presented_with_context(&sent);
    assert!(!lower.contains("rqdata"));
    // A second challenge is not offered again: one retry per solution.
    reply.send(response(400, CHALLENGE)).unwrap();
    assert_eq!(
        app.next_data().await,
        result(2, Err(JoinError::VerificationRequired))
    );
    app.running.commands.send(verify(2)).await.unwrap();
    assert_eq!(
        app.next_data().await,
        result(2, Err(JoinError::Unavailable))
    );
    quiet(&mut app).await;
    assert!(app.verifications.try_recv().is_err());
}

#[tokio::test]
async fn a_closed_window_or_another_request_sends_nothing_and_discards_the_challenge() {
    let mut app = Harness::new().await;
    challenged(&mut app, 2).await;
    app.running.commands.send(verify(2)).await.unwrap();
    let (_, answer) = app.verifications.recv().await.unwrap();
    answer
        .send(Err(LoginError::new("Se cerró la ventana.")))
        .unwrap();
    assert_eq!(
        app.next_data().await,
        result(2, Err(JoinError::Unavailable))
    );
    quiet(&mut app).await;
    // Another request, or the same one again, finds no challenge to resume.
    challenged(&mut app, 3).await;
    app.running.commands.send(verify(4)).await.unwrap();
    assert_eq!(
        app.next_data().await,
        Event::InvitationAcceptance {
            request: 4,
            destination: rune_provider::InvitationDestination::Server(ServerId(10)),
            result: Err(JoinError::Unavailable)
        }
    );
    app.running.commands.send(verify(3)).await.unwrap();
    assert_eq!(
        app.next_data().await,
        result(3, Err(JoinError::Unavailable))
    );
    quiet(&mut app).await;
    assert!(app.verifications.try_recv().is_err());
}

#[tokio::test]
async fn losing_the_connection_closes_the_window_and_drops_a_late_answer() {
    let mut app = Harness::new().await;
    challenged(&mut app, 2).await;
    app.running.commands.send(verify(2)).await.unwrap();
    let (_, answer) = app.verifications.recv().await.unwrap();
    app.dispatch
        .send(("TEST_DISCONNECT", Value::Null))
        .await
        .unwrap();
    let mut seen = Vec::new();
    for _ in 0..2 {
        seen.push(app.next_data().await);
    }
    assert!(
        seen.contains(&result(2, Err(JoinError::Unavailable))),
        "{seen:?}"
    );
    assert!(answer.is_closed(), "the window's future was dropped");
    quiet(&mut app).await;
}

#[tokio::test]
async fn invitation_expired_preflight_never_reaches_post_even_with_a_live_command_deadline() {
    let mut app = Harness::new().await;
    app.running.commands.send(join(1)).await.unwrap();
    let (_, preflight) = app.pending().await;
    let mut data = preview();
    data["expires_at"] = json!("1970-01-01T00:00:00Z");
    preflight.send(response(200, &data.to_string())).unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), app.requests.recv())
            .await
            .is_err(),
        "HTTP 200 with an expired invitation must never dispatch an authenticated POST"
    );
    assert_eq!(app.next_data().await, result(1, Err(JoinError::Invalid)));
}

#[tokio::test]
async fn invitation_is_single_pending_and_cancel_or_membership_does_not_cancel_submitted_post() {
    let mut app = Harness::new().await;
    app.running.commands.send(join(1)).await.unwrap();
    let (_, preflight) = app.pending().await;
    app.running.commands.send(join(2)).await.unwrap();
    assert_eq!(
        app.next_data().await,
        result(2, Err(JoinError::Unavailable))
    );
    app.running.commands.send(Command::Cancel(1)).await.unwrap();
    preflight
        .send(response(200, &preview().to_string()))
        .unwrap();
    let (sent, reply) = app.pending().await;
    assert!(sent.starts_with("POST /api/v10/invites/synthetic "));
    assert!(
        app.running.events.try_recv().is_err(),
        "preflight grants no membership"
    );
    app.dispatch.send(("GUILD_CREATE", guild())).await.unwrap();
    assert!(
        matches!(app.next_data().await, Event::ServerChanged(server) if server.id==ServerId(10))
    );
    reply.send(response(200, &preview().to_string())).unwrap();
    assert_eq!(app.next_data().await, result(1, Ok(())));
    app.running.commands.send(join(3)).await.unwrap();
    assert_eq!(
        app.next_data().await,
        result(3, Err(JoinError::AlreadyJoined))
    );
    assert!(app.requests.try_recv().is_err());
    // Nothing was sent: the log says what the person was told, by name.
    assert!(crate::voice::diag::captured::lines()
        .iter()
        .any(|line| line == "rune: invitation: not accepted: AlreadyJoined"));
}

#[tokio::test]
async fn invitation_acknowledgement_does_not_add_a_server_without_gateway_evidence() {
    let mut app = Harness::new().await;
    let reply = post(&mut app, 1).await;
    reply.send(response(200, &preview().to_string())).unwrap();
    assert_eq!(app.next_data().await, result(1, Ok(())));
    assert!(
        tokio::time::timeout(Duration::from_millis(30), app.running.events.recv())
            .await
            .is_err()
    );
    assert!(app.requests.try_recv().is_err());
}

#[tokio::test]
async fn invitation_revalidates_account_canonical_target_and_current_membership_before_dispatch() {
    let mut app = Harness::new().await;
    let mut other = join(1);
    if let Command::AcceptInvitation { account, .. } = &mut other {
        *account = UserId(43);
    }
    app.running.commands.send(other).await.unwrap();
    assert_eq!(app.next_data().await, result(1, Err(JoinError::Denied)));
    let mut bad = join(2);
    if let Command::AcceptInvitation { target, .. } = &mut bad {
        target.key = "other".into();
    }
    app.running.commands.send(bad).await.unwrap();
    assert_eq!(app.next_data().await, result(2, Err(JoinError::Invalid)));
    app.running.commands.send(join(3)).await.unwrap();
    let (_, preflight) = app.pending().await;
    app.dispatch.send(("GUILD_CREATE", guild())).await.unwrap();
    assert_eq!(
        app.next_data().await,
        result(3, Err(JoinError::Unavailable))
    );
    assert!(matches!(app.next_data().await, Event::ServerChanged(_)));
    preflight
        .send(response(200, &preview().to_string()))
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), app.requests.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn friend_invitations_skip_ones_own_and_take_the_same_two_steps() {
    use rune_provider::InvitationDestination::Friend;
    let mut app = Harness::new().await;
    let friend = |request, user| Command::AcceptInvitation {
        request,
        target: target(),
        destination: Friend(UserId(user)),
        account: UserId(42),
        expires_at: std::time::Instant::now() + Duration::from_secs(60),
    };
    let answer = |request, user, result| Event::InvitationAcceptance {
        request,
        destination: Friend(UserId(user)),
        result,
    };
    // One's own link asks nothing of the service.
    app.running.commands.send(friend(1, 42)).await.unwrap();
    assert_eq!(app.next_data().await, answer(1, 42, Err(JoinError::Denied)));
    assert!(app.requests.try_recv().is_err());
    app.running.commands.send(friend(2, 9)).await.unwrap();
    let data = json!({"code":"synthetic","type":2,"inviter":{"id":"9","username":"ada"}});
    let (get, reply) = app.pending().await;
    assert!(get.starts_with("GET /api/v10/invites/synthetic?with_counts=true "));
    reply.send(response(200, &data.to_string())).unwrap();
    let (post, reply) = app.pending().await;
    assert!(post.starts_with("POST /api/v10/invites/synthetic "));
    reply.send(response(200, &data.to_string())).unwrap();
    assert_eq!(app.next_data().await, answer(2, 9, Ok(())));
}

#[tokio::test]
async fn invitation_changed_preview_and_unsolvable_verification_never_submit_extra_posts() {
    let mut app = Harness::new().await;
    app.running.commands.send(join(1)).await.unwrap();
    let (_, preflight) = app.pending().await;
    let mut changed = preview();
    changed["guild"]["id"] = json!("11");
    preflight.send(response(200, &changed.to_string())).unwrap();
    assert_eq!(app.next_data().await, result(1, Err(JoinError::Invalid)));
    assert!(app.requests.try_recv().is_err());
    let reply = post(&mut app, 2).await;
    reply
        .send(response(
            400,
            r#"{"captcha_key":["challenge"],"captcha_sitekey":"synthetic-challenge"}"#,
        ))
        .unwrap();
    assert_eq!(
        app.next_data().await,
        result(2, Err(JoinError::VerificationRequired))
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), app.requests.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn invitation_logout_discards_delayed_preflight_and_post_results() {
    for submitted in [false, true] {
        let mut app = Harness::new().await;
        let reply = if submitted {
            post(&mut app, 1).await
        } else {
            app.running.commands.send(join(1)).await.unwrap();
            app.pending().await.1
        };
        app.running.commands.send(Command::SignOut).await.unwrap();
        assert_eq!(
            app.next_data().await,
            Event::Authentication(AuthenticationState::SignedOut)
        );
        assert!(matches!(
            app.next_data().await,
            Event::Connection(ConnectionState::Disconnected { .. })
        ));
        reply.send(response(200, &preview().to_string())).unwrap();
        app.running.commands.send(join(2)).await.unwrap();
        assert_eq!(
            app.next_data().await,
            result(2, Err(JoinError::Unavailable))
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(30), app.requests.recv())
                .await
                .is_err()
        );
        assert!(app.running.events.try_recv().is_err());
    }
}

#[tokio::test]
async fn invitation_unauthorized_result_drops_session_and_queued_work() {
    let mut app = Harness::new().await;
    let reply = post(&mut app, 1).await;
    app.running.commands.send(rest::history(2)).await.unwrap();
    reply.send(response(401, SECRET)).unwrap();
    assert_eq!(app.next_data().await, result(1, Err(JoinError::Denied)));
    assert!(matches!(
        app.next_data().await,
        Event::Authentication(AuthenticationState::Failed { .. })
    ));
    tokio::time::timeout(Duration::from_secs(1), app.closed.take().unwrap())
        .await
        .unwrap()
        .unwrap();
    app.running.commands.send(join(3)).await.unwrap();
    assert_eq!(
        app.next_data().await,
        result(3, Err(JoinError::Unavailable))
    );
    assert!(app.requests.try_recv().is_err());
}

#[tokio::test]
async fn invitation_disconnect_is_uncertain_only_after_post_submission() {
    for submitted in [false, true] {
        let mut app = Harness::new().await;
        let reply = if submitted {
            post(&mut app, 1).await
        } else {
            app.running.commands.send(join(1)).await.unwrap();
            app.pending().await.1
        };
        app.dispatch
            .send((
                "CHANNEL_UPDATE",
                json!({"id":"20","type":0,"guild_id":"999","permission_overwrites":"invalid"}),
            ))
            .await
            .unwrap();
        assert_eq!(
            app.next_data().await,
            result(
                1,
                Err(if submitted {
                    JoinError::Ambiguous
                } else {
                    JoinError::Unavailable
                })
            )
        );
        assert!(matches!(
            app.next_data().await,
            Event::Connection(ConnectionState::Disconnected { .. })
        ));
        reply.send(response(200, &preview().to_string())).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), app.requests.recv())
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn invitation_expired_in_core_queue_is_rejected_before_any_http() {
    let mut app = Harness::new().await;
    let mut old = join(1);
    if let Command::AcceptInvitation { expires_at, .. } = &mut old {
        *expires_at = std::time::Instant::now() - Duration::from_secs(1);
    }
    app.running.commands.send(old).await.unwrap();
    assert_eq!(
        app.next_data().await,
        result(1, Err(JoinError::Unavailable))
    );
    assert!(app.requests.try_recv().is_err());
}

#[tokio::test]
async fn invitation_waiting_behind_other_rest_work_is_discarded_on_reconnect() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(1)).await.unwrap();
    let (_, history) = app.pending().await;
    app.running.commands.send(join(2)).await.unwrap();
    // This rejection is an ordering barrier after the queued invite.
    app.running.commands.send(join(3)).await.unwrap();
    assert_eq!(
        app.next_data().await,
        result(3, Err(JoinError::Unavailable))
    );
    app.dispatch
        .send(("TEST_DISCONNECT", Value::Null))
        .await
        .unwrap();
    assert_eq!(
        app.next_data().await,
        result(2, Err(JoinError::Unavailable))
    );
    assert!(matches!(
        app.next_data().await,
        Event::Connection(ConnectionState::Reconnecting { .. })
    ));
    history.send(response(200, "[]")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::HistoryLoaded { request: 1, .. }
    ));
    assert!(app.requests.try_recv().is_err());
}

#[tokio::test]
async fn invitation_waiting_in_adapter_queue_expires_before_preflight() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(1)).await.unwrap();
    let (_, first) = app.pending().await;
    app.running.commands.send(rest::history(2)).await.unwrap();
    app.running.commands.send(join(3)).await.unwrap();
    app.running.commands.send(join(4)).await.unwrap();
    assert_eq!(
        app.next_data().await,
        result(4, Err(JoinError::Unavailable))
    );
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(11)).await;
    first.send(response(200, "[]")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::HistoryLoaded { request: 1, .. }
    ));
    let (request, second) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/channels/20/messages"));
    tokio::time::advance(Duration::from_secs(10)).await;
    second.send(response(200, "[]")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::HistoryLoaded { request: 2, .. }
    ));
    assert_eq!(
        app.next_data().await,
        result(3, Err(JoinError::Unavailable))
    );
    assert!(app.requests.try_recv().is_err());
}

#[tokio::test]
async fn invitation_anonymous_preflight_401_does_not_invalidate_the_session() {
    let mut app = Harness::new().await;
    app.running.commands.send(join(1)).await.unwrap();
    let (_, preflight) = app.pending().await;
    preflight.send(response(401, SECRET)).unwrap();
    assert_eq!(app.next_data().await, result(1, Err(JoinError::Denied)));
    assert!(app.running.events.try_recv().is_err());
    // A subsequent existing read proves the same account remains online.
    app.running.commands.send(rest::history(2)).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/channels/20/messages"));
    reply.send(response(200, "[]")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::HistoryLoaded { request: 2, .. }
    ));
}
