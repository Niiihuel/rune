use super::*;

fn message(channel: ChannelId, id: u64) -> Message {
    let provider = mock();
    let (mut messages, _) = provider.history(channel, None, 1);
    let mut message = messages.pop().unwrap();
    message.id = MessageId(id);
    message
}

#[test]
fn unsolicited_context_never_reaches_the_host() {
    let mut client = client(Arc::new(mock()), None);
    let channel = ChannelId(104);
    let event = Event::ContextLoaded {
        request: 77,
        channel,
        anchor: MessageId(1),
        messages: vec![message(channel, 1)],
    };
    assert!(client
        .accept(
            Envelope {
                generation: 0,
                event
            },
            &mut Vec::new()
        )
        .is_none());
}

#[test]
fn admitted_context_requires_exact_scope_anchor_and_limit() {
    let mut client = client(Arc::new(mock()), None);
    let channel = ChannelId(104);
    client.command(Command::LoadContext {
        request: 77,
        channel,
        anchor: MessageId(1),
        limit: 1,
    });
    let wrong = Event::ContextLoaded {
        request: 76,
        channel,
        anchor: MessageId(1),
        messages: vec![message(channel, 1)],
    };
    assert!(client
        .accept(
            Envelope {
                generation: 0,
                event: wrong
            },
            &mut Vec::new()
        )
        .is_none());
    let invalid = Event::ContextLoaded {
        request: 77,
        channel,
        anchor: MessageId(1),
        messages: vec![message(channel, 2)],
    };
    assert!(client
        .accept(
            Envelope {
                generation: 0,
                event: invalid
            },
            &mut Vec::new()
        )
        .is_none());
    assert!(client.found.iter().any(|event| matches!(event, Event::Failed { failure: Failure::Context { request: 77, channel: c }, ambiguous: false, .. } if *c == channel)));
}

#[test]
fn cancelled_context_cannot_be_restored_by_a_late_response() {
    let mut client = client(Arc::new(mock()), None);
    let channel = ChannelId(104);
    client.command(Command::LoadContext {
        request: 77,
        channel,
        anchor: MessageId(1),
        limit: 1,
    });
    client.command(Command::Cancel(77));
    let event = Event::ContextLoaded {
        request: 77,
        channel,
        anchor: MessageId(1),
        messages: vec![message(channel, 1)],
    };
    assert!(client
        .accept(
            Envelope {
                generation: 0,
                event
            },
            &mut Vec::new()
        )
        .is_none());
}

#[test]
fn context_timeout_is_terminal_and_nonambiguous() {
    let mut client = client(Arc::new(mock()), None);
    client.command(Command::LoadContext {
        request: 77,
        channel: ChannelId(104),
        anchor: MessageId(1),
        limit: 1,
    });
    client.expire(Instant::now() + client.deadlines.history);
    assert!(client.found.iter().any(|event| matches!(
        event,
        Event::Failed {
            failure: Failure::Context { request: 77, .. },
            ambiguous: false,
            ..
        }
    )));
}

#[test]
fn context_persists_rows_without_inventing_a_history_range() {
    let mut client = client(Arc::new(mock()), None);
    client.storage_active = true;
    let channel = ChannelId(104);
    client.command(Command::LoadContext {
        request: 77,
        channel,
        anchor: MessageId(2),
        limit: 3,
    });
    let event = Event::ContextLoaded {
        request: 77,
        channel,
        anchor: MessageId(2),
        messages: vec![
            message(channel, 1),
            message(channel, 2),
            message(channel, 3),
        ],
    };
    let mut saved = Vec::new();
    assert!(client
        .accept(
            Envelope {
                generation: 0,
                event
            },
            &mut saved
        )
        .is_some());
    assert_eq!(saved.len(), 3);
    assert!(saved
        .iter()
        .all(|change| matches!(change, MessageMutation::Upsert(_))));
}

#[test]
fn context_cannot_overwrite_a_patch_that_arrived_while_the_read_was_pending() {
    let mut client = client(Arc::new(mock()), None);
    client.storage_active = true;
    let channel = ChannelId(104);
    let original = message(channel, 1);
    client.command(Command::LoadContext {
        request: 78,
        channel,
        anchor: original.id,
        limit: 1,
    });
    let patch = rune_model::MessagePatch {
        channel,
        id: original.id,
        content: Some("confirmed newer content without edited timestamp".into()),
        pinned: Some(true),
        ..Default::default()
    };
    let mut saved = Vec::new();
    client.accept(
        Envelope {
            generation: 0,
            event: Event::MessagePatched(patch),
        },
        &mut saved,
    );
    assert_eq!(saved.len(), 1);
    saved.clear();
    let stale = Event::ContextLoaded {
        request: 78,
        channel,
        anchor: original.id,
        messages: vec![original],
    };
    assert!(client
        .accept(
            Envelope {
                generation: 0,
                event: stale
            },
            &mut saved
        )
        .is_none());
    assert!(
        saved.is_empty(),
        "the stale snapshot must never reach SQLite"
    );
    assert!(client.found.iter().any(|event| matches!(
        event,
        Event::Failed {
            failure: Failure::Context { request: 78, .. },
            ambiguous: false,
            ..
        }
    )));
}

#[test]
fn a_live_reaction_while_the_read_was_pending_fails_the_context() {
    let mut client = client(Arc::new(mock()), None);
    client.storage_active = true;
    let channel = ChannelId(104);
    let original = message(channel, 1);
    client.command(Command::LoadContext {
        request: 79,
        channel,
        anchor: original.id,
        limit: 1,
    });
    let mut saved = Vec::new();
    client.accept(
        Envelope {
            generation: 0,
            event: Event::ReactionChanged {
                channel,
                message: original.id,
                emoji: "🦀".into(),
                me: false,
                added: true,
            },
        },
        &mut saved,
    );
    saved.clear();
    let stale = Event::ContextLoaded {
        request: 79,
        channel,
        anchor: original.id,
        messages: vec![original],
    };
    assert!(client
        .accept(
            Envelope {
                generation: 0,
                event: stale
            },
            &mut saved
        )
        .is_none());
    assert!(saved.is_empty(), "the snapshot may miss the reaction");
    assert!(client.found.iter().any(|event| matches!(
        event,
        Event::Failed {
            failure: Failure::Context { request: 79, .. },
            ..
        }
    )));
}
