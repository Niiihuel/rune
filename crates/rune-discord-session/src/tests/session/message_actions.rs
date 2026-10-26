//! Message actions and voice moderation (P4) through the whole provider,
//! against local fixtures only. None of this reached Discord.

use super::message_recovery::guild_channel;
use super::*;
use rune_model::{ServerId, UserId};
use rune_provider::{NewPoll, NewPollAnswer, VoiceModeration};

const VIEW: u64 = 1 << 10;
const SEND: u64 = 1 << 11;
const HISTORY: u64 = 1 << 16;
const MUTE: u64 = 1 << 22;
const MOVE: u64 = 1 << 24;
const CONNECT: u64 = 1 << 20;

fn body(request: &str) -> Value {
    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
}

fn poll() -> NewPoll {
    let answer = |text: &str, emoji: Option<&str>| NewPollAnswer {
        text: text.into(),
        emoji: emoji.map(str::to_owned),
    };
    NewPoll {
        question: "¿Pizza o empanadas?".into(),
        answers: vec![answer("Pizza", Some("🍕")), answer("Empanadas", None)],
        hours: 24,
        multiselect: true,
    }
}

#[tokio::test]
async fn a_forward_names_the_original_and_comes_back_with_its_nonce() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW | SEND | HISTORY).await;
    app.running
        .commands
        .send(Command::ForwardMessage {
            channel: ChannelId(30),
            message: MessageId(99),
            to: ChannelId(20),
            nonce: 7,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/messages "));
    let sent = body(&request);
    assert_eq!(
        sent["message_reference"],
        json!({"type": 1, "channel_id": "30", "message_id": "99", "guild_id": "10"})
    );
    assert!(sent.get("content").is_none());
    assert_eq!(sent["allowed_mentions"]["parse"], json!([]));
    let mut made = rest::message(120, "");
    made["nonce"] = sent["nonce"].clone();
    made["message_reference"] = json!({"type": 1, "channel_id": "30", "message_id": "99"});
    made["message_snapshots"] = json!([{"message": {"content": "original", "type": 0,
        "timestamp": "2026-10-14T12:00:00Z", "attachments": [], "embeds": []}}]);
    reply.send(response(200, &made.to_string())).unwrap();
    assert!(matches!(app.next_data().await,
        Event::MessageCreated { nonce: Some(7), message }
            if message.id == MessageId(120)
                && message.forwarded.as_ref().is_some_and(|copy| copy.content == "original")));
}

#[tokio::test]
async fn a_poll_starts_with_its_answers_and_its_author_ends_it() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::CreatePoll {
            channel: ChannelId(20),
            poll: poll(),
            nonce: 3,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/messages "));
    let sent = body(&request);
    assert_eq!(
        sent["poll"],
        json!({"question": {"text": "¿Pizza o empanadas?"},
            "answers": [{"poll_media": {"text": "Pizza", "emoji": {"name": "🍕"}}},
                {"poll_media": {"text": "Empanadas"}}],
            "duration": 24, "allow_multiselect": true, "layout_type": 1})
    );
    let mut made = rest::message(130, "");
    made["nonce"] = sent["nonce"].clone();
    made["poll"] = json!({"question": {"text": "¿Pizza o empanadas?"},
        "answers": [{"answer_id": 1, "poll_media": {"text": "Pizza", "emoji": {"name": "🍕"}}},
            {"answer_id": 2, "poll_media": {"text": "Empanadas"}}],
        "expiry": "2099-01-01T00:00:00Z", "allow_multiselect": true});
    reply.send(response(200, &made.to_string())).unwrap();
    assert!(matches!(app.next_data().await,
        Event::MessageCreated { nonce: Some(3), message }
            if message.poll.as_ref().is_some_and(|poll| poll.answers.len() == 2)));
    app.running
        .commands
        .send(Command::EndPoll {
            channel: ChannelId(20),
            message: MessageId(130),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/polls/130/expire "));
    made["poll"]["expiry"] = json!("2026-10-14T12:40:00Z");
    reply.send(response(200, &made.to_string())).unwrap();
    assert!(matches!(app.next_data().await,
        Event::MessageUpdated(message) if message.id == MessageId(130)));
    // Someone else's poll is not the account's to end: nothing leaves.
    app.running
        .commands
        .send(Command::EndPoll {
            channel: ChannelId(20),
            message: MessageId(131),
        })
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: rune_provider::Failure::EndPoll { .. },
            ..
        }
    ));
}

#[tokio::test]
async fn a_silent_reply_without_its_mention_says_so_in_the_request() {
    let mut app = Harness::new().await;
    let send = |content: &str, mention_reply| Command::SendMessage {
        channel: ChannelId(20),
        content: content.into(),
        reply_to: Some(MessageId(100)),
        nonce: 4,
        mention_reply,
    };
    app.running
        .commands
        .send(send("@silent hola", false))
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    let sent = body(&request);
    assert_eq!(sent["content"], "hola");
    assert_eq!(sent["flags"], 4096);
    assert_eq!(sent["allowed_mentions"]["replied_user"], false);
    let mut made = rest::message(140, "hola");
    made["nonce"] = sent["nonce"].clone();
    reply.send(response(200, &made.to_string())).unwrap();
    app.next_data().await;
    app.running.commands.send(send("hola", true)).await.unwrap();
    let (request, _reply) = app.pending().await;
    let sent = body(&request);
    assert_eq!(sent["content"], "hola");
    assert!(sent.get("flags").is_none());
    assert_eq!(sent["allowed_mentions"]["replied_user"], true);
}

#[tokio::test]
async fn who_reacted_and_who_voted_come_a_page_at_a_time() {
    let mut app = Harness::new().await;
    app.running
        .commands
        .send(Command::LoadReactions {
            request: 5,
            channel: ChannelId(20),
            message: MessageId(100),
            emoji: "👍".into(),
            after: None,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request
        .starts_with("GET /api/v10/channels/20/messages/100/reactions/%F0%9F%91%8D?limit=100 "));
    reply
        .send(response(
            200,
            r#"[{"id":"43","username":"Peer"},{"id":"44","username":"Otra"}]"#,
        ))
        .unwrap();
    assert!(matches!(app.next_data().await,
        Event::PeopleLoaded { request: 5, people, reached_end: true }
            if people == vec![UserId(43), UserId(44)]));
    app.running
        .commands
        .send(Command::LoadPollVoters {
            request: 6,
            channel: ChannelId(20),
            message: MessageId(100),
            answer: 2,
            after: Some(UserId(43)),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/channels/20/polls/100/answers/2?limit=100&after=43 "));
    reply
        .send(response(
            200,
            r#"{"users":[{"id":"44","username":"Otra"}]}"#,
        ))
        .unwrap();
    assert!(matches!(app.next_data().await,
        Event::PeopleLoaded { request: 6, people, .. } if people == vec![UserId(44)]));
}

#[tokio::test]
async fn a_conversation_named_by_a_link_is_read_by_its_id() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW | SEND | HISTORY).await;
    // A direct conversation off the list comes back to it.
    app.running
        .commands
        .send(Command::LoadChannel {
            request: 8,
            channel: ChannelId(21),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("GET /api/v10/channels/21 "));
    reply
        .send(response(
            200,
            r#"{"id":"21","type":1,"recipients":[{"id":"45","username":"Lejana"}]}"#,
        ))
        .unwrap();
    assert!(matches!(app.next_data().await,
        Event::ChannelChanged(channel) if channel.id == ChannelId(21) && channel.server.is_none()));
    assert!(matches!(
        app.next_data().await,
        Event::ChannelLoaded {
            request: 8,
            channel: ChannelId(21)
        }
    ));
    // One of a server is kept as its creation's dispatch would be.
    app.running
        .commands
        .send(Command::LoadChannel {
            request: 9,
            channel: ChannelId(31),
        })
        .await
        .unwrap();
    let (_, reply) = app.pending().await;
    reply
        .send(response(
            200,
            r#"{"id":"31","guild_id":"10","type":0,"name":"otro","permission_overwrites":[]}"#,
        ))
        .unwrap();
    assert!(matches!(app.next_data().await,
        Event::ChannelChanged(channel) if channel.id == ChannelId(31) && channel.server == Some(ServerId(10))));
    assert!(matches!(
        app.next_data().await,
        Event::ChannelLoaded { request: 9, .. }
    ));
    // Another id in the answer is not the one asked for.
    app.running
        .commands
        .send(Command::LoadChannel {
            request: 10,
            channel: ChannelId(32),
        })
        .await
        .unwrap();
    let (_, reply) = app.pending().await;
    reply
        .send(response(200, r#"{"id":"33","type":1,"recipients":[]}"#))
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: rune_provider::Failure::Place { request: 10 },
            ambiguous: false,
            ..
        }
    ));
}

#[tokio::test]
async fn voice_moderation_patches_the_member_within_the_permission() {
    let mut app = Harness::new().await;
    app.dispatch
        .send((
            "GUILD_CREATE",
            json!({"id":"60","name":"Synthetic","owner_id":"99",
                "roles":[{"id":"60","name":"@everyone",
                    "permissions":(VIEW | CONNECT | MUTE | MOVE).to_string()},
                    {"id":"63","name":"Moderación","position":1,"permissions":"0"},
                    {"id":"64","name":"Admin","position":2,"permissions":"0"}],
                // Peer's role is above the account's: voice moderation asks
                // only for the permission, as Discord's does.
                "members":[{"user":{"id":"42","username":"Local"},"roles":["63"]},
                    {"user":{"id":"43","username":"Peer"},"roles":["64"]}],
                "channels":[{"id":"61","type":2,"name":"Voz","permission_overwrites":[]},
                    {"id":"62","type":2,"name":"Otra","permission_overwrites":[]}],
                "voice_states":[{"user_id":"43","channel_id":"61"}]}),
        ))
        .await
        .unwrap();
    loop {
        if matches!(app.next_data().await, Event::ChannelChanged(channel) if channel.id == ChannelId(62))
        {
            break;
        }
    }
    let moderate = |change| Command::ModerateVoice {
        server: ServerId(60),
        user: UserId(43),
        change,
    };
    for (change, expected) in [
        (VoiceModeration::Mute(true), json!({"mute": true})),
        (
            VoiceModeration::Move(ChannelId(62)),
            json!({"channel_id": "62"}),
        ),
        (VoiceModeration::Disconnect, json!({"channel_id": null})),
    ] {
        app.running.commands.send(moderate(change)).await.unwrap();
        let (request, reply) = app.pending().await;
        assert!(request.starts_with("PATCH /api/v10/guilds/60/members/43 "));
        assert_eq!(body(&request), expected);
        reply.send(response(204, "")).unwrap();
    }
    // Deafening needs its own bit, which the account lacks: nothing leaves.
    app.running
        .commands
        .send(moderate(VoiceModeration::Deafen(true)))
        .await
        .unwrap();
    loop {
        if let Event::Failed { failure, .. } = app.next_data().await {
            assert_eq!(
                failure,
                rune_provider::Failure::Member {
                    server: ServerId(60),
                    user: UserId(43),
                }
            );
            break;
        }
    }
}
