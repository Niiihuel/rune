//! Acknowledgements, read state from other devices, passive updates, remote
//! search and member lookups, against local fixtures only. None of this
//! reached Discord.

use super::message_recovery::guild_channel;
use super::*;
use rune_model::{RemoteSearch, RemoteSearchError, ServerId, UserId};

const VIEW: u64 = 1 << 10;
const HISTORY: u64 = 1 << 16;

fn read(request: u64, message: u64) -> Command {
    Command::MarkRead {
        request,
        account: UserId(42),
        channel: ChannelId(20),
        message: MessageId(message),
    }
}

fn body(request: &str) -> Value {
    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[tokio::test]
async fn acks_go_one_at_a_time_the_newest_per_channel_with_the_last_token() {
    let mut app = Harness::new().await;
    app.running.commands.send(read(1, 100)).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/messages/100/ack "));
    assert_eq!(body(&request), json!({"token": null, "manual": false}));
    // Two more while the first is in flight: only the newest waits.
    app.running.commands.send(read(2, 101)).await.unwrap();
    app.running.commands.send(read(3, 102)).await.unwrap();
    reply
        .send(response(200, r#"{"token":"SYNTHETIC_ACK_TOKEN"}"#))
        .unwrap();
    assert!(matches!(app.next_data().await, Event::ReadStateConfirmed {
        request: Some(1), message: MessageId(100), channel: ChannelId(20), version, ..
    } if version > 0));
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/messages/102/ack "));
    assert_eq!(
        body(&request),
        json!({"token": "SYNTHETIC_ACK_TOKEN", "manual": false})
    );
    reply.send(response(200, r#"{"token":null}"#)).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::ReadStateConfirmed {
            request: Some(3),
            message: MessageId(102),
            ..
        }
    ));
}

#[tokio::test]
async fn other_devices_reads_and_reminders_arrive_and_a_reminder_goes_out() {
    let mut app = Harness::new().await;
    app.dispatch
        .send((
            "MESSAGE_ACK",
            json!({"channel_id":"20","message_id":"105","version":3}),
        ))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::ReadStateConfirmed {
            request: None,
            message: MessageId(105),
            ..
        }
    ));
    // An older version is late: dropped.
    app.dispatch
        .send((
            "MESSAGE_ACK",
            json!({"channel_id":"20","message_id":"106","version":2}),
        ))
        .await
        .unwrap();
    app.dispatch
        .send((
            "MESSAGE_ACK",
            json!({"channel_id":"20","message_id":"103","manual":true,"mention_count":1,"version":4}),
        ))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::MarkedUnread {
            through: Some(MessageId(103)),
            anchor: None,
            mentions: 1,
            ..
        }
    ));
    app.running
        .commands
        .send(Command::MarkUnread {
            account: UserId(42),
            channel: ChannelId(20),
            message: MessageId(110),
            mentions: 2,
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/channels/20/messages/109/ack "));
    assert_eq!(body(&request), json!({"manual": true, "mention_count": 2}));
    reply.send(response(200, r#"{"token":null}"#)).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::MarkedUnread {
            through: Some(MessageId(109)),
            anchor: Some(MessageId(110)),
            mentions: 2,
            ..
        }
    ));
}

#[tokio::test]
async fn passive_updates_and_a_whole_server_read() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW | HISTORY).await;
    app.dispatch
        .send((
            "PASSIVE_UPDATE_V1",
            json!({"guild_id":"10","channels":[{"id":"30","last_message_id":"77"}],
                "voice_states":[{"user_id":"43","channel_id":"30"}]}),
        ))
        .await
        .unwrap();
    // The voice state, through the calls, then what is new to read.
    loop {
        match app.next_data().await {
            Event::VoiceChanged(_) => continue,
            Event::LatestMessages(latest) => {
                assert_eq!(latest, vec![(ChannelId(30), MessageId(77))]);
                break;
            }
            other => panic!("unexpected {other:?}"),
        }
    }
    app.running
        .commands
        .send(Command::MarkServerRead {
            account: UserId(42),
            server: ServerId(10),
        })
        .await
        .unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with("POST /api/v10/guilds/10/ack "));
    reply.send(response(204, "")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::ReadStateConfirmed {
            request: None,
            channel: ChannelId(30),
            message: MessageId(77),
            ..
        }
    ));
}

#[tokio::test]
async fn a_search_waits_for_its_index_then_pages_with_its_authors() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW | HISTORY).await;
    let search = Command::SearchMessages {
        request: 9,
        search: RemoteSearch {
            channel: ChannelId(30),
            server: Some(ServerId(10)),
            text: "hola".into(),
            ..Default::default()
        },
        offset: 0,
    };
    app.running.commands.send(search.clone()).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(request.starts_with(
        "GET /api/v10/guilds/10/messages/search?limit=25&sort_by=timestamp&sort_order=desc&content=%68%6F%6C%61 "
    ));
    reply
        .send(response(202, r#"{"code":110000,"retry_after":2}"#))
        .unwrap();
    assert_eq!(
        app.next_data().await,
        Event::SearchResults {
            request: 9,
            result: Err(RemoteSearchError::Indexing {
                retry_after_ms: 2_000
            })
        }
    );
    app.running.commands.send(search).await.unwrap();
    let (_, reply) = app.pending().await;
    let mut hit = rest::message(90, "hola a todos");
    hit["channel_id"] = json!("30");
    hit["hit"] = json!(true);
    reply
        .send(response(
            200,
            &json!({"total_results": 1, "messages": [[hit]]}).to_string(),
        ))
        .unwrap();
    assert!(matches!(app.running.next().await, Event::UserChanged(_)));
    let Event::SearchResults {
        request: 9,
        result: Ok(page),
    } = app.running.next().await
    else {
        panic!("a page");
    };
    assert_eq!(page.total, 1);
    assert_eq!(page.messages[0].id, MessageId(90));
}

#[tokio::test]
async fn members_are_looked_up_by_name_over_the_gateway() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, VIEW | HISTORY).await;
    app.running
        .commands
        .send(Command::SearchMembers {
            server: ServerId(10),
            query: "Pa".into(),
        })
        .await
        .unwrap();
    let frame = app.frame().await;
    assert_eq!(frame["op"], 8);
    assert_eq!(frame["d"]["guild_id"], "10");
    assert_eq!(frame["d"]["query"], "pa");
    assert_eq!(frame["d"]["limit"], 100);
    let nonce = frame["d"]["nonce"].as_str().unwrap().to_owned();
    app.dispatch
        .send((
            "GUILD_MEMBERS_CHUNK",
            json!({"guild_id":"10","nonce":nonce,"chunk_index":0,"chunk_count":1,
                "members":[{"user":{"id":"77","username":"pablo"},"nick":"Pablito","roles":[]}]}),
        ))
        .await
        .unwrap();
    assert!(matches!(app.running.next().await, Event::UserChanged(user) if user.id == UserId(77)));
    assert!(matches!(app.running.next().await,
        Event::MembersFound { server: ServerId(10), query, members }
            if query == "pa" && members[0].nick.as_deref() == Some("Pablito")));
}
