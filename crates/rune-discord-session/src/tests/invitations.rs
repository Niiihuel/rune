//! Synthetic loopback transport tests; no real invite or account is contacted.
use super::*;
use rune_model::{ChannelId, ServerId, UserId};
use rune_provider::{InvitationDestination, InvitationJoinError as JoinError, InvitationTarget};
use serde_json::{json, Value};

/// Where the synthetic invitation leads.
const TEN: InvitationDestination = InvitationDestination::Server(ServerId(10));

pub(super) fn target() -> InvitationTarget {
    crate::invitations::target("https://discord.gg/synthetic").unwrap()
}

pub(super) fn preview() -> Value {
    json!({"code":"synthetic","type":0,"guild":{"id":"10","name":"Synthetic"},"flags":0})
}

#[tokio::test]
async fn invitation_preflight_has_no_credentials_and_post_presents_the_browser() {
    let (http, captured) = fixture(response(200, &preview().to_string())).await;
    assert!(http
        .inspect_acceptance(&target(), TEN)
        .await
        .unwrap()
        .is_empty());
    let request = captured.await.unwrap();
    crate::browser::assert_presented(&request, SECRET);
    let request = request.to_ascii_lowercase();
    assert!(request.starts_with("get /api/v10/invites/synthetic?with_counts=true "));
    assert!(!request.contains("authorization:"));
    let (http, captured) = fixture(response(200, &preview().to_string())).await;
    assert_eq!(
        http.accept_invitation(&secret(), &target(), TEN, 7, None)
            .await
            .unwrap(),
        vec![Event::InvitationAcceptance {
            request: 7,
            destination: TEN,
            result: Ok(())
        }]
    );
    let request = captured.await.unwrap();
    let lower = request.to_ascii_lowercase();
    assert!(request.starts_with("POST /api/v10/invites/synthetic "));
    assert_eq!(request.split_once("\r\n\r\n").unwrap().1, "{}");
    assert_eq!(lower.matches("\r\nauthorization:").count(), 1);
    assert!(request.contains(&format!("authorization: {SECRET}\r\n")));
    // Browser identity (2026-10-19): the web client's headers, its `Origin`
    // on a write, and no context for an invitation this transport never saw.
    crate::browser::assert_presented(&request, SECRET);
    assert!(lower.contains("\r\norigin: https://discord.com\r\n"));
    for absent in ["x-context-properties", "x-captcha"] {
        assert!(!lower.contains(absent), "unexpected header {absent}");
    }
}

#[tokio::test]
async fn invitation_preflight_rejects_expired_or_malformed_reported_expiration() {
    for expiration in [
        json!("1970-01-01T00:00:00Z"),
        json!("not-a-date"),
        json!(42),
    ] {
        let mut data = preview();
        data["expires_at"] = expiration;
        let (http, captured) = fixture(response(200, &data.to_string())).await;
        let error = http
            .inspect_acceptance(&target(), TEN)
            .await
            .expect_err("an expired or malformed preflight cannot authorize POST");
        assert_eq!(error.invitation, Some(JoinError::Invalid));
        assert!(!error.ends_session && !error.ambiguous);
        assert!(!captured
            .await
            .unwrap()
            .to_ascii_lowercase()
            .contains("authorization:"));
    }
    for expiration in [Value::Null, json!("9999-01-01T00:00:00Z")] {
        let mut data = preview();
        data["expires_at"] = expiration;
        let (http, _) = fixture(response(200, &data.to_string())).await;
        assert!(http
            .inspect_acceptance(&target(), TEN)
            .await
            .unwrap()
            .is_empty());
    }
}

#[tokio::test]
async fn invitation_scope_checks_reject_changed_servers_guest_targets_and_malformed_ids() {
    let mut cases = vec![];
    for (field, value) in [
        ("code", json!("another")),
        ("type", json!(1)),
        ("flags", json!(1)),
        ("flags", json!(2)),
        ("flags", json!(null)),
        ("target_type", json!(1)),
        ("target_application", json!({"id":"7"})),
    ] {
        let mut data = preview();
        data[field] = value;
        cases.push(data);
    }
    for id in ["11", "010", "0"] {
        let mut data = preview();
        data["guild"]["id"] = json!(id);
        cases.push(data);
    }
    for data in cases {
        let (http, _) = fixture(response(200, &data.to_string())).await;
        assert_eq!(
            http.inspect_acceptance(&target(), TEN)
                .await
                .unwrap_err()
                .invitation,
            Some(JoinError::Invalid)
        );
        let (http, _) = fixture(response(200, &data.to_string())).await;
        assert_eq!(
            http.accept_invitation(&secret(), &target(), TEN, 7, None)
                .await
                .unwrap_err()
                .invitation,
            Some(JoinError::Ambiguous)
        );
    }
    let http = Http::local("http://127.0.0.1:9");
    let mut invalid = target();
    invalid.key = "other".into();
    assert_eq!(
        http.accept_invitation(&secret(), &invalid, TEN, 7, None)
            .await
            .unwrap_err()
            .invitation,
        Some(JoinError::Invalid)
    );
}

#[tokio::test]
async fn group_friend_and_guest_invitations_use_the_same_post_and_check_their_own_scope() {
    let group = json!({"code":"synthetic","type":1,"channel":{"id":"55","type":3}});
    let friend = json!({"code":"synthetic","type":2,"inviter":{"id":"9","username":"ada"}});
    let guest = json!({"code":"synthetic","type":0,"flags":1,
        "guild":{"id":"10","name":"Synthetic"},"channel":{"id":"77","type":2}});
    let guest_to = InvitationDestination::Guest {
        server: ServerId(10),
        channel: ChannelId(77),
    };
    for (data, destination) in [
        (group, InvitationDestination::Group(ChannelId(55))),
        (friend, InvitationDestination::Friend(UserId(9))),
        (guest, guest_to),
    ] {
        let (http, _) = fixture(response(200, &data.to_string())).await;
        assert!(http
            .inspect_acceptance(&target(), destination)
            .await
            .unwrap()
            .is_empty());
        let (http, captured) = fixture(response(200, &data.to_string())).await;
        assert_eq!(
            http.accept_invitation(&secret(), &target(), destination, 7, None)
                .await
                .unwrap(),
            vec![Event::InvitationAcceptance {
                request: 7,
                destination,
                result: Ok(())
            }]
        );
        let request = captured.await.unwrap();
        assert!(request.starts_with("POST /api/v10/invites/synthetic "));
        assert_eq!(request.split_once("\r\n\r\n").unwrap().1, "{}");
        // Another destination than the one shown is never written to.
        for other in [
            TEN,
            InvitationDestination::Group(ChannelId(56)),
            InvitationDestination::Friend(UserId(8)),
            InvitationDestination::Guest {
                server: ServerId(10),
                channel: ChannelId(78),
            },
        ] {
            let (http, _) = fixture(response(200, &data.to_string())).await;
            assert_eq!(
                http.inspect_acceptance(&target(), other)
                    .await
                    .unwrap_err()
                    .invitation,
                Some(JoinError::Invalid),
                "{data} as {other:?}"
            );
        }
    }
    // A guest invitation with any other flag is not one.
    let odd = json!({"code":"synthetic","type":0,"flags":3,
        "guild":{"id":"10","name":"Synthetic"},"channel":{"id":"77","type":2}});
    let (http, _) = fixture(response(200, &odd.to_string())).await;
    assert_eq!(
        http.inspect_acceptance(&target(), guest_to)
            .await
            .unwrap_err()
            .invitation,
        Some(JoinError::Invalid)
    );
}

#[tokio::test]
async fn invitation_rejections_are_typed_and_challenge_material_never_leaks() {
    for (status, body, expected, ends_session) in [
        (400, json!({"message":SECRET}), JoinError::Invalid, false),
        (403, json!({"message":SECRET}), JoinError::Denied, false),
        (404, json!({"message":SECRET}), JoinError::Invalid, false),
        (401, json!({"message":SECRET}), JoinError::Denied, true),
        (
            400,
            json!({"captcha_key":["challenge"],"captcha_sitekey":SECRET}),
            JoinError::VerificationRequired,
            false,
        ),
        (
            403,
            json!({"captcha_rqtoken":SECRET}),
            JoinError::VerificationRequired,
            false,
        ),
        (500, json!({"message":SECRET}), JoinError::Ambiguous, false),
        (302, json!({}), JoinError::Ambiguous, false),
        (201, preview(), JoinError::Ambiguous, false),
    ] {
        let (http, _) = fixture(response(status, &body.to_string())).await;
        let error = http
            .accept_invitation(&secret(), &target(), TEN, 7, None)
            .await
            .unwrap_err();
        assert_eq!(error.invitation, Some(expected));
        assert_eq!(error.ends_session, ends_session);
        assert!(!format!("{error:?}").contains(SECRET));
    }
}

fn hcaptcha() -> Value {
    json!({"captcha_key":["captcha-required"],"captcha_service":"hcaptcha",
        "captcha_sitekey":"synthetic-site-key","captcha_rqdata":SECRET,
        "captcha_rqtoken":"SYNTHETIC_RQTOKEN","captcha_session_id":"SYNTHETIC_CAPTCHA_SESSION",
        "should_serve_invisible":true,"message":SECRET})
}

#[tokio::test]
async fn only_a_bounded_hcaptcha_on_the_account_post_is_offered_to_the_person() {
    let (http, _) = fixture(response(400, &hcaptcha().to_string())).await;
    let error = http
        .accept_invitation(&secret(), &target(), TEN, 7, None)
        .await
        .unwrap_err();
    assert_eq!(error.invitation, Some(JoinError::ChallengeRequired));
    assert!(!error.ends_session && !error.ambiguous);
    assert!(!format!("{error:?}").contains(SECRET));
    let issued = error.challenge.expect("an offered challenge");
    assert_eq!(issued.challenge.sitekey(), "synthetic-site-key");
    assert_eq!(issued.challenge.rqdata(), Some(SECRET));
    assert!(issued.challenge.invisible());
    assert_eq!(
        issued.rqtoken.as_deref().map(String::as_str),
        Some("SYNTHETIC_RQTOKEN")
    );
    // The anonymous preview never carries an offer.
    let (http, _) = fixture(response(400, &hcaptcha().to_string())).await;
    let error = http.inspect_acceptance(&target(), TEN).await.unwrap_err();
    assert_eq!(error.invitation, Some(JoinError::VerificationRequired));
    assert!(error.challenge.is_none());
    for (status, field, value) in [
        (403, "captcha_service", json!("hcaptcha")),
        (400, "captcha_service", json!("recaptcha")),
        (400, "captcha_service", Value::Null),
        (400, "captcha_sitekey", Value::Null),
        (400, "captcha_sitekey", json!("../evil")),
        (400, "captcha_rqtoken", json!("x".repeat(2049))),
        (400, "captcha_session_id", json!("line\nbreak")),
        (400, "captcha_rqdata", json!(7)),
    ] {
        let mut body = hcaptcha();
        body[field] = value;
        let (http, _) = fixture(response(status, &body.to_string())).await;
        let error = http
            .accept_invitation(&secret(), &target(), TEN, 7, None)
            .await
            .unwrap_err();
        assert_eq!(
            error.invitation,
            Some(JoinError::VerificationRequired),
            "{field}"
        );
        assert!(error.challenge.is_none());
    }
}

#[tokio::test]
async fn invitation_rejection_metadata_only_ends_authenticated_account_sessions() {
    for (code, expected, account_failure) in [
        (50014, JoinError::Denied, true),
        (40002, JoinError::VerificationRequired, true),
        (50178, JoinError::VerificationRequired, true),
        (60003, JoinError::VerificationRequired, false),
    ] {
        for authenticated in [false, true] {
            let body = json!({"code":code,"captcha_key":[SECRET],"message":SECRET});
            let (http, _) = fixture(response(403, &body.to_string())).await;
            let error = if authenticated {
                http.accept_invitation(&secret(), &target(), TEN, 7, None)
                    .await
            } else {
                http.inspect_acceptance(&target(), TEN).await
            }
            .unwrap_err();
            assert_eq!(error.invitation, Some(expected));
            assert_eq!(error.ends_session, authenticated && account_failure);
            assert!(!error.ambiguous);
            assert!(!format!("{error:?}").contains(SECRET));
        }
    }
    for body in ["invalid-json", r#"{"code":999999,"captcha_key":null}"#] {
        let (http, _) = fixture(response(400, body)).await;
        let error = http
            .accept_invitation(&secret(), &target(), TEN, 7, None)
            .await
            .unwrap_err();
        assert_eq!(error.invitation, Some(JoinError::Invalid));
        assert!(!error.ends_session && !error.ambiguous);
    }
    for authenticated in [false, true] {
        let (http, _) = fixture(
            "HTTP/1.1 400 Rejected\r\nContent-Length: 65537\r\nConnection: close\r\n\r\n".into(),
        )
        .await;
        let error = if authenticated {
            http.accept_invitation(&secret(), &target(), TEN, 7, None)
                .await
        } else {
            http.inspect_acceptance(&target(), TEN).await
        }
        .unwrap_err();
        assert_eq!(error.invitation, Some(JoinError::Invalid));
        assert!(!error.ends_session && !error.ambiguous);
    }
}

#[tokio::test]
async fn invitation_limits_success_body_and_refuses_redirects() {
    for reply in [
        "HTTP/1.1 200 OK\r\nContent-Length: 65537\r\nConnection: close\r\n\r\n".into(),
        format!(
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}",
            "x".repeat(65537)
        ),
        response(200, "invalid-json"),
    ] {
        let (http, _) = fixture(reply).await;
        assert_eq!(
            http.accept_invitation(&secret(), &target(), TEN, 7, None)
                .await
                .unwrap_err()
                .invitation,
            Some(JoinError::Ambiguous)
        );
    }
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (http, _) = fixture(format!("HTTP/1.1 307 Redirect\r\nLocation: http://{}/trap\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", trap.local_addr().unwrap())).await;
    assert_eq!(
        http.accept_invitation(&secret(), &target(), TEN, 7, None)
            .await
            .unwrap_err()
            .invitation,
        Some(JoinError::Ambiguous)
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(30), trap.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn invitation_rate_limit_is_shared_and_keeps_actual_retry_delay() {
    for reply in [
        response(429, r#"{"retry_after":2.5}"#),
        "HTTP/1.1 429 Slow\r\nRetry-After: 2.5\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            .into(),
    ] {
        let (http, captured) = fixture(reply).await;
        let error = http
            .accept_invitation(&secret(), &target(), TEN, 7, None)
            .await
            .unwrap_err();
        assert_eq!(
            error.invitation,
            Some(JoinError::RateLimited {
                retry_after_secs: 3
            })
        );
        captured.await.unwrap();
        let clone = http.clone();
        assert!(matches!(
            clone
                .inspect_acceptance(&target(), TEN)
                .await
                .unwrap_err()
                .invitation,
            Some(JoinError::RateLimited {
                retry_after_secs: 3
            })
        ));
        assert!(
            !clone
                .execute(&secret(), &rest::send(2), "")
                .await
                .unwrap_err()
                .ambiguous
        );
    }
}

#[tokio::test(start_paused = true)]
async fn invitation_post_and_preflight_each_have_a_deadline_and_401_does_not_wait_for_body() {
    for mutating in [false, true] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http = Http::local(&format!("http://{}", listener.local_addr().unwrap()));
        let work = tokio::spawn(async move {
            if mutating {
                http.accept_invitation(&secret(), &target(), TEN, 7, None)
                    .await
            } else {
                http.inspect_acceptance(&target(), TEN).await
            }
        });
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        tokio::time::advance(Duration::from_secs(16)).await;
        assert_eq!(
            work.await.unwrap().unwrap_err().invitation,
            Some(if mutating {
                JoinError::Ambiguous
            } else {
                JoinError::Unavailable
            })
        );
    }
    let (http, _) = fixture(
        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 9999\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert!(
        http.accept_invitation(&secret(), &target(), TEN, 7, None)
            .await
            .unwrap_err()
            .ends_session
    );
}

/// What a refusal's body says beside its code: never in a line or a label.
const PRIVATE_RESPONSE: &str = "SYNTHETIC_PRIVATE_ERROR_BODY";

#[tokio::test]
async fn refused_acceptances_name_their_cause_by_the_services_code() {
    for (status, code, expected) in [
        (400, 30001, JoinError::ServerLimit),
        (403, 40007, JoinError::Banned),
        (400, 30019, JoinError::ServerFull),
        (403, 40069, JoinError::InvitesPaused),
        (403, 50095, JoinError::Region),
        (403, 20024, JoinError::AgeRestricted),
        (400, 700_000, JoinError::AgeRestricted),
        (403, 400_002, JoinError::LimitedAccess),
        (403, 40068, JoinError::AccountLimited),
        (400, 10006, JoinError::Invalid),
        // A code RUNE does not know stays the plain refusal.
        (403, 99_999, JoinError::Denied),
        (400, 99_999, JoinError::Invalid),
    ] {
        let body = json!({"code":code,"message":PRIVATE_RESPONSE});
        let (http, _) = fixture(response(status, &body.to_string())).await;
        let error = http
            .accept_invitation(&secret(), &target(), TEN, 7, None)
            .await
            .unwrap_err();
        assert_eq!(error.invitation, Some(expected), "{status} {code}");
        assert!(!expected.can_retry(), "{code} is a refusal");
        assert!(!error.ends_session && !error.ambiguous);
        assert!(!format!("{error:?}").contains(PRIVATE_RESPONSE));
    }
}

#[tokio::test]
async fn a_refused_acceptance_leaves_its_status_and_code_in_the_log_and_nothing_private() {
    let body = json!({"code":40007,"message":PRIVATE_RESPONSE,"guild_id":"10"});
    let (http, _) = fixture(response(403, &body.to_string())).await;
    let error = http
        .accept_invitation(&secret(), &target(), TEN, 7, None)
        .await
        .unwrap_err();
    assert_eq!(error.invitation, Some(JoinError::Banned));
    let (http, _) = fixture(response(
        403,
        &json!({"message":PRIVATE_RESPONSE}).to_string(),
    ))
    .await;
    http.accept_invitation(&secret(), &target(), TEN, 8, None)
        .await
        .unwrap_err();
    let (http, _) = fixture(response(401, SECRET)).await;
    http.accept_invitation(&secret(), &target(), TEN, 9, None)
        .await
        .unwrap_err();
    let lines = crate::voice::diag::captured::lines();
    for said in [
        "rune: invitation: POST: HTTP 403, error code 40007",
        "rune: invitation: POST: HTTP 403, no error code",
        "rune: invitation: POST: HTTP 401",
    ] {
        assert!(lines.iter().any(|line| line == said), "no line {said:?}");
    }
    // Of every test running: no invitation line holds the body, the token,
    // the invitation's code or where it leads.
    for line in lines
        .iter()
        .filter(|line| line.starts_with("rune: invitation:"))
    {
        for private in [PRIVATE_RESPONSE, SECRET, "synthetic", "guild", "/invites/"] {
            assert!(!line.contains(private), "{private} in {line:?}");
        }
    }
}
