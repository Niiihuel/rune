//! Voice protocol (voz-protocolo): since when each voice channel has had
//! someone in it, and the call alone in its channel.

use rune_model::VoiceState;

use super::*;

fn in_voice(user: UserId) -> VoiceState {
    VoiceState {
        user,
        channel: VOICE,
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

fn times(times: &[(ChannelId, Option<i64>)], whole: bool) -> Event {
    Event::VoiceStartTimes {
        server: ServerId(1),
        times: times
            .iter()
            .map(|(channel, at)| (*channel, at.map(Timestamp)))
            .collect(),
        whole,
    }
}

#[test]
fn a_whole_server_replaces_its_start_times_and_only_occupied_channels_have_one() {
    let mut store = ready();
    let now = Instant::now();
    store.apply(
        times(&[(VOICE, Some(1_000)), (GENERAL, Some(2_000))], true),
        now,
    );
    // Nobody is in the room yet: nothing to show.
    assert_eq!(store.voice_started(VOICE), None);
    store.apply(Event::VoiceChanged(in_voice(ADA)), now);
    assert_eq!(store.voice_started(VOICE), Some(Timestamp(1_000)));
    // One channel changes alone; a whole list drops what it leaves out.
    store.apply(times(&[(VOICE, Some(3_000))], false), now);
    assert_eq!(store.voice_started(VOICE), Some(Timestamp(3_000)));
    store.apply(times(&[], true), now);
    assert_eq!(store.voice_started(VOICE), None);
    store.apply(times(&[(VOICE, Some(4_000))], false), now);
    store.apply(times(&[(VOICE, None)], false), now);
    assert_eq!(store.voice_started(VOICE), None);
    // A new session tells them again.
    store.apply(times(&[(VOICE, Some(5_000))], false), now);
    store.apply(Event::Ready(Box::new(snapshot())), now);
    assert_eq!(store.voice_started(VOICE), None);
}

#[test]
fn waiting_alone_is_connected_and_keeps_the_call_clock() {
    let mut store = ready();
    let start = Instant::now();
    store.begin_call(VOICE, false, false);
    let phase = |store: &mut Store, phase: CallPhase, at: Instant| {
        store.apply(
            Event::CallPhase {
                channel: VOICE,
                phase,
                quality: None,
            },
            at,
        );
    };
    phase(&mut store, CallPhase::Waiting, start);
    assert!(store.call().unwrap().phase.is_connected());
    let later = start + Duration::from_secs(30);
    // Someone came: the same connection, the same clock.
    phase(&mut store, CallPhase::Connected, later);
    assert_eq!(store.call_duration(later), Some(Duration::from_secs(30)));
    phase(&mut store, CallPhase::Waiting, later);
    assert_eq!(store.call_duration(later), Some(Duration::from_secs(30)));
}

#[test]
fn a_start_time_is_only_taken_for_its_own_server_and_goes_once_the_room_empties() {
    let mut store = ready();
    let now = Instant::now();
    store.apply(Event::VoiceChanged(in_voice(ADA)), now);
    // Told by another server, or of a channel not known: not taken.
    store.apply(
        Event::VoiceStartTimes {
            server: ServerId(2),
            times: vec![(VOICE, Some(Timestamp(1_000)))],
            whole: false,
        },
        now,
    );
    store.apply(times(&[(ChannelId(99), Some(1_000))], false), now);
    assert_eq!(store.voice_started(VOICE), None);
    assert_eq!(store.voice_started_all().count(), 0);
    // Its own: kept while someone is there.
    store.apply(times(&[(VOICE, Some(2_000))], false), now);
    assert_eq!(store.voice_started(VOICE), Some(Timestamp(2_000)));
    // Moved out, the room is empty: whoever comes next does not inherit it.
    store.apply(
        Event::VoiceChanged(VoiceState {
            channel: GENERAL,
            ..in_voice(ADA)
        }),
        now,
    );
    store.apply(Event::VoiceChanged(in_voice(ME)), now);
    assert_eq!(store.voice_started(VOICE), None);
    // Likewise when the last one leaves.
    store.apply(times(&[(VOICE, Some(3_000))], false), now);
    assert_eq!(store.voice_started(VOICE), Some(Timestamp(3_000)));
    store.apply(
        Event::VoiceLeft {
            user: ME,
            channel: VOICE,
        },
        now,
    );
    store.apply(Event::VoiceChanged(in_voice(ADA)), now);
    assert_eq!(store.voice_started(VOICE), None);
}
