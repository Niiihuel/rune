//! Session-level and action-level rejections over synthetic loopback transports.
use super::*;
use rune_provider::Failure;

const PRIVATE_RESPONSE: &str = "SYNTHETIC_PRIVATE_ERROR_BODY";

#[tokio::test]
async fn authenticated_session_rejections_discard_queued_actions_without_retry_or_automatic_login()
{
    for (status, code) in [(400, 50014), (403, 50014), (400, 40002), (403, 50178)] {
        let mut app = Harness::new().await;
        app.running.commands.send(rest::send(1)).await.unwrap();
        let (request, reply) = app.pending().await;
        assert!(request.starts_with("POST /api/v10/channels/20/messages "));
        app.running.commands.send(rest::history(2)).await.unwrap();
        app.running.commands.send(rest::send(3)).await.unwrap();

        // A rejected local command is a barrier: both valid commands above
        // have reached the provider queue before the HTTP rejection arrives.
        app.running
            .commands
            .send(Command::LoadHistory {
                request: 99,
                channel: ChannelId(999),
                before: None,
                limit: 1,
            })
            .await
            .unwrap();
        assert!(matches!(
            app.next_data().await,
            Event::Failed {
                failure: Failure::History { request: 99, .. },
                ambiguous: false,
                ..
            }
        ));
        reply
            .send(response(
                status,
                &json!({"code":code,"message":PRIVATE_RESPONSE}).to_string(),
            ))
            .unwrap();
        let Event::Failed {
            failure: Failure::Send { nonce: 1, .. },
            reason,
            ambiguous: false,
        } = app.next_data().await
        else {
            panic!("a definitive rejection must finish the original write");
        };
        assert!(!reason.contains(PRIVATE_RESPONSE));
        let Event::Authentication(AuthenticationState::Failed { reason }) = app.next_data().await
        else {
            panic!("the authenticated rejection must invalidate the session");
        };
        assert!(!reason.contains(PRIVATE_RESPONSE));
        assert_eq!(
            reason,
            if code == 50014 {
                "Discord rechazó la sesión; iniciá sesión otra vez."
            } else {
                "Discord requiere verificar la cuenta en su aplicación oficial."
            }
        );
        tokio::time::timeout(Duration::from_secs(1), app.closed.take().unwrap())
            .await
            .unwrap()
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(30), app.requests.recv())
                .await
                .is_err(),
            "queued history and writes must be discarded, not submitted or repeated"
        );
        assert!(app.attempts.try_recv().is_err(), "no automatic login");

        // A new caller cannot keep using the invalidated account either.
        app.running.commands.send(rest::history(4)).await.unwrap();
        assert!(matches!(
            app.next_data().await,
            Event::Failed {
                failure: Failure::History { request: 4, .. },
                ambiguous: false,
                ..
            }
        ));
        assert!(app.requests.try_recv().is_err());
        assert!(app.attempts.try_recv().is_err());
    }
}

#[tokio::test]
async fn action_verification_never_reposts_or_ends_the_session_and_allows_an_explicit_read() {
    for (status, body) in [
        (400, json!({"code":60003,"message":PRIVATE_RESPONSE})),
        (403, json!({"code":60003,"message":PRIVATE_RESPONSE})),
        (
            400,
            json!({"captcha_key":["required"],"captcha_sitekey":PRIVATE_RESPONSE}),
        ),
        (403, json!({"captcha_rqtoken":PRIVATE_RESPONSE})),
    ] {
        let mut app = Harness::new().await;
        app.running.commands.send(rest::send(1)).await.unwrap();
        let (request, reply) = app.pending().await;
        assert!(request.starts_with("POST /api/v10/channels/20/messages "));
        reply.send(response(status, &body.to_string())).unwrap();
        let Event::Failed {
            failure: Failure::Send { nonce: 1, .. },
            reason,
            ambiguous: false,
        } = app.next_data().await
        else {
            panic!("verification must reject only the explicit action");
        };
        assert!(!reason.contains(PRIVATE_RESPONSE));
        assert!(!reason.contains(SECRET));
        assert_eq!(
            reason,
            "Discord requiere una verificación para completar esta acción."
        );
        assert!(
            tokio::time::timeout(Duration::from_millis(30), app.requests.recv())
                .await
                .is_err(),
            "verification must not trigger a second request"
        );
        assert!(app.running.events.try_recv().is_err());
        assert!(app.attempts.try_recv().is_err());

        app.running.commands.send(rest::history(2)).await.unwrap();
        let (request, reply) = app.pending().await;
        assert!(request.starts_with("GET /api/v10/channels/20/messages?limit=50 "));
        reply.send(response(200, "[]")).unwrap();
        assert!(matches!(
            app.next_data().await,
            Event::HistoryLoaded { request: 2, .. }
        ));
        assert!(matches!(
            app.closed.as_mut().unwrap().try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        assert!(app.running.events.try_recv().is_err());
        assert!(app.attempts.try_recv().is_err());
        assert!(app.requests.try_recv().is_err());
    }
}
