//! Correlation and lifetime of the single pins read, without a provider race.

use super::*;

fn harness() -> (Client, mpsc::Receiver<Command>, Vec<rune_model::Channel>) {
    let mut client = client(Arc::new(mock()), None);
    let channels = ready(&mut client);
    let (sender, receiver) = mpsc::channel(1);
    client.commands = sender;
    (client, receiver, channels)
}

fn accept(client: &mut Client, event: Event) -> Option<Event> {
    client.accept(
        Envelope {
            generation: client.generation,
            event,
        },
        &mut Vec::new(),
    )
}

fn loaded(channel: ChannelId, request: RequestId) -> Event {
    Event::PinsLoaded {
        channel,
        request,
        messages: Vec::new(),
    }
}

fn failed(channel: ChannelId, request: RequestId) -> Event {
    Event::Failed {
        failure: Failure::Pins { channel, request },
        reason: "fixture".into(),
        ambiguous: false,
    }
}

fn assert_failed(client: &mut Client, channel: ChannelId, request: RequestId) {
    assert!(matches!(
        client.found.pop_front(),
        Some(Event::Failed {
            failure: Failure::Pins { channel: actual, request: id },
            ambiguous: false,
            ..
        }) if actual == channel && id == request
    ));
    assert!(
        client.found.is_empty(),
        "the read has exactly one terminal result"
    );
}

#[test]
fn pins_only_the_current_admitted_channel_and_request_can_complete() {
    let (mut client, mut commands, channels) = harness();
    let (channel, other) = (channels[0].id, channels[1].id);
    assert!(accept(&mut client, loaded(channel, 90)).is_none());

    client.command(Command::LoadPins {
        channel,
        request: 90,
    });
    commands.try_recv().unwrap();
    assert!(accept(&mut client, loaded(other, 90)).is_none());
    assert!(accept(&mut client, failed(channel, 89)).is_none());
    client.command(Command::LoadPins {
        channel: other,
        request: 91,
    });
    commands.try_recv().unwrap();
    assert!(accept(&mut client, loaded(channel, 90)).is_none());
    assert!(accept(&mut client, failed(channel, 90)).is_none());
    assert_eq!(
        accept(&mut client, loaded(other, 91)),
        Some(loaded(other, 91))
    );
    assert!(accept(&mut client, loaded(other, 91)).is_none());
    assert_eq!(client.next_deadline(Instant::now()), None);

    client.command(Command::LoadPins {
        channel,
        request: 92,
    });
    assert_eq!(
        accept(&mut client, failed(channel, 92)),
        Some(failed(channel, 92))
    );
    assert!(accept(&mut client, failed(channel, 92)).is_none());
}

#[test]
fn pins_queue_rejection_keeps_the_previous_read_and_emits_its_own_failure() {
    let (mut client, mut commands, channels) = harness();
    let (channel, other) = (channels[0].id, channels[1].id);
    client.command(Command::LoadPins {
        channel,
        request: 100,
    });
    client.command(Command::LoadPins {
        channel: other,
        request: 101,
    });
    assert_failed(&mut client, other, 101);
    assert!(accept(&mut client, loaded(other, 101)).is_none());
    assert_eq!(
        accept(&mut client, loaded(channel, 100)),
        Some(loaded(channel, 100))
    );
    commands.try_recv().unwrap();
    drop(commands);
    client.command(Command::LoadPins {
        channel,
        request: 102,
    });
    assert_failed(&mut client, channel, 102);
    assert_eq!(client.next_deadline(Instant::now()), None);
    assert!(accept(&mut client, loaded(channel, 102)).is_none());
}

#[test]
fn pins_cancel_is_local_even_when_the_provider_queue_is_full() {
    let (mut client, mut commands, channels) = harness();
    let channel = channels[0].id;
    client.command(Command::LoadPins {
        channel,
        request: 110,
    });
    assert!(client.next_deadline(Instant::now()).is_some());
    client.command(Command::Cancel(109));
    assert!(client.next_deadline(Instant::now()).is_some());
    client.command(Command::Cancel(110));
    assert_eq!(client.next_deadline(Instant::now()), None);
    assert!(accept(&mut client, loaded(channel, 110)).is_none());
    assert_eq!(
        commands.try_recv().unwrap(),
        Command::LoadPins {
            channel,
            request: 110
        }
    );
    assert!(commands.try_recv().is_err());
    assert!(client.found.is_empty());
}

#[test]
fn pins_use_the_history_deadline_and_reject_late_answers_after_timeout() {
    let (mut client, mut commands, channels) = harness();
    let channel = channels[0].id;
    client.deadlines.history = Duration::from_secs(7);
    client.command(Command::LoadPins {
        channel,
        request: 120,
    });
    commands.try_recv().unwrap();
    assert!(client
        .next_deadline(Instant::now())
        .is_some_and(|left| left > Duration::from_secs(6) && left <= Duration::from_secs(7)));
    client.expire(Instant::now() + Duration::from_secs(8));
    assert_failed(&mut client, channel, 120);
    assert_eq!(client.next_deadline(Instant::now()), None);
    assert!(accept(&mut client, loaded(channel, 120)).is_none());
    client.expire(Instant::now() + Duration::from_secs(100));
    assert!(client.found.is_empty());
}

#[test]
fn pins_deadlines_pause_while_storage_prevents_delivery_of_the_answer() {
    let (mut client, mut commands, channels) = harness();
    let channel = channels[0].id;
    client.storage_paused_at = Some(Instant::now());
    client.command(Command::LoadPins {
        channel,
        request: 130,
    });
    commands.try_recv().unwrap();
    let resumed = Instant::now() + Duration::from_secs(100);
    client.expire(resumed);
    assert!(client.found.is_empty());
    assert_eq!(client.next_deadline(resumed), None);
    client.resume_storage_deadlines(resumed);
    assert_eq!(
        client.next_deadline(resumed),
        Some(client.deadlines.history)
    );
    client.expire(resumed + client.deadlines.history);
    assert_failed(&mut client, channel, 130);
}

#[test]
fn pins_connection_loss_finishes_the_read_with_a_correlated_failure() {
    for state in [
        ConnectionState::Connecting,
        ConnectionState::Disconnected {
            reason: "fixture".into(),
        },
        ConnectionState::Reconnecting {
            attempt: 1,
            retry_in_ms: 500,
        },
    ] {
        let (mut client, mut commands, channels) = harness();
        let channel = channels[0].id;
        client.command(Command::LoadPins {
            channel,
            request: 140,
        });
        commands.try_recv().unwrap();
        assert_eq!(
            accept(&mut client, Event::Connection(state.clone())),
            Some(Event::Connection(state))
        );
        assert_failed(&mut client, channel, 140);
        assert_eq!(client.next_deadline(Instant::now()), None);
        assert!(accept(&mut client, loaded(channel, 140)).is_none());
    }
}

#[test]
fn pins_session_boundaries_drop_answers_from_the_replaced_account() {
    for event in [
        Event::Authentication(AuthenticationState::SignedOut),
        Event::Authentication(AuthenticationState::Failed {
            reason: "fixture".into(),
        }),
        Event::Ready(Box::new(mock().snapshot())),
    ] {
        let (mut client, mut commands, channels) = harness();
        let channel = channels[0].id;
        client.command(Command::LoadPins {
            channel,
            request: 150,
        });
        commands.try_recv().unwrap();
        accept(&mut client, event);
        assert_eq!(client.next_deadline(Instant::now()), None);
        assert!(accept(&mut client, loaded(channel, 150)).is_none());
        assert!(
            client.found.is_empty(),
            "Ready and auth close the UI read themselves"
        );
    }
}

#[test]
fn pins_a_new_generation_cannot_complete_the_previous_sessions_read() {
    let (mut client, mut commands, channels) = harness();
    let channel = channels[0].id;
    client.command(Command::LoadPins {
        channel,
        request: 155,
    });
    commands.try_recv().unwrap();
    let generation = client.generation + 1;
    assert!(client
        .accept(
            Envelope {
                generation,
                event: loaded(channel, 155)
            },
            &mut Vec::new()
        )
        .is_none());
    assert_failed(&mut client, channel, 155);
}

#[test]
fn pins_removed_or_unreadable_channels_finish_their_read() {
    let (mut client, mut commands, channels) = harness();
    let channel = channels
        .iter()
        .find(|channel| channel.kind.has_messages())
        .unwrap()
        .clone();
    let mut revoked = channel.clone();
    revoked.permissions = revoked
        .permissions
        .without(rune_model::Permissions::READ_HISTORY);
    for event in [
        Event::ChannelChanged(revoked),
        Event::ChannelRemoved {
            channel: channel.id,
        },
    ] {
        client.command(Command::LoadPins {
            channel: channel.id,
            request: 160,
        });
        commands.try_recv().unwrap();
        accept(&mut client, event);
        assert_failed(&mut client, channel.id, 160);
        assert!(accept(&mut client, loaded(channel.id, 160)).is_none());
    }
}

#[test]
fn pins_sign_out_changes_local_lifetime_only_after_command_admission() {
    let (mut client, mut commands, channels) = harness();
    let channel = channels[0].id;
    client.command(Command::LoadPins {
        channel,
        request: 170,
    });
    client.command(Command::SignOut);
    assert!(client.next_deadline(Instant::now()).is_some());
    client.found.clear(); // The rejected sign-out failure is unrelated to the read.
    commands.try_recv().unwrap();
    client.command(Command::SignOut);
    assert_eq!(client.next_deadline(Instant::now()), None);
    assert!(accept(&mut client, loaded(channel, 170)).is_none());
}
