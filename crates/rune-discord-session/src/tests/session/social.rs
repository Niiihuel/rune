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
