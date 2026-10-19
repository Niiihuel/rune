//! The simulation's private calls (2026-10-17), on paused time: each
//! direct conversation shows one way a call goes, and the host can have
//! someone call the account and answer it elsewhere.

use super::*;
use rune_model::PrivateCall;

/// The direct conversation with `person`, by the index of their account.
fn direct(session: &Session, person: u64) -> ChannelId {
    session
        .snapshot
        .channels
        .iter()
        .find(|channel| {
            channel.kind == ChannelKind::Direct && channel.recipients == [UserId(10 + person)]
        })
        .unwrap()
        .id
}

/// Events until one matches, the others kept in `seen`.
async fn until(session: &mut Session, seen: &mut Vec<Event>, wanted: impl Fn(&Event) -> bool) {
    loop {
        let event = session.next().await;
        let found = wanted(&event);
        seen.push(event);
        if found {
            return;
        }
    }
}

fn ringing(channel: ChannelId, people: &[UserId]) -> impl Fn(&Event) -> bool + '_ {
    move |event| {
        *event
            == Event::PrivateCall(PrivateCall {
                channel,
                ringing: people.to_vec(),
            })
    }
}

#[tokio::test(start_paused = true)]
async fn whom_the_account_rings_answers_declines_or_lets_it_ring_and_a_call_ends_empty() {
    let mut session = Session::start(MockProvider::new(config())).await;
    let mut seen = Vec::new();
    // The first person answers, and hangs up after a while.
    let first = direct(&session, 0);
    let ada = UserId(10);
    session
        .send(Command::JoinVoice {
            channel: first,
            muted: false,
            deafened: false,
        })
        .await;
    until(&mut session, &mut seen, ringing(first, &[])).await;
    // Alone in it, the call waits for the others (voz-protocolo).
    assert!(seen.iter().any(|event| matches!(
        event,
        Event::CallPhase {
            phase: CallPhase::Waiting,
            ..
        }
    )));
    session
        .send(Command::RingCall {
            channel: first,
            users: Vec::new(),
        })
        .await;
    until(&mut session, &mut seen, ringing(first, &[ada])).await;
    let rung = tokio::time::Instant::now();
    until(&mut session, &mut seen, ringing(first, &[])).await;
    until(&mut session, &mut seen, |event| {
        matches!(event, Event::VoiceChanged(state) if state.user == ada && state.channel == first)
    })
    .await;
    // With her in it, it no longer waits.
    until(&mut session, &mut seen, |event| {
        matches!(event, Event::CallPhase { channel, phase: CallPhase::Connected, .. } if *channel == first)
    })
    .await;
    assert!(rung.elapsed() >= Duration::from_secs(3));
    until(&mut session, &mut seen, |event| {
        *event
            == Event::VoiceLeft {
                user: ada,
                channel: first,
            }
    })
    .await;
    assert!(rung.elapsed() >= Duration::from_secs(45), "talked a while");
    // Leaving the call empty ends it.
    session.send(Command::LeaveVoice).await;
    until(&mut session, &mut seen, |event| {
        *event == Event::PrivateCallEnded { channel: first }
    })
    .await;
    // The second declines soon, the third lets it ring until the service gives up.
    for (person, after) in [(1, 2), (2, 30)] {
        let channel = direct(&session, person);
        let user = UserId(10 + person);
        session
            .send(Command::JoinVoice {
                channel,
                muted: false,
                deafened: false,
            })
            .await;
        until(&mut session, &mut seen, ringing(channel, &[])).await;
        session
            .send(Command::RingCall {
                channel,
                users: Vec::new(),
            })
            .await;
        until(&mut session, &mut seen, ringing(channel, &[user])).await;
        let rung = tokio::time::Instant::now();
        seen.clear();
        until(&mut session, &mut seen, ringing(channel, &[])).await;
        let waited = rung.elapsed();
        assert!(
            waited >= Duration::from_secs(after) && waited < Duration::from_secs(after + 1),
            "{person}: {waited:?}"
        );
        assert!(
            !seen
                .iter()
                .any(|event| matches!(event, Event::VoiceChanged(state) if state.user == user)),
            "{person} never came"
        );
        session.send(Command::LeaveVoice).await;
        until(&mut session, &mut seen, |event| {
            *event == Event::PrivateCallEnded { channel }
        })
        .await;
    }
    // Ringing outside the call is refused.
    session
        .send(Command::RingCall {
            channel: first,
            users: Vec::new(),
        })
        .await;
    until(&mut session, &mut seen, |event| {
        matches!(
            event,
            Event::Failed {
                failure: Failure::Ring { .. },
                ..
            }
        )
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn someone_calls_the_account_which_declines_or_answers_on_another_device() {
    let provider = MockProvider::new(config());
    let control = provider.control();
    let mut session = Session::start(provider).await;
    let mut seen = Vec::new();
    let first = direct(&session, 0);
    let ada = UserId(10);
    control.ring_me();
    until(&mut session, &mut seen, ringing(first, &[ME])).await;
    assert!(seen.iter().any(|event| {
        matches!(event, Event::VoiceChanged(state) if state.user == ada && state.channel == first)
    }));
    // Declined: the caller gives up a moment later, and the call is over.
    session
        .send(Command::StopRinging {
            channel: first,
            users: vec![ME],
        })
        .await;
    until(&mut session, &mut seen, ringing(first, &[])).await;
    until(&mut session, &mut seen, |event| {
        *event == Event::PrivateCallEnded { channel: first }
    })
    .await;
    // Called again and answered elsewhere: the account is in it, not from here.
    control.ring_me();
    until(&mut session, &mut seen, ringing(first, &[ME])).await;
    control.answer_elsewhere();
    until(&mut session, &mut seen, ringing(first, &[])).await;
    until(&mut session, &mut seen, |event| {
        matches!(event, Event::VoiceChanged(state) if state.user == ME && state.channel == first)
    })
    .await;
    assert!(!seen
        .iter()
        .any(|event| matches!(event, Event::CallPhase { channel, .. } if *channel == first)));
    // Joining it from here takes it.
    session
        .send(Command::JoinVoice {
            channel: first,
            muted: false,
            deafened: false,
        })
        .await;
    until(&mut session, &mut seen, |event| {
        matches!(event, Event::CallPhase { channel, phase: CallPhase::Connected, .. } if *channel == first)
    })
    .await;
}

#[tokio::test(start_paused = true)]
async fn a_call_nobody_answers_ends_when_its_caller_gives_up() {
    let provider = MockProvider::new(config());
    let control = provider.control();
    let mut session = Session::start(provider).await;
    let mut seen = Vec::new();
    let first = direct(&session, 0);
    control.ring_me();
    until(&mut session, &mut seen, ringing(first, &[ME])).await;
    let rang = tokio::time::Instant::now();
    until(&mut session, &mut seen, |event| {
        *event == Event::PrivateCallEnded { channel: first }
    })
    .await;
    assert!(
        rang.elapsed() >= Duration::from_secs(29),
        "{:?}",
        rang.elapsed()
    );
}

#[tokio::test(start_paused = true)]
async fn a_call_answered_here_goes_on_past_the_ring() {
    let provider = MockProvider::new(config());
    let control = provider.control();
    let mut session = Session::start(provider).await;
    let mut seen = Vec::new();
    let first = direct(&session, 0);
    control.ring_me();
    until(&mut session, &mut seen, ringing(first, &[ME])).await;
    session
        .send(Command::JoinVoice {
            channel: first,
            muted: false,
            deafened: false,
        })
        .await;
    until(&mut session, &mut seen, ringing(first, &[])).await;
    tokio::time::sleep(Duration::from_secs(40)).await;
    while let Ok(envelope) = session.events.try_recv() {
        assert!(
            !matches!(
                envelope.event,
                Event::VoiceLeft { .. } | Event::PrivateCallEnded { .. }
            ),
            "{:?}",
            envelope.event
        );
    }
}

/// The direct conversation with `user`, opened as the interface opens one.
async fn opened(session: &mut Session, seen: &mut Vec<Event>, user: UserId) -> ChannelId {
    session.send(Command::OpenDirect { request: 7, user }).await;
    until(session, seen, |event| {
        matches!(event, Event::Created { request: 7, .. })
    })
    .await;
    match seen.last() {
        Some(Event::Created {
            created: crate::Created::Channel(channel),
            ..
        }) => *channel,
        other => panic!("{other:?}"),
    }
}

#[tokio::test(start_paused = true)]
async fn a_direct_call_rings_a_friend_starts_silent_with_a_request_and_is_refused_otherwise() {
    let mut session = Session::start(MockProvider::new(config())).await;
    let mut seen = Vec::new();
    let friend = direct(&session, 0);
    session.send(Command::CheckCall { channel: friend }).await;
    until(&mut session, &mut seen, |event| {
        *event
            == Event::CallChecked {
                channel: friend,
                ringable: true,
            }
    })
    .await;
    // Someone who asked to be friends: the call starts, ringing nobody.
    let asked = opened(&mut session, &mut seen, UserId(19)).await;
    session.send(Command::CheckCall { channel: asked }).await;
    until(&mut session, &mut seen, |event| {
        *event
            == Event::CallChecked {
                channel: asked,
                ringable: false,
            }
    })
    .await;
    // Someone the account is not friends with: no call at all.
    let stranger = opened(&mut session, &mut seen, UserId(13)).await;
    session.send(Command::CheckCall { channel: stranger }).await;
    until(&mut session, &mut seen, |event| {
        matches!(
            event,
            Event::Failed {
                failure: Failure::CallCheck {
                    channel,
                    refused: true
                },
                ambiguous: false,
                ..
            } if *channel == stranger
        )
    })
    .await;
    // A group is not asked about: it fails too.
    let group = session
        .snapshot
        .channels
        .iter()
        .find(|channel| channel.kind == ChannelKind::Group)
        .unwrap()
        .id;
    session.send(Command::CheckCall { channel: group }).await;
    until(&mut session, &mut seen, |event| {
        matches!(
            event,
            Event::Failed {
                failure: Failure::CallCheck {
                    channel,
                    refused: false
                },
                ..
            } if *channel == group
        )
    })
    .await;
}
