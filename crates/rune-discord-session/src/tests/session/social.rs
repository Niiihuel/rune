//! Conversations outside servers through the session's checked queue: the
//! answer keeps the session's own list current, and a closed conversation
//! comes back with its next message. Local fixtures only.

use super::*;
use rune_provider::Created;

#[tokio::test]
async fn a_closed_conversation_leaves_the_list_and_comes_back_with_a_message() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::CloseDirect {
            channel: ChannelId(20),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("DELETE /api/v10/channels/20 "));
    reply
        .send(response(200, r#"{"id":"20","type":1}"#))
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::ChannelRemoved {
            channel: ChannelId(20)
        }
    );
    // Closed here, written in by the other person: listed again first.
    let mut message = rest::message(300, "¿seguís ahí?");
    message["author"] = json!({"id":"43","username":"Peer"});
    app.dispatch
        .send(("MESSAGE_CREATE", message))
        .await
        .unwrap();
    assert!(
        matches!(app.next_data().await, Event::ChannelChanged(channel) if channel.id == ChannelId(20))
    );
    assert!(matches!(
        app.next_data().await,
        Event::MessageCreated { message, .. } if message.id == MessageId(300)
    ));
}

#[tokio::test]
async fn an_opened_conversation_can_be_written_in_before_the_gateway_lists_it() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::OpenDirect {
            request: 3,
            user: UserId(44),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/users/@me/channels "));
    reply
        .send(response(
            200,
            r#"{"id":"21","type":1,"recipients":[{"id":"44","username":"Nueva"}]}"#,
        ))
        .unwrap();
    assert!(
        matches!(app.next_data().await, Event::ChannelChanged(channel) if channel.id == ChannelId(21))
    );
    assert_eq!(
        app.next_data().await,
        Event::Created {
            request: 3,
            created: Created::Channel(ChannelId(21)),
        }
    );
    app.running
        .commands
        .send(Command::SendMessage {
            channel: ChannelId(21),
            content: "hola".into(),
            reply_to: None,
            nonce: 9,
            mention_reply: false,
        })
        .await
        .unwrap();
    let (request, _) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/21/messages "));
}

#[tokio::test]
async fn a_friend_request_waits_for_the_gateway_to_say_what_changed() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::SendFriendRequest {
            username: "zoe".into(),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/users/@me/relationships "));
    reply
        .send("HTTP/1.1 204 No Content\r\nConnection: close\r\n\r\n".into())
        .unwrap();
    // Accepting a request this session never heard of does not leave.
    app.running
        .commands
        .send(Command::AcceptFriendRequest { user: UserId(77) })
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: rune_provider::Failure::Relationship {
                user: Some(UserId(77))
            },
            ambiguous: false,
            ..
        }
    ));
}

/// A synthetic READY with one incoming request; no account traffic.
async fn incoming_request(app: &mut Harness) {
    app.dispatch
        .send((
            "READY",
            json!({"user":{"id":"42"}, "session_id":"friend-request-fixture",
                "resume_gateway_url":"wss://gateway.discord.gg", "guilds":[],
                "private_channels":[{"id":"20","type":1,
                    "recipients":[{"id":"43","username":"Peer"}]}],
                "relationships":[{"id":"77","type":3,
                    "user":{"id":"77","username":"Requests"}}]}),
        ))
        .await
        .unwrap();
    assert!(matches!(app.next_data().await, Event::Ready(snapshot)
        if snapshot.relationships.as_ref().is_some_and(|known|
            known.iter().any(|r| r.user == UserId(77)
                && r.kind == rune_model::RelationshipKind::IncomingRequest))));
}

async fn relationship_update(app: &mut Harness, user: u64, kind: u8) {
    app.dispatch
        .send((
            "RELATIONSHIP_UPDATE",
            json!({"id":user.to_string(),"type":kind}),
        ))
        .await
        .unwrap();
}

/// Skip READY's account notices, but never hide a failed command.
async fn next_relationship(app: &mut Harness) -> Event {
    loop {
        if let event @ (Event::RelationshipChanged(_) | Event::Failed { .. }) = app.next_data().await {
            return event;
        }
    }
}

#[tokio::test]
async fn accepted_friendship_before_http_reply_does_not_report_a_failed_acceptance() {
    let mut app = Harness::new().await;
    incoming_request(&mut app).await;
    app.running
        .commands
        .send(Command::AcceptFriendRequest { user: UserId(77) })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PUT /api/v10/users/@me/relationships/77 "));
    // The HTTP response stays pending until the authoritative update arrives.
    relationship_update(&mut app, 77, 1).await;
    assert!(matches!(next_relationship(&mut app).await,
        Event::RelationshipChanged(known) if known.user == UserId(77)
            && known.kind == rune_model::RelationshipKind::Friend));
    reply.send(response(204, "")).unwrap();
    // A subsequent read is a command-order barrier and also proves that no
    // duplicate acceptance was issued after the request was retired.
    app.running.commands.send(rest::history(91)).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/channels/20/messages?limit="));
    reply.send(response(200, "[]")).unwrap();
    loop {
        match app.next_data().await {
            Event::HistoryLoaded { request: 91, .. } => break,
            Event::Failed {
                failure, reason, ..
            } => panic!("{failure:?}: {reason}"),
            _ => {}
        }
    }
}

#[tokio::test]
async fn another_friendship_does_not_hide_a_request_that_became_blocked() {
    let mut app = Harness::new().await;
    incoming_request(&mut app).await;
    app.running
        .commands
        .send(Command::AcceptFriendRequest { user: UserId(77) })
        .await
        .unwrap();
    let (_, _reply) = app.pending().await;
    relationship_update(&mut app, 78, 1).await;
    assert!(matches!(next_relationship(&mut app).await,
        Event::RelationshipChanged(known) if known.user == UserId(78)));
    relationship_update(&mut app, 77, 2).await;
    assert!(matches!(
        next_relationship(&mut app).await,
        Event::Failed {
            failure: rune_provider::Failure::Relationship {
                user: Some(UserId(77))
            },
            ..
        }
    ));
}

#[tokio::test]
async fn a_queued_acceptance_confirmed_while_another_read_waits_is_not_sent_or_failed() {
    let mut app = Harness::new().await;
    incoming_request(&mut app).await;
    app.running.commands.send(rest::history(92)).await.unwrap();
    let (_, blocked) = app.pending().await;
    app.running
        .commands
        .send(Command::AcceptFriendRequest { user: UserId(77) })
        .await
        .unwrap();
    // An invalid later command is a local command-order barrier: the valid
    // acceptance is queued before the Gateway confirmation is dispatched.
    app.running
        .commands
        .send(Command::AcceptFriendRequest { user: UserId(999) })
        .await
        .unwrap();
    assert!(matches!(
        next_relationship(&mut app).await,
        Event::Failed {
            failure: rune_provider::Failure::Relationship {
                user: Some(UserId(999))
            },
            ..
        }
    ));
    relationship_update(&mut app, 77, 1).await;
    assert!(matches!(next_relationship(&mut app).await,
        Event::RelationshipChanged(known) if known.user == UserId(77)
            && known.kind == rune_model::RelationshipKind::Friend));
    blocked.send(response(200, "[]")).unwrap();
    app.running.commands.send(rest::history(93)).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/channels/20/messages?limit="));
    reply.send(response(200, "[]")).unwrap();
    loop {
        match app.next_data().await {
            Event::HistoryLoaded { request: 93, .. } => break,
            Event::Failed {
                failure, reason, ..
            } => panic!("{failure:?}: {reason}"),
            _ => {}
        }
    }
}
