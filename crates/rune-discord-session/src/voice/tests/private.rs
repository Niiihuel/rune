//! Private calls (2026-10-17) against the synthetic voice server: joining
//! a conversation's call, ringing once the service has it, calls coming in
//! and ending, and what the requests carry. Nothing here reaches Discord.

use rune_model::PrivateCall;
use rune_provider::Command;

use super::super::diag::captured;
use super::super::private::{allowed, request};
use super::*;

/// The direct conversation of the fixture, with Bob.
const DIRECT: u64 = 50;

fn ring(users: &[u64]) -> Command {
    Command::RingCall {
        channel: ChannelId(DIRECT),
        users: users.iter().copied().map(UserId).collect(),
    }
}

#[tokio::test]
async fn a_conversations_call_has_no_guild_rings_once_the_service_has_it_and_goes_by_its_channel() {
    let server = Server::new().await;
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    calls.local = Some(server.url());
    let mut wanted = calls.gateway();
    let asked = calls.status_watch();
    let mut snapshot = snapshot();
    // A ring needs the account in that call.
    assert!(calls.ring(ring(&[])).is_err());
    let events = calls
        .join(&snapshot, ChannelId(DIRECT), false, false)
        .unwrap();
    assert!(matches!(
        events[..],
        [Event::CallPhase {
            phase: CallPhase::Joining,
            ..
        }]
    ));
    // What the service knows of it is asked first (op 13), and op 4 has no guild.
    assert_eq!(asked.borrow().map(|(channel, _)| channel), Some(DIRECT));
    let want = wanted.borrow_and_update().unwrap();
    assert_eq!((want.guild, want.channel), (None, Some(DIRECT)));
    // Before the service has the call, the ring waits for it.
    assert!(calls.ring(ring(&[])).unwrap().is_none());
    assert!(calls.take_ring().is_none());
    let events = calls
        .dispatch(
            &mut snapshot,
            "CALL_CREATE",
            &json!({
                "channel_id": DIRECT.to_string(), "message_id": "900", "region": "brazil",
                "ringing": [], "ongoing_rings": {},
                "voice_states": [{"user_id": ALICE.to_string(), "session_id": "SYNTHETIC_VOICE_SESSION",
                    "self_mute": false, "self_deaf": false}],
            }),
        )
        .unwrap();
    assert_eq!(
        events[0],
        Event::PrivateCall(PrivateCall {
            channel: ChannelId(DIRECT),
            ringing: Vec::new(),
        })
    );
    assert!(
        matches!(&events[1..], [Event::VoiceChanged(state)] if state.user == UserId(ALICE) && state.channel == ChannelId(DIRECT)),
        "{events:?}"
    );
    assert_eq!(calls.take_ring(), Some(ring(&[])), "the ring goes now");
    assert_eq!(
        calls.ring(ring(&[BOB])).unwrap(),
        Some(ring(&[BOB])),
        "again, at once"
    );
    // Its voice server is named by the channel; another one's is not ours.
    calls.dispatch(
        &mut snapshot,
        "VOICE_SERVER_UPDATE",
        &json!({
            "guild_id": GUILD.to_string(), "channel_id": DIRECT.to_string(),
            "token": "SYNTHETIC_VOICE_TOKEN", "endpoint": "c-test.discord.media:443",
        }),
    );
    assert!(calls.call.as_ref().unwrap().server.is_none(), "a guild's");
    calls.dispatch(
        &mut snapshot,
        "VOICE_SERVER_UPDATE",
        &json!({
            "guild_id": null, "channel_id": DIRECT.to_string(),
            "token": "SYNTHETIC_VOICE_TOKEN", "endpoint": "c-test.discord.media:443",
        }),
    );
    let handshake = tokio::spawn(async move {
        let mut server = server;
        let mut peer = server.accept().await;
        server
            .text(&mut peer, 8, json!({"heartbeat_interval": 200}))
            .await;
        let identify = expect(&mut peer, &mut Vec::new(), 0).await;
        (peer, identify)
    });
    while !handshake.is_finished() {
        let _ = timeout(Duration::from_millis(20), calls.next()).await;
    }
    let (_peer, identify) = handshake.await.unwrap();
    assert_eq!(
        identify["server_id"],
        DIRECT.to_string(),
        "the call goes by its channel"
    );
    assert_eq!(identify["channel_id"], DIRECT.to_string());
    // Whom it rings, by the official client's field or Serein's; not said, as it was.
    for (update, rings) in [
        (
            json!({"ongoing_rings": {BOB.to_string(): ALICE.to_string()}}),
            vec![UserId(BOB)],
        ),
        (json!({"message_id": "900"}), vec![UserId(BOB)]),
        (json!({"ringing": []}), vec![]),
        (
            json!({"ringing": [BOB.to_string()], "ongoing_rings": {BOB.to_string(): null}}),
            vec![UserId(BOB)],
        ),
    ] {
        let mut update = update;
        update["channel_id"] = json!(DIRECT.to_string());
        let events = calls
            .dispatch(&mut snapshot, "CALL_UPDATE", &update)
            .unwrap();
        assert_eq!(
            events,
            [Event::PrivateCall(PrivateCall {
                channel: ChannelId(DIRECT),
                ringing: rings,
            })]
        );
    }
    // Streams in it are the call's.
    let want = VoiceWant {
        share: Some(ALICE),
        streams: std::array::from_fn(|at| (at == 0).then_some((BOB, 0))),
        ..wanted.borrow_and_update().unwrap()
    };
    assert_eq!(want.share_key().as_deref(), Some("call:50:1"));
    assert_eq!(want.stream_keys(), [("call:50:2".to_owned(), 0)]);
    // The service ends it: the account leaves, told (op 4 without a channel).
    let events = calls
        .dispatch(
            &mut snapshot,
            "CALL_DELETE",
            &json!({"channel_id": DIRECT.to_string()}),
        )
        .unwrap();
    assert!(
        matches!(
            &events[..],
            [
                Event::PrivateCallEnded { .. },
                Event::VoiceLeft { .. },
                Event::VoiceLeft { .. },
                Event::CallEnded {
                    reason: Some(_),
                    ..
                }
            ]
        ),
        "{events:?}"
    );
    let left = wanted.borrow_and_update().unwrap();
    assert_eq!((left.guild, left.channel), (None, None));
    assert!(calls.ring(ring(&[])).is_err(), "no call to ring in");
    // Nothing said names an id, a token or a session.
    for line in captured::lines()
        .iter()
        .filter(|line| line.contains("voice private call"))
    {
        assert!(
            !["50", "SYNTHETIC", "brazil"]
                .iter()
                .any(|secret| line.contains(secret)),
            "{line}"
        );
    }
}

#[tokio::test]
async fn calls_coming_in_name_only_the_conversations_people_and_go_with_their_session() {
    let (port, _link) = voice_link();
    let mut calls = Calls::new(Some(port));
    let _wanted = calls.gateway();
    let mut snapshot = snapshot();
    // Ringing the account, and someone who is not in the conversation.
    let events = calls
        .dispatch(
            &mut snapshot,
            "CALL_CREATE",
            &json!({
                "channel_id": DIRECT.to_string(),
                "ongoing_rings": {ALICE.to_string(): BOB.to_string(), "999": BOB.to_string()},
                "voice_states": [
                    {"user_id": BOB.to_string(), "self_mute": false},
                    {"user_id": "999", "self_mute": false},
                ],
            }),
        )
        .unwrap();
    assert_eq!(
        events[0],
        Event::PrivateCall(PrivateCall {
            channel: ChannelId(DIRECT),
            ringing: vec![UserId(ALICE)],
        })
    );
    assert!(
        matches!(&events[1..], [Event::VoiceChanged(state)] if state.user == UserId(BOB)),
        "{events:?}"
    );
    // A server's channel, or one not in the list, has no such call.
    for channel in [CHANNEL, 77] {
        assert!(calls
            .dispatch(
                &mut snapshot,
                "CALL_CREATE",
                &json!({"channel_id": channel.to_string(), "ringing": [ALICE.to_string()]})
            )
            .unwrap()
            .is_empty());
    }
    // An outage ends it too, and who was in it leaves.
    let events = calls
        .dispatch(
            &mut snapshot,
            "CALL_DELETE",
            &json!({"channel_id": DIRECT.to_string(), "unavailable": true}),
        )
        .unwrap();
    assert_eq!(
        events,
        [
            Event::PrivateCallEnded {
                channel: ChannelId(DIRECT)
            },
            Event::VoiceLeft {
                user: UserId(BOB),
                channel: ChannelId(DIRECT)
            }
        ]
    );
    // A new Gateway session forgets the calls it was told, and asks again.
    calls.dispatch(
        &mut snapshot,
        "CALL_CREATE",
        &json!({"channel_id": DIRECT.to_string()}),
    );
    let _wanted = calls.gateway();
    let asked = calls.status_watch();
    calls.opened(&snapshot, ChannelId(CHANNEL));
    assert_eq!(
        *asked.borrow(),
        None,
        "a server's channel is not asked about"
    );
    calls.opened(&snapshot, ChannelId(DIRECT));
    calls.opened(&snapshot, ChannelId(DIRECT));
    assert_eq!(*asked.borrow(), Some((DIRECT, 1)), "once a session");
}

#[test]
fn rings_name_the_conversations_people_and_decline_names_the_account() {
    let snapshot = snapshot();
    let stop = |users: &[u64]| Command::StopRinging {
        channel: ChannelId(DIRECT),
        users: users.iter().copied().map(UserId).collect(),
    };
    assert!(allowed(&snapshot, &ring(&[])).is_ok(), "everyone");
    assert!(allowed(&snapshot, &ring(&[BOB])).is_ok());
    // Asking whether a call may ring (llamadas-y-panel-de-voz): only of a
    // direct conversation in the list.
    let check = |channel| Command::CheckCall {
        channel: ChannelId(channel),
    };
    assert!(allowed(&snapshot, &check(DIRECT)).is_ok());
    assert!(allowed(&snapshot, &check(CHANNEL)).is_err(), "a server's");
    assert!(allowed(&snapshot, &check(999)).is_err());
    assert!(
        allowed(&snapshot, &ring(&[ALICE])).is_err(),
        "not the account"
    );
    assert!(allowed(&snapshot, &ring(&[999])).is_err());
    assert!(allowed(&snapshot, &stop(&[ALICE])).is_ok(), "declining");
    assert!(allowed(&snapshot, &stop(&[])).is_err());
    let elsewhere = Command::RingCall {
        channel: ChannelId(CHANNEL),
        users: Vec::new(),
    };
    assert!(
        allowed(&snapshot, &elsewhere).is_err(),
        "a server's channel"
    );
    assert_eq!(
        request(&ring(&[])),
        Some(("/channels/50/call/ring".into(), json!({"recipients": null})))
    );
    assert_eq!(
        request(&stop(&[ALICE])),
        Some((
            "/channels/50/call/stop-ringing".into(),
            json!({"recipients": ["1"]})
        ))
    );
}
