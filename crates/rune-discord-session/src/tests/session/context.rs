//! Exact-anchor reads over the same bounded HTTP and Gateway session fixture.

use super::*;

fn command(request: u64, limit: usize) -> Command {
    Command::LoadContext {
        request,
        channel: ChannelId(20),
        anchor: MessageId(100),
        limit,
    }
}

async fn context_request(app: &mut Harness) -> PendingRequest {
    tokio::select! {
        request = app.requests.recv() => request.expect("around HTTP request"),
        event = app.running.next() => panic!("context needs an around read, got {event:?}"),
    }
}

async fn captured_request(captured: oneshot::Receiver<String>) -> String {
    tokio::time::timeout(Duration::from_secs(1), captured)
        .await
        .expect("the context endpoint must issue its HTTP read")
        .unwrap()
}

#[tokio::test]
async fn context_http_fetches_around_only_and_returns_the_anchor_oldest_first() {
    let data = json!([
        rest::message(101, "after"),
        rest::message(100, "anchor"),
        rest::message(99, "before")
    ]);
    let (http, captured) = fixture(response(200, &data.to_string())).await;
    let updates = http
        .execute(&secret(), &command(1, 3), "")
        .await
        .expect("around context read");
    let request = captured_request(captured).await;
    assert!(
        request.starts_with("GET /api/v10/channels/20/messages?around=100&limit=3 "),
        "{request}"
    );
    assert!(!request.contains("before=") && !request.contains("after="));
    let Event::ContextLoaded {
        request,
        channel,
        anchor,
        messages,
    } = updates.last().unwrap()
    else {
        panic!("context event")
    };
    assert_eq!(
        (*request, *channel, *anchor),
        (1, ChannelId(20), MessageId(100))
    );
    assert_eq!(
        messages
            .iter()
            .map(|message| message.id.0)
            .collect::<Vec<_>>(),
        vec![99, 100, 101]
    );
}

#[tokio::test]
async fn context_session_reads_seed_authorship_for_subsequent_edits() {
    let mut app = Harness::new().await;
    app.running.commands.send(command(2, 3)).await.unwrap();
    let (get, reply) = context_request(&mut app).await;
    assert!(get.starts_with("GET /api/v10/channels/20/messages?around=100&limit=3 "));
    reply
        .send(response(
            200,
            &json!([rest::message(100, "anchor")]).to_string(),
        ))
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::ContextLoaded {
            request: 2,
            channel: ChannelId(20),
            anchor: MessageId(100),
            ..
        }
    ));
    app.running
        .commands
        .send(Command::EditMessage {
            channel: ChannelId(20),
            message: MessageId(100),
            content: "from context".into(),
        })
        .await
        .unwrap();
    let (patch, reply) = app.pending().await;
    assert!(
        patch.starts_with("PATCH /api/v10/channels/20/messages/100 "),
        "a confirmed context supplies proof without another GET"
    );
    reply
        .send(response(
            200,
            &rest::message(100, "from context").to_string(),
        ))
        .unwrap();
    assert!(
        matches!(app.next_data().await, Event::MessageUpdated(message) if message.content == "from context")
    );
}

fn context_failure(event: Event, request: u64, channel: ChannelId) {
    assert!(
        matches!(event, Event::Failed { failure: rune_provider::Failure::Context { request: id, channel: actual }, ambiguous: false, .. } if id == request && actual == channel),
        "{event:?}"
    );
}

async fn no_context(app: &mut Harness) {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(40);
    while let Ok(Some(late)) = tokio::time::timeout_at(deadline, app.running.events.recv()).await {
        assert!(
            matches!(
                late.event,
                Event::Connection(_)
                    | Event::RememberedAccounts(_)
                    | Event::UploadLimit { .. }
                    // A new connection reads the account's settings again.
                    | Event::PresenceChanged { .. }
                    | Event::ServerFolders(_)
                    | Event::MessagingPrivacy(_)
                    | Event::ActivitySharing(_)
                    // And what its plan allows (P5).
                    | Event::Perks(_)
            ),
            "a canceled read must not publish late data or another outcome: {late:?}"
        );
    }
}

#[tokio::test]
async fn context_limits_are_clamped_to_the_documented_range() {
    for (limit, expected) in [(0, 1), (1, 1), (100, 100), (usize::MAX, 100)] {
        let (http, captured) = fixture(response(
            200,
            &json!([rest::message(100, "anchor")]).to_string(),
        ))
        .await;
        http.execute(&secret(), &command(1, limit), "")
            .await
            .unwrap();
        assert!(captured_request(captured).await.starts_with(&format!(
            "GET /api/v10/channels/20/messages?around=100&limit={expected} "
        )));
    }
}

#[tokio::test]
async fn context_missing_anchor_is_unavailable_and_never_a_successful_neighbor_page() {
    for data in [
        json!([]),
        json!([rest::message(99, "before"), rest::message(101, "after")]),
    ] {
        let (http, captured) = fixture(response(200, &data.to_string())).await;
        let error = http
            .execute(&secret(), &command(1, 3), "")
            .await
            .unwrap_err();
        assert!(!error.ambiguous && !error.ends_session);
        assert_eq!(
            error.reason.to_string(),
            "No encontramos ese mensaje en este contexto."
        );
        assert!(captured_request(captured)
            .await
            .starts_with("GET /api/v10/channels/20/messages?around=100&limit=3 "));
    }
}

#[tokio::test]
async fn context_rejects_cross_channel_duplicate_oversized_and_malformed_pages() {
    let mut cross_channel = rest::message(100, "wrong channel");
    cross_channel["channel_id"] = json!("21");
    for data in [
        json!({}),
        json!([cross_channel]),
        json!([rest::message(100, "a"), rest::message(100, "duplicate")]),
        json!([
            rest::message(99, "a"),
            rest::message(100, "anchor"),
            rest::message(101, "b"),
            rest::message(102, "extra")
        ]),
        json!([{"id":"100","channel_id":"20"}]),
    ] {
        let (http, captured) = fixture(response(200, &data.to_string())).await;
        let error = http
            .execute(&secret(), &command(1, 3), "")
            .await
            .unwrap_err();
        assert!(!error.ambiguous && !error.ends_session);
        assert!(captured_request(captured)
            .await
            .starts_with("GET /api/v10/channels/20/messages?around=100&limit=3 "));
    }
    let (http, _) = fixture(response(200, &" ".repeat(8 * 1024 * 1024 + 1))).await;
    let error = http
        .execute(&secret(), &command(1, 3), "")
        .await
        .unwrap_err();
    assert!(!error.ambiguous && !error.ends_session);
}

#[tokio::test]
async fn context_cancel_drops_the_inflight_result_and_only_matches_its_request() {
    for cancel in [9, 10] {
        let mut app = Harness::new().await;
        app.running.commands.send(command(10, 3)).await.unwrap();
        let (_, reply) = context_request(&mut app).await;
        app.running
            .commands
            .send(Command::Cancel(cancel))
            .await
            .unwrap();
        // A queued history operation proves cancellation was consumed first.
        app.running.commands.send(rest::history(11)).await.unwrap();
        reply
            .send(response(
                200,
                &json!([rest::message(100, "anchor")]).to_string(),
            ))
            .unwrap();
        let (history, reply) = app.pending().await;
        assert!(history.starts_with("GET /api/v10/channels/20/messages?limit="));
        reply.send(response(200, "[]")).unwrap();
        if cancel == 9 {
            assert!(matches!(
                app.next_data().await,
                Event::ContextLoaded { request: 10, .. }
            ));
        }
        assert!(matches!(
            app.next_data().await,
            Event::HistoryLoaded { request: 11, .. }
        ));
        no_context(&mut app).await;
    }
}

#[tokio::test]
async fn context_cancel_removes_a_queued_read_before_http_starts() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(20)).await.unwrap();
    let (_, blocker) = app.pending().await;
    app.running.commands.send(command(21, 3)).await.unwrap();
    app.running
        .commands
        .send(Command::Cancel(21))
        .await
        .unwrap();
    app.running.commands.send(rest::history(22)).await.unwrap();
    blocker.send(response(200, "[]")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::HistoryLoaded { request: 20, .. }
    ));
    let (history, reply) = app.pending().await;
    assert!(history.starts_with("GET /api/v10/channels/20/messages?limit="));
    reply.send(response(200, "[]")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::HistoryLoaded { request: 22, .. }
    ));
    no_context(&mut app).await;
}

#[tokio::test]
async fn context_anchor_deletion_cancels_queued_and_inflight_reads() {
    for queued in [false, true] {
        let mut app = Harness::new().await;
        let blocker = if queued {
            app.running.commands.send(rest::history(30)).await.unwrap();
            Some(app.pending().await.1)
        } else {
            None
        };
        app.running.commands.send(command(31, 3)).await.unwrap();
        let late = if queued {
            None
        } else {
            Some(context_request(&mut app).await.1)
        };
        app.dispatch
            .send(("MESSAGE_DELETE", json!({"id":"100","channel_id":"20"})))
            .await
            .unwrap();
        context_failure(app.next_data().await, 31, ChannelId(20));
        assert!(matches!(
            app.next_data().await,
            Event::MessageDeleted {
                message: MessageId(100),
                ..
            }
        ));
        if let Some(reply) = blocker {
            reply.send(response(200, "[]")).unwrap();
            assert!(matches!(
                app.next_data().await,
                Event::HistoryLoaded { request: 30, .. }
            ));
        }
        if let Some(reply) = late {
            reply
                .send(response(
                    200,
                    &json!([rest::message(100, "late")]).to_string(),
                ))
                .unwrap();
        }
        no_context(&mut app).await;
        assert!(app.requests.try_recv().is_err());
    }
}

#[tokio::test]
async fn context_neighbors_changed_during_queue_or_http_cannot_repopulate_old_baselines() {
    for queued in [false, true] {
        for kind in ["MESSAGE_DELETE", "MESSAGE_UPDATE", "MESSAGE_CREATE"] {
            let mut app = Harness::new().await;
            let blocker = if queued {
                app.running.commands.send(rest::history(32)).await.unwrap();
                Some(app.pending().await.1)
            } else {
                None
            };
            app.running.commands.send(command(33, 3)).await.unwrap();
            let late = if queued {
                None
            } else {
                Some(context_request(&mut app).await.1)
            };
            let changed = match kind {
                "MESSAGE_DELETE" => json!({"id":"99","channel_id":"20"}),
                "MESSAGE_UPDATE" => json!({"id":"99","channel_id":"20","content":"current"}),
                _ => rest::message(99, "current"),
            };
            app.dispatch.send((kind, changed)).await.unwrap();
            context_failure(app.next_data().await, 33, ChannelId(20));
            assert!(matches!(
                app.next_data().await,
                Event::MessageDeleted { .. }
                    | Event::MessagePatched(_)
                    | Event::MessageCreated { .. }
            ));
            if let Some(reply) = blocker {
                reply.send(response(200, "[]")).unwrap();
                assert!(matches!(
                    app.next_data().await,
                    Event::HistoryLoaded { request: 32, .. }
                ));
            }
            if let Some(reply) = late {
                reply
                    .send(response(
                        200,
                        &json!([
                            rest::message(99, "old neighbor"),
                            rest::message(100, "anchor")
                        ])
                        .to_string(),
                    ))
                    .unwrap();
            }
            no_context(&mut app).await;
            assert!(app.requests.try_recv().is_err());
            if kind == "MESSAGE_DELETE" {
                app.running
                    .commands
                    .send(Command::EditMessage {
                        channel: ChannelId(20),
                        message: MessageId(99),
                        content: "attempt".into(),
                    })
                    .await
                    .unwrap();
                let (read, reply) = app.pending().await;
                assert!(
                    read.starts_with("GET /api/v10/channels/20/messages/99 "),
                    "a deleted neighbor must not regain old write proof: {read}"
                );
                reply.send(response(404, "{}")).unwrap();
                assert!(matches!(
                    app.next_data().await,
                    Event::Failed {
                        failure: rune_provider::Failure::Edit {
                            message: MessageId(99),
                            ..
                        },
                        ambiguous: false,
                        ..
                    }
                ));
            }
        }
    }
}

#[tokio::test]
async fn context_channel_removal_and_reconnection_are_definite_failures() {
    for (kind, data) in [
        ("CHANNEL_DELETE", json!({"id":"20","type":1})),
        ("TEST_DISCONNECT", Value::Null),
    ] {
        let mut app = Harness::new().await;
        app.running.commands.send(command(40, 3)).await.unwrap();
        let (_, reply) = context_request(&mut app).await;
        app.dispatch.send((kind, data)).await.unwrap();
        context_failure(app.next_data().await, 40, ChannelId(20));
        assert!(matches!(
            app.next_data().await,
            Event::ChannelRemoved { .. }
                | Event::Connection(
                    ConnectionState::Reconnecting { .. } | ConnectionState::Disconnected { .. }
                )
        ));
        reply
            .send(response(
                200,
                &json!([rest::message(100, "late")]).to_string(),
            ))
            .unwrap();
        no_context(&mut app).await;
    }
}

#[tokio::test]
async fn context_queued_behind_history_is_failed_when_the_connection_is_lost() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(41)).await.unwrap();
    let (_, blocker) = app.pending().await;
    app.running.commands.send(command(42, 3)).await.unwrap();
    app.dispatch
        .send(("TEST_DISCONNECT", Value::Null))
        .await
        .unwrap();
    context_failure(app.next_data().await, 42, ChannelId(20));
    assert!(matches!(
        app.next_data().await,
        Event::Connection(ConnectionState::Reconnecting { .. })
    ));
    // Cancel the unrelated blocker while offline. A command rejected in that
    // state proves the cancel was processed before the old HTTP reply arrives.
    app.running
        .commands
        .send(Command::Cancel(41))
        .await
        .unwrap();
    app.running.commands.send(rest::history(43)).await.unwrap();
    loop {
        match app.next_data().await {
            Event::Connection(_) => continue,
            Event::Failed {
                failure: rune_provider::Failure::History { request: 43, .. },
                ambiguous: false,
                ..
            } => break,
            event => panic!("offline history rejection: {event:?}"),
        }
    }
    blocker.send(response(200, "[]")).unwrap();
    no_context(&mut app).await;
    assert!(app.requests.try_recv().is_err());
}

#[tokio::test]
async fn context_revoked_history_access_cannot_publish_a_late_page() {
    let mut app = Harness::new().await;
    super::message_recovery::guild_channel(&mut app, (1 << 10) | (1 << 11) | (1 << 16)).await;
    app.running
        .commands
        .send(Command::LoadContext {
            request: 50,
            channel: ChannelId(30),
            anchor: MessageId(100),
            limit: 3,
        })
        .await
        .unwrap();
    let (_, reply) = context_request(&mut app).await;
    app.dispatch
        .send((
            "CHANNEL_UPDATE",
            json!({"id":"30","guild_id":"10","type":0,"name":"fixture",
        "permission_overwrites":[{"id":"42","type":1,"allow":"0","deny":"65536"}]}),
        ))
        .await
        .unwrap();
    context_failure(app.next_data().await, 50, ChannelId(30));
    assert!(
        matches!(app.next_data().await, Event::ChannelChanged(channel) if channel.id == ChannelId(30))
    );
    let mut message = rest::message(100, "late");
    message["channel_id"] = json!("30");
    reply
        .send(response(200, &json!([message]).to_string()))
        .unwrap();
    no_context(&mut app).await;
    app.running
        .commands
        .send(Command::LoadContext {
            request: 51,
            channel: ChannelId(30),
            anchor: MessageId(100),
            limit: 3,
        })
        .await
        .unwrap();
    context_failure(app.next_data().await, 51, ChannelId(30));
    assert!(app.requests.try_recv().is_err());
}

#[tokio::test]
async fn context_queued_behind_another_channel_fails_on_removal_or_revocation() {
    for removed in [false, true] {
        let mut app = Harness::new().await;
        super::message_recovery::guild_channel(&mut app, (1 << 10) | (1 << 11) | (1 << 16)).await;
        app.running.commands.send(rest::history(55)).await.unwrap();
        let (_, blocker) = app.pending().await;
        app.running
            .commands
            .send(Command::LoadContext {
                request: 56,
                channel: ChannelId(30),
                anchor: MessageId(100),
                limit: 3,
            })
            .await
            .unwrap();
        let data = if removed {
            json!({"id":"30","guild_id":"10","type":0})
        } else {
            json!({"id":"30","guild_id":"10","type":0,"name":"fixture",
                "permission_overwrites":[{"id":"42","type":1,"allow":"0","deny":"65536"}]})
        };
        app.dispatch
            .send((
                if removed {
                    "CHANNEL_DELETE"
                } else {
                    "CHANNEL_UPDATE"
                },
                data,
            ))
            .await
            .unwrap();
        context_failure(app.next_data().await, 56, ChannelId(30));
        assert!(matches!(
            app.next_data().await,
            Event::ChannelChanged(_) | Event::ChannelRemoved { .. }
        ));
        app.running
            .commands
            .send(Command::Cancel(55))
            .await
            .unwrap();
        app.running.commands.send(rest::history(57)).await.unwrap();
        blocker.send(response(200, "[]")).unwrap();
        let (history, reply) = app.pending().await;
        assert!(history.starts_with("GET /api/v10/channels/20/messages?limit="));
        reply.send(response(200, "[]")).unwrap();
        assert!(matches!(
            app.next_data().await,
            Event::HistoryLoaded { request: 57, .. }
        ));
        no_context(&mut app).await;
        assert!(app.requests.try_recv().is_err());
    }
}

#[tokio::test]
async fn context_sign_out_and_http_401_forget_the_read() {
    for status in [None, Some(401)] {
        let mut app = Harness::new().await;
        app.running.commands.send(command(60, 3)).await.unwrap();
        let (_, reply) = context_request(&mut app).await;
        if let Some(status) = status {
            reply.send(response(status, "{}")).unwrap();
            context_failure(app.next_data().await, 60, ChannelId(20));
            assert!(matches!(
                app.next_data().await,
                Event::Authentication(AuthenticationState::Failed { .. })
            ));
        } else {
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
                .send(response(
                    200,
                    &json!([rest::message(100, "late")]).to_string(),
                ))
                .unwrap();
        }
        no_context(&mut app).await;
    }
}

#[tokio::test]
async fn context_http_timeout_is_a_definite_correlated_failure() {
    let mut app = Harness::new().await;
    app.running.commands.send(command(70, 3)).await.unwrap();
    let (_, _unanswered) = context_request(&mut app).await;
    tokio::time::pause();
    tokio::time::advance(Duration::from_secs(16)).await;
    context_failure(app.next_data().await, 70, ChannelId(20));
}

#[tokio::test]
async fn context_ready_replacement_cancels_the_earlier_connection_read() {
    let mut app = Harness::new().await;
    app.running.commands.send(command(80, 3)).await.unwrap();
    let (_, reply) = context_request(&mut app).await;
    app.dispatch.send(("READY", json!({"user":{"id":"42"}, "session_id":"replacement",
        "resume_gateway_url":"wss://gateway.discord.gg", "guilds":[],
        "private_channels":[{"id":"20","type":1,"recipients":[{"id":"43","username":"Peer"}]}]
    }))).await.unwrap();
    context_failure(app.next_data().await, 80, ChannelId(20));
    assert!(matches!(app.next_data().await, Event::Ready(_)));
    assert_eq!(
        app.next_data().await,
        Event::Connection(ConnectionState::Connected)
    );
    reply
        .send(response(
            200,
            &json!([rest::message(100, "late")]).to_string(),
        ))
        .unwrap();
    no_context(&mut app).await;
}

#[tokio::test]
async fn context_uses_the_existing_sixteen_entry_queue_and_reports_rejection() {
    let mut app = Harness::new().await;
    app.running.commands.send(rest::history(90)).await.unwrap();
    let (_, blocker) = app.pending().await;
    for request in 100..=116 {
        app.running
            .commands
            .send(command(request, 3))
            .await
            .unwrap();
    }
    context_failure(app.next_data().await, 116, ChannelId(20));
    for request in 100..116 {
        app.running
            .commands
            .send(Command::Cancel(request))
            .await
            .unwrap();
    }
    // Sending a fresh history read is a command-order barrier for all cancels.
    app.running.commands.send(rest::history(117)).await.unwrap();
    blocker.send(response(200, "[]")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::HistoryLoaded { request: 90, .. }
    ));
    let (history, reply) = app.pending().await;
    assert!(history.starts_with("GET /api/v10/channels/20/messages?limit="));
    reply.send(response(200, "[]")).unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::HistoryLoaded { request: 117, .. }
    ));
    no_context(&mut app).await;
}
