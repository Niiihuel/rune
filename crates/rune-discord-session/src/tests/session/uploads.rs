//! File sends through the whole provider: admission, the one upload at a
//! time beside other requests, and the service's own confirmation.

use super::*;
use bytes::Bytes;
use rune_model::{PendingAttachment, UploadToken, UserId};
use rune_provider::{Failure, PreparedUpload, UploadError, UploadFence, UploadSendScope};

fn send_files(nonce: u64, generation: u64, fence: UploadFence, bytes: Bytes) -> Command {
    Command::SendFiles {
        scope: UploadSendScope {
            request: nonce + 100,
            account: UserId(42),
            channel: ChannelId(20),
            generation,
            nonce,
            expires_at: std::time::Instant::now() + Duration::from_secs(30),
        },
        content: String::new(),
        reply_to: None,
        files: vec![PreparedUpload::new(
            PendingAttachment {
                token: UploadToken(nonce),
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

/// The next event other than progress.
async fn outcome(app: &mut Harness) -> Event {
    loop {
        match app.next_data().await {
            Event::UploadProgress { .. } => {}
            event => return event,
        }
    }
}

fn refused(event: Event, nonce: u64, error: UploadError) {
    assert!(
        matches!(&event, Event::Failed { failure: Failure::SendFiles { nonce: n, .. }, reason, ambiguous: false }
            if *n == nonce && reason == error.label()),
        "{event:?}"
    );
}

#[tokio::test]
async fn a_file_send_is_staged_stored_and_confirmed_by_the_service_alone() {
    let mut app = Harness::new().await;
    let (_control, fence) = UploadFence::new();
    app.running
        .commands
        .send(send_files(5, 0, fence, Bytes::from_static(b"hola")))
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/attachments "));
    let host = request
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("loopback host");
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
    assert!(request.ends_with("\r\n\r\nhola") && !request.contains(SECRET));
    reply.send(response(200, "")).unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/messages "));
    let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(
        body["attachments"][0]["uploaded_filename"],
        "synthetic/0/hola.txt"
    );
    assert!(
        body.get("content").is_none(),
        "a file-only message carries no text"
    );
    let mut created = rest::message(300, "");
    created["author"]["id"] = json!("42");
    created["attachments"] = json!([{"id": "301", "filename": "hola.txt", "size": 4}]);
    reply.send(response(200, &created.to_string())).unwrap();
    let mut confirmed = false;
    while !confirmed {
        match app.next_data().await {
            Event::MessageCreated { message, nonce } => {
                assert_eq!((message.id, nonce), (MessageId(300), Some(5)));
                assert_eq!(message.attachments[0].filename, "hola.txt");
                confirmed = true;
            }
            Event::UploadProgress { .. } => {}
            other => panic!("{other:?}"),
        }
    }
}

#[tokio::test]
async fn a_new_posts_files_go_through_the_provider_with_their_progress() {
    let mut app = Harness::new().await;
    // A server whose forum lets everyone post with files.
    let everyone: u64 = 1 << 10 | 1 << 11 | 1 << 15 | 1 << 16;
    app.dispatch
        .send((
            "GUILD_CREATE",
            json!({"id": "10", "name": "Taller", "owner_id": "99",
                "roles": [{"id": "10", "name": "@everyone",
                    "permissions": everyone.to_string(), "position": 0}],
                "members": [{"user": {"id": "42", "username": "Owner"}, "roles": []}],
                "channels": [{"id": "50", "guild_id": "10", "type": 15, "name": "fotos",
                    "permission_overwrites": [], "flags": 0, "available_tags": []}]}),
        ))
        .await
        .unwrap();
    while !matches!(app.running.next().await, Event::ChannelChanged(channel) if channel.id == ChannelId(50))
    {
    }
    app.running
        .commands
        .send(Command::CreatePost {
            request: 4,
            forum: ChannelId(50),
            title: "Fotos".into(),
            tags: Vec::new(),
            content: String::new(),
            files: vec![PreparedUpload::new(
                PendingAttachment {
                    token: UploadToken(1),
                    filename: "hola.txt".into(),
                    size_bytes: 4,
                },
                Bytes::from_static(b"hola"),
            )
            .unwrap()],
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/50/attachments "));
    let host = request
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .expect("loopback host");
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
    assert!(request.starts_with("POST /api/v10/channels/50/threads "));
    reply
        .send(response(
            201,
            &json!({"id": "62", "type": 11, "guild_id": "10", "parent_id": "50",
                "name": "Fotos", "owner_id": "42",
                "thread_metadata": {"archived": false, "locked": false,
                    "auto_archive_duration": 4320, "archive_timestamp": "2026-10-17T12:00:00Z"}})
            .to_string(),
        ))
        .unwrap();
    // The provider hands the post's progress over, from none to all.
    let mut sent = Vec::new();
    loop {
        match app.next_data().await {
            Event::PostProgress {
                request: 4,
                sent_bytes,
                total_bytes: 4,
            } => sent.push(sent_bytes),
            Event::Created { request: 4, .. } => break,
            _ => {}
        }
    }
    assert_eq!((sent.first(), sent.last()), (Some(&0), Some(&4)));
}

#[tokio::test]
async fn a_send_from_another_session_too_large_or_while_one_runs_is_refused_at_once() {
    let mut app = Harness::new().await;
    let (_control, fence) = UploadFence::new();
    app.running
        .commands
        .send(send_files(5, 1, fence.clone(), Bytes::from_static(b"hola")))
        .await
        .unwrap();
    refused(outcome(&mut app).await, 5, UploadError::Denied);
    let large = Bytes::from(vec![0u8; 20 * 1024 * 1024 + 1]);
    app.running
        .commands
        .send(send_files(6, 0, fence.clone(), large))
        .await
        .unwrap();
    refused(outcome(&mut app).await, 6, UploadError::TooLarge);
    app.running
        .commands
        .send(send_files(7, 0, fence.clone(), Bytes::from_static(b"uno")))
        .await
        .unwrap();
    let (request, _held) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/attachments "));
    app.running
        .commands
        .send(send_files(8, 0, fence, Bytes::from_static(b"dos")))
        .await
        .unwrap();
    refused(outcome(&mut app).await, 8, UploadError::Busy);
    // Signing out drops the one in flight; nothing more reaches the service.
    app.running.commands.send(Command::SignOut).await.unwrap();
    assert_eq!(
        outcome(&mut app).await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
}
