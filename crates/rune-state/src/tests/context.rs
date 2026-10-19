use super::*;
use rune_model::{CachedContext, HistorySource, MessagePatch};

fn cached(anchor: u64, ids: impl IntoIterator<Item = u64>) -> CachedContext {
    CachedContext {
        anchor: MessageId(anchor),
        messages: ids.into_iter().map(|id| message(id, ADA)).collect(),
        has_older_cached: true,
        has_newer_cached: true,
    }
}

fn load(
    store: &mut Store,
    request: u64,
    anchor: u64,
    ids: impl IntoIterator<Item = u64>,
) -> Applied {
    store.apply_cached_context(request, GENERAL, cached(anchor, ids))
}

fn assert_failed(applied: Applied, request: u64) {
    assert!(
        matches!(applied, Applied::Failed { failure: Failure::Context { channel: GENERAL, request: found }, ambiguous: false, .. } if found == request),
        "{applied:?}"
    );
}

#[test]
fn cached_context_replaces_window_without_claiming_coverage_or_reading_it() {
    let mut store = pins_ready();
    history(&mut store, None, 50..60, true);
    let read = store.read(GENERAL);
    assert!(store
        .begin_context(GENERAL, 1, MessageId(20), HistorySource::Cached)
        .allowed());
    assert_eq!(
        load(&mut store, 1, 20, [10, 20, 40]),
        Applied::Context {
            channel: GENERAL,
            request: 1,
            anchor: MessageId(20)
        }
    );
    let timeline = store.timeline(GENERAL).unwrap();
    assert_eq!(timeline.len(), 3);
    assert_eq!(timeline.context_source(), Some(HistorySource::Cached));
    assert!(!timeline.latest && !timeline.reached_start);
    assert_eq!(store.read(GENERAL).last_read, read.last_read);
    assert_eq!(store.read(GENERAL).last_message, read.last_message);
    assert_eq!(store.read(GENERAL).mentions, read.mentions);
}

#[test]
fn cancelled_or_replaced_context_results_and_failures_are_ignored() {
    let mut store = pins_ready();
    assert!(store
        .begin_context(GENERAL, 1, MessageId(20), HistorySource::Cached)
        .allowed());
    assert!(store
        .begin_context(GENERAL, 2, MessageId(30), HistorySource::Cached)
        .allowed());
    store.cancel_context(1);
    assert_eq!(load(&mut store, 1, 20, [20]), Applied::Nothing);
    assert_eq!(
        store.apply(
            Event::Failed {
                failure: Failure::Context {
                    channel: VOICE,
                    request: 2
                },
                reason: "old".into(),
                ambiguous: false
            },
            Instant::now()
        ),
        Applied::Nothing
    );
    assert_eq!(
        load(&mut store, 2, 30, [30]),
        Applied::Context {
            channel: GENERAL,
            request: 2,
            anchor: MessageId(30)
        }
    );
    assert_eq!(load(&mut store, 2, 30, [30]), Applied::Nothing);
    assert!(store
        .begin_context(GENERAL, 3, MessageId(40), HistorySource::Cached)
        .allowed());
    store.cancel_context(3);
    assert_eq!(load(&mut store, 3, 40, [40]), Applied::Nothing);
}

#[test]
fn mutations_during_context_read_require_an_explicit_retry_without_retaining_payloads() {
    for event in [
        Event::MessageCreated {
            message: message(100, ADA),
            nonce: None,
        },
        Event::MessageUpdated(message(20, ADA)),
        Event::MessagePatched(MessagePatch {
            channel: GENERAL,
            id: MessageId(20),
            reactions: Some(Vec::new()),
            ..Default::default()
        }),
        Event::MessageDeleted {
            channel: GENERAL,
            message: MessageId(20),
        },
        // Live reactions and votes change a neighbor too (2026-10-18).
        Event::ReactionChanged {
            channel: GENERAL,
            message: MessageId(21),
            emoji: "🦀".into(),
            me: false,
            added: true,
        },
        Event::BurstReactionChanged {
            channel: GENERAL,
            message: MessageId(21),
            emoji: "🦀".into(),
            me: false,
            added: true,
            colors: Vec::new(),
        },
        Event::PollVoteChanged {
            channel: GENERAL,
            message: MessageId(21),
            answer: 1,
            me: false,
            added: true,
        },
        Event::ReactionsCleared {
            channel: GENERAL,
            message: MessageId(21),
            emoji: None,
        },
    ] {
        let mut store = pins_ready();
        assert!(store
            .begin_context(GENERAL, 1, MessageId(20), HistorySource::Cached)
            .allowed());
        store.apply(event, Instant::now());
        assert_failed(load(&mut store, 1, 20, [20]), 1);
        assert!(store
            .begin_context(GENERAL, 2, MessageId(30), HistorySource::Cached)
            .allowed());
        assert_eq!(
            load(&mut store, 2, 30, [30]),
            Applied::Context {
                channel: GENERAL,
                request: 2,
                anchor: MessageId(30)
            }
        );
    }
}

#[test]
fn return_to_latest_drops_sparse_context_even_when_windows_overlap() {
    let mut store = pins_ready();
    assert!(store
        .begin_context(GENERAL, 1, MessageId(100), HistorySource::Cached)
        .allowed());
    assert!(matches!(
        load(&mut store, 1, 100, [1, 100]),
        Applied::Context { .. }
    ));
    history(&mut store, None, 90..110, false);
    let timeline = store.timeline(GENERAL).unwrap();
    assert!(timeline.latest);
    assert_eq!(timeline.context_source(), None);
    assert_eq!(timeline.len(), 20);
    assert!(timeline.get(MessageId(1)).is_none());
}

#[test]
fn context_keeps_pending_sends_tombstones_and_confirmed_undo() {
    let mut store = pins_ready();
    let now = Instant::now();
    history(&mut store, None, 1..6, true);
    store.apply(
        Event::MessageUpdated(Message {
            edited: Some(Timestamp(10)),
            ..message(4, ADA)
        }),
        now,
    );
    store.update_local(GENERAL, MessageId(4), |message| {
        message.content = "edición pendiente".into()
    });
    store.add_pending(GENERAL, "envío pendiente".into(), None, 99, Timestamp(100));
    store.apply(
        Event::MessageDeleted {
            channel: GENERAL,
            message: MessageId(3),
        },
        now,
    );
    assert!(store
        .begin_context(GENERAL, 1, MessageId(4), HistorySource::Cached)
        .allowed());
    assert!(matches!(
        load(&mut store, 1, 4, [3, 4]),
        Applied::Context { .. }
    ));
    let timeline = store.timeline(GENERAL).unwrap();
    assert!(timeline.get(MessageId(3)).is_none());
    assert_eq!(
        timeline.get(MessageId(4)).unwrap().content,
        "edición pendiente"
    );
    assert!(timeline
        .messages()
        .iter()
        .any(|message| message.delivery.is_local() && message.content == "envío pendiente"));
    store.apply(
        Event::Failed {
            failure: Failure::Edit {
                channel: GENERAL,
                message: MessageId(4),
            },
            reason: "no".into(),
            ambiguous: false,
        },
        now,
    );
    let message = store.timeline(GENERAL).unwrap().get(MessageId(4)).unwrap();
    assert_eq!(message.content, "mensaje 4");
    assert_eq!(message.edited, Some(Timestamp(10)));
}

#[test]
fn cached_context_is_available_offline_but_provider_context_requires_support_and_session() {
    let mut store = pins_ready();
    store.connection = ConnectionState::Disconnected {
        reason: "offline".into(),
    };
    store.from_cache = true;
    assert!(store
        .begin_context(GENERAL, 1, MessageId(20), HistorySource::Cached)
        .allowed());
    assert!(!store
        .begin_context(GENERAL, 2, MessageId(20), HistorySource::Provider)
        .allowed());
    assert_eq!(
        load(&mut store, 1, 20, [20]),
        Applied::Context {
            channel: GENERAL,
            request: 1,
            anchor: MessageId(20)
        }
    );
    store.connection = ConnectionState::Connected;
    store.from_cache = false;
    store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .message_context = true;
    assert!(store
        .begin_context(GENERAL, 3, MessageId(20), HistorySource::Provider)
        .allowed());
    assert_eq!(
        load(&mut store, 3, 20, [20]),
        Applied::Nothing,
        "a cached response cannot consume a provider read"
    );
    assert_eq!(
        store.apply(
            Event::ContextLoaded {
                channel: GENERAL,
                request: 3,
                anchor: MessageId(20),
                messages: vec![message(20, ADA)]
            },
            Instant::now()
        ),
        Applied::Context {
            channel: GENERAL,
            request: 3,
            anchor: MessageId(20)
        }
    );
    assert_eq!(
        store.timeline(GENERAL).unwrap().context_source(),
        Some(HistorySource::Provider)
    );
}

#[test]
fn malformed_context_or_deleted_anchor_fails_without_replacing_the_visible_window() {
    for context in [
        cached(20, [30]),
        cached(20, [20, 20]),
        cached(20, [20, 10]),
        cached(20, 1..=52),
    ] {
        let mut store = pins_ready();
        history(&mut store, None, 1..6, true);
        assert!(store
            .begin_context(GENERAL, 1, MessageId(20), HistorySource::Cached)
            .allowed());
        assert_failed(store.apply_cached_context(1, GENERAL, context), 1);
        assert_eq!(store.timeline(GENERAL).unwrap().len(), 5);
    }
    let mut store = pins_ready();
    store.apply(
        Event::MessageDeleted {
            channel: GENERAL,
            message: MessageId(20),
        },
        Instant::now(),
    );
    assert!(store
        .begin_context(GENERAL, 1, MessageId(20), HistorySource::Cached)
        .allowed());
    assert_failed(load(&mut store, 1, 20, [20]), 1);
    assert!(store
        .timeline(GENERAL)
        .unwrap()
        .get(MessageId(20))
        .is_none());
}

#[test]
fn ready_logout_and_history_revocation_invalidate_context_scope() {
    for event in [
        Event::Ready(Box::new(snapshot())),
        Event::Authentication(AuthenticationState::SignedOut),
    ] {
        let mut store = pins_ready();
        assert!(store
            .begin_context(GENERAL, 1, MessageId(20), HistorySource::Cached)
            .allowed());
        store.apply(event, Instant::now());
        assert_eq!(load(&mut store, 1, 20, [20]), Applied::Nothing);
    }
    let mut store = pins_ready();
    assert!(store
        .begin_context(GENERAL, 1, MessageId(20), HistorySource::Cached)
        .allowed());
    store.channels.get_mut(&GENERAL).unwrap().permissions = Permissions::VIEW;
    assert_failed(load(&mut store, 1, 20, [20]), 1);
    assert!(!store
        .begin_context(GENERAL, 2, MessageId(20), HistorySource::Cached)
        .allowed());
}
