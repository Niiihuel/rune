use super::*;
use rune_provider::video::MAX_STREAMS;
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::{
    accept_async,
    tungstenite::protocol::{frame::coding::CloseCode, CloseFrame},
};

type Peer = WebSocketStream<TcpStream>;

async fn fixture() -> (TcpListener, Gateway) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway = Gateway::local(&format!("ws://{}/", listener.local_addr().unwrap()));
    (listener, gateway)
}

fn secret() -> Arc<SessionSecret> {
    Arc::new(SessionSecret::try_from_local("SYNTHETIC_GATEWAY_SESSION".into()).unwrap())
}

async fn peer(listener: &TcpListener, interval: u64) -> Peer {
    let (stream, _) = timeout(Duration::from_secs(2), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut socket = accept_async(stream).await.unwrap();
    write(
        &mut socket,
        json!({"op":10,"d":{"heartbeat_interval":interval}}),
    )
    .await;
    socket
}

async fn write(socket: &mut Peer, data: Value) {
    socket
        .send(Message::Text(data.to_string().into()))
        .await
        .unwrap();
}

/// The next message that is not a heartbeat.
async fn beyond_heartbeats(socket: &mut Peer) -> Value {
    loop {
        let message = read(socket).await;
        if message["op"] != 1 {
            return message;
        }
    }
}

async fn read(socket: &mut Peer) -> Value {
    loop {
        let message = timeout(Duration::from_secs(2), socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Message::Text(text) = message {
            return serde_json::from_str(&text).unwrap();
        }
    }
}

fn ready() -> Value {
    json!({"op":0,"s":1,"t":"READY","d":{
        "session_id":"SYNTHETIC_RESUME_SESSION", "resume_gateway_url":"wss://gateway-us-east1-b.discord.gg",
        "token":"SYNTHETIC_NOT_DOMAIN_DATA", "user":{"id":"1","username":"Local"}, "guilds":[]
    }})
}

async fn event(receiver: &mut mpsc::Receiver<GatewayEvent>) -> GatewayEvent {
    loop {
        let event = timeout(Duration::from_secs(2), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        // The session's id comes before its Ready, for apps (P3).
        if !matches!(event, GatewayEvent::Session(_)) {
            return event;
        }
    }
}

#[test]
fn resume_endpoints_cannot_redirect_the_session_elsewhere() {
    for valid in [
        "wss://gateway.discord.gg",
        "wss://gateway-us-east1-b.discord.gg/?encoding=etf",
    ] {
        let url = resume_url(valid).ok().unwrap();
        assert!(url.ends_with("?v=10&encoding=json"));
    }
    for invalid in [
        "ws://gateway.discord.gg",
        "https://gateway.discord.gg",
        "wss://gateway.discord.gg.evil.invalid",
        "wss://gateway-evil.attacker.discord.gg",
        "wss://evil.invalid@gateway.discord.gg",
        "wss://gateway.discord.gg:444",
        "wss://gateway.discord.gg/private",
        "wss://gateway.discord.gg/#fragment",
        "wss://127.0.0.1/",
        "wss://gateway-.discord.gg/",
    ] {
        assert!(resume_url(invalid).is_err());
    }
}

#[tokio::test]
// The handshake callback's `Result` is tungstenite's, not ours to shrink.
#[allow(clippy::result_large_err)]
async fn identifies_as_the_browser_delivers_ready_and_domain_dispatches_then_cancels() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
    // Browser identity (2026-10-19): the socket and Identify present the
    // same browser as REST; no capability flags change the payloads.
    let (stream, _) = timeout(Duration::from_secs(2), listener.accept())
        .await
        .unwrap()
        .unwrap();
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
    let (heard, handshake) = std::sync::mpsc::channel();
    let mut socket = tokio_tungstenite::accept_hdr_async(
        stream,
        move |request: &Request, response: Response| -> Result<Response, ErrorResponse> {
            let header = |name: &str| {
                let value = request.headers().get(name)?;
                Some(value.to_str().unwrap().to_owned())
            };
            heard
                .send((header("user-agent"), header("origin")))
                .unwrap();
            Ok(response)
        },
    )
    .await
    .unwrap();
    assert_eq!(
        handshake.recv().unwrap(),
        (
            Some(crate::browser::user_agent().to_owned()),
            Some("https://discord.com".to_owned())
        )
    );
    write(
        &mut socket,
        json!({"op":10,"d":{"heartbeat_interval":1000}}),
    )
    .await;
    let identify = read(&mut socket).await;
    assert_eq!(identify["op"], 2);
    let properties = &identify["d"]["properties"];
    assert_eq!(properties["browser"], "Safari");
    assert_eq!(properties["os"], "Linux");
    assert_eq!(properties["device"], "");
    assert_eq!(
        properties["browser_user_agent"],
        crate::browser::user_agent()
    );
    assert!(properties["client_build_number"].as_u64() > Some(0));
    assert_eq!(properties["gateway_connect_reasons"], "COLD_START");
    assert!(identify["d"].get("capabilities").is_none());
    // The token goes in the Identify's own field alone.
    assert_eq!(
        identify
            .to_string()
            .matches("SYNTHETIC_GATEWAY_SESSION")
            .count(),
        1
    );
    write(&mut socket, ready()).await;
    let GatewayEvent::Ready(data) = event(&mut rx).await else {
        panic!("expected READY")
    };
    assert!(data.get("session_id").is_none());
    assert!(data.get("resume_gateway_url").is_none());
    assert!(data.get("token").is_none());
    assert_eq!(data["user"]["id"], "1");
    for (index, name) in [
        "MESSAGE_CREATE",
        "MESSAGE_UPDATE",
        "MESSAGE_DELETE",
        "PRESENCE_UPDATE",
        "GUILD_ROLE_UPDATE",
    ]
    .into_iter()
    .enumerate()
    {
        write(
            &mut socket,
            json!({"op":0,"s":index + 2,"t":name,"d":{"id":"11","channel_id":"22"}}),
        )
        .await;
        let GatewayEvent::Dispatch { kind, data } = event(&mut rx).await else {
            panic!("expected dispatch")
        };
        assert_eq!(kind.as_str(), name);
        assert_eq!(data["id"], "11");
    }
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    // Drain a heartbeat already written before cancellation, then prove EOF.
    let closed = timeout(Duration::from_secs(1), async {
        while let Some(Ok(_)) = socket.next().await {}
    })
    .await;
    assert!(closed.is_ok());
}

#[tokio::test]
async fn ready_consumer_in_the_same_select_can_drain_a_buffered_dispatch_burst() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let server = tokio::spawn(async move {
        let mut socket = peer(&listener, 1000).await;
        assert_eq!(read(&mut socket).await["op"], 2);
        socket
            .feed(Message::Text(ready().to_string().into()))
            .await
            .unwrap();
        for sequence in 2..=97 {
            socket
                .feed(Message::Text(
                    json!({"op":0,"s":sequence,
                "t":"MESSAGE_CREATE","d":{"id":sequence.to_string()}})
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
        }
        socket.flush().await.unwrap();
        while let Some(Ok(Message::Text(text))) = socket.next().await {
            let value: Value = serde_json::from_str(&text).unwrap();
            if value["op"] == 1 {
                write(&mut socket, json!({"op":11})).await;
            }
        }
    });
    // This is the provider's actual scheduling arrangement: the transport is
    // a branch of the consumer's select, not a separately spawned task.
    let transport = gateway.run(secret(), tx);
    tokio::pin!(transport);
    let received = timeout(Duration::from_secs(2), async {
        let mut received = 0;
        while received < 97 {
            tokio::select! {
                biased;
                event = rx.recv() => {
                    if matches!(event, Some(GatewayEvent::Session(_))) {
                        continue;
                    }
                    assert!(matches!(event, Some(GatewayEvent::Ready(_) | GatewayEvent::Dispatch { .. })));
                    received += 1;
                }
                result = &mut transport => panic!("transport stopped before draining its ready consumer: {result:?}"),
            }
        }
        received
    }).await.unwrap();
    assert_eq!(received, 97);
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn missing_ack_reconnects_and_resumes_from_the_last_sequence() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
    let mut first = peer(&listener, 40).await;
    assert_eq!(read(&mut first).await["op"], 2);
    write(&mut first, ready()).await;
    assert!(matches!(event(&mut rx).await, GatewayEvent::Ready(_)));
    assert_eq!(read(&mut first).await["op"], 1);
    // No ACK: the connection must be discarded without a normal close that
    // invalidates the resumable server session.
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting { attempt: 1, .. }
    ));
    let mut second = peer(&listener, 1000).await;
    let resume = read(&mut second).await;
    assert_eq!(resume["op"], 6);
    assert_eq!(resume["d"]["seq"], 1);
    assert_eq!(resume["d"]["session_id"], "SYNTHETIC_RESUME_SESSION");
    write(
        &mut second,
        json!({"op":0,"s":2,"t":"MESSAGE_CREATE","d":{"id":"23"}}),
    )
    .await;
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Dispatch {
            kind: DispatchKind::MessageCreate,
            ..
        }
    ));
    write(&mut second, json!({"op":0,"s":3,"t":"RESUMED","d":{}})).await;
    assert!(matches!(event(&mut rx).await, GatewayEvent::Resumed));
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn socket_cut_without_close_frame_resumes_instead_of_stopping() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
    let mut first = peer(&listener, 1000).await;
    assert_eq!(read(&mut first).await["op"], 2);
    write(&mut first, ready()).await;
    assert!(matches!(event(&mut rx).await, GatewayEvent::Ready(_)));
    // A network drop after hours: the stream ends with no close handshake.
    drop(first);
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting { attempt: 1, .. }
    ));
    let mut second = peer(&listener, 1000).await;
    assert_eq!(read(&mut second).await["op"], 6);
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn invalid_nonresumable_session_reidentifies_and_server_heartbeat_is_acknowledged() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
    let mut first = peer(&listener, 1000).await;
    assert_eq!(read(&mut first).await["op"], 2);
    write(&mut first, ready()).await;
    let _ = event(&mut rx).await;
    write(&mut first, json!({"op":9,"d":false})).await;
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting { .. }
    ));
    let mut second = peer(&listener, 1000).await;
    assert_eq!(read(&mut second).await["op"], 2);
    write(&mut second, ready()).await;
    let _ = event(&mut rx).await;
    // Consume the scheduled first heartbeat and acknowledge it.
    assert_eq!(read(&mut second).await["op"], 1);
    write(&mut second, json!({"op":11})).await;
    write(&mut second, json!({"op":1,"d":null})).await;
    let beat = read(&mut second).await;
    assert_eq!(beat["op"], 1);
    assert_eq!(beat["d"], 1);
    write(&mut second, json!({"op":11})).await;
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn invalid_resume_host_and_terminal_close_never_retry_or_leak_remote_reason() {
    for close in [false, true] {
        let (listener, gateway) = fixture().await;
        let (tx, mut rx) = mpsc::channel(32);
        let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
        let mut socket = peer(&listener, 1000).await;
        let _ = read(&mut socket).await;
        if close {
            socket
                .send(Message::Close(Some(CloseFrame {
                    code: CloseCode::from(4004),
                    reason: "REMOTE_PRIVATE_REASON".into(),
                })))
                .await
                .unwrap();
        } else {
            let mut payload = ready();
            payload["d"]["resume_gateway_url"] = json!("wss://evil.invalid/");
            write(&mut socket, payload).await;
        }
        let error = timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(!error.to_string().contains("REMOTE_PRIVATE_REASON"));
        assert!(!error.to_string().contains("SYNTHETIC"));
        assert!(!error.to_string().contains("evil.invalid"));
        assert!(rx.recv().await.is_none());
        assert!(timeout(Duration::from_millis(80), listener.accept())
            .await
            .is_err());
    }
}

#[tokio::test]
async fn malformed_binary_and_oversized_messages_fail_closed() {
    for payload in [
        Message::Text("{not-json".into()),
        Message::Binary(vec![1, 2, 3].into()),
        Message::Text("x".repeat(MAX_PAYLOAD + 1).into()),
    ] {
        let (listener, gateway) = fixture().await;
        let (tx, _rx) = mpsc::channel(32);
        let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
        let mut socket = peer(&listener, 1000).await;
        let _ = read(&mut socket).await;
        let _ = socket.send(payload).await;
        let result = timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap();
        assert!(result.is_err());
    }
}

#[tokio::test]
async fn consumer_backpressure_is_explicit_and_never_silently_drops_dispatches() {
    let (listener, gateway) = fixture().await;
    let (tx, _rx) = mpsc::channel(1);
    let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
    let mut socket = peer(&listener, 1000).await;
    let _ = read(&mut socket).await;
    write(&mut socket, ready()).await;
    write(
        &mut socket,
        json!({"op":0,"s":2,"t":"MESSAGE_CREATE","d":{}}),
    )
    .await;
    let error = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert!(error.to_string().contains("cola"));
}

#[tokio::test]
async fn duplicate_dispatches_are_ignored_and_unknown_events_advance_resume_sequence() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
    let mut first = peer(&listener, 1000).await;
    let _ = read(&mut first).await;
    write(&mut first, ready()).await;
    let _ = event(&mut rx).await;
    write(
        &mut first,
        json!({"op":0,"s":2,"t":"MESSAGE_CREATE","d":{"id":"21"}}),
    )
    .await;
    let _ = event(&mut rx).await;
    write(
        &mut first,
        json!({"op":0,"s":2,"t":"MESSAGE_CREATE","d":{"id":"21"}}),
    )
    .await;
    write(&mut first, json!({"op":0,"s":3,"t":"FUTURE_EVENT","d":{}})).await;
    write(&mut first, json!({"op":7,"d":null})).await;
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting { .. }
    ));
    let mut second = peer(&listener, 1000).await;
    let resume = read(&mut second).await;
    assert_eq!(resume["op"], 6);
    assert_eq!(resume["d"]["seq"], 3);
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn trying_again_now_cuts_a_wait_between_attempts_short_and_only_a_wait() {
    let (listener, gateway) = fixture().await;
    let gateway = gateway.waiting(Duration::from_secs(60));
    let (tx, mut rx) = mpsc::channel(32);
    let now = Arc::new(Notify::new());
    let hurry = now.clone();
    let task = tokio::spawn(async move { gateway.run_hurried(secret(), tx, hurry).await });
    let mut first = peer(&listener, 1000).await;
    assert_eq!(read(&mut first).await["op"], 2);
    // Asked while connected: nothing to cut short, and nothing kept for
    // the next loss.
    now.notify_waiters();
    write(&mut first, ready()).await;
    assert!(matches!(event(&mut rx).await, GatewayEvent::Ready(_)));
    write(&mut first, json!({"op":7,"d":null})).await;
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting {
            attempt: 1,
            retry_in_ms: 60_000
        }
    ));
    // The wait goes on by itself…
    assert!(timeout(Duration::from_millis(300), listener.accept())
        .await
        .is_err());
    // …until asked: the same session resumes, without being made again.
    now.notify_waiters();
    let mut second = peer(&listener, 1000).await;
    let resume = read(&mut second).await;
    assert_eq!(resume["op"], 6);
    assert_eq!(resume["d"]["session_id"], "SYNTHETIC_RESUME_SESSION");
    // A hurried attempt that fails uses none of the budget: clicks on a
    // dead network never make it give up sooner.
    write(&mut second, json!({"op":7,"d":null})).await;
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting {
            attempt: 1,
            retry_in_ms: 60_000
        }
    ));
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn trying_again_now_still_keeps_the_spacing_between_identifies() {
    let (listener, gateway) = fixture().await;
    let gap = Duration::from_millis(800);
    let gateway = gateway.waiting(Duration::from_secs(60)).spaced(gap);
    let (tx, mut rx) = mpsc::channel(32);
    let now = Arc::new(Notify::new());
    let hurry = now.clone();
    let started = Instant::now();
    let task = tokio::spawn(async move { gateway.run_hurried(secret(), tx, hurry).await });
    let mut first = peer(&listener, 1000).await;
    assert_eq!(read(&mut first).await["op"], 2);
    // Lost before READY: no session to resume, the next attempt identifies.
    write(&mut first, json!({"op":7,"d":null})).await;
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting { attempt: 1, .. }
    ));
    now.notify_waiters();
    let mut second = peer(&listener, 1000).await;
    assert_eq!(read(&mut second).await["op"], 2);
    // Never sooner than the spacing after the first IDENTIFY, which went
    // after `started`.
    assert!(started.elapsed() >= gap, "{:?}", started.elapsed());
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn websocket_handshake_redirect_is_not_followed_and_has_no_credential() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (listener, gateway) = fixture().await;
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (tx, _rx) = mpsc::channel(32);
    let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
    let (mut stream, _) = listener.accept().await.unwrap();
    let mut request = [0; 4096];
    let count = stream.read(&mut request).await.unwrap();
    assert!(!String::from_utf8_lossy(&request[..count]).contains("SYNTHETIC"));
    let response = format!(
        "HTTP/1.1 307 Temporary Redirect\r\nLocation: ws://{}/\r\nContent-Length: 0\r\n\r\n",
        trap.local_addr().unwrap()
    );
    stream.write_all(response.as_bytes()).await.unwrap();
    drop(stream);
    assert!(timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .is_err());
    assert!(timeout(Duration::from_millis(80), trap.accept())
        .await
        .is_err());
}

#[tokio::test(start_paused = true)]
async fn outgoing_budget_is_a_sliding_minute_and_stops_heartbeat_floods() {
    let mut budget = WriteBudget::default();
    for _ in 0..100 {
        assert!(budget.record().is_ok());
    }
    assert!(matches!(budget.record(), Err(Failure::Stop(_))));
    tokio::time::advance(Duration::from_secs(59)).await;
    assert!(budget.record().is_err());
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(budget.record().is_ok());
}

#[tokio::test]
async fn acknowledged_heartbeats_keep_the_connection_alive() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
    let mut socket = peer(&listener, 50).await;
    assert_eq!(read(&mut socket).await["op"], 2);
    write(&mut socket, ready()).await;
    let _ = event(&mut rx).await;
    for _ in 0..4 {
        assert_eq!(read(&mut socket).await["op"], 1);
        write(&mut socket, json!({"op":11})).await;
    }
    assert!(rx.try_recv().is_err());
    assert!(!task.is_finished());
    task.abort();
    let _ = task.await;
}

/// Closes the peer with a transient code, as a server does when it restarts.
async fn drop_transiently(socket: &mut Peer) {
    socket
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::from(4000),
            reason: "SYNTHETIC_TRANSIENT".into(),
        })))
        .await
        .unwrap();
}

#[tokio::test]
async fn transient_failures_keep_reconnecting_past_any_old_budget_and_a_terminal_close_stops() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
    // Well past the six attempts the old budget allowed.
    let failures = 10;
    for _ in 0..failures {
        let mut socket = peer(&listener, 1000).await;
        assert_eq!(read(&mut socket).await["op"], 2);
        drop_transiently(&mut socket).await;
    }
    let mut socket = peer(&listener, 1000).await;
    assert_eq!(read(&mut socket).await["op"], 2);
    socket
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::from(4004),
            reason: "SYNTHETIC_TERMINAL".into(),
        })))
        .await
        .unwrap();
    let error = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.gateway_close_code(), Some(4004));
    assert!(!error.to_string().contains("SYNTHETIC"));
    let mut retries = 0;
    while let Some(event) = rx.recv().await {
        assert!(matches!(event, GatewayEvent::Reconnecting { .. }));
        retries += 1;
    }
    assert_eq!(retries, failures);
    assert!(timeout(Duration::from_millis(80), listener.accept())
        .await
        .is_err());
}

#[tokio::test]
async fn five_failures_in_a_row_forget_the_session_and_identify_again() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
    let mut first = peer(&listener, 1000).await;
    assert_eq!(read(&mut first).await["op"], 2);
    write(&mut first, ready()).await;
    let _ = event(&mut rx).await;
    drop_transiently(&mut first).await;
    // The first four failures still resume the same session.
    for _ in 0..4 {
        assert!(matches!(
            event(&mut rx).await,
            GatewayEvent::Reconnecting { .. }
        ));
        let mut socket = peer(&listener, 1000).await;
        assert_eq!(read(&mut socket).await["op"], 6);
        drop_transiently(&mut socket).await;
    }
    // The fifth failure forgets it: the next attempt identifies.
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting { .. }
    ));
    let mut socket = peer(&listener, 1000).await;
    assert_eq!(read(&mut socket).await["op"], 2);
    task.abort();
    let _ = task.await;
}

#[test]
fn the_backoff_grows_from_a_second_and_never_passes_a_minute() {
    let mut current = BACKOFF_START_MS;
    let mut reached_cap = false;
    for _ in 0..200 {
        let next = next_backoff(current).ok().expect("random bytes");
        assert!(next >= current && next <= BACKOFF_MAX_MS, "{next}");
        reached_cap |= next == BACKOFF_MAX_MS;
        current = next;
    }
    assert!(reached_cap);
}

#[tokio::test]
async fn a_press_during_a_failed_connect_cuts_the_next_wait_short() {
    let (listener, gateway) = fixture().await;
    let gateway = gateway.waiting(Duration::from_secs(60));
    let (tx, mut rx) = mpsc::channel(32);
    let now = Arc::new(Notify::new());
    let hurry = now.clone();
    let task = tokio::spawn(async move { gateway.run_hurried(secret(), tx, hurry).await });
    let mut first = peer(&listener, 1000).await;
    assert_eq!(read(&mut first).await["op"], 2);
    // Pressed before READY, and the attempt then fails: the press is kept
    // (notify_one stores it) for the wait that follows.
    now.notify_one();
    write(&mut first, json!({"op":7,"d":null})).await;
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting {
            retry_in_ms: 60_000,
            ..
        }
    ));
    // The next attempt connects well before the 60 s wait would have ended.
    timeout(Duration::from_secs(5), listener.accept())
        .await
        .expect("the pressed wait was not cut short")
        .unwrap();
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn a_network_change_while_connected_heartbeats_now_and_drops_a_dead_connection() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let now = Arc::new(Notify::new());
    let hurry = now.clone();
    let task = tokio::spawn(async move { gateway.run_hurried(secret(), tx, hurry).await });
    let mut socket = peer(&listener, 60_000).await;
    assert_eq!(read(&mut socket).await["op"], 2);
    write(&mut socket, ready()).await;
    let _ = event(&mut rx).await;
    // The first heartbeat is the scheduled one; leave it unanswered.
    assert_eq!(read(&mut socket).await["op"], 1);
    write(&mut socket, json!({"op":11})).await;
    // Let that ACK land first: one arriving after the signal would count as life.
    tokio::time::sleep(Duration::from_millis(100)).await;
    // The network changed: a heartbeat goes out at once, and no ACK follows.
    now.notify_one();
    assert_eq!(read(&mut socket).await["op"], 1);
    // Taken as dead after the expedited deadline, then the session resumes.
    assert!(matches!(
        timeout(Duration::from_secs(8), rx.recv())
            .await
            .unwrap()
            .unwrap(),
        GatewayEvent::Reconnecting { .. }
    ));
    let mut second = peer(&listener, 1000).await;
    assert_eq!(read(&mut second).await["op"], 6);
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn terminal_close_codes_remain_distinct_without_remote_reason_or_reconnect() {
    let mut labels = Vec::new();
    for code in [4004, 4008, 4010, 4014] {
        let (listener, gateway) = fixture().await;
        let (tx, mut rx) = mpsc::channel(32);
        let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
        let mut socket = peer(&listener, 1000).await;
        assert_eq!(read(&mut socket).await["op"], 2);
        socket
            .send(Message::Close(Some(CloseFrame {
                code: CloseCode::from(code),
                reason: "REMOTE_PRIVATE_REASON_SYNTHETIC_SESSION".into(),
            })))
            .await
            .unwrap();
        let error = timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert_eq!(error.gateway_close_code(), Some(code));
        let label = error.to_string();
        assert!(label.contains(&format!("Gateway {code}")));
        assert!(!format!("{error:?} {label}").contains("REMOTE_PRIVATE_REASON"));
        assert!(!format!("{error:?} {label}").contains("SYNTHETIC_SESSION"));
        labels.push(label);
        assert!(rx.recv().await.is_none());
        assert!(timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err());
    }
    assert_ne!(
        labels[2], labels[3],
        "previously indistinguishable terminal codes"
    );
    assert_eq!(LoginError::new("Local error").gateway_close_code(), None);
    assert_eq!(LoginError::new("Local error").to_string(), "Local error");
}

/// Windows discards what was not read yet when a reset arrives, close frame
/// included, so no read can recover the code. It costs one reconnect: a
/// rejected session is rejected again, and that close stops for good.
#[tokio::test]
async fn a_reset_that_swallowed_a_terminal_close_retries_once_and_stops_on_the_next() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let task = tokio::spawn(async move { gateway.run(secret(), tx).await });
    let mut first = peer(&listener, 1000).await;
    assert_eq!(read(&mut first).await["op"], 2);
    write(&mut first, ready()).await;
    assert!(matches!(event(&mut rx).await, GatewayEvent::Ready(_)));
    first.get_ref().set_zero_linger().unwrap();
    drop(first);
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting { attempt: 1, .. }
    ));
    let mut second = peer(&listener, 1000).await;
    assert_eq!(read(&mut second).await["op"], 6);
    second
        .send(Message::Close(Some(CloseFrame {
            code: CloseCode::from(4004),
            reason: "".into(),
        })))
        .await
        .unwrap();
    let error = timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap()
        .unwrap_err();
    assert_eq!(error.gateway_close_code(), Some(4004));
    assert!(rx.recv().await.is_none());
}

#[test]
fn member_list_frames_wait_for_room_in_their_own_minute_budget() {
    let start = Instant::now();
    let mut sent = VecDeque::new();
    for _ in 0..LIST_FRAMES {
        assert_eq!(list_room(&mut sent, start, 1), Ok(()));
        sent.push_back(start);
    }
    assert_eq!(list_room(&mut sent, start, 1), Err(start + LIST_WINDOW));
    let later = start + LIST_WINDOW;
    assert_eq!(
        list_room(&mut sent, later, 3),
        Ok(()),
        "the old ones expired"
    );
    assert!(sent.is_empty());
    const { assert!(LIST_FRAMES < 100, "below the connection write budget") };
}

#[tokio::test]
async fn voice_wishes_become_one_voice_state_update_each_and_voice_dispatches_pass() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let (want, wanted) = watch::channel(None);
    let (ask, asked) = watch::channel(None);
    let task = tokio::spawn(async move {
        gateway
            .run_watching(
                secret(),
                tx,
                watch::channel(None).1,
                wanted,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                asked,
                watch::channel(None).1,
                Arc::default(),
            )
            .await
    });
    let mut socket = peer(&listener, 60_000).await;
    assert_eq!(read(&mut socket).await["op"], 2);
    // A wish before READY waits for it; a burst of them sends the latest.
    want.send_replace(Some(VoiceWant {
        guild: Some(40),
        channel: Some(30),
        muted: false,
        deafened: false,
        request: 1,
        streams: [None; MAX_STREAMS],
        share: None,
        share_request: 0,
        camera: false,
        share_paused: false,
    }));
    want.send_replace(Some(VoiceWant {
        guild: Some(40),
        channel: Some(30),
        muted: true,
        deafened: false,
        request: 1,
        streams: [None; MAX_STREAMS],
        share: None,
        share_request: 0,
        camera: false,
        share_paused: false,
    }));
    write(&mut socket, ready()).await;
    assert!(matches!(event(&mut rx).await, GatewayEvent::Ready(_)));
    let mut update = read(&mut socket).await;
    while update["op"] == 1 {
        update = read(&mut socket).await;
    }
    assert_eq!(
        update,
        json!({"op": 4, "d": {"guild_id": "40", "channel_id": "30", "self_mute": true,
            "self_deaf": false, "self_video": false}})
    );
    // Voice protocol (voz-protocolo): then, once a connection, when the
    // server's voice channels went live (op 43), as Serein asks.
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 43, "d": {"guild_id": "40", "fields": ["voice_start_time"]}})
    );
    want.send_replace(Some(VoiceWant {
        guild: Some(40),
        channel: None,
        muted: false,
        deafened: false,
        request: 1,
        streams: [None; MAX_STREAMS],
        share: None,
        share_request: 0,
        camera: false,
        share_paused: false,
    }));
    let mut update = read(&mut socket).await;
    while update["op"] == 1 {
        update = read(&mut socket).await;
    }
    assert_eq!(update["d"]["channel_id"], Value::Null, "leaving");
    // The same wish asked again (a rejoin) goes out again.
    want.send_replace(Some(VoiceWant {
        guild: Some(40),
        channel: None,
        muted: false,
        deafened: false,
        request: 2,
        streams: [None; MAX_STREAMS],
        share: None,
        share_request: 0,
        camera: false,
        share_paused: false,
    }));
    let mut update = read(&mut socket).await;
    while update["op"] == 1 {
        update = read(&mut socket).await;
    }
    assert_eq!(update["op"], 4, "asked again");
    // Back in the call, watching someone: op 4 first, then op 20; asking for
    // the stream again is op 21, and letting go op 19.
    let watching = |stream: Option<u64>, stream_request| VoiceWant {
        guild: Some(40),
        channel: Some(30),
        muted: false,
        deafened: false,
        request: 2,
        streams: std::array::from_fn(|at| {
            stream
                .filter(|_| at == 0)
                .map(|owner| (owner, stream_request))
        }),
        share: None,
        share_request: 0,
        camera: false,
        share_paused: false,
    };
    want.send_replace(Some(watching(Some(77), 0)));
    assert_eq!(beyond_heartbeats(&mut socket).await["op"], 4);
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 20, "d": {"stream_key": "guild:40:30:77"}})
    );
    want.send_replace(Some(watching(Some(77), 1)));
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 21, "d": {"stream_key": "guild:40:30:77"}})
    );
    want.send_replace(Some(watching(None, 1)));
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 19, "d": {"stream_key": "guild:40:30:77"}})
    );
    // Several at once: op 20 for each, nobody let go in between; letting
    // go of one lets go of that one only.
    let several = |owners: &[u64]| VoiceWant {
        streams: std::array::from_fn(|at| owners.get(at).map(|owner| (*owner, 0))),
        ..watching(None, 1)
    };
    for (owners, op, key) in [
        (&[77][..], 20, 77),
        (&[77, 78][..], 20, 78),
        (&[78][..], 19, 77),
        (&[][..], 19, 78),
    ] {
        want.send_replace(Some(several(owners)));
        assert_eq!(
            beyond_heartbeats(&mut socket).await,
            json!({"op": op, "d": {"stream_key": format!("guild:40:30:{key}")}})
        );
    }
    // Sharing the account's screen: op 18 creates its stream; asked again
    // after it dropped, op 21 with its own key; stopping, op 19.
    let sharing = |share_request| VoiceWant {
        share: Some(5),
        share_request,
        ..watching(None, 1)
    };
    want.send_replace(Some(sharing(0)));
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 18, "d": {"type": "guild", "guild_id": "40", "channel_id": "30",
            "preferred_region": null}})
    );
    want.send_replace(Some(sharing(1)));
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 21, "d": {"stream_key": "guild:40:30:5"}})
    );
    // Paused and resumed: op 22 each time, with its key.
    for paused in [true, false] {
        want.send_replace(Some(VoiceWant {
            share_paused: paused,
            ..sharing(1)
        }));
        assert_eq!(
            beyond_heartbeats(&mut socket).await,
            json!({"op": 22, "d": {"stream_key": "guild:40:30:5", "paused": paused}})
        );
    }
    want.send_replace(Some(watching(None, 1)));
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 19, "d": {"stream_key": "guild:40:30:5"}})
    );
    // The camera on and off again: op 4 with `self_video` each time, and
    // nothing about streams.
    for camera in [true, false] {
        want.send_replace(Some(VoiceWant {
            camera,
            ..watching(None, 1)
        }));
        assert_eq!(
            beyond_heartbeats(&mut socket).await,
            json!({"op": 4, "d": {"guild_id": "40", "channel_id": "30", "self_mute": false,
                "self_deaf": false, "self_video": camera}})
        );
    }
    // Private calls: what the service knows of a conversation's call is
    // asked first (op 13); joining it has no guild, and its streams are
    // the call's (op 18 of type «call», keys `call:{channel}:{owner}`).
    ask.send_replace(Some((50, 1)));
    let private = |share: Option<u64>, owner: Option<u64>| VoiceWant {
        guild: None,
        channel: Some(50),
        share,
        streams: std::array::from_fn(|at| owner.filter(|_| at == 0).map(|owner| (owner, 0))),
        ..watching(None, 1)
    };
    want.send_replace(Some(private(None, None)));
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 13, "d": {"channel_id": "50"}})
    );
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 4, "d": {"guild_id": null, "channel_id": "50", "self_mute": false,
            "self_deaf": false, "self_video": false}})
    );
    want.send_replace(Some(private(Some(5), Some(77))));
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 20, "d": {"stream_key": "call:50:77"}})
    );
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 18, "d": {"type": "call", "guild_id": null, "channel_id": "50",
            "preferred_region": null}})
    );
    // The same conversation asked about again goes out again.
    ask.send_replace(Some((50, 2)));
    assert_eq!(
        beyond_heartbeats(&mut socket).await,
        json!({"op": 13, "d": {"channel_id": "50"}})
    );
    for (index, name) in [
        "VOICE_STATE_UPDATE",
        "VOICE_SERVER_UPDATE",
        "STREAM_CREATE",
        "STREAM_SERVER_UPDATE",
        "STREAM_UPDATE",
        "STREAM_DELETE",
        "CALL_CREATE",
        "CALL_UPDATE",
        "CALL_DELETE",
        // Rail and notifications.
        "EMBEDDED_ACTIVITY_UPDATE_V2",
        // Voice protocol (voz-protocolo).
        "CHANNEL_INFO",
        "VOICE_CHANNEL_START_TIME_UPDATE",
    ]
    .into_iter()
    .enumerate()
    {
        write(
            &mut socket,
            json!({"op":0,"s":index + 2,"t":name,"d":{"guild_id":"40"}}),
        )
        .await;
        let GatewayEvent::Dispatch { kind, .. } = event(&mut rx).await else {
            panic!("expected dispatch")
        };
        assert_eq!(kind.as_str(), name);
    }
    task.abort();
}

/// The two frames that open the watched list: the server's, then its rows.
async fn list_opened(socket: &mut Peer) {
    let opening = beyond_heartbeats(socket).await;
    assert_eq!(opening["op"], 37);
    assert_eq!(opening["d"]["subscriptions"]["40"]["channels"], json!({}));
    let rows = beyond_heartbeats(socket).await;
    assert_eq!(rows["op"], 37);
    assert_eq!(
        rows["d"]["subscriptions"]["40"]["channels"],
        json!({"41": [[0, 99]]})
    );
}

#[tokio::test]
async fn a_member_list_is_sent_again_without_a_sync_and_after_every_new_ready() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let (_want, wanted) = watch::channel(Some(Watch {
        server: rune_model::ServerId(40),
        channel: rune_model::ChannelId(41),
        list: "everyone".into(),
        ranges: vec![[0, 99]],
        people: Vec::new(),
    }));
    let task = tokio::spawn(async move {
        gateway
            .run_watching(
                secret(),
                tx,
                wanted,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                Arc::default(),
            )
            .await
    });
    let mut first = peer(&listener, 60_000).await;
    assert_eq!(read(&mut first).await["op"], 2);
    write(&mut first, ready()).await;
    assert!(matches!(event(&mut rx).await, GatewayEvent::Ready(_)));
    list_opened(&mut first).await;
    // No sync came: the list goes again from nothing (the wait is short in tests).
    list_opened(&mut first).await;
    // Once it syncs, nothing more is sent for it.
    write(
        &mut first,
        json!({"op":0,"s":2,"t":"GUILD_MEMBER_LIST_UPDATE","d":{
            "guild_id":"40","id":"everyone","groups":[],
            "ops":[{"op":"SYNC","range":[0,99],"items":[]}]
        }}),
    )
    .await;
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Dispatch { .. }
    ));
    let quiet = timeout(Duration::from_millis(900), async {
        loop {
            if read(&mut first).await["op"] == 37 {
                return;
            }
        }
    })
    .await;
    assert!(quiet.is_err(), "a synced list is not sent again");
    // A session that cannot resume starts anew: its READY subscribes again.
    write(&mut first, json!({"op":9,"d":false})).await;
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting { .. }
    ));
    let mut second = peer(&listener, 60_000).await;
    assert_eq!(read(&mut second).await["op"], 2);
    write(&mut second, ready()).await;
    assert!(matches!(event(&mut rx).await, GatewayEvent::Ready(_)));
    list_opened(&mut second).await;
    task.abort();
    let _ = task.await;
}

// Voice protocol (voz-protocolo).
#[tokio::test]
async fn the_server_shown_is_asked_once_a_connection_when_its_voice_rooms_went_live() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    // Its channels show, its member list does not.
    let (show, shown) = watch::channel(Some(40));
    let task = tokio::spawn(async move {
        gateway
            .run_watching(
                secret(),
                tx,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                shown,
                Arc::default(),
            )
            .await
    });
    let asked =
        |guild: &str| json!({"op": 43, "d": {"guild_id": guild, "fields": ["voice_start_time"]}});
    let mut first = peer(&listener, 60_000).await;
    assert_eq!(read(&mut first).await["op"], 2);
    // Nothing before READY, then the server shown.
    write(&mut first, ready()).await;
    assert!(matches!(event(&mut rx).await, GatewayEvent::Ready(_)));
    assert_eq!(beyond_heartbeats(&mut first).await, asked("40"));
    // Another server, then the first again and none: only the new one.
    show.send_replace(Some(50));
    assert_eq!(beyond_heartbeats(&mut first).await, asked("50"));
    show.send_replace(Some(40));
    show.send_replace(None);
    let quiet = timeout(Duration::from_millis(300), beyond_heartbeats(&mut first)).await;
    assert!(quiet.is_err(), "asked once a connection: {quiet:?}");
    // A new connection asks again for the one shown.
    show.send_replace(Some(50));
    write(&mut first, json!({"op":9,"d":false})).await;
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting { .. }
    ));
    let mut second = peer(&listener, 60_000).await;
    assert_eq!(read(&mut second).await["op"], 2);
    write(&mut second, ready()).await;
    assert!(matches!(event(&mut rx).await, GatewayEvent::Ready(_)));
    assert_eq!(beyond_heartbeats(&mut second).await, asked("50"));
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn the_account_starts_with_its_status_and_presence_changes_are_paced() {
    use crate::settings_proto::{Custom, Own};
    use rune_model::Status;
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let (want, told) = watch::channel(Some(Own {
        status: Status::Invisible,
        custom: Some(Custom {
            text: "nadie me ve".into(),
            emoji: None,
            server_emoji: None,
            expires: None,
        }),
    }));
    let task = tokio::spawn(async move {
        gateway
            .run_watching(
                secret(),
                tx,
                watch::channel(None).1,
                watch::channel(None).1,
                told,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                Arc::default(),
            )
            .await
    });
    let mut socket = peer(&listener, 60_000).await;
    let identify = read(&mut socket).await;
    // Invisible from the first moment; the custom status never goes with it.
    assert_eq!(
        identify["d"]["presence"],
        json!({"since": null, "activities": [], "status": "invisible", "afk": false})
    );
    write(&mut socket, ready()).await;
    assert!(matches!(event(&mut rx).await, GatewayEvent::Ready(_)));
    want.send_replace(Some(Own {
        status: Status::Idle,
        custom: Some(Custom {
            text: "afuera".into(),
            emoji: Some("🌙".into()),
            server_emoji: None,
            expires: None,
        }),
    }));
    let update = beyond_heartbeats(&mut socket).await;
    assert_eq!(update["op"], 3);
    assert_eq!(update["d"]["status"], "idle");
    assert_eq!(
        update["d"]["activities"],
        json!([{"name": "Custom Status", "type": 4, "state": "afuera", "emoji": {"name": "🌙"}}])
    );
    // A burst sends only the latest, after the gap.
    let sent = Instant::now();
    for status in [Status::Online, Status::DoNotDisturb] {
        want.send_replace(Some(Own {
            status,
            custom: None,
        }));
    }
    let latest = beyond_heartbeats(&mut socket).await;
    assert!(sent.elapsed() >= Duration::from_millis(40));
    assert_eq!(latest["d"]["status"], "dnd");
    assert_eq!(latest["d"]["activities"], json!([]));
    task.abort();
    let _ = task.await;
}

#[tokio::test]
async fn a_new_session_asks_the_watched_stream_again() {
    let (listener, gateway) = fixture().await;
    let (tx, mut rx) = mpsc::channel(32);
    let (want, wanted) = watch::channel(None);
    let task = tokio::spawn(async move {
        gateway
            .run_watching(
                secret(),
                tx,
                watch::channel(None).1,
                wanted,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                watch::channel(None).1,
                Arc::default(),
            )
            .await
    });
    let mut socket = peer(&listener, 60_000).await;
    assert_eq!(read(&mut socket).await["op"], 2);
    write(&mut socket, ready()).await;
    let _ = event(&mut rx).await;
    // Watching 77 in the call: op 4, then op 20.
    want.send_replace(Some(VoiceWant {
        guild: Some(40),
        channel: Some(30),
        muted: false,
        deafened: false,
        request: 1,
        streams: std::array::from_fn(|at| (at == 0).then_some((77, 0))),
        share: None,
        share_request: 0,
        camera: false,
        share_paused: false,
    }));
    assert_eq!(beyond_heartbeats(&mut socket).await["op"], 4);
    let mut message = beyond_heartbeats(&mut socket).await;
    while message["op"] == 43 {
        message = beyond_heartbeats(&mut socket).await;
    }
    assert_eq!(
        message,
        json!({"op": 20, "d": {"stream_key": "guild:40:30:77"}})
    );
    // A new session (not resumed): what was sent went with the old one, so the
    // stream is asked for again, op 20 once more, and nothing else changed.
    write(&mut socket, json!({"op":9,"d":false})).await;
    assert!(matches!(
        event(&mut rx).await,
        GatewayEvent::Reconnecting { .. }
    ));
    let mut socket = peer(&listener, 60_000).await;
    assert_eq!(read(&mut socket).await["op"], 2);
    write(&mut socket, ready()).await;
    let _ = event(&mut rx).await;
    // The call's start times are asked for first (op 43), as on any READY.
    let mut message = beyond_heartbeats(&mut socket).await;
    while message["op"] == 43 {
        message = beyond_heartbeats(&mut socket).await;
    }
    assert_eq!(
        message,
        json!({"op": 20, "d": {"stream_key": "guild:40:30:77"}})
    );
    task.abort();
    let _ = task.await;
}
