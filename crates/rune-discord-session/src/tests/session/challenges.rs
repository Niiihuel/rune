//! An hCaptcha on any write through the whole provider: offered, shown only
//! when the person asks, answered once with that same request. Local
//! fixtures only; the "window" is the test's.

use super::*;
use bytes::Bytes;
use rune_model::{PendingAttachment, UploadToken, UserId};
use rune_provider::{Failure, PreparedUpload, UploadFence, UploadSendScope};

pub(super) const CHALLENGE: &str = r#"{"captcha_key":["captcha-required"],"captcha_service":"hcaptcha","captcha_sitekey":"synthetic-site-key","captcha_rqdata":"SYNTHETIC_RQDATA","captcha_rqtoken":"SYNTHETIC_RQTOKEN","captcha_session_id":"SYNTHETIC_CAPTCHA_SESSION"}"#;

const SENT: Failure = Failure::Send {
    channel: ChannelId(20),
    nonce: 7,
};

async fn quiet(app: &mut Harness) {
    assert!(
        tokio::time::timeout(Duration::from_millis(30), app.requests.recv())
            .await
            .is_err(),
        "nothing may leave without the person's answer"
    );
}

/// `command`'s first request, answered with a challenge, and its offer.
async fn challenged(app: &mut Harness, command: Command, route: &str) -> u64 {
    app.running.commands.send(command).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with(route), "{request}");
    reply.send(response(400, CHALLENGE)).unwrap();
    let Event::ChallengeRequired { challenge, .. } = next_offer(app).await else {
        unreachable!()
    };
    quiet(app).await;
    assert!(
        app.verifications.try_recv().is_err(),
        "only the person's click opens the window"
    );
    challenge
}

async fn next_offer(app: &mut Harness) -> Event {
    loop {
        match app.next_data().await {
            Event::UploadProgress { .. } => {}
            event @ Event::ChallengeRequired { .. } => return event,
            other => panic!("{other:?}"),
        }
    }
}

pub(super) async fn solve(app: &mut Harness, challenge: u64) {
    app.running
        .commands
        .send(Command::SolveChallenge {
            challenge,
            solve: true,
        })
        .await
        .unwrap();
    let (shown, answer) = app.verifications.recv().await.unwrap();
    assert_eq!(shown.sitekey(), "synthetic-site-key");
    assert_eq!(shown.rqdata(), Some("SYNTHETIC_RQDATA"));
    quiet(app).await;
    answer
        .send(ChallengeAnswer::try_from_local(
            "SYNTHETIC_HCAPTCHA_TOKEN".into(),
        ))
        .unwrap();
}

pub(super) fn answered(request: &str) {
    for header in [
        "x-captcha-key: SYNTHETIC_HCAPTCHA_TOKEN\r\n",
        "x-captcha-rqtoken: SYNTHETIC_RQTOKEN\r\n",
        "x-captcha-session-id: SYNTHETIC_CAPTCHA_SESSION\r\n",
    ] {
        assert!(request.contains(header), "{header}");
    }
    // Browser identity (2026-10-19): the retry presents the same browser.
    crate::browser::assert_presented(request, "SYNTHETIC_SESSION_MARKER");
    assert!(!request.to_ascii_lowercase().contains("rqdata"));
}

fn unanswered(request: &str) {
    assert!(
        !request.to_ascii_lowercase().contains("x-captcha"),
        "{request}"
    );
}

#[tokio::test]
async fn a_message_waits_for_the_person_and_goes_out_once_with_the_answer() {
    let mut app = Harness::new().await;
    let challenge = challenged(
        &mut app,
        rest::send(7),
        "POST /api/v10/channels/20/messages ",
    )
    .await;
    solve(&mut app, challenge).await;
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/messages "));
    answered(&request);
    reply
        .send(response(200, &rest::message(300, "hello").to_string()))
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::MessageCreated { message, nonce: Some(7) } if message.id == MessageId(300)
    ));
    // The answer went with that request only.
    app.running.commands.send(rest::send(8)).await.unwrap();
    let (request, _) = app.pending().await;
    unanswered(&request);
}

#[tokio::test]
async fn a_second_challenge_ends_the_write_and_is_not_offered_again() {
    let mut app = Harness::new().await;
    let challenge = challenged(
        &mut app,
        rest::send(7),
        "POST /api/v10/channels/20/messages ",
    )
    .await;
    solve(&mut app, challenge).await;
    let (request, reply) = app.pending().await;
    answered(&request);
    reply.send(response(400, CHALLENGE)).unwrap();
    assert!(matches!(app.next_data().await,
        Event::Failed { failure, reason, ambiguous: false }
            if failure == SENT && reason == crate::challenges::AGAIN.to_string()));
    quiet(&mut app).await;
    assert!(app.verifications.try_recv().is_err());
}

#[tokio::test]
async fn cancelling_or_closing_the_window_sends_nothing() {
    let mut app = Harness::new().await;
    let challenge = challenged(
        &mut app,
        rest::send(7),
        "POST /api/v10/channels/20/messages ",
    )
    .await;
    app.running
        .commands
        .send(Command::SolveChallenge {
            challenge,
            solve: false,
        })
        .await
        .unwrap();
    assert!(matches!(app.next_data().await,
        Event::Failed { failure, reason, ambiguous: false }
            if failure == SENT && reason == crate::challenges::CANCELLED.to_string()));
    quiet(&mut app).await;
    assert!(app.verifications.try_recv().is_err());

    let challenge = challenged(
        &mut app,
        rest::send(7),
        "POST /api/v10/channels/20/messages ",
    )
    .await;
    app.running
        .commands
        .send(Command::SolveChallenge {
            challenge,
            solve: true,
        })
        .await
        .unwrap();
    let (_, answer) = app.verifications.recv().await.unwrap();
    answer
        .send(Err(LoginError::new("Cerraste la ventana.")))
        .unwrap();
    assert!(matches!(app.next_data().await,
        Event::Failed { failure, ambiguous: false, .. } if failure == SENT));
    quiet(&mut app).await;
    // An answer to a challenge that no longer waits shows nothing.
    app.running
        .commands
        .send(Command::SolveChallenge {
            challenge,
            solve: true,
        })
        .await
        .unwrap();
    quiet(&mut app).await;
    assert!(app.verifications.try_recv().is_err());
}

#[tokio::test]
async fn a_friend_request_is_retried_on_its_own_route() {
    let mut app = Harness::new().await;
    let challenge = challenged(
        &mut app,
        Command::SendFriendRequest {
            username: "zoe".into(),
        },
        "POST /api/v10/users/@me/relationships ",
    )
    .await;
    solve(&mut app, challenge).await;
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/users/@me/relationships "));
    answered(&request);
    assert!(request.contains(r#""username":"zoe""#));
    reply
        .send("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into())
        .unwrap();
    quiet(&mut app).await;
}

#[tokio::test]
async fn a_lost_connection_ends_the_wait_without_sending() {
    let mut app = Harness::new().await;
    challenged(
        &mut app,
        rest::send(7),
        "POST /api/v10/channels/20/messages ",
    )
    .await;
    app.dispatch
        .send(("TEST_DISCONNECT", Value::Null))
        .await
        .unwrap();
    loop {
        match app.next_data().await {
            Event::Failed {
                failure, reason, ..
            } if failure == SENT => {
                assert_eq!(reason, crate::challenges::OFFLINE.to_string());
                break;
            }
            _ => {}
        }
    }
}

fn upload(fence: UploadFence) -> Command {
    let bytes = Bytes::from_static(b"hola");
    Command::SendFiles {
        scope: UploadSendScope {
            request: 105,
            account: UserId(42),
            channel: ChannelId(20),
            generation: 0,
            nonce: 5,
            expires_at: std::time::Instant::now() + Duration::from_secs(30),
        },
        content: String::new(),
        reply_to: None,
        files: vec![PreparedUpload::new(
            PendingAttachment {
                token: UploadToken(5),
                filename: "hola.txt".into(),
                size_bytes: bytes.len() as u64,
            },
            bytes,
        )
        .unwrap()],
        fence,
        mention_reply: false,
    }
}

/// One file staged and stored, up to its message request.
async fn staged(app: &mut Harness) -> PendingRequest {
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/attachments "));
    unanswered(&request);
    let host = request
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("loopback host")
        .to_owned();
    reply
        .send(response(
            200,
            &json!({"attachments": [{"id": "0", "upload_url": format!("http://{host}/storage/0"),
                "upload_filename": "synthetic/0/hola.txt"}]})
            .to_string(),
        ))
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PUT /storage/0 "));
    reply.send(response(200, "")).unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/messages "));
    (request, reply)
}

#[tokio::test]
async fn a_file_message_is_staged_again_and_sent_once_with_the_answer() {
    let mut app = Harness::new().await;
    let (_control, fence) = UploadFence::new();
    app.running.commands.send(upload(fence)).await.unwrap();
    let (request, reply) = staged(&mut app).await;
    unanswered(&request);
    reply.send(response(400, CHALLENGE)).unwrap();
    let Event::ChallengeRequired { challenge, failure } = next_offer(&mut app).await else {
        unreachable!()
    };
    assert_eq!(
        failure,
        Failure::SendFiles {
            request: 105,
            channel: ChannelId(20),
            nonce: 5
        }
    );
    solve(&mut app, challenge).await;
    let (request, reply) = staged(&mut app).await;
    answered(&request);
    let mut created = rest::message(300, "");
    created["attachments"] = json!([{"id": "301", "filename": "hola.txt", "size": 4}]);
    reply.send(response(200, &created.to_string())).unwrap();
    loop {
        match app.next_data().await {
            Event::MessageCreated { message, nonce } => {
                assert_eq!((message.id, nonce), (MessageId(300), Some(5)));
                break;
            }
            Event::UploadProgress { .. } => {}
            other => panic!("{other:?}"),
        }
    }
}
