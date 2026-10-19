//! No account traffic: terminal session lifetime is exercised over loopback.
use super::*;

#[tokio::test]
async fn gateway_4004_discards_pending_work_and_allows_only_a_new_explicit_login() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(1)).await.unwrap();
    let (_, late) = app.pending().await;
    app.running.commands.send(rest::history(2)).await.unwrap();
    app.dispatch
        .send(("TEST_CLOSE", json!(4004)))
        .await
        .unwrap();
    let mut failed = Vec::new();
    let reason = loop {
        match app.next_data().await {
            Event::Failed {
                failure: rune_provider::Failure::History { request, .. },
                ..
            } => failed.push(request),
            Event::Authentication(AuthenticationState::Failed { reason }) => break reason,
            _ => panic!("unexpected event before authentication failure"),
        }
    };
    assert_eq!(failed, vec![1, 2]);
    assert!(reason.contains("Gateway 4004"));
    assert!(!reason.contains("SYNTHETIC_PRIVATE_CLOSE_REASON"));
    assert!(matches!(
        app.next_data().await,
        Event::Connection(ConnectionState::Disconnected { .. })
    ));
    assert!(
        app.attempts.try_recv().is_err(),
        "no automatic reauthentication"
    );
    late.send(response(200, "[]")).unwrap();
    app.running.commands.send(rest::history(3)).await.unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: rune_provider::Failure::History { request: 3, .. },
            ..
        }
    ));
    assert!(app.requests.try_recv().is_err());
    // This command was previously ignored because the rejected session survived.
    app.running
        .commands
        .send(Command::BeginSignIn)
        .await
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::Authentication(AuthenticationState::AwaitingExternal)
    );
    let attempt = tokio::time::timeout(Duration::from_secs(1), app.attempts.recv())
        .await
        .unwrap()
        .unwrap();
    app.running
        .commands
        .send(Command::CancelSignIn)
        .await
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    assert!(attempt.is_closed());
}

#[tokio::test]
async fn gateway_other_terminal_close_stays_offline_without_automatic_login() {
    let mut app = Harness::new().await;
    app.dispatch
        .send(("TEST_CLOSE", json!(4014)))
        .await
        .unwrap();
    let Event::Connection(ConnectionState::Disconnected { reason }) = app.next_data().await else {
        panic!("terminal connection state");
    };
    assert!(reason.contains("Gateway 4014"));
    assert!(!reason.contains("SYNTHETIC_PRIVATE_CLOSE_REASON"));
    assert!(app.attempts.try_recv().is_err());
    app.running
        .commands
        .send(Command::BeginSignIn)
        .await
        .unwrap();
    // An offline command is a barrier after BeginSignIn: no new login was started.
    app.running.commands.send(rest::history(1)).await.unwrap();
    assert!(matches!(app.next_data().await, Event::Failed { .. }));
    assert!(app.attempts.try_recv().is_err());
    assert!(app.requests.try_recv().is_err());
}

/// The Gateway as far as its IDENTIFY, its connection handed back.
async fn identified(
    listener: &TcpListener,
) -> tokio_tungstenite::WebSocketStream<tokio::net::TcpStream> {
    let (socket, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
        .await
        .expect("the Gateway connects")
        .unwrap();
    let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
    socket
        .send(Message::text(
            json!({"op":10,"d":{"heartbeat_interval":60000}}).to_string(),
        ))
        .await
        .unwrap();
    loop {
        let frame = socket.next().await.unwrap().unwrap();
        let value: Value = serde_json::from_str(frame.to_text().unwrap()).unwrap();
        if value["op"] == 2 {
            return socket;
        }
    }
}

/// «Reintentar ya» reaches the wait of the very Gateway the provider
/// started: a lost connection is tried again at once, not in a minute.
#[tokio::test]
async fn reconnect_now_cuts_the_gateways_wait_short() {
    let (http, _) = fixture(response(200, USER)).await;
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let gateway = Gateway::local(&format!("ws://{}", listener.local_addr().unwrap()))
        .waiting(Duration::from_secs(60));
    let (login, mut attempts) = login();
    let mut running = run_with_gateway(login, http, gateway);
    assert_eq!(
        running.next().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    running.commands.send(Command::BeginSignIn).await.unwrap();
    attempts.recv().await.unwrap().send(Ok(secret())).unwrap();
    while running.next().await != Event::Connection(ConnectionState::Connecting) {}
    // Lost before READY: the provider says the full wait.
    let mut first = identified(&listener).await;
    first
        .send(Message::text(json!({"op":7,"d":null}).to_string()))
        .await
        .unwrap();
    assert_eq!(
        running.next().await,
        Event::Connection(ConnectionState::Reconnecting {
            attempt: 1,
            retry_in_ms: 60_000
        })
    );
    running.commands.send(Command::ReconnectNow).await.unwrap();
    identified(&listener).await;
}
