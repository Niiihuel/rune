//! Reactions, typing and pins through the same checked queue as messages.
//! Local fixtures only: no request here has reached the real service.

use super::message_recovery::guild_channel;
use super::*;
use rune_provider::Failure;

const VIEW: u64 = 1 << 10;
const SEND: u64 = 1 << 11;
const HISTORY: u64 = 1 << 16;
const REACT: u64 = 1 << 6;
const CRAB: &str = "%F0%9F%A6%80";

fn react(channel: u64, message: u64, add: bool) -> Command {
    Command::ToggleReaction {
        channel: ChannelId(channel),
        message: MessageId(message),
        emoji: "🦀".into(),
        add,
    }
}

fn toggle(channel: u64, message: u64) -> Command {
    react(channel, message, true)
}

fn reaction(user: &str, name: &str) -> Value {
    json!({"channel_id":"20","message_id":"100","user_id":user,"emoji":{"id":null,"name":name}})
}

fn changed(me: bool, added: bool, emoji: &str) -> Event {
    Event::ReactionChanged {
        channel: ChannelId(20),
        message: MessageId(100),
        emoji: emoji.into(),
        me,
        added,
    }
}

async fn quiet(app: &mut Harness) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(80);
    while let Ok(Some(envelope)) =
        tokio::time::timeout_at(deadline, app.running.events.recv()).await
    {
        assert!(
            matches!(envelope.event, Event::UserChanged(_)),
            "unexpected {:?}",
            envelope.event
        );
    }
    assert!(app.requests.try_recv().is_err(), "no request was expected");
}

async fn load(app: &mut Harness, channel: u64, messages: Value) {
    app.running
        .commands
        .send(Command::LoadHistory {
            request: 1,
            channel: ChannelId(channel),
            before: None,
            limit: 50,
        })
        .await
        .unwrap();
    let (_, reply) = app.pending().await;
    reply.send(response(200, &messages.to_string())).unwrap();
    assert!(matches!(app.next_data().await, Event::HistoryLoaded { .. }));
}

fn in_channel(channel: u64, id: u64) -> Value {
    let mut message = rest::message(id, "fixture");
    message["channel_id"] = json!(channel.to_string());
    message["author"] = json!({"id":"43","username":"Peer"});
    message
}

#[tokio::test]
async fn an_own_reaction_counts_once_whichever_confirmation_arrives_first() {
    let mut app = Harness::new().await;
    load(&mut app, 20, json!([in_channel(20, 100)])).await;

    // The answer first, then the Gateway's echo of the same change.
    app.running.commands.send(toggle(20, 100)).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with(&format!(
        "PUT /api/v10/channels/20/messages/100/reactions/{CRAB}/@me "
    )));
    reply.send(response(204, "")).unwrap();
    assert_eq!(app.next_data().await, changed(true, true, "🦀"));
    for data in [reaction("42", "🦀"), reaction("43", "🎉")] {
        app.dispatch
            .send(("MESSAGE_REACTION_ADD", data))
            .await
            .unwrap();
    }
    assert_eq!(
        app.next_data().await,
        changed(false, true, "🎉"),
        "the echo of the account's own reaction was dropped"
    );

    // Taking it back; this time the Gateway confirms before the answer.
    app.running
        .commands
        .send(react(20, 100, false))
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with(&format!(
        "DELETE /api/v10/channels/20/messages/100/reactions/{CRAB}/@me "
    )));
    app.dispatch
        .send(("MESSAGE_REACTION_REMOVE", reaction("42", "🦀")))
        .await
        .unwrap();
    assert_eq!(app.next_data().await, changed(true, false, "🦀"));
    reply.send(response(204, "")).unwrap();
    quiet(&mut app).await;
}

#[tokio::test]
async fn an_unloaded_message_is_read_once_and_the_command_says_which_way() {
    let mut app = Harness::new().await;
    // The baseline already shows the reaction: adding goes out all the same,
    // since only what the person asked last says which way it goes.
    app.running.commands.send(toggle(20, 100)).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/channels/20/messages/100 "));
    let mut baseline = in_channel(20, 100);
    baseline["reactions"] = json!([{"emoji":{"id":null,"name":"🦀"},"count":2,"me":true}]);
    reply.send(response(200, &baseline.to_string())).unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with(&format!(
        "PUT /api/v10/channels/20/messages/100/reactions/{CRAB}/@me "
    )));
    reply.send(response(204, "")).unwrap();
    assert_eq!(app.next_data().await, changed(true, true, "🦀"));
}

#[tokio::test]
async fn of_two_quick_reactions_the_last_wins_when_the_first_fails() {
    let mut app = Harness::new().await;
    load(&mut app, 20, json!([in_channel(20, 100)])).await;
    // Added and taken back before any answer; the first is refused.
    app.running.commands.send(toggle(20, 100)).await.unwrap();
    app.running
        .commands
        .send(react(20, 100, false))
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PUT "));
    reply
        .send(response(403, r#"{"code":50013,"message":"SYNTHETIC"}"#))
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::React { .. },
            ..
        }
    ));
    // A flip read from the unchanged baseline would add it again here.
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with(&format!(
            "DELETE /api/v10/channels/20/messages/100/reactions/{CRAB}/@me "
        )),
        "{request}"
    );
    reply.send(response(204, "")).unwrap();
    assert_eq!(app.next_data().await, changed(true, false, "🦀"));
    quiet(&mut app).await;
}

#[tokio::test]
async fn a_reaction_without_permission_or_revoked_before_its_turn_sends_nothing() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW | SEND | HISTORY).await;
    load(&mut app, 30, json!([in_channel(30, 300)])).await;
    for command in [
        toggle(30, 300),
        // Custom and malformed reactions never reach a request path.
        Command::ToggleReaction {
            channel: ChannelId(20),
            message: MessageId(100),
            emoji: "name:9".into(),
            add: true,
        },
    ] {
        app.running.commands.send(command).await.unwrap();
        assert!(matches!(
            app.next_data().await,
            Event::Failed {
                failure: Failure::React { .. },
                ambiguous: false,
                ..
            }
        ));
    }
    quiet(&mut app).await;

    // Allowed when clicked, revoked while another request holds the queue.
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW | SEND | HISTORY | REACT).await;
    load(&mut app, 30, json!([in_channel(30, 300)])).await;
    app.running.commands.send(rest::history(2)).await.unwrap();
    let (_, busy) = app.pending().await;
    app.running.commands.send(toggle(30, 300)).await.unwrap();
    app.dispatch
        .send((
            "CHANNEL_UPDATE",
            json!({"id":"30","guild_id":"10","type":0,"name":"fixture",
                "permission_overwrites":[{"id":"42","type":1,"allow":"0","deny":REACT.to_string()}]}),
        ))
        .await
        .unwrap();
    assert!(matches!(app.next_data().await, Event::ChannelChanged(_)));
    busy.send(response(200, "[]")).unwrap();
    assert!(matches!(app.next_data().await, Event::HistoryLoaded { .. }));
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::React { .. },
            ambiguous: false,
            ..
        }
    ));
    quiet(&mut app).await;
}

#[tokio::test]
async fn a_revocation_during_the_write_drops_it_as_ambiguous() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW | SEND | HISTORY | REACT).await;
    load(&mut app, 30, json!([in_channel(30, 300)])).await;
    app.running.commands.send(toggle(30, 300)).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PUT /api/v10/channels/30/messages/300/reactions/"));
    app.dispatch
        .send((
            "CHANNEL_UPDATE",
            json!({"id":"30","guild_id":"10","type":0,"name":"fixture",
                "permission_overwrites":[{"id":"42","type":1,"allow":"0","deny":REACT.to_string()}]}),
        ))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::React { .. },
            ambiguous: true,
            ..
        }
    ));
    assert!(matches!(app.next_data().await, Event::ChannelChanged(_)));
    reply.send(response(204, "")).unwrap();
    quiet(&mut app).await;
}

#[tokio::test]
async fn refused_reactions_are_classified_and_never_repeated() {
    let mut app = Harness::new().await;
    load(&mut app, 20, json!([in_channel(20, 100)])).await;
    app.running.commands.send(toggle(20, 100)).await.unwrap();
    let (_, reply) = app.pending().await;
    reply
        .send(response(403, r#"{"code":50013,"message":"SYNTHETIC"}"#))
        .unwrap();
    let Event::Failed {
        failure: Failure::React { .. },
        reason,
        ambiguous: false,
    } = app.next_data().await
    else {
        panic!("a definite refusal")
    };
    assert!(reason.contains("permiso") && !reason.contains("SYNTHETIC"));
    quiet(&mut app).await;

    app.running.commands.send(toggle(20, 100)).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PUT "), "a refusal is never repeated");
    reply
        .send(
            "HTTP/1.1 429 Test\r\nRetry-After: 30\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .into(),
        )
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::React { .. },
            ambiguous: false,
            ..
        }
    ));
    // The hold covers every route; nothing is retried or sent meanwhile.
    app.running.commands.send(toggle(20, 100)).await.unwrap();
    let Event::Failed { reason, .. } = app.next_data().await else {
        panic!("held")
    };
    assert!(reason.contains("esperar"));
    quiet(&mut app).await;
}

#[tokio::test]
async fn typing_is_one_post_and_others_typing_is_shown() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::StartTyping {
            channel: ChannelId(20),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/typing "));
    reply.send(response(204, "")).unwrap();
    for data in [
        json!({"channel_id":"20","user_id":"42","timestamp":1}),
        json!({"channel_id":"99","user_id":"43","timestamp":1}),
        json!({"channel_id":"20","user_id":"x"}),
        json!({"channel_id":"20","user_id":"43","timestamp":1}),
    ] {
        app.dispatch.send(("TYPING_START", data)).await.unwrap();
    }
    assert_eq!(
        app.next_data().await,
        Event::Typing {
            channel: ChannelId(20),
            user: UserId(43)
        },
        "the account itself, an unknown channel and a malformed one are dropped"
    );
    quiet(&mut app).await;

    // Without the send bit, typing is refused silently: it has no failure.
    guild_channel(&mut app, VIEW | HISTORY).await;
    app.running
        .commands
        .send(Command::StartTyping {
            channel: ChannelId(30),
        })
        .await
        .unwrap();
    quiet(&mut app).await;
}

#[tokio::test]
async fn pins_need_moderation_in_servers_and_answer_with_the_pinned_field() {
    let mut app = Harness::new().await;
    for (command, method) in [
        (
            Command::PinMessage {
                channel: ChannelId(20),
                message: MessageId(100),
            },
            "PUT",
        ),
        (
            Command::UnpinMessage {
                channel: ChannelId(20),
                message: MessageId(100),
            },
            "DELETE",
        ),
    ] {
        app.running.commands.send(command).await.unwrap();
        let (request, reply) = app.pending().await;
        assert!(request.starts_with(&format!("{method} /api/v10/channels/20/messages/pins/100 ")));
        reply.send(response(204, "")).unwrap();
        let Event::MessagePatched(patch) = app.next_data().await else {
            panic!("confirmed pin")
        };
        assert_eq!(patch.pinned, Some(method == "PUT"));
        assert_eq!(patch.content, None);
    }

    app.running
        .commands
        .send(Command::LoadPins {
            request: 5,
            channel: ChannelId(20),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/channels/20/messages/pins?limit=50 "));
    reply
        .send(response(
            200,
            &json!({"items":[{"pinned_at":"2026-10-14T12:00:00Z","message":in_channel(20, 100)}],
                "has_more":false})
            .to_string(),
        ))
        .unwrap();
    let Event::PinsLoaded {
        request: 5,
        messages,
        ..
    } = app.next_data().await
    else {
        panic!("pins")
    };
    assert!(messages.len() == 1 && messages[0].pinned);

    guild_channel(&mut app, VIEW | SEND | HISTORY).await;
    app.running
        .commands
        .send(Command::PinMessage {
            channel: ChannelId(30),
            message: MessageId(300),
        })
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::Pin { .. },
            ambiguous: false,
            ..
        }
    ));
    quiet(&mut app).await;

    // Discord's pin bit (51) with the history is enough.
    let pin_bit = (1_u64 << 51).to_string();
    let channel = json!({"id":"30","guild_id":"10","type":0,"name":"fixture",
        "permission_overwrites":[{"id":"42","type":1,"allow":pin_bit,"deny":"0"}]});
    app.dispatch
        .send(("CHANNEL_UPDATE", channel))
        .await
        .unwrap();
    assert!(matches!(app.next_data().await, Event::ChannelChanged(_)));
    load(&mut app, 30, json!([in_channel(30, 300)])).await;
    app.running
        .commands
        .send(Command::PinMessage {
            channel: ChannelId(30),
            message: MessageId(300),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("PUT /api/v10/channels/30/messages/pins/300 "));
    reply.send(response(204, "")).unwrap();
    assert!(matches!(app.next_data().await, Event::MessagePatched(_)));
    quiet(&mut app).await;
}

#[tokio::test]
async fn of_two_quick_pin_requests_the_last_wins_when_the_first_fails() {
    let mut app = Harness::new().await;
    let request = |pinned: bool| {
        let (channel, message) = (ChannelId(20), MessageId(100));
        if pinned {
            Command::PinMessage { channel, message }
        } else {
            Command::UnpinMessage { channel, message }
        }
    };
    // Each asks for a state, so a refused first one cannot turn the second.
    app.running.commands.send(request(true)).await.unwrap();
    app.running.commands.send(request(false)).await.unwrap();
    let (first, reply) = app.pending().await;
    assert!(first.starts_with("PUT "));
    reply
        .send(response(403, r#"{"code":50013,"message":"SYNTHETIC"}"#))
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::Pin { .. },
            ..
        }
    ));
    let (second, reply) = app.pending().await;
    assert!(second.starts_with("DELETE /api/v10/channels/20/messages/pins/100 "));
    reply.send(response(204, "")).unwrap();
    let Event::MessagePatched(patch) = app.next_data().await else {
        panic!("confirmed")
    };
    assert_eq!(patch.pinned, Some(false));
    quiet(&mut app).await;
}
