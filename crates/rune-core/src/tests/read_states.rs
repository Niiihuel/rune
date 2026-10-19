use super::*;
use rune_model::UserId;

fn confirmation(request: Option<u64>, channel: ChannelId, message: MessageId) -> Event {
    Event::ReadStateConfirmed {
        account: UserId(1),
        request,
        channel,
        message,
        version: 1,
    }
}

#[test]
fn read_receipts_require_admitted_scope_current_account_and_visibility() {
    let mut client = client(Arc::new(mock()), None);
    let channel = text_channels(&ready(&mut client))[0];
    let mut apply = |event| {
        client.accept(
            Envelope {
                generation: client.generation,
                event,
            },
            &mut Vec::new(),
        )
    };
    assert!(apply(confirmation(Some(17), channel, MessageId(100))).is_none());
    let mut wrong_account = confirmation(None, channel, MessageId(100));
    if let Event::ReadStateConfirmed { account, .. } = &mut wrong_account {
        *account = UserId(99);
    }
    assert!(apply(wrong_account).is_none());
    assert!(apply(confirmation(None, ChannelId(u64::MAX), MessageId(100))).is_none());
    assert!(apply(confirmation(None, channel, MessageId(100))).is_some());
}

#[test]
fn full_read_queue_failure_does_not_replace_a_previously_admitted_scope() {
    let mut client = client(Arc::new(mock()), None);
    let channel = text_channels(&ready(&mut client))[0];
    let (sender, mut receiver) = mpsc::channel(1);
    client.commands = sender;
    let command = |request, id| Command::MarkRead {
        request,
        account: UserId(1),
        channel,
        message: MessageId(id),
    };
    client.command(command(10, 100));
    client.command(command(11, 101));
    assert!(matches!(
        client.found.pop_back(),
        Some(Event::Failed {
            failure: Failure::ReadState { request: 11, .. },
            ambiguous: false,
            ..
        })
    ));
    assert_eq!(receiver.try_recv().unwrap(), command(10, 100));
    assert!(client
        .accept(
            Envelope {
                generation: client.generation,
                event: confirmation(Some(11), channel, MessageId(101)),
            },
            &mut Vec::new()
        )
        .is_none());
    assert!(
        client
            .accept(
                Envelope {
                    generation: client.generation,
                    event: confirmation(Some(10), channel, MessageId(99)),
                },
                &mut Vec::new()
            )
            .is_none(),
        "a short cursor cannot acknowledge the target"
    );
    assert!(client
        .accept(
            Envelope {
                generation: client.generation,
                event: confirmation(Some(10), channel, MessageId(100)),
            },
            &mut Vec::new()
        )
        .is_some());
}

#[test]
fn read_timeout_is_terminal_ambiguous_and_late_receipts_are_dropped() {
    let mut client = client(Arc::new(mock()), None);
    let channel = text_channels(&ready(&mut client))[0];
    let (sender, _receiver) = mpsc::channel(1);
    client.commands = sender;
    client.command(Command::MarkRead {
        request: 10,
        account: UserId(1),
        channel,
        message: MessageId(100),
    });
    let deadline = client.next_deadline(Instant::now()).unwrap();
    client.expire(Instant::now() + deadline + Duration::from_millis(1));
    assert!(matches!(
        client.found.pop_back(),
        Some(Event::Failed {
            failure: Failure::ReadState { request: 10, .. },
            ambiguous: true,
            ..
        })
    ));
    assert!(client
        .accept(
            Envelope {
                generation: client.generation,
                event: confirmation(Some(10), channel, MessageId(100)),
            },
            &mut Vec::new()
        )
        .is_none());
}

#[test]
fn unsupported_reads_never_reach_the_provider() {
    let mut client = client(Arc::new(mock()), None);
    let channel = text_channels(&ready(&mut client))[0];
    let (sender, mut receiver) = mpsc::channel(1);
    client.commands = sender;
    client.info.capabilities.read_state = false;
    client.command(Command::MarkRead {
        request: 10,
        account: UserId(1),
        channel,
        message: MessageId(100),
    });
    assert!(receiver.try_recv().is_err());
    assert!(matches!(
        client.found.pop_back(),
        Some(Event::Failed {
            failure: Failure::ReadState { request: 10, .. },
            ambiguous: false,
            ..
        })
    ));
    assert!(client
        .accept(
            Envelope {
                generation: client.generation,
                event: confirmation(None, channel, MessageId(100)),
            },
            &mut Vec::new()
        )
        .is_none());
}

#[test]
fn reconnect_revocation_cancel_and_generation_end_only_the_admitted_read() {
    for transition in 0..4 {
        let provider = Arc::new(mock());
        let snapshot = provider.snapshot();
        let mut client = client(provider, None);
        let channel = text_channels(&ready(&mut client))[0];
        let (sender, mut receiver) = mpsc::channel(4);
        client.commands = sender;
        let command = Command::MarkRead {
            request: 10,
            account: UserId(1),
            channel,
            message: MessageId(100),
        };
        client.command(command.clone());
        client.command(command.clone());
        client.command(Command::MarkRead {
            request: 10,
            account: UserId(99),
            channel,
            message: MessageId(0),
        });
        assert_eq!(receiver.try_recv().unwrap(), command);
        assert!(
            receiver.try_recv().is_err(),
            "a replay cannot enqueue a second read or fail the first"
        );
        assert!(client.found.is_empty());
        match transition {
            0 => {
                client.accept(
                    Envelope {
                        generation: client.generation,
                        event: Event::Connection(ConnectionState::Reconnecting {
                            attempt: 1,
                            retry_in_ms: 10,
                        }),
                    },
                    &mut Vec::new(),
                );
            }
            1 => {
                client.accept(
                    Envelope {
                        generation: client.generation,
                        event: Event::ChannelRemoved { channel },
                    },
                    &mut Vec::new(),
                );
            }
            2 => client.command(Command::Cancel(10)),
            _ => {
                client.accept(
                    Envelope {
                        generation: client.generation + 1,
                        event: Event::Ready(Box::new(snapshot)),
                    },
                    &mut Vec::new(),
                );
            }
        }
        assert!(client
            .accept(
                Envelope {
                    generation: client.generation,
                    event: confirmation(Some(10), channel, MessageId(100)),
                },
                &mut Vec::new()
            )
            .is_none());
        assert!(client
            .read_receipts
            .deadline(client.deadlines.history)
            .is_none());
    }
}
