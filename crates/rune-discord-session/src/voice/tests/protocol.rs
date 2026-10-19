//! Voice protocol (voz-protocolo) against the synthetic voice server: the
//! call alone in its channel, what its connection measures, a call held for
//! a server that comes after READY, and when voice channels went live.
//! Nothing here reaches Discord.

use rune_model::{ServerId, Timestamp};

use super::*;

/// Hello (with `interval`), Identify, Ready, IP discovery, Select Protocol
/// and Session Description: the call is alone in its channel. Its UDP
/// address.
async fn alone(
    server: &mut Server,
    peer: &mut Peer,
    acks: &mut Vec<i64>,
    interval: u64,
) -> SocketAddr {
    server
        .text(peer, 8, json!({"heartbeat_interval": interval}))
        .await;
    expect(peer, acks, 0).await;
    let udp_port = server.udp.local_addr().unwrap().port();
    server
        .text(
            peer,
            2,
            json!({"ssrc": 11, "ip": "127.0.0.1", "port": udp_port, "modes": [rtp::MODE]}),
        )
        .await;
    let mut datagram = [0u8; 4096];
    let (_, client): (usize, SocketAddr) = timeout(WAIT, server.udp.recv_from(&mut datagram))
        .await
        .unwrap()
        .unwrap();
    let mut reply = [0u8; 74];
    reply[..4].copy_from_slice(&[0, 2, 0, 70]);
    reply[4..8].copy_from_slice(&11u32.to_be_bytes());
    reply[8..17].copy_from_slice(b"127.0.0.1");
    reply[72..].copy_from_slice(&client.port().to_be_bytes());
    server.udp.send_to(&reply, client).await.unwrap();
    expect(peer, acks, 1).await;
    server
        .text(
            peer,
            4,
            json!({"mode": rtp::MODE, "secret_key": KEY.to_vec(), "dave_protocol_version": 1}),
        )
        .await;
    expect(peer, acks, 5).await;
    next(peer, acks).await.expect_err("binary key package");
    client
}

#[tokio::test]
async fn the_call_measures_its_round_trip_and_the_audio_that_never_came() {
    let mut server = Server::new().await;
    let (port, _link) = voice_link();
    let (controls, watched) = watch::channel(Controls::default());
    let (phase_tx, mut phases) = watch::channel(CallPhase::Joining);
    let (quality, mut measured) = watch::channel(None);
    let call = tokio::spawn(connection::run(
        Credentials {
            codes: Some(connection::Codes {
                privacy: watch::channel(None).0,
                verify: tokio::sync::mpsc::channel(1).1,
                quality,
            }),
            ..credentials(server.url())
        },
        port,
        None,
        watched,
        phase_tx,
    ));
    let mut acks = Vec::new();
    let mut peer = server.accept().await;
    let client = alone(&mut server, &mut peer, &mut acks, 1_000).await;
    // Alone in the channel: connected, waiting for the others.
    phase(&mut phases, CallPhase::Waiting).await;
    // Someone's audio, with packets 3 and 4 lost on the way.
    let mut sealer = Transport::new(&Zeroizing::new(KEY));
    for sequence in [0u16, 1, 2, 5, 6] {
        let packet = sealer
            .seal(
                &rtp::header(sequence, u32::from(sequence) * 960, 22),
                b"opus",
            )
            .unwrap();
        server.udp.send_to(&packet, client).await.unwrap();
    }
    // Each heartbeat answered measures the call: its round trip, and the
    // packets that never came since the one before.
    let lossy = timeout(WAIT, async {
        loop {
            let _ = timeout(Duration::from_millis(100), next(&mut peer, &mut acks)).await;
            if let Some(quality) = *measured.borrow_and_update() {
                if quality.loss_permille > 0 {
                    return quality;
                }
            }
        }
    })
    .await
    .expect("a measure with the loss");
    assert!(lossy.rtt_ms < 5_000, "{lossy:?}");
    // 2 of 7 when they came together; more when a heartbeat fell between.
    assert!((285..=500).contains(&lossy.loss_permille), "{lossy:?}");
    // Nothing more lost: the next measure says so.
    let clean = timeout(WAIT, async {
        loop {
            let _ = timeout(Duration::from_millis(100), next(&mut peer, &mut acks)).await;
            if measured.has_changed().unwrap() {
                return *measured.borrow_and_update();
            }
        }
    })
    .await
    .unwrap()
    .unwrap();
    assert_eq!(clean.loss_permille, 0);
    drop(controls);
    assert_eq!(timeout(WAIT, call).await.unwrap().unwrap(), Ok(()));
}

#[tokio::test]
async fn a_call_waits_for_its_server_after_a_ready_without_it_then_asks_or_ends() {
    tokio::time::pause();
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let mut wanted = calls.gateway();
    let mut known = snapshot();
    let unavailable = json!({"guilds": [{"id": GUILD.to_string(), "unavailable": true}]});
    let created = |session: &str| {
        json!({"id": GUILD.to_string(), "voice_states": [
            {"user_id": ALICE.to_string(), "channel_id": CHANNEL.to_string(), "session_id": session},
        ]})
    };
    // A call this device's voice server accepted.
    let mut accepted = |calls: &mut Calls, session: &str| {
        calls.dispatch(&mut known, "VOICE_STATE_UPDATE", &own(session, CHANNEL));
        calls.call.as_mut().unwrap().confirmed = true;
    };
    calls
        .join(&snapshot(), ChannelId(CHANNEL), false, false)
        .unwrap();
    accepted(&mut calls, "OLD_SESSION");
    let asked = wanted.borrow_and_update().unwrap();
    // The new session's READY has its server unavailable: op 4 waits.
    assert!(matches!(
        &calls.ready(&unavailable)[..],
        [Event::CallPhase {
            phase: CallPhase::Joining,
            ..
        }]
    ));
    assert!(!wanted.has_changed().unwrap());
    // Another server changes nothing.
    assert!(calls
        .guild_created(&json!({"id": "99", "voice_states": []}))
        .is_empty());
    assert!(!wanted.has_changed().unwrap());
    // Its own comes with the account still on this device: asked again, once.
    assert!(calls.guild_created(&created("OLD_SESSION")).is_empty());
    let again = wanted.borrow_and_update().unwrap();
    assert_eq!(again.channel, Some(CHANNEL));
    assert_ne!(again.request, asked.request);
    assert!(calls.guild_created(&created("OLD_SESSION")).is_empty());
    assert!(!wanted.has_changed().unwrap());

    // Meanwhile a phone took the call: not taken back, nothing told, even
    // after another READY without the server in between.
    accepted(&mut calls, "NEW_SESSION");
    calls.ready(&unavailable);
    calls.ready(&unavailable);
    assert!(matches!(
        &calls.guild_created(&created("PHONE"))[..],
        [Event::CallEnded { reason: Some(reason), .. }] if reason == super::super::TAKEN_OVER
    ));
    assert_eq!(*wanted.borrow_and_update(), None);

    // A server that never comes: asked again all the same, in time.
    calls
        .join(&snapshot(), ChannelId(CHANNEL), false, false)
        .unwrap();
    accepted(&mut calls, "OLD_SESSION");
    let before = wanted.borrow_and_update().unwrap();
    calls.ready(&unavailable);
    assert!(!wanted.has_changed().unwrap());
    assert!(calls.next().await.is_empty());
    assert_ne!(wanted.borrow_and_update().unwrap().request, before.request);
    // A READY that has it asks at once, as before.
    let available = json!({"guilds": [{"id": GUILD.to_string(), "voice_states": []}]});
    calls.ready(&available);
    assert!(wanted.has_changed().unwrap());
}

#[tokio::test]
async fn a_call_held_for_its_server_tells_nothing_until_it_comes_nor_takes_this_device_for_another()
{
    tokio::time::pause();
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let mut wanted = calls.gateway();
    let mut known = snapshot();
    // This device's call, connected, then its connection drops: a rejoin
    // is due in a second.
    calls.gateway_session(Zeroizing::new("OLD_GATEWAY".into()));
    calls
        .join(&snapshot(), ChannelId(CHANNEL), false, false)
        .unwrap();
    calls.dispatch(
        &mut known,
        "VOICE_STATE_UPDATE",
        &own("OLD_GATEWAY", CHANNEL),
    );
    let (phase, lose) = connect(&mut calls);
    phase.send_replace(CallPhase::Connected);
    calls.next().await;
    let _ = lose.send(());
    assert!(matches!(
        &calls.next().await[..],
        [Event::CallPhase {
            phase: CallPhase::Reconnecting { attempt: 1 },
            ..
        }]
    ));
    wanted.borrow_and_update();
    // Meanwhile the Gateway starts anew, without the call's server yet.
    calls.gateway_session(Zeroizing::new("NEW_GATEWAY".into()));
    calls.ready(&json!({"guilds": [{"id": GUILD.to_string(), "unavailable": true}]}));
    // Neither muting nor the rejoin's time tells the service anything.
    calls.set_self(true, false);
    assert!(!wanted.has_changed().unwrap());
    assert!(timeout(Duration::from_secs(5), calls.next()).await.is_err());
    assert!(!wanted.has_changed().unwrap());
    // The server comes with the account in the channel with this device's
    // new Gateway session: its own, not another device's. Asked once, muted.
    let created = json!({"id": GUILD.to_string(), "voice_states": [
        {"user_id": ALICE.to_string(), "channel_id": CHANNEL.to_string(), "session_id": "NEW_GATEWAY"},
    ]});
    assert!(calls.guild_created(&created).is_empty());
    let asked = wanted.borrow_and_update().unwrap();
    assert_eq!((asked.channel, asked.muted), (Some(CHANNEL), true));
    let call = calls.call.as_ref().expect("the call goes on");
    assert!(call.guild_wait.is_none() && call.rejoin_at.is_none());
}

#[tokio::test]
async fn a_new_connection_of_the_call_counts_its_own_time_connected() {
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let _wanted = calls.gateway();
    let mut known = snapshot();
    calls
        .join(&snapshot(), ChannelId(CHANNEL), false, false)
        .unwrap();
    calls.dispatch(&mut known, "VOICE_STATE_UPDATE", &own("SESSION", CHANNEL));
    // Connected a minute on the connection before.
    calls.call.as_mut().unwrap().connected_at =
        Some(tokio::time::Instant::now() - Duration::from_secs(60));
    // The service names a voice server: a new connection, its own clock, so
    // a loss soon after does not pass for a stable call's.
    calls.dispatch(&mut known, "VOICE_SERVER_UPDATE", &json!({
        "guild_id": GUILD.to_string(), "token": "SYNTHETIC_VOICE_TOKEN", "endpoint": "c-test.discord.media:443",
    }));
    let call = calls.call.as_ref().unwrap();
    assert!(call.connection.is_some());
    assert_eq!(call.connected_at, None);
}

#[test]
fn op_43_answers_and_one_channel_s_changes_become_start_times() {
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let mut snapshot = snapshot();
    let info = calls.dispatch(
        &mut snapshot,
        "CHANNEL_INFO",
        &json!({"guild_id": GUILD.to_string(), "channels": [
            {"id": CHANNEL.to_string(), "voice_start_time": 1_700_000_000u64},
            {"id": STAGE.to_string(), "voice_start_time": null},
            // Malformed, so skipped: an id that is not one, a time out of
            // range or of another kind (never read as nobody there).
            {"id": "not an id", "voice_start_time": 1_700_000_000u64},
            {"id": "36", "voice_start_time": 12},
            {"id": "37", "voice_start_time": "1700000000"},
        ]}),
    );
    assert_eq!(
        info,
        Some(vec![Event::VoiceStartTimes {
            server: ServerId(GUILD),
            times: vec![
                (ChannelId(CHANNEL), Some(Timestamp(1_700_000_000_000))),
                (ChannelId(STAGE), None),
            ],
            whole: true,
        }])
    );
    let update = calls.dispatch(
        &mut snapshot,
        "VOICE_CHANNEL_START_TIME_UPDATE",
        &json!({"id": CHANNEL.to_string(), "guild_id": GUILD.to_string(), "voice_start_time": null}),
    );
    assert_eq!(
        update,
        Some(vec![Event::VoiceStartTimes {
            server: ServerId(GUILD),
            times: vec![(ChannelId(CHANNEL), None)],
            whole: false,
        }])
    );
    // Without a server, nothing.
    assert_eq!(
        calls.dispatch(&mut snapshot, "CHANNEL_INFO", &json!({"channels": []})),
        Some(Vec::new())
    );
}
