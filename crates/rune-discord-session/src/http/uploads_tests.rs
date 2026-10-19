use std::{
    sync::{atomic::AtomicBool, Arc, Mutex},
    time::{Duration, Instant},
};

use bytes::Bytes;
use rune_model::{ChannelId, MessageId, PendingAttachment, UploadToken, UserId};
use rune_provider::{Envelope, Event, EventSink, PreparedUpload, UploadFence, UploadSendScope};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

use super::Http;
use crate::SessionSecret;

const SECRET: &str = "SYNTHETIC_UPLOAD_SESSION";

/// One request as the fixture saw it: the head and the body.
struct Seen {
    head: String,
    body: Vec<u8>,
}

impl Seen {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}

async fn read(socket: &mut TcpStream) -> Seen {
    let mut head = Vec::new();
    while !head.ends_with(b"\r\n\r\n") {
        assert!(head.len() < 8192, "fixture header limit");
        head.push(socket.read_u8().await.unwrap());
    }
    let head = String::from_utf8(head).unwrap();
    let length: usize = head
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            key.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse().unwrap())
        })
        .unwrap_or(0);
    assert!(length <= 4 * 1024 * 1024, "fixture body limit");
    let mut body = vec![0; length];
    socket.read_exact(&mut body).await.unwrap();
    Seen { head, body }
}

fn reply(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// A loopback service answering each request in turn with `answer`, which
/// gets the origin to name storage targets on itself.
async fn service(
    answer: impl Fn(&str, &Seen) -> String + Send + 'static,
) -> (Http, Arc<Mutex<Vec<Seen>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    let base = origin.clone();
    tokio::spawn(async move {
        while let Ok((mut socket, _)) = listener.accept().await {
            let request = read(&mut socket).await;
            let response = answer(&base, &request);
            log.lock().unwrap().push(request);
            let _ = socket.write_all(response.as_bytes()).await;
        }
    });
    (Http::local(&origin), seen)
}

fn target(origin: &str, index: usize) -> String {
    reply(
        200,
        &json!({"attachments": [{"id": index, "upload_url": format!("{origin}/storage/{index}?upload_id=synthetic"),
            "upload_filename": format!("synthetic/{index}/file")}]})
        .to_string(),
    )
}

fn created(files: &[(&str, usize)]) -> String {
    let attachments: Vec<Value> = files
        .iter()
        .enumerate()
        .map(|(index, (name, size))| json!({"id": (500 + index).to_string(), "filename": name, "size": size}))
        .collect();
    reply(
        200,
        &json!({"id": "900", "channel_id": "20", "author": {"id": "42", "username": "Owner"},
            "content": "con archivos", "timestamp": "2026-10-17T12:00:00Z", "edited_timestamp": null,
            "attachments": attachments, "mentions": [], "pinned": false})
        .to_string(),
    )
}

/// The usual answers: a target per file, the storage's OK and the message.
fn ordinary(origin: &str, seen: &Seen) -> String {
    let first = seen.head.lines().next().unwrap().to_owned();
    if first.starts_with("POST /api/v10/channels/20/attachments ") {
        target(
            origin,
            seen.json()["files"][0]["id"]
                .as_str()
                .unwrap()
                .parse()
                .unwrap(),
        )
    } else if first.starts_with("PUT /storage/") {
        reply(200, "")
    } else {
        created(&[("uno.txt", 300_000), ("dos.png", 3)])
    }
}

fn file(token: u64, name: &str, bytes: &[u8]) -> PreparedUpload {
    PreparedUpload::new(
        PendingAttachment {
            token: UploadToken(token),
            filename: name.into(),
            size_bytes: bytes.len() as u64,
        },
        Bytes::copy_from_slice(bytes),
    )
    .unwrap()
}

fn scope() -> UploadSendScope {
    UploadSendScope {
        request: 7,
        account: UserId(42),
        channel: ChannelId(20),
        generation: 0,
        nonce: 11,
        expires_at: Instant::now() + Duration::from_secs(30),
    }
}

fn sink() -> (EventSink, mpsc::Receiver<Envelope>) {
    let (sent, received) = mpsc::channel(256);
    (
        EventSink::new(sent, Arc::default(), Arc::new(|| {})),
        received,
    )
}

fn secret() -> SessionSecret {
    SessionSecret::try_from_local(SECRET.into()).unwrap()
}

#[tokio::test]
async fn each_file_is_staged_stored_without_the_session_and_named_by_one_message() {
    let (http, seen) = service(ordinary).await;
    let (events, mut received) = sink();
    let big = vec![7u8; 300_000];
    let files = [file(1, "uno.txt", &big), file(2, "dos.png", b"png")];
    let (_control, fence) = UploadFence::new();
    let posting = AtomicBool::new(false);
    let result = http
        .send_files(
            &secret(),
            &events,
            scope(),
            tokio::time::Instant::from_std(scope().expires_at),
            "con archivos",
            Some(MessageId(99)),
            true,
            &files,
            &fence,
            "r0000007b000000000000000b",
            &posting,
        )
        .await
        .unwrap();
    assert!(
        matches!(result.last(), Some(Event::MessageCreated { nonce: Some(11), message })
            if message.attachments.len() == 2 && message.id == MessageId(900)),
        "the service's own message confirms the send"
    );
    let seen = seen.lock().unwrap();
    let lines: Vec<_> = seen
        .iter()
        .map(|request| request.head.lines().next().unwrap().to_owned())
        .collect();
    assert_eq!(
        lines,
        [
            "POST /api/v10/channels/20/attachments HTTP/1.1",
            "PUT /storage/0?upload_id=synthetic HTTP/1.1",
            "POST /api/v10/channels/20/attachments HTTP/1.1",
            "PUT /storage/1?upload_id=synthetic HTTP/1.1",
            "POST /api/v10/channels/20/messages HTTP/1.1",
        ]
    );
    assert_eq!(
        seen[0].json(),
        json!({"files": [{"id": "0", "filename": "uno.txt", "file_size": 300_000, "is_clip": false}]})
    );
    // The storage gets the bytes and their type, never the account's session.
    for (put, (bytes, kind)) in [
        (1, (&big[..], "text/plain")),
        (3, (&b"png"[..], "image/png")),
    ] {
        assert_eq!(seen[put].body, bytes);
        let head = seen[put].head.to_ascii_lowercase();
        assert!(
            !head.contains("authorization") && !head.contains(SECRET.to_ascii_lowercase().as_str())
        );
        assert!(head.contains(&format!("content-type: {kind}")));
    }
    assert!(
        seen[0].head.contains(SECRET),
        "Discord's own routes carry it"
    );
    let message = seen[4].json();
    assert_eq!(
        message["attachments"],
        json!([{"id": "0", "filename": "uno.txt", "uploaded_filename": "synthetic/0/file"},
               {"id": "1", "filename": "dos.png", "uploaded_filename": "synthetic/1/file"}])
    );
    assert_eq!(message["nonce"], "r0000007b000000000000000b");
    assert_eq!(message["content"], "con archivos");
    assert_eq!(message["message_reference"]["message_id"], "99");
    // A reply with files notifies its author when asked, as a text one.
    assert_eq!(message["allowed_mentions"]["replied_user"], true);
    assert!(posting.load(std::sync::atomic::Ordering::Acquire));
    // Progress climbs to the whole batch; it is not a confirmation.
    let mut last = 0;
    while let Ok(envelope) = received.try_recv() {
        if let Event::UploadProgress {
            sent_bytes,
            total_bytes,
            ..
        } = envelope.event
        {
            assert!(sent_bytes >= last && total_bytes == 300_003);
            last = sent_bytes;
        }
    }
    assert_eq!(last, 300_003);
}

#[tokio::test]
async fn a_target_outside_discords_storage_or_a_refused_store_sends_nothing() {
    for refusal in ["foreign", "rejected"] {
        let (http, seen) = service(move |origin, seen| {
            let first = seen.head.lines().next().unwrap().to_owned();
            if first.starts_with("POST /api/v10/channels/20/attachments ") {
                if refusal == "foreign" {
                    reply(200, &json!({"attachments": [{"id": "0",
                        "upload_url": "https://storage.example.test/upload", "upload_filename": "x"}]})
                        .to_string())
                } else {
                    target(origin, 0)
                }
            } else {
                reply(403, "{}")
            }
        })
        .await;
        let (events, _received) = sink();
        let (_control, fence) = UploadFence::new();
        let posting = AtomicBool::new(false);
        let error = http
            .send_files(
                &secret(),
                &events,
                scope(),
                tokio::time::Instant::from_std(scope().expires_at),
                "",
                None,
                false,
                &[file(1, "uno.txt", b"uno")],
                &fence,
                "nonce",
                &posting,
            )
            .await
            .unwrap_err();
        assert!(!error.ambiguous && !posting.load(std::sync::atomic::Ordering::Acquire));
        let seen = seen.lock().unwrap();
        assert!(
            !seen
                .iter()
                .any(|request| request.head.contains("/messages")),
            "{refusal}: no message without every file stored"
        );
    }
}

#[tokio::test]
async fn cancelling_or_expiring_before_the_message_sends_nothing() {
    let (http, seen) = service(ordinary).await;
    let (events, _received) = sink();
    let (control, fence) = UploadFence::new();
    control.send_replace(true);
    let posting = AtomicBool::new(false);
    let files = [file(1, "uno.txt", b"uno")];
    let error = http
        .send_files(
            &secret(),
            &events,
            scope(),
            tokio::time::Instant::from_std(scope().expires_at),
            "",
            None,
            false,
            &files,
            &fence,
            "nonce",
            &posting,
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.reason.to_string(),
        rune_provider::UploadError::Cancelled.label()
    );
    let (_control, fence) = UploadFence::new();
    let expired = UploadSendScope {
        expires_at: Instant::now(),
        ..scope()
    };
    let error = http
        .send_files(
            &secret(),
            &events,
            expired,
            tokio::time::Instant::from_std(expired.expires_at),
            "",
            None,
            false,
            &files,
            &fence,
            "nonce",
            &posting,
        )
        .await
        .unwrap_err();
    assert_eq!(
        error.reason.to_string(),
        rune_provider::UploadError::Expired.label()
    );
    assert!(!error.ambiguous && !posting.load(std::sync::atomic::Ordering::Acquire));
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn only_discords_upload_storage_receives_bytes() {
    let http = Http::new().unwrap();
    assert!(http
        .storage_url("https://discord-attachments-uploads-prd.storage.googleapis.com/a?upload_id=x")
        .is_some());
    for refused in [
        "http://discord-attachments-uploads-prd.storage.googleapis.com/a",
        "https://discord-attachments-uploads-prd.storage.googleapis.com:8443/a",
        "https://user:pass@discord-attachments-uploads-prd.storage.googleapis.com/a",
        "https://discord-attachments-uploads-prd.storage.googleapis.com/a#f",
        "https://storage.googleapis.com/a",
        "https://discord-attachments-uploads-prd.storage.googleapis.com.evil.test/a",
        "http://127.0.0.1:9/a",
    ] {
        assert!(http.storage_url(refused).is_none(), "{refused}");
    }
}

#[tokio::test]
async fn a_forum_post_stages_its_files_on_the_forum_and_names_them_in_its_first_message() {
    let (http, seen) = service(|origin, seen| {
        let first = seen.head.lines().next().unwrap().to_owned();
        if first.starts_with("POST /api/v10/channels/23/attachments ") {
            target(origin, 0)
        } else if first.starts_with("PUT /storage/") {
            reply(200, "")
        } else {
            reply(
                201,
                &json!({"id": "62", "type": 11, "guild_id": "1", "parent_id": "23",
                    "name": "¿Este lente?", "owner_id": "42",
                    "thread_metadata": {"archived": false, "locked": false,
                        "auto_archive_duration": 4320, "archive_timestamp": "2026-10-17T12:00:00Z"}})
                .to_string(),
            )
        }
    })
    .await;
    let command = rune_provider::Command::CreatePost {
        request: 4,
        forum: ChannelId(23),
        title: "¿Este lente?".into(),
        tags: Vec::new(),
        content: String::new(),
        files: vec![file(1, "lente.jpg", b"jpg")],
    };
    let (sink, mut received) = sink();
    let events = http
        .create_post(&secret(), &command, Some(&sink))
        .await
        .unwrap();
    assert!(matches!(
        events.last(),
        Some(Event::Created { request: 4, .. })
    ));
    // Its files report their bytes as the post's, from none to all.
    let mut reports = Vec::new();
    while let Ok(envelope) = received.try_recv() {
        if let Event::PostProgress {
            request: 4,
            sent_bytes,
            total_bytes: 3,
        } = envelope.event
        {
            reports.push(sent_bytes);
        }
    }
    assert_eq!(reports.first(), Some(&0));
    assert_eq!(reports.last(), Some(&3));
    let seen = seen.lock().unwrap();
    let lines: Vec<&str> = seen
        .iter()
        .map(|request| request.head.lines().next().unwrap())
        .collect();
    assert!(lines[0].starts_with("POST /api/v10/channels/23/attachments "));
    assert!(lines[1].starts_with("PUT /storage/0"));
    assert!(lines[2].starts_with("POST /api/v10/channels/23/threads "));
    // The storage gets the bytes alone, without the account.
    assert!(!seen[1].head.contains(SECRET) && seen[1].body == b"jpg");
    let body = seen[2].json();
    assert_eq!(
        body["message"]["attachments"],
        json!([{"id": "0", "filename": "lente.jpg", "uploaded_filename": "synthetic/0/file"}])
    );
    assert_eq!(body["name"], "¿Este lente?");
}

#[test]
fn the_storage_is_told_the_type_of_the_extension_in_any_case() {
    assert_eq!(super::content_type("Foto.JPG"), "image/jpeg");
    assert_eq!(super::content_type("voz.opus"), "audio/ogg");
    assert_eq!(
        super::content_type("notas.tar.gz"),
        "application/octet-stream"
    );
    assert_eq!(super::content_type("LEEME"), "application/octet-stream");
}
