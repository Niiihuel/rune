//! Joining and leaving a server voice channel through the whole provider,
//! against the local Gateway fixture. The voice server itself is covered by
//! `voice::tests`.

use super::*;
use rune_provider::Failure;

impl Harness {
    /// Events until one matches, skipping the rest.
    async fn until(&mut self, wanted: impl Fn(&Event) -> bool) -> Event {
        loop {
            let event = self.next_data().await;
            if wanted(&event) {
                return event;
            }
        }
    }
}

#[tokio::test]
async fn join_reports_the_state_the_gateway_answers_and_leave_and_sign_out_release_the_call() {
    let mut app = Harness::new().await;
    app.dispatch
        .send((
            "GUILD_CREATE",
            json!({"id":"60","name":"Synthetic",
                "roles":[{"id":"60","name":"@everyone","permissions":"3146752"}],
                "members":[{"user":{"id":"42","username":"Local"},"roles":[]}],
                "channels":[{"id":"61","type":2,"name":"Voz","permission_overwrites":[]}],
                "voice_states":[{"user_id":"43","channel_id":"61","self_deaf":true}]}),
        ))
        .await
        .unwrap();
    app.until(
        |event| matches!(event, Event::ChannelChanged(channel) if channel.id == ChannelId(61)),
    )
    .await;
    let join = Command::JoinVoice {
        channel: ChannelId(61),
        muted: false,
        deafened: false,
    };
    app.running.commands.send(join).await.unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::CallPhase {
            channel: ChannelId(61),
            phase: rune_model::CallPhase::Joining,
            ..
        }
    ));
    let update = app.frame().await;
    assert_eq!(update["op"], 4);
    assert_eq!(update["d"]["guild_id"], "60");
    assert_eq!(update["d"]["channel_id"], "61");
    // Then when the server's voice channels went live (op 43, voz-protocolo).
    assert_eq!(
        app.frame().await,
        json!({"op": 43, "d": {"guild_id": "60", "fields": ["voice_start_time"]}})
    );
    // The service puts the account in the channel.
    app.dispatch
        .send((
            "VOICE_STATE_UPDATE",
            json!({"guild_id":"60","channel_id":"61","user_id":"42","session_id":"SYNTHETIC_VOICE_SESSION"}),
        ))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::VoiceChanged(state) if state.user == UserId(42) && state.channel == ChannelId(61)
    ));
    // Muting reaches the service as the same voice state, muted.
    app.running
        .commands
        .send(Command::SetVoiceSelf {
            muted: true,
            deafened: false,
        })
        .await
        .unwrap();
    let update = app.frame().await;
    assert_eq!(
        (update["op"].clone(), update["d"]["self_mute"].clone()),
        (json!(4), json!(true))
    );
    app.running
        .commands
        .send(Command::LeaveVoice)
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::VoiceLeft {
            user: UserId(42),
            ..
        }
    ));
    assert!(matches!(
        app.next_data().await,
        Event::CallEnded { reason: None, .. }
    ));
    assert_eq!(app.frame().await["d"]["channel_id"], Value::Null);
    // A private conversation's call (2026-10-17): asked about first (op
    // 13), joined without a guild (op 4), rung once the service has it, by
    // the common REST path, and left as any call.
    app.running
        .commands
        .send(Command::JoinVoice {
            channel: ChannelId(20),
            muted: false,
            deafened: false,
        })
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::CallPhase {
            channel: ChannelId(20),
            phase: rune_model::CallPhase::Joining,
            ..
        }
    ));
    assert_eq!(
        app.frame().await,
        json!({"op": 13, "d": {"channel_id": "20"}})
    );
    let update = app.frame().await;
    assert_eq!(
        (
            &update["op"],
            &update["d"]["guild_id"],
            &update["d"]["channel_id"]
        ),
        (&json!(4), &Value::Null, &json!("20"))
    );
    app.running
        .commands
        .send(Command::RingCall {
            channel: ChannelId(20),
            users: Vec::new(),
        })
        .await
        .unwrap();
    app.dispatch
        .send((
            "CALL_CREATE",
            json!({"channel_id": "20", "ringing": [], "voice_states": []}),
        ))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::PrivateCall(call) if call.channel == ChannelId(20) && call.ringing.is_empty()
    ));
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("POST /api/v10/channels/20/call/ring ")
            && request.ends_with(r#"{"recipients":null}"#),
        "{request}"
    );
    reply.send(response(204, "")).unwrap();
    app.dispatch
        .send((
            "CALL_UPDATE",
            json!({"channel_id": "20", "ongoing_rings": {"43": "42"}}),
        ))
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::PrivateCall(call) if call.ringing == [UserId(43)]
    ));
    app.running
        .commands
        .send(Command::LeaveVoice)
        .await
        .unwrap();
    app.until(|event| matches!(event, Event::CallEnded { .. }))
        .await;
    let left = app.frame().await;
    assert_eq!(
        (&left["d"]["guild_id"], &left["d"]["channel_id"]),
        (&Value::Null, &Value::Null)
    );
    // Signing out in a call ends it before the session goes.
    app.running
        .commands
        .send(Command::JoinVoice {
            channel: ChannelId(61),
            muted: false,
            deafened: false,
        })
        .await
        .unwrap();
    app.until(|event| matches!(event, Event::CallPhase { .. }))
        .await;
    app.running.commands.send(Command::SignOut).await.unwrap();
    assert!(matches!(app.next_data().await, Event::VoiceLeft { .. }));
    assert!(matches!(app.next_data().await, Event::CallEnded { .. }));
    assert_eq!(
        app.next_data().await,
        Event::Authentication(AuthenticationState::SignedOut)
    );
}

#[tokio::test]
async fn a_voice_channel_s_and_a_stage_s_chat_are_read_and_received_like_a_text_channel_s() {
    let mut app = Harness::new().await;
    // View, history, send, connect and speak.
    app.dispatch
        .send((
            "GUILD_CREATE",
            json!({"id":"60","name":"Synthetic",
                "roles":[{"id":"60","name":"@everyone","permissions":"3214336"}],
                "members":[{"user":{"id":"42","username":"Local"},"roles":[]}],
                "channels":[{"id":"61","type":2,"name":"Voz","permission_overwrites":[]},
                    {"id":"62","type":13,"name":"Escenario","permission_overwrites":[]}]}),
        ))
        .await
        .unwrap();
    app.until(
        |event| matches!(event, Event::ChannelChanged(channel) if channel.id == ChannelId(62)),
    )
    .await;
    // A stage's chat is a voice channel's (2026-10-18).
    for (request, channel) in [(1, 61), (3, 62)] {
        app.running
            .commands
            .send(Command::LoadHistory {
                request,
                channel: ChannelId(channel),
                before: None,
                limit: 50,
            })
            .await
            .unwrap();
        let (line, reply) = app.pending().await;
        assert!(line.starts_with(&format!(
            "GET /api/v10/channels/{channel}/messages?limit=50 "
        )));
        let mut old = rest::message(100 + channel, "en la voz");
        old["channel_id"] = json!(channel.to_string());
        reply
            .send(response(200, &json!([old]).to_string()))
            .unwrap();
        assert!(matches!(
            app.next_data().await,
            Event::HistoryLoaded { request: r, messages, .. } if r == request && messages.len() == 1
        ));
        let mut live = rest::message(200 + channel, "hola");
        live["channel_id"] = json!(channel.to_string());
        app.dispatch.send(("MESSAGE_CREATE", live)).await.unwrap();
        assert!(matches!(
            app.until(|event| matches!(event, Event::MessageCreated { .. })).await,
            Event::MessageCreated { message, .. } if message.channel == ChannelId(channel)
        ));
        app.running
            .commands
            .send(Command::SendMessage {
                channel: ChannelId(channel),
                content: "hola".into(),
                reply_to: None,
                nonce: request + 1,
                mention_reply: false,
            })
            .await
            .unwrap();
        let (line, reply) = app.pending().await;
        assert!(line.starts_with(&format!("POST /api/v10/channels/{channel}/messages ")));
        let body: Value = serde_json::from_str(line.split_once("\r\n\r\n").unwrap().1).unwrap();
        let mut sent = rest::message(300 + channel, "hola");
        sent["channel_id"] = json!(channel.to_string());
        sent["nonce"] = body["nonce"].clone();
        reply.send(response(200, &sent.to_string())).unwrap();
        assert!(matches!(
            app.until(|event| matches!(event, Event::MessageCreated { .. })).await,
            Event::MessageCreated { message, nonce: Some(nonce) }
                if message.channel == ChannelId(channel) && nonce == request + 1
        ));
    }
}

// Voice protocol (voz-protocolo).
#[tokio::test]
async fn start_times_arrive_as_events_and_a_call_waits_for_its_server_after_a_ready_without_it() {
    use rune_model::{CallPhase, ServerId, Timestamp};
    let mut app = Harness::new().await;
    let guild = json!({"id":"60","name":"Synthetic",
        "roles":[{"id":"60","name":"@everyone","permissions":"3146752"}],
        "members":[{"user":{"id":"42","username":"Local"},"roles":[]}],
        "channels":[{"id":"61","type":2,"name":"Voz","permission_overwrites":[]}],
        "voice_states":[{"user_id":"43","channel_id":"61"}]});
    app.dispatch
        .send(("GUILD_CREATE", guild.clone()))
        .await
        .unwrap();
    app.until(
        |event| matches!(event, Event::ChannelChanged(channel) if channel.id == ChannelId(61)),
    )
    .await;
    // The server shown is asked when its rooms went live; the answer is an event.
    app.running
        .commands
        .send(Command::WatchServer(Some(ServerId(60))))
        .await
        .unwrap();
    assert_eq!(
        app.frame().await,
        json!({"op": 43, "d": {"guild_id": "60", "fields": ["voice_start_time"]}})
    );
    app.dispatch
        .send((
            "CHANNEL_INFO",
            json!({"guild_id": "60", "channels": [{"id": "61", "voice_start_time": 1_700_000_000u64}]}),
        ))
        .await
        .unwrap();
    assert_eq!(
        app.until(|event| matches!(event, Event::VoiceStartTimes { .. }))
            .await,
        Event::VoiceStartTimes {
            server: ServerId(60),
            times: vec![(ChannelId(61), Some(Timestamp(1_700_000_000_000)))],
            whole: true,
        }
    );
    app.running
        .commands
        .send(Command::JoinVoice {
            channel: ChannelId(61),
            muted: false,
            deafened: false,
        })
        .await
        .unwrap();
    app.until(|event| matches!(event, Event::CallPhase { .. }))
        .await;
    // Asked already in this connection: op 4 alone.
    assert_eq!(app.frame().await["op"], 4);
    // A new Gateway session whose READY has the server unavailable: the
    // call waits for it, and muting meanwhile tells nothing yet.
    app.dispatch
        .send((
            "READY",
            json!({"user": {"id": "42"}, "session_id": "second-session",
                "resume_gateway_url": "wss://gateway.discord.gg",
                "guilds": [{"id": "60", "unavailable": true}], "private_channels": []}),
        ))
        .await
        .unwrap();
    app.until(|event| {
        matches!(
            event,
            Event::CallPhase {
                phase: CallPhase::Joining,
                ..
            }
        )
    })
    .await;
    app.running
        .commands
        .send(Command::SetVoiceSelf {
            muted: true,
            deafened: false,
        })
        .await
        .unwrap();
    // Its GUILD_CREATE reaches the call: asked again once, muted.
    app.dispatch.send(("GUILD_CREATE", guild)).await.unwrap();
    let update = app.frame().await;
    assert_eq!(
        (
            &update["op"],
            &update["d"]["channel_id"],
            &update["d"]["self_mute"]
        ),
        (&json!(4), &json!("61"), &json!(true))
    );
    assert!(
        tokio::time::timeout(Duration::from_millis(300), app.frames.recv())
            .await
            .is_err(),
        "one op 4"
    );
}

// Calls and the voice panel (llamadas-y-panel-de-voz).
#[tokio::test]
async fn a_direct_call_is_asked_about_first_and_a_refusal_is_a_plain_failure() {
    // llamadas-y-panel-de-voz: `GET /channels/{id}/call`, as the official
    // client asks before calling someone in a direct conversation.
    let mut app = Harness::new().await;
    let check = || Command::CheckCall {
        channel: ChannelId(20),
    };
    for (answer, ringable) in [
        (json!({"ringable": true}), true),
        (json!({"ringable": false}), false),
        // Not said: as the official client reads it, it does not ring.
        (json!({}), false),
    ] {
        app.running.commands.send(check()).await.unwrap();
        let (request, reply) = app.pending().await;
        assert!(
            request.starts_with("GET /api/v10/channels/20/call "),
            "{request}"
        );
        reply.send(response(200, &answer.to_string())).unwrap();
        assert_eq!(
            app.next_data().await,
            Event::CallChecked {
                channel: ChannelId(20),
                ringable
            }
        );
    }
    // Refused by Discord: no call, and nothing ambiguous about it. A
    // failure of its own is not a refusal: the interface says it apart.
    for (status, refused) in [(403, true), (404, true), (500, false), (502, false)] {
        app.running.commands.send(check()).await.unwrap();
        let (_, reply) = app.pending().await;
        let body = json!({"code": 50001, "message": "Missing Access"});
        reply.send(response(status, &body.to_string())).unwrap();
        let failed = app.next_data().await;
        assert!(
            matches!(
                failed,
                Event::Failed {
                    failure: Failure::CallCheck {
                        channel: ChannelId(20),
                        refused: said,
                    },
                    ambiguous: false,
                    ..
                } if said == refused
            ),
            "{status}: {failed:?}"
        );
    }
    // Only a direct conversation in the list is asked about.
    app.running
        .commands
        .send(Command::CheckCall {
            channel: ChannelId(99),
        })
        .await
        .unwrap();
    assert!(matches!(
        app.next_data().await,
        Event::Failed {
            failure: Failure::CallCheck {
                channel: ChannelId(99),
                refused: false,
            },
            ..
        }
    ));
}

// Streams (transmisiones).
#[tokio::test]
async fn a_stream_preview_that_cannot_be_asked_is_answered_with_none_and_a_calls_is_asked() {
    let mut app = Harness::new().await;
    let preview = |server, owner| Command::LoadStreamPreview {
        server,
        channel: ChannelId(20),
        owner: rune_model::UserId(owner),
    };
    // Not a voice channel of that server, or nobody's: none at once, never
    // a request, so the interface's backoff runs.
    for (server, owner) in [(Some(rune_model::ServerId(10)), 2), (None, 0)] {
        app.running
            .commands
            .send(preview(server, owner))
            .await
            .unwrap();
        let answer = app
            .until(|event| matches!(event, Event::StreamPreview { .. }))
            .await;
        assert_eq!(
            answer,
            Event::StreamPreview {
                channel: ChannelId(20),
                owner: rune_model::UserId(owner),
                image: None,
                retry_after: None,
            }
        );
    }
    assert!(app.requests.try_recv().is_err());
    // The conversation's call: asked under its call key, as the official
    // client asks it.
    app.running.commands.send(preview(None, 2)).await.unwrap();
    let (request, reply) = app.pending().await;
    assert!(
        request.starts_with("GET /api/v10/streams/call:20:2/preview?version="),
        "{request}"
    );
    let url = "https://cdn.discordapp.com/streams/call:20:2/abc.jpg";
    reply
        .send(response(200, &json!({ "url": url }).to_string()))
        .unwrap();
    let answer = app
        .until(|event| matches!(event, Event::StreamPreview { .. }))
        .await;
    assert!(
        matches!(&answer, Event::StreamPreview { image: Some(image), .. } if image.url == url),
        "{answer:?}"
    );
    let lines = crate::voice::diag::captured::lines();
    for wanted in [
        "rune: stream preview: GET not asked: not a known voice channel or call",
        "rune: stream preview: GET not asked: no owner",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "{wanted}");
    }
}
