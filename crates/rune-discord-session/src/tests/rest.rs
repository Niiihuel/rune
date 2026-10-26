use super::*;
use rune_model::{ChannelId, MessageId};
use serde_json::{json, Value};

pub(super) fn message(id: u64, content: &str) -> Value {
    json!({"id":id.to_string(),"channel_id":"20","author":{"id":"42","username":"Owner"},
        "content":content,"timestamp":"2026-10-14T12:34:56Z","edited_timestamp":null,
        "attachments":[],"mentions":[],"pinned":false})
}

pub(super) fn history(request: u64) -> Command {
    Command::LoadHistory {
        request,
        channel: ChannelId(20),
        before: None,
        limit: 50,
    }
}

pub(super) fn send(nonce: u64) -> Command {
    Command::SendMessage {
        channel: ChannelId(20),
        content: "hello".into(),
        reply_to: None,
        nonce,
        mention_reply: false,
    }
}

#[tokio::test]
async fn history_uses_bounded_cursor_and_returns_oldest_first() {
    let (http, captured) = fixture(response(
        200,
        &json!([message(102, "newer"), message(100, "older")]).to_string(),
    ))
    .await;
    let command = Command::LoadHistory {
        request: 7,
        channel: ChannelId(20),
        before: Some(MessageId(200)),
        limit: 200,
    };
    let events = http.execute(&secret(), &command, "").await.unwrap();
    let Event::HistoryLoaded {
        request,
        channel,
        before,
        messages,
        reached_start,
    } = events.last().unwrap()
    else {
        panic!("history result");
    };
    assert_eq!(
        (*request, *channel, *before),
        (7, ChannelId(20), Some(MessageId(200)))
    );
    assert_eq!(
        messages.iter().map(|m| m.id).collect::<Vec<_>>(),
        vec![MessageId(100), MessageId(102)]
    );
    assert!(*reached_start);
    assert!(captured
        .await
        .unwrap()
        .starts_with("GET /api/v10/channels/20/messages?limit=100&before=200 HTTP/1.1"));
}

#[tokio::test]
async fn history_rejects_foreign_channels_and_out_of_cursor_pages() {
    for mut wrong in [message(100, "foreign"), message(200, "cursor")] {
        if wrong["id"] == "100" {
            wrong["channel_id"] = json!("21");
        }
        let (http, _) = fixture(response(200, &json!([wrong]).to_string())).await;
        let command = Command::LoadHistory {
            request: 7,
            channel: ChannelId(20),
            before: Some(MessageId(200)),
            limit: 50,
        };
        let error = http.execute(&secret(), &command, "").await.unwrap_err();
        assert!(!error.ambiguous);
    }
}

#[tokio::test]
async fn send_has_a_bounded_nonce_explicit_mentions_and_reply_reference() {
    let (http, captured) = fixture(response(200, &message(100, "hello").to_string())).await;
    let command = Command::SendMessage {
        channel: ChannelId(20),
        content: "hello".into(),
        reply_to: Some(MessageId(99)),
        nonce: 7,
        mention_reply: false,
    };
    let nonce = crate::messages::send_nonce(123, 7);
    let events = http.execute(&secret(), &command, &nonce).await.unwrap();
    assert!(
        matches!(events.last(),Some(Event::MessageCreated {nonce:Some(7),message}) if message.id==MessageId(100))
    );
    let request = captured.await.unwrap();
    assert!(request.starts_with("POST /api/v10/channels/20/messages HTTP/1.1"));
    let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["nonce"], nonce);
    assert_eq!(
        body["allowed_mentions"],
        json!({"parse":["users"],"replied_user":false})
    );
    assert_eq!(
        body["message_reference"],
        json!({"channel_id":"20","message_id":"99","fail_if_not_exists":true})
    );
    assert!(body.get("enforce_nonce").is_none());
}

#[tokio::test]
async fn sticker_send_is_a_separate_message_with_a_nonce_and_no_mentions() {
    let mut result = message(104, "");
    result["sticker_items"] = json!([{"id":"501","name":"Wave","format_type":1}]);
    let (http, captured) = fixture(response(200, &result.to_string())).await;
    let command = Command::SendSticker {
        channel: ChannelId(20),
        sticker: rune_model::StickerId(501),
        nonce: 9,
    };
    let nonce = crate::messages::send_nonce(123, 9);
    let events = http.execute(&secret(), &command, &nonce).await.unwrap();
    assert!(
        matches!(events.last(), Some(Event::MessageCreated { nonce:Some(9), message })
        if message.content.is_empty() && message.stickers[0].id == rune_model::StickerId(501))
    );
    let request = captured.await.unwrap();
    assert!(request.starts_with("POST /api/v10/channels/20/messages HTTP/1.1"));
    let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body["sticker_ids"], json!(["501"]));
    assert_eq!(body["nonce"], nonce);
    assert_eq!(body["allowed_mentions"]["parse"], json!([]));
    assert!(
        body.get("content").is_none(),
        "the text draft is not sent with a sticker"
    );
}

#[tokio::test]
async fn edits_omit_attachment_mutations_and_require_matching_confirmation() {
    let (http, captured) = fixture(response(200, &message(100, "edited").to_string())).await;
    let command = Command::EditMessage {
        channel: ChannelId(20),
        message: MessageId(100),
        content: "edited".into(),
    };
    let events = http.execute(&secret(), &command, "").await.unwrap();
    assert!(
        matches!(events.last(),Some(Event::MessageUpdated(message)) if message.content=="edited")
    );
    let request = captured.await.unwrap();
    assert!(request.starts_with("PATCH /api/v10/channels/20/messages/100 HTTP/1.1"));
    let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert!(body.get("attachments").is_none());
    let (http, _) = fixture(response(200, &message(101, "wrong").to_string())).await;
    assert!(
        http.execute(&secret(), &command, "")
            .await
            .unwrap_err()
            .ambiguous
    );
}

#[tokio::test]
async fn deletion_needs_the_documented_no_content_confirmation() {
    let command = Command::DeleteMessage {
        channel: ChannelId(20),
        message: MessageId(100),
    };
    let (http, captured) = fixture(response(204, "")).await;
    assert_eq!(
        http.execute(&secret(), &command, "").await.unwrap(),
        vec![Event::MessageDeleted {
            channel: ChannelId(20),
            message: MessageId(100)
        }]
    );
    assert!(captured
        .await
        .unwrap()
        .starts_with("DELETE /api/v10/channels/20/messages/100 HTTP/1.1"));
    let (http, _) = fixture(response(200, "{}")).await;
    assert!(
        http.execute(&secret(), &command, "")
            .await
            .unwrap_err()
            .ambiguous
    );
}

#[tokio::test]
async fn a_vote_replaces_the_accounts_answers_and_needs_no_content() {
    let command = Command::VotePoll {
        channel: ChannelId(20),
        message: MessageId(100),
        answers: vec![1, 3],
    };
    let (http, captured) = fixture(response(204, "")).await;
    assert_eq!(http.execute(&secret(), &command, "").await.unwrap(), vec![]);
    let request = captured.await.unwrap();
    assert!(request.starts_with("PUT /api/v10/channels/20/polls/100/answers/@me HTTP/1.1"));
    let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    assert_eq!(body, json!({"answer_ids":["1","3"]}));
    let (http, _) = fixture(response(200, "{}")).await;
    assert!(
        http.execute(&secret(), &command, "")
            .await
            .unwrap_err()
            .ambiguous,
        "an unexpected confirmation leaves the vote's fate unknown"
    );
    let (http, _) = fixture(response(403, "{}")).await;
    let refused = http.execute(&secret(), &command, "").await.unwrap_err();
    assert!(!refused.ambiguous && !refused.ends_session);
}

#[tokio::test]
async fn rate_limit_holds_subsequent_requests_without_automatic_retry_or_body_echo() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http = Http::local(&format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        socket
            .write_all(
                response(
                    429,
                    &json!({"retry_after":60.0,"message":SECRET}).to_string(),
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let first = http
        .execute(&secret(), &send(1), "fixture")
        .await
        .unwrap_err();
    assert!(!first.ambiguous && !first.ends_session);
    assert!(!format!("{first:?}").contains(SECRET));
    let second = http.execute(&secret(), &history(2), "").await.unwrap_err();
    assert!(second.reason.to_string().contains("esperar"));
    server.await.unwrap();
}

fn limited(headers: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 429 Test\r\nContent-Type: application/json\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn a_typing_rate_limit_holds_only_typing_and_never_a_message() {
    let typing = Command::StartTyping {
        channel: ChannelId(20),
    };
    // Its bucket, by header or by body: the message still goes at once.
    for first in [
        limited("Retry-After: 30\r\n", "{}"),
        limited("", r#"{"retry_after": 30.0, "global": false}"#),
    ] {
        let (http, captured) = exchanges(vec![
            first,
            response(200, &message(100, "hello").to_string()),
        ])
        .await;
        let mut captured = captured;
        assert!(http.execute(&secret(), &typing, "").await.is_err());
        assert!(captured
            .recv()
            .await
            .unwrap()
            .starts_with("POST /api/v10/channels/20/typing"));
        let held = http.execute(&secret(), &typing, "").await.unwrap_err();
        assert!(held.reason.to_string().contains("esperar"), "typing waits");
        http.execute(&secret(), &send(1), "fixture").await.unwrap();
        assert!(captured
            .recv()
            .await
            .unwrap()
            .starts_with("POST /api/v10/channels/20/messages"));
    }
    // A global limit met by typing still holds everything.
    for global in [
        limited("Retry-After: 30\r\nX-RateLimit-Global: true\r\n", "{}"),
        limited("Retry-After: 30\r\nX-RateLimit-Scope: global\r\n", "{}"),
        limited("", r#"{"retry_after": 30.0, "global": true}"#),
    ] {
        let (http, _captured) = exchanges(vec![global]).await;
        assert!(http.execute(&secret(), &typing, "").await.is_err());
        let held = http
            .execute(&secret(), &send(1), "fixture")
            .await
            .unwrap_err();
        assert!(held.reason.to_string().contains("esperar"));
    }
    // And a message's limit holds typing too.
    let (http, _captured) = exchanges(vec![limited("Retry-After: 30\r\n", "{}")]).await;
    assert!(http.execute(&secret(), &send(1), "fixture").await.is_err());
    assert!(http.execute(&secret(), &typing, "").await.is_err());
}

#[tokio::test]
async fn failed_send_is_ambiguous_and_never_repeats_itself() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http = Http::local(&format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        socket
            .write_all(response(503, SECRET).as_bytes())
            .await
            .unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    let error = http
        .execute(&secret(), &send(1), "fixture")
        .await
        .unwrap_err();
    assert!(error.ambiguous && !error.ends_session);
    assert!(!format!("{error:?}").contains(SECRET));
    server.await.unwrap();
}

#[tokio::test]
async fn limits_and_known_rejections_do_not_claim_an_ambiguous_write() {
    let http = Http::local("http://127.0.0.1:9");
    for content in [" ".into(), "🦀".repeat(1001)] {
        let command = Command::SendMessage {
            channel: ChannelId(20),
            content,
            reply_to: None,
            nonce: 1,
            mention_reply: false,
        };
        assert!(
            !http
                .execute(&secret(), &command, "fixture")
                .await
                .unwrap_err()
                .ambiguous
        );
    }
    for code in [400, 401, 403, 404] {
        let (http, _) = fixture(response(code, SECRET)).await;
        let error = http
            .execute(&secret(), &send(1), "fixture")
            .await
            .unwrap_err();
        assert!(!error.ambiguous);
        assert_eq!(error.ends_session, code == 401);
        assert!(!format!("{error:?}").contains(SECRET));
    }
}

#[tokio::test]
async fn rejected_sessions_and_rate_limits_survive_malformed_oversized_error_bodies() {
    let (http, _) = fixture(
        "HTTP/1.1 401 Rejected\r\nContent-Length: 999999999\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    let error = http
        .execute(&secret(), &send(1), "fixture")
        .await
        .unwrap_err();
    assert!(error.ends_session && !error.ambiguous);
    let (http, _) = fixture(
        "HTTP/1.1 429 Limited\r\nContent-Length: 999999999\r\nConnection: close\r\n\r\n".into(),
    )
    .await;
    assert!(
        !http
            .execute(&secret(), &send(1), "fixture")
            .await
            .unwrap_err()
            .ambiguous
    );
    assert!(http
        .execute(&secret(), &history(2), "")
        .await
        .unwrap_err()
        .reason
        .to_string()
        .contains("esperar"));
}

#[tokio::test]
async fn rejection_metadata_separates_account_and_action_requirements_without_remote_text() {
    for (code, ends_session, label) in [
        (50014, true, "sesión"),
        (40002, true, "cuenta"),
        (50178, true, "cuenta"),
        (60003, false, "esta acción"),
    ] {
        for status in [400, 403] {
            // A code has priority over simultaneous challenge fields.
            let body = json!({"code":code,"message":SECRET,"captcha_key":[SECRET],
                "errors":{"private":{"nested":SECRET}}});
            let (http, _) = fixture(response(status, &body.to_string())).await;
            let error = http
                .execute(&secret(), &send(1), "fixture")
                .await
                .unwrap_err();
            assert_eq!(error.ends_session, ends_session);
            assert!(!error.ambiguous);
            assert!(error.reason.to_string().contains(label));
            assert!(!format!("{error:?}").contains(SECRET));
        }
    }
    for field in [
        "captcha_key",
        "captcha_sitekey",
        "captcha_rqdata",
        "captcha_rqtoken",
        "captcha_session_id",
    ] {
        let mut body = json!({"code":999999,"message":SECRET});
        body[field] = json!({"ignored":[SECRET]});
        let (http, _) = fixture(response(400, &body.to_string())).await;
        let error = http
            .execute(&secret(), &send(1), "fixture")
            .await
            .unwrap_err();
        assert!(!error.ends_session && !error.ambiguous);
        assert!(error.reason.to_string().contains("verificación"));
        assert!(!format!("{error:?}").contains(SECRET));
    }
}

#[tokio::test]
async fn malformed_unknown_and_oversized_rejections_never_invent_session_failure() {
    for body in [
        "invalid-json",
        "null",
        r#"{"code":999999,"captcha_key":null}"#,
        "[50014,null,null,null,null,null]",
        " \n [40002,null,null,null,null,null]",
        r#"{"code":"50014"}"#,
        r#"{"code":50014,"code":60003}"#,
    ] {
        let (http, _) = fixture(response(400, body)).await;
        let error = http
            .execute(&secret(), &send(1), "fixture")
            .await
            .unwrap_err();
        assert!(!error.ends_session && !error.ambiguous);
        assert_eq!(
            error.reason.to_string(),
            "Discord rechazó los datos de esta acción."
        );
    }
    let body = format!("{{\"code\":50014,\"message\":\"{}\"}}", "x".repeat(65536));
    for reply in [
        response(403, &body),
        format!("HTTP/1.1 403 Rejected\r\nConnection: close\r\n\r\n{body}"),
    ] {
        let (http, _) = fixture(reply).await;
        let error = http
            .execute(&secret(), &send(1), "fixture")
            .await
            .unwrap_err();
        assert!(!error.ends_session && !error.ambiguous);
        assert_eq!(
            error.reason.to_string(),
            "No tenés permiso para completar esta acción."
        );
    }
}

/// Several sequential exchanges, one connection each.
async fn exchanges(replies: Vec<String>) -> (Http, mpsc::UnboundedReceiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http = Http::local(&format!("http://{}", listener.local_addr().unwrap()));
    let (sent, received) = mpsc::unbounded_channel();
    tokio::spawn(async move {
        for reply in replies {
            let (mut socket, _) = listener.accept().await.unwrap();
            let _ = sent.send(request(&mut socket).await);
            let _ = socket.write_all(reply.as_bytes()).await;
        }
    });
    (http, received)
}

fn pins(items: Vec<(u64, &str)>, has_more: bool) -> String {
    let items: Vec<Value> = items
        .into_iter()
        .map(|(id, at)| json!({"pinned_at":at,"message":message(id, "pinned")}))
        .collect();
    response(200, &json!({"items":items,"has_more":has_more}).to_string())
}

#[tokio::test]
async fn pins_are_read_in_bounded_pages_newest_first_and_never_cut() {
    let (http, mut captured) = exchanges(vec![
        pins(vec![(103, "2026-10-14T12:00:00Z")], true),
        pins(vec![(101, "2026-10-13T12:00:00+00:00")], false),
    ])
    .await;
    let load = Command::LoadPins {
        request: 3,
        channel: ChannelId(20),
    };
    let events = http.execute(&secret(), &load, "").await.unwrap();
    let Some(Event::PinsLoaded {
        request: 3,
        messages,
        ..
    }) = events.last()
    else {
        panic!("pins")
    };
    assert_eq!(
        messages
            .iter()
            .map(|m| (m.id, m.pinned))
            .collect::<Vec<_>>(),
        [(MessageId(103), true), (MessageId(101), true)]
    );
    assert!(captured
        .recv()
        .await
        .unwrap()
        .starts_with("GET /api/v10/channels/20/messages/pins?limit=50 HTTP/1.1"));
    let second = captured.recv().await.unwrap();
    assert!(second.starts_with("GET /api/v10/channels/20/messages/pins?limit=50&before=%32%30"));
    assert!(!second.contains("12:00"), "the cursor is escaped whole");

    // Past the documented maximum the list is refused, not shown partial.
    let pages = (0..5)
        .map(|page| pins(vec![(200 + page, "2026-10-14T12:00:00Z")], true))
        .collect();
    let (http, _) = exchanges(pages).await;
    let error = http.execute(&secret(), &load, "").await.unwrap_err();
    assert!(!error.ambiguous);
    let mut foreign = message(100, "elsewhere");
    foreign["channel_id"] = json!("21");
    for broken in [
        response(
            200,
            &json!({"items":[{"pinned_at":"x","message":foreign}],"has_more":false}).to_string(),
        ),
        response(200, &json!({"items":[]}).to_string()),
        pins(vec![(100, "a"), (100, "b")], false),
    ] {
        let (http, _) = exchanges(vec![broken]).await;
        assert!(
            !http
                .execute(&secret(), &load, "")
                .await
                .unwrap_err()
                .ambiguous
        );
    }
}

#[tokio::test]
async fn typing_accepts_its_documented_answer_and_a_reaction_only_no_content() {
    let typing = Command::StartTyping {
        channel: ChannelId(20),
    };
    for reply in [
        response(204, ""),
        response(200, r#"{"message_send_cooldown_ms":0}"#),
    ] {
        let (http, captured) = fixture(reply).await;
        assert!(http
            .execute(&secret(), &typing, "")
            .await
            .unwrap()
            .is_empty());
        let request = captured.await.unwrap();
        assert!(request.starts_with("POST /api/v10/channels/20/typing HTTP/1.1"));
        assert!(request.ends_with("\r\n\r\n"), "no body");
    }
    // A reaction answered with anything but 204 may have happened: ambiguous.
    let (http, _) = fixture(response(200, "{}")).await;
    let error = http
        .react(&secret(), ChannelId(20), MessageId(100), "👍", true)
        .await
        .unwrap_err();
    assert!(error.ambiguous);
    let (http, captured) = fixture(response(204, "")).await;
    http.react(&secret(), ChannelId(20), MessageId(100), "1️⃣", false)
        .await
        .unwrap();
    assert!(captured.await.unwrap().starts_with(
        "DELETE /api/v10/channels/20/messages/100/reactions/%31%EF%B8%8F%E2%83%A3/@me HTTP/1.1"
    ));
}

fn stream_preview() -> Command {
    Command::LoadStreamPreview {
        server: Some(rune_model::ServerId(40)),
        channel: ChannelId(30),
        owner: rune_model::UserId(2),
    }
}

/// The picture a preview answer carries, if any.
fn preview_image(events: &[Event]) -> Option<rune_model::RemoteImage> {
    match events {
        [Event::StreamPreview {
            channel: ChannelId(30),
            owner: rune_model::UserId(2),
            image,
            ..
        }] => image.clone(),
        other => panic!("{other:?}"),
    }
}

/// The picture `url` names for the stream of [`stream_preview`], when
/// Discord answers `{url}`.
async fn previewed(url: &str) -> Option<rune_model::RemoteImage> {
    let (http, _) = fixture(response(200, &json!({ "url": url }).to_string())).await;
    let events = http
        .execute(&secret(), &stream_preview(), "")
        .await
        .unwrap();
    preview_image(&events)
}

#[tokio::test]
async fn a_stream_preview_is_a_get_under_its_key_and_only_a_cdn_picture_of_it_is_shown() {
    let good = "https://cdn.discordapp.com/streams/guild:40:30:2/0123456789abcdef.png";
    let (http, captured) = fixture(response(200, &json!({"url": good}).to_string())).await;
    let events = http
        .execute(&secret(), &stream_preview(), "")
        .await
        .unwrap();
    // The time asked goes along, as the official client sends it.
    let asked = captured.await.unwrap();
    let version = asked
        .strip_prefix("GET /api/v10/streams/guild:40:30:2/preview?version=")
        .and_then(|rest| rest.split_once(" HTTP/1.1"))
        .map(|(version, _)| version.parse::<u64>().unwrap())
        .unwrap();
    assert!(version > 1_700_000_000_000, "{asked}");
    let image = preview_image(&events).unwrap();
    assert_eq!(image.url, good);
    assert_eq!(image.kind, rune_model::ImageKind::Banner);
    // What the service has not shown is not assumed: either host, the key's
    // colons encoded, any plain file name, a signed query.
    for shape in [
        "https://media.discordapp.net/streams/guild:40:30:2/0123456789abcdef.png",
        "https://cdn.discordapp.com/streams/guild%3A40%3A30%3A2/0123456789abcdef.png",
        "https://cdn.discordapp.com/streams/guild%3a40%3a30%3a2/preview.jpg",
        "https://cdn.discordapp.com/streams/guild:40:30:2/preview?ex=6720f1a2&is=671fa022&hm=ab12",
        "https://media.discordapp.net/streams/guild:40:30:2/1730000000000/thumb.jpeg?width=512&height=288",
        "https://cdn.discordapp.com/streams/guild:40:30:2/0123",
    ] {
        let image = previewed(shape).await.unwrap_or_else(|| panic!("{shape}"));
        assert!(image.url.starts_with("https://"), "{shape}");
    }
    // Elsewhere, another stream's, a way out of it, or anything an address
    // could smuggle: no picture.
    for bad in [
        "https://evil.example/streams/guild:40:30:2/0123456789abcdef.png",
        "https://cdn.discordapp.com.evil.example/streams/guild:40:30:2/a.png",
        "https://evil.cdn.discordapp.com/streams/guild:40:30:2/a.png",
        "http://cdn.discordapp.com/streams/guild:40:30:2/a.png",
        "https://cdn.discordapp.com:8443/streams/guild:40:30:2/a.png",
        "https://user:pass@cdn.discordapp.com/streams/guild:40:30:2/a.png",
        "https://cdn.discordapp.com/streams/guild:40:30:2/a.png#x",
        "https://cdn.discordapp.com/streams/guild:40:30:3/0123456789abcdef.png",
        "https://cdn.discordapp.com/streams/guild%3A40%3A30%3A3/a.png",
        "https://cdn.discordapp.com/streams/guild:40:30:2/../token.png",
        "https://cdn.discordapp.com/streams/guild:40:30:2/%2e%2e/token.png",
        "https://cdn.discordapp.com/streams/guild:40:30:2/%2E%2E/%2E%2E/attachments/a.png",
        "https://cdn.discordapp.com/streams/guild:40:30:2/a%2Fb.png",
        "https://cdn.discordapp.com/streams/guild:40:30:2/",
        "https://cdn.discordapp.com/streams/guild:40:30:2",
        "https://cdn.discordapp.com/attachments/guild:40:30:2/a.png",
        "https://cdn.discordapp.com/streams/guild:40:30:2/a\\b.png",
        "https://cdn.discordapp.com/streams/guild:40:30:2/a\nb.png",
        "data:image/jpeg;base64,AAAA",
        "",
    ] {
        assert_eq!(previewed(bad).await, None, "{bad}");
    }
    // None yet: no picture, and nothing for the interface to report.
    let (http, _) = fixture(response(404, "{}")).await;
    let events = http
        .execute(&secret(), &stream_preview(), "")
        .await
        .unwrap();
    assert_eq!(preview_image(&events), None);
    let (http, _) = fixture(response(200, "{}")).await;
    let events = http
        .execute(&secret(), &stream_preview(), "")
        .await
        .unwrap();
    assert_eq!(preview_image(&events), None);
    // Each answer said how it went, and never with what: no key, id or
    // address.
    let lines: Vec<_> = crate::voice::diag::captured::lines()
        .into_iter()
        .filter(|line| line.starts_with("rune: stream preview: GET "))
        .collect();
    for wanted in [
        "rune: stream preview: GET answered 200: a picture on cdn.discordapp.com",
        "rune: stream preview: GET answered 200: a picture on media.discordapp.net",
        "rune: stream preview: GET answered 200: no picture, a host not of Discord's media (evil.example)",
        "rune: stream preview: GET answered 200: no picture, a port in the url (cdn.discordapp.com)",
        "rune: stream preview: GET answered 200: no picture, another stream's key (cdn.discordapp.com)",
        "rune: stream preview: GET answered 200: no picture, not under /streams/ (cdn.discordapp.com)",
        "rune: stream preview: GET answered 200: no picture, no url in the answer",
        "rune: stream preview: GET answered 404: no picture",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "{wanted}: {lines:#?}");
    }
    for line in &lines {
        assert!(
            !line.contains("guild") && !line.contains("30:2") && !line.contains("0123456789"),
            "{line}"
        );
    }
}

#[tokio::test]
async fn a_calls_stream_preview_is_asked_under_its_call_key() {
    let good = "https://media.discordapp.net/streams/call%3A30%3A2/abc.jpg";
    let (http, captured) = fixture(response(200, &json!({"url": good}).to_string())).await;
    let command = Command::LoadStreamPreview {
        server: None,
        channel: ChannelId(30),
        owner: rune_model::UserId(2),
    };
    let events = http.execute(&secret(), &command, "").await.unwrap();
    assert!(captured
        .await
        .unwrap()
        .starts_with("GET /api/v10/streams/call:30:2/preview?version="));
    assert_eq!(preview_image(&events).unwrap().url, good);
}

#[tokio::test]
async fn a_rate_limited_preview_waits_its_retry_after_and_a_rejected_session_ends() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http = Http::local(&format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        let limited = response(429, &json!({"retry_after": 30.0}).to_string());
        socket.write_all(limited.as_bytes()).await.unwrap();
        // Nothing more is asked while it holds.
        assert!(
            tokio::time::timeout(Duration::from_millis(100), listener.accept())
                .await
                .is_err()
        );
    });
    // Each answer says how long is left to wait, for the interface to ask
    // again only then.
    for _ in 0..2 {
        let events = http
            .execute(&secret(), &stream_preview(), "")
            .await
            .unwrap();
        assert_eq!(preview_image(&events), None);
        let [Event::StreamPreview {
            retry_after: Some(wait),
            ..
        }] = events.as_slice()
        else {
            panic!("{events:?}")
        };
        assert!(*wait > Duration::from_secs(29) && *wait <= Duration::from_secs(30));
    }
    server.await.unwrap();
    // Pictures alone are held: a message still goes.
    assert!(http.preview_hold().is_some());
    assert!(!http.held_for_everything());
    let (http, _) = fixture(response(401, "{}")).await;
    let error = http
        .execute(&secret(), &stream_preview(), "")
        .await
        .unwrap_err();
    assert!(error.ends_session);
}

// Streams (transmisiones).
#[tokio::test]
async fn the_own_streams_picture_goes_up_as_a_jpeg_data_url_under_its_key() {
    let jpeg = vec![0xFF, 0xD8, 0xFF, 0xE0, 1, 2, 3];
    let upload = |server| Command::UploadStreamPreview {
        server,
        channel: ChannelId(30),
        owner: rune_model::UserId(42),
        jpeg: jpeg.clone(),
    };
    assert!(crate::http::preview_fits(&upload(None)));
    let (http, captured) = fixture(response(204, "")).await;
    let events = http
        .execute(&secret(), &upload(Some(rune_model::ServerId(40))), "")
        .await
        .unwrap();
    assert!(events.is_empty());
    let sent = captured.await.unwrap();
    assert!(
        sent.starts_with("POST /api/v10/streams/guild:40:30:42/preview HTTP/1.1"),
        "{sent}"
    );
    let body: Value = serde_json::from_str(sent.split("\r\n\r\n").nth(1).unwrap()).unwrap();
    assert_eq!(
        body,
        json!({"thumbnail": "data:image/jpeg;base64,/9j/4AECAw=="})
    );
    // In a conversation's call, its own key; a refusal is nobody's to see.
    let (http, captured) = fixture(response(403, "{}")).await;
    assert!(http
        .execute(&secret(), &upload(None), "")
        .await
        .unwrap()
        .is_empty());
    assert!(captured
        .await
        .unwrap()
        .starts_with("POST /api/v10/streams/call:30:42/preview HTTP/1.1"));
    // Not a JPEG, or far too large: never sent.
    for jpeg in [b"\x89PNG".to_vec(), vec![0xFF; 600 * 1024]] {
        let command = Command::UploadStreamPreview {
            server: None,
            channel: ChannelId(30),
            owner: rune_model::UserId(42),
            jpeg,
        };
        assert!(!crate::http::preview_fits(&command));
    }
}

#[tokio::test]
async fn an_applications_icon_is_read_from_its_public_object_and_a_failure_is_none() {
    let hash = "0123456789abcdef0123456789abcdef";
    let (http, captured) =
        fixture(response(200, &json!({"id": "7", "icon": hash}).to_string())).await;
    let icon = http.application_icon(&secret(), 7).await.unwrap();
    assert!(captured
        .await
        .unwrap()
        .starts_with("GET /api/v10/applications/7/public HTTP/1.1"));
    assert_eq!(
        icon.url,
        format!("https://cdn.discordapp.com/app-icons/7/{hash}.png?size=128")
    );
    let (http, _) = fixture(response(404, "{}")).await;
    assert!(http.application_icon(&secret(), 7).await.is_none());
}

/// Nothing listens when the first attempt goes: a read is sent once more a
/// second later, and reaches the server that came up meanwhile.
#[tokio::test]
async fn a_read_is_sent_once_more_after_a_dead_connection() {
    let probe = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = probe.local_addr().unwrap();
    drop(probe);
    let http = Http::local(&format!("http://{address}"));
    let server = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(300)).await;
        let listener = TcpListener::bind(address).await.unwrap();
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        socket
            .write_all(response(200, &json!([message(100, "older")]).to_string()).as_bytes())
            .await
            .unwrap();
    });
    let events = http.execute(&secret(), &history(7), "").await.unwrap();
    assert!(matches!(
        events.last(),
        Some(Event::HistoryLoaded { request: 7, .. })
    ));
    server.await.unwrap();
}

/// The same dead connection for a write: it is never sent a second time.
#[tokio::test]
async fn a_write_to_a_dead_connection_is_never_sent_twice() {
    // The connection dies after the request instead of being refused: Windows
    // keeps a refused SYN retrying for seconds, so a late bind sees the first try.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let http = Http::local(&format!("http://{address}"));
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        drop(socket);
        // A retry would have arrived about a second after the first attempt.
        assert!(
            tokio::time::timeout(Duration::from_millis(1_500), listener.accept())
                .await
                .is_err()
        );
    });
    let error = http
        .execute(&secret(), &send(1), "fixture")
        .await
        .unwrap_err();
    assert!(error.ambiguous);
    server.await.unwrap();
}
