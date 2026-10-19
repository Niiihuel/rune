//! Message actions recover evicted proof without trusting the interface's copy.

use super::*;

async fn evict_original(app: &mut Harness) -> rune_model::Message {
    app.running
        .commands
        .send(Command::LoadHistory {
            request: 40,
            channel: ChannelId(20),
            before: None,
            limit: 1,
        })
        .await
        .unwrap();
    let (_, reply) = app.pending().await;
    reply
        .send(response(
            200,
            &json!([rest::message(100, "still visible")]).to_string(),
        ))
        .unwrap();
    let Event::HistoryLoaded { messages, .. } = app.next_data().await else {
        panic!("history")
    };
    let retained = messages.into_iter().next().unwrap();
    evict_cache(app).await;
    retained
}

async fn evict_cache(app: &mut Harness) {
    for id in 1000..=1512 {
        app.dispatch
            .send(("MESSAGE_CREATE", rest::message(id, "later traffic")))
            .await
            .unwrap();
        assert!(
            matches!(app.next_data().await, Event::MessageCreated { message, .. } if message.id == MessageId(id))
        );
    }
}

async fn recovery_request(app: &mut Harness) -> PendingRequest {
    tokio::select! {
        request = app.requests.recv() => request.expect("message GET request"),
        event = app.running.next() => panic!("an evicted own message needs GET preflight, got {event:?}"),
    }
}

#[tokio::test]
async fn an_own_message_can_be_edited_after_more_than_512_cache_evictions() {
    let mut app = Harness::new().await;
    let retained = evict_original(&mut app).await;
    app.running
        .commands
        .send(Command::EditMessage {
            channel: retained.channel,
            message: retained.id,
            content: "edited after eviction".into(),
        })
        .await
        .unwrap();
    let (get, reply) = recovery_request(&mut app).await;
    assert!(get.starts_with("GET /api/v10/channels/20/messages/100 "));
    reply
        .send(response(
            200,
            &rest::message(100, "still visible").to_string(),
        ))
        .unwrap();
    let (patch, reply) = app.pending().await;
    assert!(patch.starts_with("PATCH /api/v10/channels/20/messages/100 "));
    evict_cache(&mut app).await; // The running write's proof must survive another LRU eviction.
    reply
        .send(response(
            200,
            &rest::message(100, "edited after eviction").to_string(),
        ))
        .unwrap();
    assert!(
        matches!(app.next_data().await, Event::MessageUpdated(message) if message.id == retained.id && message.author == retained.author && message.content == "edited after eviction")
    );
}

#[tokio::test]
async fn an_own_message_can_be_deleted_after_more_than_512_cache_evictions() {
    let mut app = Harness::new().await;
    let retained = evict_original(&mut app).await;
    app.running
        .commands
        .send(Command::DeleteMessage {
            channel: retained.channel,
            message: retained.id,
        })
        .await
        .unwrap();
    let (get, reply) = recovery_request(&mut app).await;
    assert!(get.starts_with("GET /api/v10/channels/20/messages/100 "));
    reply
        .send(response(
            200,
            &rest::message(100, "still visible").to_string(),
        ))
        .unwrap();
    let (delete, reply) = app.pending().await;
    assert!(delete.starts_with("DELETE /api/v10/channels/20/messages/100 "));
    reply.send(response(204, "")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::MessageDeleted {
            channel: ChannelId(20),
            message: MessageId(100)
        }
    ));
}

async fn missing_edit(app: &mut Harness, channel: ChannelId) -> oneshot::Sender<String> {
    app.running
        .commands
        .send(Command::EditMessage {
            channel,
            message: MessageId(100),
            content: "requested edit".into(),
        })
        .await
        .unwrap();
    let (get, reply) = recovery_request(app).await;
    assert!(get.starts_with(&format!("GET /api/v10/channels/{channel}/messages/100 ")));
    reply
}

fn edit_failure(event: Event, channel: ChannelId) {
    assert!(
        matches!(event, Event::Failed { failure: rune_provider::Failure::Edit { channel: actual, message: MessageId(100) }, ambiguous: false, .. } if actual == channel),
        "{event:?}"
    );
}

async fn no_write(app: &mut Harness) {
    assert!(
        tokio::time::timeout(Duration::from_millis(40), app.requests.recv())
            .await
            .is_err(),
        "a rejected recovery must never issue a write"
    );
}

#[tokio::test]
async fn message_preflight_rejects_foreign_authorship_and_wrong_scope_without_write() {
    for (field, value) in [
        ("author", json!({"id":"43","username":"Peer"})),
        ("id", json!("101")),
        ("channel_id", json!("21")),
    ] {
        let mut app = Harness::new().await;
        let reply = missing_edit(&mut app, ChannelId(20)).await;
        let mut data = rest::message(100, "baseline");
        data[field] = value;
        reply.send(response(200, &data.to_string())).unwrap();
        edit_failure(app.next_data().await, ChannelId(20));
        no_write(&mut app).await;
    }
}

#[tokio::test]
async fn message_preflight_errors_are_definite_and_only_401_ends_the_session() {
    for status in [401, 403, 404, 500] {
        let mut app = Harness::new().await;
        let reply = missing_edit(&mut app, ChannelId(20)).await;
        reply.send(response(status, "{}")).unwrap();
        edit_failure(app.next_data().await, ChannelId(20));
        if status == 401 {
            assert!(matches!(
                app.next_data().await,
                Event::Authentication(AuthenticationState::Failed { .. })
            ));
            tokio::time::timeout(Duration::from_secs(1), app.closed.take().unwrap())
                .await
                .unwrap()
                .unwrap();
            no_write(&mut app).await;
        } else {
            // A normal read is a barrier proving the failed GET did not start a write.
            app.running.commands.send(rest::history(50)).await.unwrap();
            let (get, reply) = app.pending().await;
            assert!(get.starts_with("GET /api/v10/channels/20/messages?"));
            reply.send(response(200, "[]")).unwrap();
            assert!(matches!(
                app.next_data().await,
                Event::HistoryLoaded { request: 50, .. }
            ));
        }
    }
}

#[tokio::test]
async fn deleting_the_message_or_channel_during_preflight_prevents_the_write() {
    for (kind, data) in [
        ("MESSAGE_DELETE", json!({"id":"100","channel_id":"20"})),
        ("CHANNEL_DELETE", json!({"id":"20","type":1})),
    ] {
        let mut app = Harness::new().await;
        let reply = missing_edit(&mut app, ChannelId(20)).await;
        app.dispatch.send((kind, data)).await.unwrap();
        edit_failure(app.next_data().await, ChannelId(20));
        assert!(matches!(
            app.next_data().await,
            Event::MessageDeleted {
                message: MessageId(100),
                ..
            } | Event::ChannelRemoved {
                channel: ChannelId(20)
            }
        ));
        reply
            .send(response(
                200,
                &rest::message(100, "late baseline").to_string(),
            ))
            .unwrap();
        no_write(&mut app).await;
    }
}

#[tokio::test]
async fn sign_out_during_message_preflight_never_starts_the_write() {
    let mut app = Harness::new().await;
    let reply = missing_edit(&mut app, ChannelId(20)).await;
    app.running.commands.send(Command::SignOut).await.unwrap();
    assert_eq!(
        app.next_data().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
    reply
        .send(response(
            200,
            &rest::message(100, "late baseline").to_string(),
        ))
        .unwrap();
    no_write(&mut app).await;
}

#[tokio::test]
async fn reconnecting_during_message_preflight_fails_definitely_without_write() {
    let mut app = Harness::new().await;
    let reply = missing_edit(&mut app, ChannelId(20)).await;
    app.dispatch
        .send(("TEST_DISCONNECT", Value::Null))
        .await
        .unwrap();
    edit_failure(app.next_data().await, ChannelId(20));
    assert!(matches!(
        app.next_data().await,
        Event::Connection(
            ConnectionState::Reconnecting { .. } | ConnectionState::Disconnected { .. }
        )
    ));
    reply
        .send(response(
            200,
            &rest::message(100, "late baseline").to_string(),
        ))
        .unwrap();
    no_write(&mut app).await;
}

#[tokio::test]
async fn a_queued_edit_for_a_deleted_target_never_launches_its_preflight_get() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(70)).await.unwrap();
    let (_, blocker) = app.pending().await;
    app.running
        .commands
        .send(Command::EditMessage {
            channel: ChannelId(20),
            message: MessageId(100),
            content: "queued".into(),
        })
        .await
        .unwrap();
    app.dispatch
        .send(("MESSAGE_DELETE", json!({"id":"100","channel_id":"20"})))
        .await
        .unwrap();
    edit_failure(app.next_data().await, ChannelId(20));
    assert!(matches!(
        app.next_data().await,
        Event::MessageDeleted { .. }
    ));
    blocker.send(response(200, "[]")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::HistoryLoaded { request: 70, .. }
    ));
    no_write(&mut app).await;
}

#[tokio::test]
async fn a_gateway_patch_for_an_evicted_message_needs_no_get_or_reconstruction() {
    let mut app = Harness::new().await;
    let mut retained = evict_original(&mut app).await;
    app.dispatch
        .send((
            "MESSAGE_UPDATE",
            json!({"id":"100","channel_id":"20","content":"live patch"}),
        ))
        .await
        .unwrap();
    let Event::MessagePatched(patch) = app.next_data().await else {
        panic!("typed patch")
    };
    assert!(patch.apply(&mut retained));
    assert_eq!(retained.content, "live patch");
    assert_eq!(retained.author, UserId(42));
    assert!(app.requests.try_recv().is_err());
}

pub(super) async fn guild_channel(app: &mut Harness, permissions: u64) {
    app.dispatch.send(("GUILD_CREATE", json!({"id":"10","name":"Fixture","owner_id":"43",
        "roles":[{"id":"10","name":"@everyone","permissions":permissions.to_string()}],
        "members":[{"user":{"id":"42","username":"Owner"},"roles":[]}],
        "channels":[{"id":"30","guild_id":"10","type":0,"name":"fixture","permission_overwrites":[]}]
    }))).await.unwrap();
    loop {
        if matches!(app.next_data().await, Event::ChannelChanged(channel) if channel.id == ChannelId(30))
        {
            break;
        }
    }
}

#[tokio::test]
async fn revoking_read_history_during_message_preflight_prevents_the_write() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, (1 << 10) | (1 << 11) | (1 << 16)).await;
    let reply = missing_edit(&mut app, ChannelId(30)).await;
    app.dispatch
        .send((
            "CHANNEL_UPDATE",
            json!({"id":"30","guild_id":"10","type":0,"name":"fixture",
        "permission_overwrites":[{"id":"42","type":1,"allow":"0","deny":"65536"}]}),
        ))
        .await
        .unwrap();
    edit_failure(app.next_data().await, ChannelId(30));
    assert!(
        matches!(app.next_data().await, Event::ChannelChanged(channel) if channel.id == ChannelId(30) && !channel.permissions.contains(rune_model::Permissions::READ_HISTORY))
    );
    let mut data = rest::message(100, "late baseline");
    data["channel_id"] = json!("30");
    reply.send(response(200, &data.to_string())).unwrap();
    no_write(&mut app).await;
}

#[tokio::test]
async fn a_moderator_can_delete_an_unknown_message_without_requiring_an_extra_read() {
    let mut app = Harness::new().await;
    guild_channel(&mut app, (1 << 10) | (1 << 13)).await;
    app.running
        .commands
        .send(Command::DeleteMessage {
            channel: ChannelId(30),
            message: MessageId(100),
        })
        .await
        .unwrap();
    let (delete, reply) = recovery_request(&mut app).await;
    assert!(delete.starts_with("DELETE /api/v10/channels/30/messages/100 "));
    reply.send(response(204, "")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::MessageDeleted {
            channel: ChannelId(30),
            message: MessageId(100)
        }
    ));
}
