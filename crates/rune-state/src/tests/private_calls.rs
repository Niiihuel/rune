//! Private calls (2026-10-17): what the account sees of a conversation's
//! call through each way it goes, and what it may do there.

use std::time::Duration;

use rune_model::{PrivateCall, VoiceState};
use rune_provider::{Capabilities, ProviderInfo, ProviderKind, VoiceSupport};

use super::*;
use crate::CallStatus;

const BRUNO: UserId = UserId(3);
const DIRECT: ChannelId = ChannelId(20);
const GROUP: ChannelId = ChannelId(21);

fn store(private_calls: bool) -> Store {
    let mut store = Store::new();
    store.provider = Some(ProviderInfo {
        name: "Prueba".into(),
        kind: ProviderKind::Simulated,
        capabilities: Capabilities {
            voice: VoiceSupport::Simulated,
            private_calls,
            share_screen: true,
            watch_streams: true,
            send_messages: true,
            ..Default::default()
        },
        notice: String::new(),
    });
    let mut snapshot = snapshot();
    snapshot.users.push(user(BRUNO, "bruno"));
    let mut direct = channel(DIRECT, ChannelKind::Direct, "");
    direct.server = None;
    direct.recipients = vec![ADA];
    let mut group = channel(GROUP, ChannelKind::Group, "grupo");
    group.server = None;
    group.recipients = vec![ADA, BRUNO];
    snapshot.channels.extend([direct, group]);
    snapshot.relationships = Some(Vec::new());
    store.apply(Event::Ready(Box::new(snapshot)), Instant::now());
    store
}

fn state(user: UserId, channel: ChannelId) -> VoiceState {
    VoiceState {
        user,
        channel,
        muted: false,
        deafened: false,
        speaking: false,
        streaming: false,
        server_muted: false,
        server_deafened: false,
        camera: false,
        suppressed: false,
        speak_request: None,
    }
}

fn rings(store: &mut Store, channel: ChannelId, ringing: &[UserId], now: Instant) -> Applied {
    store.apply(
        Event::PrivateCall(PrivateCall {
            channel,
            ringing: ringing.to_vec(),
        }),
        now,
    )
}

fn connected(store: &mut Store, channel: ChannelId, now: Instant) {
    store.apply(Event::VoiceChanged(state(ME, channel)), now);
    store.apply(
        Event::CallPhase {
            channel,
            phase: CallPhase::Connected,
            quality: None,
        },
        now,
    );
}

#[test]
fn a_call_rings_then_is_answered_and_hung_up_and_one_unanswered_or_declined_says_so() {
    let now = Instant::now();
    let mut store = store(true);
    assert_eq!(store.call_status(DIRECT), None, "no call");
    // Calling Ada: ringing until she comes, then live, then she hangs up.
    store.begin_call(DIRECT, false, false);
    connected(&mut store, DIRECT, now);
    assert_eq!(store.call_status(DIRECT), Some(CallStatus::Alone));
    assert_eq!(
        rings(&mut store, DIRECT, &[ADA], now),
        Applied::PrivateCall(DIRECT)
    );
    assert_eq!(store.call_status(DIRECT), Some(CallStatus::Ringing));
    assert_eq!(store.ringing(DIRECT), [ADA]);
    store.apply(Event::VoiceChanged(state(ADA, DIRECT)), now);
    rings(&mut store, DIRECT, &[], now);
    assert_eq!(store.call_status(DIRECT), Some(CallStatus::Live));
    store.apply(
        Event::VoiceLeft {
            user: ADA,
            channel: DIRECT,
        },
        now,
    );
    assert_eq!(store.call_status(DIRECT), Some(CallStatus::HungUp));
    // A new call: the ring stops soon and she never came, so she declined.
    store.end_call();
    store.begin_call(DIRECT, false, false);
    connected(&mut store, DIRECT, now);
    rings(&mut store, DIRECT, &[ADA], now);
    rings(&mut store, DIRECT, &[], now + Duration::from_secs(2));
    assert_eq!(store.call_status(DIRECT), Some(CallStatus::Declined));
    // Rung again, the service gives up on it later: not answered.
    rings(&mut store, DIRECT, &[ADA], now + Duration::from_secs(3));
    rings(&mut store, DIRECT, &[], now + Duration::from_secs(60));
    assert_eq!(store.call_status(DIRECT), Some(CallStatus::Unanswered));
    // In a group, someone declining while another lets it ring is unanswered.
    store.end_call();
    store.begin_call(GROUP, false, false);
    connected(&mut store, GROUP, now);
    rings(&mut store, GROUP, &[ADA, BRUNO], now);
    rings(&mut store, GROUP, &[BRUNO], now + Duration::from_secs(1));
    rings(&mut store, GROUP, &[], now + Duration::from_secs(60));
    assert_eq!(store.call_status(GROUP), Some(CallStatus::Unanswered));
    // The call ends for everyone.
    assert_eq!(
        store.apply(Event::PrivateCallEnded { channel: GROUP }, now),
        Applied::PrivateCall(GROUP)
    );
    assert!(!store.has_private_call(GROUP));
}

#[test]
fn a_call_coming_in_rings_until_answered_declined_or_taken_on_another_device() {
    let now = Instant::now();
    let mut store = store(true);
    store.apply(Event::VoiceChanged(state(ADA, DIRECT)), now);
    rings(&mut store, DIRECT, &[ME], now);
    assert_eq!(store.call_status(DIRECT), Some(CallStatus::Incoming));
    assert_eq!(store.incoming_calls(), [DIRECT]);
    // Declined here: it rings no more, ahead of the provider.
    store.decline_call(DIRECT);
    assert!(store.incoming_calls().is_empty());
    assert_eq!(store.call_status(DIRECT), Some(CallStatus::InProgress));
    // Rung again and answered: Ada was in it, so her leaving is hanging up.
    rings(&mut store, DIRECT, &[ME], now);
    store.begin_call(DIRECT, false, false);
    assert!(store.incoming_calls().is_empty(), "answering");
    connected(&mut store, DIRECT, now);
    rings(&mut store, DIRECT, &[], now);
    assert_eq!(store.call_status(DIRECT), Some(CallStatus::Live));
    store.apply(
        Event::VoiceLeft {
            user: ADA,
            channel: DIRECT,
        },
        now,
    );
    assert_eq!(store.call_status(DIRECT), Some(CallStatus::HungUp));
    // Taken on another device: the account is there, but not from here.
    store.end_call();
    store.apply(Event::VoiceChanged(state(ADA, GROUP)), now);
    rings(&mut store, GROUP, &[ME], now);
    rings(&mut store, GROUP, &[], now);
    store.apply(Event::VoiceChanged(state(ME, GROUP)), now);
    assert_eq!(store.call_status(GROUP), Some(CallStatus::Elsewhere));
    // A server's channel is no conversation's call.
    rings(&mut store, GENERAL, &[ME], now);
    assert!(!store.has_private_call(GENERAL));
    // A new session tells its calls again.
    store.apply(Event::Ready(Box::new(snapshot())), now);
    assert!(store.incoming_calls().is_empty());
}

#[test]
fn calling_needs_the_capability_and_nobody_blocked() {
    let without = store(false);
    let abilities = without.abilities(DIRECT).unwrap();
    assert!(!abilities.ring.allowed() && !abilities.join_voice.allowed());
    let mut with = store(true);
    let abilities = with.abilities(DIRECT).unwrap();
    // A conversation asks no permission of its own to call or share there.
    assert!(abilities.ring.allowed() && abilities.join_voice.allowed());
    assert!(abilities.speak.allowed() && abilities.share_screen.allowed());
    assert!(
        !abilities.watch_stream.allowed(),
        "watching asks for the call to be there"
    );
    assert!(!with.abilities(GENERAL).unwrap().ring.allowed());
    with.apply(
        Event::RelationshipChanged(rune_model::Relationship {
            user: ADA,
            kind: rune_model::RelationshipKind::Blocked,
            since: None,
            nickname: None,
            spam: false,
        }),
        Instant::now(),
    );
    let blocked = with.abilities(DIRECT).unwrap();
    assert!(blocked.ring.reason().is_some_and(|why| why.contains("ada")));
    assert!(!blocked.join_voice.allowed());
}
