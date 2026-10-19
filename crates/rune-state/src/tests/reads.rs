use super::*;
use rune_model::ReadMark;

fn supported() -> Store {
    let mut store = ready();
    store.provider = Some(rune_provider::ProviderInfo {
        name: "Lecturas simuladas".into(),
        kind: rune_provider::ProviderKind::Simulated,
        capabilities: rune_provider::Capabilities {
            read_state: true,
            ..Default::default()
        },
        notice: "Fixture".into(),
    });
    store
}

fn command(store: &mut Store) -> (u64, MessageId) {
    let Some(Command::MarkRead {
        request,
        account: ME,
        channel: GENERAL,
        message,
    }) = store.mark_read_command(GENERAL)
    else {
        panic!("expected a scoped read command")
    };
    (request, message)
}

fn ack(store: &mut Store, request: Option<u64>, message: MessageId, version: u64) -> Applied {
    store.apply(
        Event::ReadStateConfirmed {
            account: ME,
            request,
            channel: GENERAL,
            message,
            version,
        },
        Instant::now(),
    )
}

#[test]
fn read_requests_use_the_reserved_namespace_and_survive_account_reset() {
    let mut store = supported();
    let (first, _) = command(&mut store);
    assert!(first >= 1 << 63);
    store.apply(
        Event::Authentication(AuthenticationState::SignedOut),
        Instant::now(),
    );
    store.apply(Event::Ready(Box::new(snapshot())), Instant::now());
    let (next, _) = command(&mut store);
    assert!(next > first);
}

#[test]
fn read_cursor_reminder_and_confirmation_are_independent() {
    let mut store = supported();
    let (request, target) = command(&mut store);
    assert_eq!(store.read(GENERAL).last_read, Some(MessageId(5)));
    assert_eq!(store.read(GENERAL).confirmed_read, Some(MessageId(3)));
    assert_eq!(store.mark_unread_command(GENERAL, MessageId(4)), None);
    ack(&mut store, Some(request), target, 1);
    let read = store.read(GENERAL);
    assert_eq!(read.confirmed_read, Some(MessageId(5)));
    assert_eq!(read.last_read, Some(MessageId(5)));
    assert_eq!(read.unread_from, Some(MessageId(4)));
    assert!(read.unread());
}

#[test]
fn old_or_wrong_ack_never_rewinds_or_completes_another_pending_read() {
    let mut store = supported();
    let (request, target) = command(&mut store);
    for event in [
        Event::ReadStateConfirmed {
            account: ADA,
            request: Some(request),
            channel: GENERAL,
            message: target,
            version: 2,
        },
        Event::ReadStateConfirmed {
            account: ME,
            request: Some(request + 1),
            channel: GENERAL,
            message: target,
            version: 2,
        },
        Event::ReadStateConfirmed {
            account: ME,
            request: Some(request),
            channel: GENERAL,
            message: MessageId(4),
            version: 2,
        },
    ] {
        assert_eq!(store.apply(event, Instant::now()), Applied::Nothing);
    }
    assert_eq!(
        ack(&mut store, Some(request), target, 2),
        Applied::ReadState(GENERAL)
    );
    assert_eq!(ack(&mut store, None, MessageId(4), 3), Applied::Nothing);
    assert_eq!(ack(&mut store, None, MessageId(6), 1), Applied::Nothing);
    assert_eq!(store.read(GENERAL).confirmed_read, Some(target));
}

#[test]
fn a_read_ack_preserves_newer_messages_and_mentions() {
    let mut store = supported();
    let (request, target) = command(&mut store);
    store.apply(
        Event::MessageCreated {
            message: Message {
                mentions: vec![ME],
                ..message(6, ADA)
            },
            nonce: None,
        },
        Instant::now(),
    );
    ack(&mut store, Some(request), target, 1);
    assert!(store.read(GENERAL).unread());
    assert_eq!(store.read(GENERAL).mentions, 1);
    assert_eq!(store.read(GENERAL).last_read, Some(MessageId(5)));
}

#[test]
fn failed_read_retries_only_on_explicit_action() {
    let mut store = supported();
    let (request, _) = command(&mut store);
    store.apply(
        Event::Failed {
            failure: Failure::ReadState {
                request,
                channel: GENERAL,
            },
            reason: "offline".into(),
            ambiguous: false,
        },
        Instant::now(),
    );
    assert_eq!(store.mark_read_visible_command(GENERAL), None);
    let (retry, _) = command(&mut store);
    assert_ne!(request, retry);
    assert_eq!(
        store.apply(
            Event::Failed {
                failure: Failure::ReadState {
                    request,
                    channel: GENERAL
                },
                reason: "stale".into(),
                ambiguous: false
            },
            Instant::now()
        ),
        Applied::Nothing
    );
    assert_eq!(
        ack(&mut store, Some(retry), MessageId(5), 1),
        Applied::ReadState(GENERAL)
    );
}

#[test]
fn duplicate_created_and_own_messages_do_not_consume_unread_state() {
    let mut store = ready();
    let mention = Message {
        mentions: vec![ME],
        ..message(6, ADA)
    };
    for _ in 0..2 {
        store.apply(
            Event::MessageCreated {
                message: mention.clone(),
                nonce: None,
            },
            Instant::now(),
        );
    }
    assert_eq!(store.read(GENERAL).mentions, 2);
    store.apply(
        Event::MessageCreated {
            message: message(7, ME),
            nonce: None,
        },
        Instant::now(),
    );
    assert_eq!(store.read(GENERAL).last_read, Some(MessageId(3)));
    assert_eq!(store.read(GENERAL).mentions, 2);
    assert!(store.read(GENERAL).unread());
}

#[test]
fn local_marks_restore_only_the_same_account_without_overwriting_new_choices() {
    let mut store = ready();
    let saved = ReadMark {
        through: Some(MessageId(5)),
        unread_from: Some(MessageId(4)),
        revision: 8,
    };
    store.restore_read_marks(ADA, vec![(GENERAL, saved)]);
    assert_eq!(store.read(GENERAL).last_read, Some(MessageId(3)));
    store.restore_read_marks(ME, vec![(GENERAL, saved)]);
    assert_eq!(store.read(GENERAL).last_read, Some(MessageId(5)));
    assert!(store.read(GENERAL).unread());
    assert!(store.dirty_read_marks().is_empty());
    store.mark_unread(GENERAL, MessageId(2));
    store.restore_read_marks(ME, vec![(GENERAL, saved)]);
    assert_eq!(store.read(GENERAL).unread_from, Some(MessageId(2)));
    assert_eq!(store.dirty_read_marks().len(), 1);
}

#[test]
fn saved_revision_cannot_clear_a_newer_dirty_choice_and_forget_stays_forgotten() {
    let mut store = ready();
    store.mark_unread(GENERAL, MessageId(4));
    let first = store.dirty_read_marks().pop().unwrap().1;
    store.mark_read(GENERAL);
    store.read_mark_saved(GENERAL, first);
    let next = store.dirty_read_marks().pop().unwrap().1;
    assert_ne!(first.revision, next.revision);
    store.discard_dirty_read_marks();
    assert!(store.dirty_read_marks().is_empty());
    store.mark_unread(GENERAL, MessageId(4));
    assert_eq!(store.dirty_read_marks().len(), 1);
}

#[test]
fn reconnect_ready_preserves_local_reminders_but_new_account_does_not() {
    let mut store = ready();
    store.mark_read(GENERAL);
    store.mark_unread(GENERAL, MessageId(4));
    store.apply(Event::Ready(Box::new(snapshot())), Instant::now());
    assert_eq!(store.read(GENERAL).last_read, Some(MessageId(5)));
    assert_eq!(store.read(GENERAL).unread_from, Some(MessageId(4)));
    let mut other = snapshot();
    other.me = Some(user(ADA, "otra cuenta"));
    store.apply(Event::Ready(Box::new(other)), Instant::now());
    assert_eq!(store.read(GENERAL).unread_from, None);
    assert!(store.dirty_read_marks().is_empty());
}

#[test]
fn snapshot_versions_and_external_confirmations_are_monotonic() {
    let mut store = supported();
    assert_eq!(
        ack(&mut store, None, MessageId(4), 5),
        Applied::ReadState(GENERAL)
    );
    let mut old = snapshot();
    old.read_versions = vec![(GENERAL, 4)];
    store.apply(Event::Ready(Box::new(old)), Instant::now());
    assert_eq!(store.read(GENERAL).confirmed_read, Some(MessageId(4)));
    assert_eq!(store.read(GENERAL).confirmed_version, 5);
    assert_eq!(store.read(GENERAL).last_read, Some(MessageId(4)));
}

#[test]
fn first_unread_is_a_real_bounded_anchor_and_clears_only_when_covered() {
    let mut store = Store::new();
    let mut initial = snapshot();
    initial.first_unread = vec![(GENERAL, MessageId(4)), (VOICE, MessageId(999))];
    store.apply(Event::Ready(Box::new(initial)), Instant::now());
    assert_eq!(store.read(GENERAL).first_unread, Some(MessageId(4)));
    assert_eq!(store.read(VOICE).first_unread, None);
    store.mark_read(GENERAL);
    assert_eq!(store.read(GENERAL).first_unread, None);
    store.mark_unread(GENERAL, MessageId(4));
    assert_eq!(store.read(GENERAL).unread_from, Some(MessageId(4)));
    assert_eq!(store.read(GENERAL).first_unread, None);
}

#[test]
fn mention_totals_saturate_instead_of_overflowing() {
    let mut initial = snapshot();
    let other = ChannelId(12);
    initial
        .channels
        .push(channel(other, ChannelKind::Text, "otro"));
    initial.read_states = vec![
        (GENERAL, Some(MessageId(3)), u32::MAX),
        (other, None, u32::MAX),
    ];
    initial.last_messages.push((other, MessageId(5)));
    let mut store = Store::new();
    store.apply(Event::Ready(Box::new(initial)), Instant::now());
    assert_eq!(store.server_unread(ServerId(1)).1, u32::MAX);
    store.apply(
        Event::MessageCreated {
            message: Message {
                mentions: vec![ME],
                ..message(6, ADA)
            },
            nonce: None,
        },
        Instant::now(),
    );
    assert_eq!(store.read(GENERAL).mentions, u32::MAX);
}

#[test]
fn loading_history_with_a_pending_send_never_creates_a_local_message_read_cursor() {
    let mut store = ready();
    store.add_pending(GENERAL, "borrador enviado".into(), None, 77, Timestamp(100));
    history(&mut store, None, 1..6, true);
    assert_eq!(store.read(GENERAL).last_message, Some(MessageId(5)));
    assert_eq!(store.mark_read(GENERAL), Some(MessageId(5)));
}

#[test]
fn an_idempotent_ack_may_cover_more_than_the_requested_cursor() {
    let mut store = supported();
    let (request, target) = command(&mut store);
    assert_eq!(target, MessageId(5));
    assert_eq!(
        ack(&mut store, Some(request), MessageId(7), 2),
        Applied::ReadState(GENERAL)
    );
    assert_eq!(store.read(GENERAL).confirmed_read, Some(MessageId(7)));
    assert!(!store.read(GENERAL).sync_pending());
}

#[test]
fn equal_positive_snapshot_versions_cannot_claim_different_read_progress() {
    let mut store = supported();
    ack(&mut store, None, MessageId(4), 2);
    let mut conflicting = snapshot();
    conflicting.read_states = vec![(GENERAL, Some(MessageId(5)), 0)];
    conflicting.read_versions = vec![(GENERAL, 2)];
    store.apply(Event::Ready(Box::new(conflicting)), Instant::now());
    assert_eq!(store.read(GENERAL).confirmed_read, Some(MessageId(4)));
    assert_eq!(store.read(GENERAL).last_read, Some(MessageId(4)));
}

#[test]
fn reconnect_does_not_reintroduce_mentions_already_covered_by_a_local_read() {
    let mut store = ready();
    store.mark_read(GENERAL);
    store.apply(
        Event::MessageCreated {
            message: Message {
                mentions: vec![ME],
                ..message(6, ADA)
            },
            nonce: None,
        },
        Instant::now(),
    );
    assert_eq!(store.read(GENERAL).mentions, 1);
    let mut stale = snapshot();
    stale.read_states = vec![(GENERAL, Some(MessageId(3)), 10)];
    stale.last_messages = vec![(GENERAL, MessageId(6))];
    store.apply(Event::Ready(Box::new(stale)), Instant::now());
    assert_eq!(store.read(GENERAL).mentions, 1);
}

#[test]
fn matching_ack_with_a_conflicting_old_version_finishes_with_a_retryable_failure() {
    let mut store = supported();
    ack(&mut store, None, MessageId(4), 1);
    let (request, target) = command(&mut store);
    assert_eq!(target, MessageId(5));
    let applied = ack(&mut store, Some(request), target, 1);
    assert!(
        matches!(applied, Applied::Failed { failure: Failure::ReadState { request: found, channel: GENERAL }, ambiguous: true, .. } if found == request)
    );
    assert!(!store.read(GENERAL).sync_pending());
    assert!(store.read(GENERAL).sync_failed());
    assert_eq!(store.mark_read_visible_command(GENERAL), None);
    let (retry, _) = command(&mut store);
    assert_ne!(retry, request);
}

#[test]
fn a_deleted_first_unread_is_replaced_by_the_fresh_real_anchor_on_ready() {
    let mut store = Store::new();
    let mut initial = snapshot();
    initial.first_unread = vec![(GENERAL, MessageId(4))];
    initial.read_versions = vec![(GENERAL, 1)];
    store.apply(Event::Ready(Box::new(initial.clone())), Instant::now());
    store.apply(
        Event::MessageDeleted {
            channel: GENERAL,
            message: MessageId(4),
        },
        Instant::now(),
    );
    assert_eq!(store.read(GENERAL).first_unread, None);
    initial.first_unread = vec![(GENERAL, MessageId(5))];
    store.apply(Event::Ready(Box::new(initial)), Instant::now());
    assert_eq!(store.read(GENERAL).first_unread, Some(MessageId(5)));
}

#[test]
fn a_reconnect_snapshot_replaces_the_first_unread_when_a_delete_was_missed() {
    let mut store = Store::new();
    let mut initial = snapshot();
    initial.first_unread = vec![(GENERAL, MessageId(4))];
    initial.read_versions = vec![(GENERAL, 1)];
    store.apply(Event::Ready(Box::new(initial.clone())), Instant::now());
    initial.first_unread = vec![(GENERAL, MessageId(5))];
    store.apply(Event::Ready(Box::new(initial)), Instant::now());
    assert_eq!(store.read(GENERAL).first_unread, Some(MessageId(5)));
}

#[test]
fn restored_unknown_channel_preserves_its_saved_clock_when_it_appears_later() {
    let mut store = ready();
    let later = ChannelId(99);
    let mark = ReadMark {
        through: Some(MessageId(500)),
        unread_from: Some(MessageId(400)),
        revision: 20,
    };
    store.restore_read_marks(ME, vec![(later, mark)]);
    assert!(!store.read_channels().any(|channel| channel.id == later));
    assert!(store.dirty_read_marks().is_empty());
    store.apply(
        Event::ChannelChanged(channel(later, ChannelKind::Text, "aparece después")),
        Instant::now(),
    );
    assert_eq!(store.read(later).unread_from, Some(MessageId(400)));
    store.mark_unread(later, MessageId(401));
    let saved = store
        .dirty_read_marks()
        .into_iter()
        .find(|(id, _)| *id == later)
        .unwrap()
        .1;
    assert_eq!(saved.revision, 21);
}

#[test]
fn a_ready_that_omits_a_channel_preserves_only_its_local_choice_and_clock() {
    let mut store = ready();
    store.mark_read(GENERAL);
    store.mark_unread(GENERAL, MessageId(4));
    let previous = store.dirty_read_marks().pop().unwrap().1;
    store.read_mark_saved(GENERAL, previous);
    let mut missing = snapshot();
    missing.channels.retain(|channel| channel.id != GENERAL);
    missing.read_states.clear();
    missing.last_messages.clear();
    store.apply(Event::Ready(Box::new(missing)), Instant::now());
    assert_eq!(store.read(GENERAL).unread_from, Some(MessageId(4)));
    assert_eq!(store.read(GENERAL).last_message, None);
    assert!(store.dirty_read_marks().is_empty());
    store.apply(
        Event::ChannelChanged(channel(GENERAL, ChannelKind::Text, "vuelve")),
        Instant::now(),
    );
    store.mark_unread(GENERAL, MessageId(2));
    assert!(store.dirty_read_marks().pop().unwrap().1.revision > previous.revision);
}

#[test]
fn channel_removal_and_reappearance_do_not_reset_persisted_local_revision() {
    let mut store = ready();
    store.mark_unread(GENERAL, MessageId(4));
    let previous = store.dirty_read_marks().pop().unwrap().1;
    store.read_mark_saved(GENERAL, previous);
    store.apply(Event::ChannelRemoved { channel: GENERAL }, Instant::now());
    assert_eq!(store.read(GENERAL).unread_from, Some(MessageId(4)));
    assert_eq!(store.read(GENERAL).last_message, None);
    store.apply(
        Event::ChannelChanged(channel(GENERAL, ChannelKind::Text, "vuelve")),
        Instant::now(),
    );
    store.mark_unread(GENERAL, MessageId(2));
    assert!(store.dirty_read_marks().pop().unwrap().1.revision > previous.revision);
}

#[test]
fn removing_and_readding_a_channel_cannot_rewind_a_versioned_provider_cursor() {
    let mut store = supported();
    ack(&mut store, None, MessageId(5), 10);
    store.apply(Event::ChannelRemoved { channel: GENERAL }, Instant::now());
    let mut stale = snapshot();
    stale.read_versions = vec![(GENERAL, 2)];
    store.apply(Event::Ready(Box::new(stale)), Instant::now());
    assert_eq!(store.read(GENERAL).confirmed_read, Some(MessageId(5)));
    assert_eq!(store.read(GENERAL).confirmed_version, 10);
}

#[test]
fn retry_confirms_only_existing_progress_and_preserves_new_mentions_and_reminders() {
    let mut store = supported();
    let (request, _) = command(&mut store);
    store.apply(
        Event::Failed {
            failure: Failure::ReadState {
                request,
                channel: GENERAL,
            },
            reason: "sin ACK".into(),
            ambiguous: true,
        },
        Instant::now(),
    );
    store.apply(
        Event::MessageCreated {
            message: Message {
                mentions: vec![ME],
                ..message(6, ADA)
            },
            nonce: None,
        },
        Instant::now(),
    );
    store.mark_unread(GENERAL, MessageId(4));
    assert!(matches!(
        store.retry_read_command(GENERAL),
        Some(Command::MarkRead {
            message: MessageId(5),
            ..
        })
    ));
    assert_eq!(store.read(GENERAL).last_read, Some(MessageId(5)));
    assert_eq!(store.read(GENERAL).unread_from, Some(MessageId(4)));
    assert_eq!(store.read(GENERAL).mentions, 1);
}

#[test]
fn maximum_local_revision_still_tracks_choices_and_old_save_cannot_clear_a_new_choice() {
    let mut store = ready();
    let original = ReadMark {
        through: Some(MessageId(5)),
        unread_from: Some(MessageId(4)),
        revision: u64::MAX,
    };
    store.restore_read_marks(ME, vec![(GENERAL, original)]);
    store.mark_unread(GENERAL, MessageId(2));
    let changed = store.dirty_read_marks().pop().unwrap().1;
    assert_eq!(changed.revision, u64::MAX);
    assert_eq!(changed.unread_from, Some(MessageId(2)));
    store.read_mark_saved(GENERAL, original);
    assert_eq!(store.dirty_read_marks(), vec![(GENERAL, changed)]);
    store.read_mark_saved(GENERAL, changed);
    assert!(store.dirty_read_marks().is_empty());
    store.mark_unread(GENERAL, MessageId(3));
    assert_eq!(store.dirty_read_marks().pop().unwrap().1.revision, u64::MAX);
    store.discard_dirty_read_marks();
    assert!(store.dirty_read_marks().is_empty());
}

#[test]
fn a_pending_maximum_revision_ack_preserves_a_new_maximum_revision_reminder() {
    let mut store = supported();
    let original = ReadMark {
        through: Some(MessageId(5)),
        unread_from: Some(MessageId(4)),
        revision: u64::MAX,
    };
    store.restore_read_marks(ME, vec![(GENERAL, original)]);
    let Some(Command::MarkRead { request, .. }) = store.retry_read_command(GENERAL) else {
        panic!("retry")
    };
    store.mark_unread(GENERAL, MessageId(2));
    assert_eq!(store.dirty_read_marks().pop().unwrap().1.revision, u64::MAX);
    ack(&mut store, Some(request), MessageId(5), 1);
    assert_eq!(store.read(GENERAL).unread_from, Some(MessageId(2)));
    assert!(store.read(GENERAL).unread());
    assert!(!store.read(GENERAL).sync_pending());
    store.read_mark_saved(GENERAL, original);
    assert_eq!(store.dirty_read_marks().len(), 1);
}
