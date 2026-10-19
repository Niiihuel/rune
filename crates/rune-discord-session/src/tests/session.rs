//! End-to-end local HTTP/Gateway fixtures; no real account traffic.

mod account;
mod actions;
mod admin;
mod challenges;
mod context;
// Commands and interactions (P3).
mod interactions;
// Presence, identity and expressions (P5).
mod identity;
mod members;
mod message_recovery;
mod reads;
mod social;
mod uploads;
mod voice;
// Message actions and moderation (P4).
mod message_actions;
// Desktop, language and local voice (P6).
mod desktop;
// No declared command ends in «todavía no está disponible» (2026-10-18).
mod unavailable;
// Rules and onboarding (2026-10-18).
mod onboarding;
// Threads, forums and stages (2026-10-18).
mod threads;
// Scheduled events (eventos-programados).
mod events;
// AutoMod (automod).
mod automod;

use super::*;
use futures_util::{SinkExt, StreamExt};
use rune_model::{ChannelId, MessageId};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::Message;

type PendingRequest = (String, oneshot::Sender<String>);

struct Harness {
    running: Running,
    attempts: mpsc::UnboundedReceiver<LoginReply>,
    /// Challenges shown in the fake verification window.
    verifications: mpsc::UnboundedReceiver<VerifyReply>,
    requests: mpsc::Receiver<PendingRequest>,
    dispatch: mpsc::Sender<(&'static str, Value)>,
    /// What the client sent the Gateway, heartbeats and presence aside.
    frames: mpsc::UnboundedReceiver<Value>,
    /// The presence updates (op 3) the client sent.
    presences: mpsc::UnboundedReceiver<Value>,
    closed: Option<oneshot::Receiver<()>>,
    servers: Vec<tokio::task::JoinHandle<()>>,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.running.task.abort();
        for server in &self.servers {
            server.abort();
        }
    }
}

impl Harness {
    async fn new() -> Self {
        let http_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http = Http::local(&format!("http://{}", http_listener.local_addr().unwrap()));
        let (capture, requests) = mpsc::channel(8);
        let http_server = tokio::spawn(async move {
            let (mut socket, _) = http_listener.accept().await.unwrap();
            assert!(request(&mut socket)
                .await
                .starts_with("GET /api/v10/users/@me "));
            socket
                .write_all(response(200, USER).as_bytes())
                .await
                .unwrap();
            drop(socket);
            while let Ok((mut socket, _)) = http_listener.accept().await {
                let request = request(&mut socket).await;
                // The account's settings, read after each READY: none set.
                if request.starts_with("GET /api/v10/users/@me/settings-proto/1 ") {
                    let answer = crate::settings_proto::answer(1, &[]).to_string();
                    let _ = socket.write_all(response(200, &answer).as_bytes()).await;
                    continue;
                }
                let (reply, delayed) = oneshot::channel::<String>();
                if capture.send((request, reply)).await.is_err() {
                    break;
                }
                if let Ok(reply) = delayed.await {
                    let _ = socket.write_all(reply.as_bytes()).await;
                }
            }
        });
        let gateway_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let gateway = Gateway::local(&format!("ws://{}", gateway_listener.local_addr().unwrap()));
        let (dispatch, mut updates) = mpsc::channel::<(&'static str, Value)>(8);
        let (closed, closing) = oneshot::channel();
        let (seen, frames) = mpsc::unbounded_channel();
        let (told, presences) = mpsc::unbounded_channel();
        let gateway_server = tokio::spawn(async move {
            let (socket, _) = gateway_listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(socket).await.unwrap();
            socket
                .send(Message::text(
                    json!({"op":10,"d":{"heartbeat_interval":60000}}).to_string(),
                ))
                .await
                .unwrap();
            let identify: Value =
                serde_json::from_str(socket.next().await.unwrap().unwrap().to_text().unwrap())
                    .unwrap();
            assert_eq!(identify["op"], 2);
            assert_eq!(identify["d"]["token"], SECRET);
            let ready = json!({"op":0,"t":"READY","s":1,"d":{
                "user":{"id":"42"},"session_id":"synthetic-session",
                "resume_gateway_url":"wss://gateway.discord.gg",
                "guilds":[],"private_channels":[{"id":"20","type":1,
                    "recipients":[{"id":"43","username":"Peer"}]}]
            }});
            socket.send(Message::text(ready.to_string())).await.unwrap();
            let mut sequence = 1;
            loop {
                tokio::select! {
                    update = updates.recv() => {
                        let Some((kind,data)) = update else { break; };
                        if kind == "TEST_CLOSE" {
                            use tokio_tungstenite::tungstenite::protocol::{CloseFrame, frame::coding::CloseCode};
                            let code=u16::try_from(data.as_u64().unwrap()).unwrap();
                            let _ = socket.close(Some(CloseFrame {
                                code:CloseCode::from(code), reason:"SYNTHETIC_PRIVATE_CLOSE_REASON".into(),
                            })).await;
                            break;
                        }
                        if kind == "TEST_DISCONNECT" {
                            let _ = socket.close(None).await;
                            break;
                        }
                        sequence += 1;
                        if socket.send(Message::text(json!({"op":0,"t":kind,"s":sequence,"d":data}).to_string())).await.is_err() { break; }
                    }
                    frame = socket.next() => {
                        let Some(Ok(frame)) = frame else { break; };
                        if frame.is_close() { break; }
                        if let Ok(text) = frame.to_text() {
                            let value = serde_json::from_str::<Value>(text).unwrap();
                            if value["op"] == 3 {
                                let _ = told.send(value);
                            } else if value["op"] != 1 {
                                let _ = seen.send(value);
                            } else if socket.send(Message::text(r#"{"op":11,"d":null}"#)).await.is_err() { break; }
                        }
                    }
                }
            }
            let _ = closed.send(());
        });
        let (login, mut attempts, verifications) = controlled();
        let mut running = run_with_gateway(login, http, gateway);
        assert_eq!(
            running.next().await,
            Event::Authentication(AuthenticationState::SignedOut)
        );
        running.commands.send(Command::BeginSignIn).await.unwrap();
        assert_eq!(
            running.next().await,
            Event::Authentication(AuthenticationState::AwaitingExternal)
        );
        attempts.recv().await.unwrap().send(Ok(secret())).unwrap();
        assert_eq!(
            running.next().await,
            Event::Authentication(AuthenticationState::Exchanging)
        );
        assert!(
            matches!(running.next().await,Event::Ready(snapshot) if snapshot.channels.is_empty())
        );
        assert_eq!(
            running.next().await,
            Event::Authentication(AuthenticationState::SignedIn { scopes: vec![] })
        );
        assert_eq!(
            running.next().await,
            Event::Connection(ConnectionState::Connecting)
        );
        let Event::Ready(snapshot) = running.next().await else {
            panic!("account snapshot");
        };
        assert_eq!(snapshot.channels.len(), 1);
        assert_eq!(snapshot.channels[0].id, ChannelId(20));
        assert_eq!(snapshot.channels[0].name, "Peer");
        assert_eq!(
            running.next().await,
            Event::Connection(ConnectionState::Connected)
        );
        // What its plan allows: no plan, no emoji or stickers elsewhere (P5).
        assert!(matches!(
            running.next_any().await,
            Event::Perks(perks) if !perks.emoji_anywhere && !perks.stickers_anywhere
        ));
        // Invisible, as the Gateway was told, until its settings are read.
        assert!(matches!(
            running.next_any().await,
            Event::PresenceChanged { user: rune_model::UserId(42), presence }
                if presence.status == rune_model::Status::Invisible
        ));
        // The account's own upload limit follows the snapshot.
        assert_eq!(
            running.next_any().await,
            Event::UploadLimit {
                server: None,
                max_file_bytes: 20 * 1024 * 1024
            }
        );
        // Then its settings are read: none set.
        assert!(matches!(
            running.next_any().await,
            Event::PresenceChanged {
                user: rune_model::UserId(42),
                ..
            }
        ));
        assert_eq!(running.next_any().await, Event::ServerFolders(Vec::new()));
        assert!(matches!(
            running.next_any().await,
            Event::MessagingPrivacy(_)
        ));
        // Desktop session and games: none set, the account shows it.
        assert_eq!(running.next_any().await, Event::ActivitySharing(true));
        Self {
            running,
            attempts,
            verifications,
            requests,
            dispatch,
            frames,
            presences,
            closed: Some(closing),
            servers: vec![http_server, gateway_server],
        }
    }

    async fn pending(&mut self) -> PendingRequest {
        tokio::time::timeout(Duration::from_secs(1), self.requests.recv())
            .await
            .unwrap()
            .unwrap()
    }

    async fn next_data(&mut self) -> Event {
        loop {
            let event = self.running.next().await;
            if !matches!(event, Event::UserChanged(_)) {
                return event;
            }
        }
    }
}

#[tokio::test]
async fn account_bootstrap_history_send_edit_delete_and_gateway_patch_work_together() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(1)).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/channels/20/messages?limit=50 "));
    reply
        .send(response(
            200,
            &json!([rest::message(100, "old")]).to_string(),
        ))
        .unwrap();
    assert!(
        matches!(app.next_data().await,Event::HistoryLoaded {request:1,messages,..} if messages.len()==1)
    );
    app.running.commands.send(rest::send(2)).await.unwrap();
    let (request, reply) = app.pending().await;
    let body: Value = serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap();
    let mut sent = rest::message(101, "hello");
    sent["nonce"] = body["nonce"].clone();
    reply.send(response(200, &sent.to_string())).unwrap();
    assert!(
        matches!(app.next_data().await,Event::MessageCreated {nonce:Some(2),message} if message.id==MessageId(101))
    );
    app.dispatch.send(("MESSAGE_CREATE", sent)).await.unwrap();
    assert!(
        matches!(app.next_data().await,Event::MessageCreated {nonce:Some(2),message} if message.id==MessageId(101))
    );
    app.running
        .commands
        .send(Command::EditMessage {
            channel: ChannelId(20),
            message: MessageId(101),
            content: "edited".into(),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PATCH /api/v10/channels/20/messages/101 "));
    reply
        .send(response(200, &rest::message(101, "edited").to_string()))
        .unwrap();
    assert!(
        matches!(app.next_data().await,Event::MessageUpdated(message) if message.content=="edited")
    );
    app.dispatch
        .send((
            "MESSAGE_UPDATE",
            json!({"id":"101","channel_id":"20","content":"gateway edit"}),
        ))
        .await
        .unwrap();
    assert!(
        matches!(app.next_data().await,Event::MessagePatched(patch) if patch.content.as_deref()==Some("gateway edit") && patch.author.is_none())
    );
    app.running
        .commands
        .send(Command::DeleteMessage {
            channel: ChannelId(20),
            message: MessageId(101),
        })
        .await
        .unwrap();
    let (_, reply) = app.pending().await;
    reply.send(response(204, "")).unwrap();
    assert_eq!(
        app.next_data().await,
        Event::MessageDeleted {
            channel: ChannelId(20),
            message: MessageId(101)
        }
    );
}

#[tokio::test]
async fn live_tallies_are_forwarded_once_and_a_vote_is_one_checked_write() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(1)).await.unwrap();
    let (_, reply) = app.pending().await;
    let mut open = rest::message(100, "");
    open["author"] = json!({"id":"43","username":"Peer"});
    open["poll"] = json!({"question":{"text":"¿Cuándo?"},"answers":[
        {"answer_id":1,"poll_media":{"text":"Hoy"}},{"answer_id":2,"poll_media":{"text":"Mañana"}}],
        "expiry":null,"allow_multiselect":false,
        "results":{"is_finalized":false,"answer_counts":[{"id":1,"count":2,"me_voted":false}]}});
    let mut ended = open.clone();
    ended["id"] = json!("99");
    ended["poll"]["expiry"] = json!("2020-01-01T00:00:00Z");
    reply
        .send(response(200, &json!([open, ended]).to_string()))
        .unwrap();
    assert!(matches!(app.next_data().await, Event::HistoryLoaded { .. }));
    let vote =
        |user: &str| json!({"channel_id":"20","message_id":"100","user_id":user,"answer_id":1});
    for _ in 0..2 {
        app.dispatch
            .send(("MESSAGE_POLL_VOTE_ADD", vote("43")))
            .await
            .unwrap();
    }
    app.dispatch
        .send((
            "MESSAGE_REACTION_ADD",
            json!({"channel_id":"20","message_id":"100","user_id":"42",
                "emoji":{"id":null,"name":"🦀"}}),
        ))
        .await
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::PollVoteChanged {
            channel: ChannelId(20),
            message: MessageId(100),
            answer: 1,
            me: false,
            added: true,
        }
    );
    assert_eq!(
        app.next_data().await,
        Event::ReactionChanged {
            channel: ChannelId(20),
            message: MessageId(100),
            emoji: "🦀".into(),
            me: true,
            added: true,
        },
        "the repeated vote was dropped"
    );
    // Refused before any request: an ended poll, an answer it does not
    // have, two answers of a single choice and a message never loaded.
    for (message, answers) in [
        (99, vec![1]),
        (100, vec![7]),
        (100, vec![1, 2]),
        (5, vec![1]),
    ] {
        app.running
            .commands
            .send(Command::VotePoll {
                channel: ChannelId(20),
                message: MessageId(message),
                answers,
            })
            .await
            .unwrap();
        assert!(matches!(
            app.next_data().await,
            Event::Failed { failure: rune_provider::Failure::Vote { message: id, .. }, ambiguous: false, .. }
                if id == MessageId(message)
        ));
    }
    app.running
        .commands
        .send(Command::VotePoll {
            channel: ChannelId(20),
            message: MessageId(100),
            answers: vec![2],
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PUT /api/v10/channels/20/polls/100/answers/@me "));
    assert!(request.ends_with("{\"answer_ids\":[\"2\"]}"));
    reply.send(response(204, "")).unwrap();
    app.dispatch
        .send(("MESSAGE_POLL_VOTE_ADD", vote("42")))
        .await
        .unwrap();
    assert!(
        matches!(
            app.next_data().await,
            Event::PollVoteChanged { me: true, .. }
        ),
        "the success itself emits nothing; the Gateway confirms the vote"
    );
}

#[tokio::test]
async fn removing_a_channel_cancels_delayed_history_before_it_can_restore_data() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(1)).await.unwrap();
    let (_, reply) = app.pending().await;
    app.dispatch
        .send(("CHANNEL_DELETE", json!({"id":"20","type":1})))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            ambiguous: false,
            ..
        }
    ));
    assert!(matches!(
        app.next_data().await,
        Event::ChannelRemoved {
            channel: ChannelId(20)
        }
    ));
    reply
        .send(response(
            200,
            &json!([rest::message(100, "late")]).to_string(),
        ))
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(60), app.running.events.recv())
            .await
            .is_err()
    );
    app.running.commands.send(rest::send(2)).await.unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            ambiguous: false,
            ..
        }
    ));
    assert!(app.requests.try_recv().is_err());
}

#[tokio::test]
async fn cancel_history_then_logout_discards_late_writes_and_closes_gateway() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(1)).await.unwrap();
    let (_, reply) = app.pending().await;
    app.running.commands.send(Command::Cancel(1)).await.unwrap();
    // An immediately queued send acts as a barrier: it is received after Cancel.
    app.running.commands.send(rest::send(2)).await.unwrap();
    reply
        .send(response(
            200,
            &json!([rest::message(100, "cancelled")]).to_string(),
        ))
        .unwrap();
    let (_, late_write) = app.pending().await;
    assert!(app.running.events.try_recv().is_err());
    app.running.commands.send(Command::SignOut).await.unwrap();
    assert_eq!(
        app.next_data().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    assert!(matches!(
        app.next_data().await,
        Event::Connection(ConnectionState::Disconnected { .. })
    ));
    late_write
        .send(response(200, &rest::message(101, "late").to_string()))
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), app.closed.take().unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(60), app.running.events.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn malformed_permissions_stop_connection_and_cancel_inflight_write_as_ambiguous() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::send(1)).await.unwrap();
    let (_, reply) = app.pending().await;
    app.dispatch
        .send((
            "CHANNEL_UPDATE",
            json!({"id":"20","type":0,"guild_id":"999","permission_overwrites":"invalid"}),
        ))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            ambiguous: true,
            ..
        }
    ));
    assert!(matches!(
        app.next_data().await,
        Event::Connection(ConnectionState::Disconnected { .. })
    ));
    reply
        .send(response(200, &rest::message(100, "late").to_string()))
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(60), app.running.events.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn unauthorized_action_forgets_identity_and_disconnects_gateway() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(1)).await.unwrap();
    let (_, reply) = app.pending().await;
    reply.send(response(401, SECRET)).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            ambiguous: false,
            ..
        }
    ));
    let Event::Authentication(AuthenticationState::Failed { reason }) = app.next_data().await
    else {
        panic!("expired session");
    };
    assert!(!reason.contains(SECRET));
    tokio::time::timeout(Duration::from_secs(1), app.closed.take().unwrap())
        .await
        .unwrap()
        .unwrap();
    app.running.commands.send(rest::send(2)).await.unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            ambiguous: false,
            ..
        }
    ));
    assert!(app.requests.try_recv().is_err());
}

#[tokio::test]
async fn command_queue_is_bounded_and_logout_discards_unsent_work() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::send(1)).await.unwrap();
    let (_, reply) = app.pending().await;
    for nonce in 2..=18 {
        app.running.commands.send(rest::send(nonce)).await.unwrap();
    }
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: rune_provider::Failure::Send { nonce: 18, .. },
            ambiguous: false,
            ..
        }
    ));
    app.running.commands.send(Command::SignOut).await.unwrap();
    assert_eq!(
        app.next_data().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    assert!(matches!(
        app.next_data().await,
        Event::Connection(ConnectionState::Disconnected { .. })
    ));
    reply
        .send(response(200, &rest::message(100, "late").to_string()))
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(60), app.requests.recv())
            .await
            .is_err()
    );
    assert!(app.running.events.try_recv().is_err());
}

mod invitations;

mod gateway;

mod rejections;
