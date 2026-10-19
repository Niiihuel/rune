use super::*;
use rune_model::{Delivery, MessageKind, Timestamp, UserId};

fn read_mark(revision: u64) -> ReadMark {
    ReadMark {
        through: Some(MessageId(100)),
        unread_from: Some(MessageId(80)),
        revision,
    }
}

#[test]
fn read_marks_use_reliable_tickets_and_keep_newer_local_revisions() {
    let (worker, wakes) = worker();
    let latest = worker.set_read_mark(ChannelId(5), read_mark(4)).unwrap();
    wait_committed(&worker, &wakes, latest);
    assert_eq!(
        worker.read_marks().unwrap(),
        vec![(ChannelId(5), read_mark(4))]
    );
    let older = worker.set_read_mark(ChannelId(5), read_mark(3)).unwrap();
    let fence = worker.flush().unwrap();
    wait_committed(&worker, &wakes, fence);
    assert!(worker.committed(older));
    assert_eq!(
        worker.read_marks().unwrap(),
        vec![(ChannelId(5), read_mark(4))]
    );
    assert!(worker.error().is_none());
    let forget = worker.forget().unwrap();
    wait_committed(&worker, &wakes, forget);
    assert!(worker.read_marks().unwrap().is_empty());
}

#[test]
fn read_mark_busy_leaves_the_local_choice_for_retry_and_close_flush() {
    let (worker, wakes) = worker();
    let release = pause(&worker);
    let mut last = StorageTicket(0);
    for _ in 0..QUEUE {
        last = worker.submit(Job::Flush).unwrap();
    }
    let mark = read_mark(7);
    assert_eq!(
        worker.set_read_mark(ChannelId(5), mark),
        Err(StorageWriteError::Busy)
    );
    release.send(()).unwrap();
    wait_committed(&worker, &wakes, last);
    assert!(worker.read_marks().unwrap().is_empty());
    let ticket = worker.set_read_mark(ChannelId(5), mark).unwrap();
    let close = worker.flush().unwrap();
    wait_committed(&worker, &wakes, close);
    assert!(worker.committed(ticket));
    assert_eq!(worker.read_marks().unwrap(), vec![(ChannelId(5), mark)]);
}

#[test]
fn read_marks_closed_worker_reports_unavailable_instead_of_empty_success() {
    let (mut worker, _) = worker();
    worker.jobs.take();
    assert_eq!(
        worker.set_read_mark(ChannelId(5), read_mark(1)),
        Err(StorageWriteError::Closed)
    );
    assert_eq!(
        worker.read_marks(),
        Err(rune_model::LocalReadError::Unavailable)
    );
}

#[test]
fn read_mark_startup_answers_transfer_prepaid_credit_without_a_second_reservation() {
    let (worker, wakes) = worker();
    let written = worker.set_read_mark(ChannelId(5), read_mark(9)).unwrap();
    wait_committed(&worker, &wakes, written);
    let release = pause(&worker);
    let (reply, answer) = sync_channel(1);
    let slot = AnswerSlot::reserve(&worker.shared).unwrap();
    let read = worker.submit(Job::ReadMarks { reply, slot }).unwrap();
    assert!(worker.shared.bytes.load(Ordering::Acquire) >= READ_MARKS_CREDIT);
    release.send(()).unwrap();
    let held = answer.recv_timeout(Duration::from_secs(3)).unwrap();
    assert_eq!(held.value, Ok(vec![(ChannelId(5), read_mark(9))]));
    wait_committed(&worker, &wakes, read);
    assert!(worker.shared.bytes.load(Ordering::Acquire) >= READ_MARKS_CREDIT);
    drop(held);
    assert_eq!(worker.shared.bytes.load(Ordering::Acquire), 0);
}

#[test]
fn an_unlock_restores_its_read_marks_even_with_no_room_left_behind_it() {
    let (worker, wakes) = worker();
    worker.set_read_mark(ChannelId(5), read_mark(9)).unwrap();
    worker
        .set_draft(ChannelId(5), Some("a medio escribir".into()))
        .unwrap();
    // Read back, about 5 MiB: far more than an unlock pays for up front.
    let big = Snapshot {
        last_messages: (1..=200_000)
            .map(|id| (ChannelId(id), MessageId(id)))
            .collect(),
        first_unread: (1..=50_000)
            .map(|id| (ChannelId(id), MessageId(id)))
            .collect(),
        ..Snapshot::default()
    };
    let saved = worker.save_snapshot(big).unwrap();
    wait_committed(&worker, &wakes, saved);
    let unlocking = || {
        Box::new(Unlocking {
            account: "a".into(),
            key: crate::tests::derived_key("a"),
            fresh: false,
            restore: Some(UserId(1)),
            snapshot: true,
        })
    };
    let restored = |worker: &StorageWorker| {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(restored) = worker.try_restored() {
                return restored;
            }
            assert!(Instant::now() < until, "every unlock answers");
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    let release = pause(&worker);
    let unlocked = worker.unlock(unlocking()).unwrap();
    // A session's snapshot queued behind it takes every byte left.
    let left = MAX_QUEUED_BYTES - worker.shared.bytes.load(Ordering::Acquire);
    let behind = Reservation::reserve(&worker.shared, left).unwrap();
    release.send(()).unwrap();
    wait_committed(&worker, &wakes, unlocked);
    let (account, kept) = restored(&worker);
    assert_eq!(account, "a");
    assert_eq!(kept.read_marks, Ok(vec![(ChannelId(5), read_mark(9))]));
    assert_eq!(kept.drafts, vec![(ChannelId(5), "a medio escribir".into())]);
    assert!(kept.snapshot.is_none(), "no room for it, so it is left out");
    drop(behind);
    // With room, the snapshot comes too.
    let unlocked = worker.unlock(unlocking()).unwrap();
    wait_committed(&worker, &wakes, unlocked);
    let (_, kept) = restored(&worker);
    let snapshot = kept.snapshot.unwrap();
    assert!(snapshot_bytes(&snapshot) > RESTORE_CREDIT);
    assert_eq!(snapshot.last_messages.len(), 200_000);
    assert_eq!(kept.read_marks, Ok(vec![(ChannelId(5), read_mark(9))]));
    assert_eq!(worker.shared.bytes.load(Ordering::Acquire), 0);
}

fn search_request(request: u64) -> SearchRequest {
    SearchRequest {
        tag: LocalReadTag {
            epoch: 7,
            request,
            channel: ChannelId(5),
        },
        query: rune_model::SearchQuery {
            channel: ChannelId(5),
            text: "cached".into(),
            ..Default::default()
        }
        .into(),
        cursor: None,
    }
}

fn wait_local(worker: &StorageWorker, wakes: &Receiver<()>) -> LocalReadReply {
    let until = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(reply) = worker.try_local_read() {
            return reply;
        }
        wakes
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .expect("local read progress");
    }
}

#[test]
fn local_search_pages_are_bounded_and_empty_answers_are_terminal() {
    let (worker, wakes) = worker();
    let write = worker
        .apply(
            (1..=77)
                .map(|id| {
                    MessageMutation::Upsert(Box::new(Message {
                        id: MessageId(id),
                        ..message()
                    }))
                })
                .collect(),
        )
        .unwrap();
    wait_committed(&worker, &wakes, write);
    let mut request = search_request(10);
    let mut ids = vec![];
    loop {
        worker.request_search(request.clone()).unwrap();
        let LocalReadReply::Search { tag, result } = wait_local(&worker, &wakes) else {
            panic!("search reply");
        };
        assert_eq!(tag, request.tag);
        let page = result.unwrap();
        assert!(page.hits.len() <= 20);
        ids.extend(page.hits.iter().map(|hit| hit.id.0));
        let Some(cursor) = page.continuation else {
            break;
        };
        request.cursor = Some(cursor);
        request.tag.request += 1;
    }
    assert_eq!(ids, (1..=77).rev().collect::<Vec<_>>());
    let request = SearchRequest {
        query: rune_model::SearchQuery {
            text: "absent".into(),
            ..*request.query
        }
        .into(),
        cursor: None,
        ..request
    };
    worker.request_search(request).unwrap();
    let LocalReadReply::Search { result, .. } = wait_local(&worker, &wakes) else {
        panic!("search reply");
    };
    assert_eq!(result.unwrap(), rune_model::SearchPage::default());
}

#[test]
fn local_read_busy_returns_the_request_and_pre_reserves_reply_capacity() {
    let (worker, wakes) = worker();
    let release = pause(&worker);
    let first = search_request(10);
    worker.request_search(first).unwrap();
    let rejected = worker.request_search(search_request(11)).unwrap_err();
    assert_eq!(rejected.kind, StorageWriteError::Busy);
    assert_eq!(rejected.value, search_request(11));
    assert!(worker.shared.bytes.load(Ordering::Acquire) >= 64 * 1024);
    worker.cancel_read(10);
    release.send(()).unwrap();
    let LocalReadReply::Search { result, .. } = wait_local(&worker, &wakes) else {
        panic!("search reply");
    };
    assert_eq!(result, Err(rune_model::LocalReadError::Cancelled));
    worker.request_search(rejected.value).unwrap();
    let _ = wait_local(&worker, &wakes);
    let mut text = String::with_capacity(MAX_QUEUED_BYTES);
    text.push_str("ok");
    let request = SearchRequest {
        query: rune_model::SearchQuery {
            text,
            ..*search_request(12).query
        }
        .into(),
        ..search_request(12)
    };
    let rejected = worker.request_search(request).unwrap_err();
    assert_eq!(rejected.kind, StorageWriteError::TooLarge);
    assert_eq!(rejected.value.query.text.capacity(), MAX_QUEUED_BYTES);
}

#[test]
fn a_full_answer_queue_retains_the_local_terminal_answer_while_writes_commit() {
    let (worker, wakes) = worker();
    let write = worker
        .apply(vec![MessageMutation::Upsert(Box::new(message()))])
        .unwrap();
    wait_committed(&worker, &wakes, write);
    for request in 0..CACHED_ANSWERS {
        assert!(worker.request_cached(
            CacheRequest {
                epoch: 1,
                request: request as u64,
                channel: ChannelId(5)
            },
            1
        ));
    }
    let fence = worker.submit(Job::Flush).unwrap();
    wait_committed(&worker, &wakes, fence);
    worker.request_search(search_request(10)).unwrap();
    let write = worker
        .set_setting("after-search", Some("committed".into()))
        .unwrap();
    wait_committed(&worker, &wakes, write);
    assert!(
        worker.try_local_read().is_none(),
        "sixteen queued answers already own every slot"
    );
    assert!(worker.shared.bytes.load(Ordering::Acquire) <= MAX_QUEUED_BYTES);
    assert!(worker.try_cached().is_some());
    let LocalReadReply::Search { result, .. } = wait_local(&worker, &wakes) else {
        panic!("search reply");
    };
    assert_eq!(result.unwrap().hits.len(), 1);
}

#[test]
fn local_cancel_does_not_need_job_capacity_and_flush_does_not_wait_for_consumers() {
    let (worker, wakes) = worker();
    let release = pause(&worker);
    worker.request_search(search_request(10)).unwrap();
    let mut last = StorageTicket(0);
    for _ in 1..QUEUE {
        last = worker.submit(Job::Flush).unwrap();
    }
    assert_eq!(
        worker.set_setting("full", None),
        Err(StorageWriteError::Busy)
    );
    worker.cancel_read(10);
    release.send(()).unwrap();
    wait_committed(&worker, &wakes, last);
    let LocalReadReply::Search { result, .. } = wait_local(&worker, &wakes) else {
        panic!("search reply");
    };
    assert_eq!(result, Err(rune_model::LocalReadError::Cancelled));
    let ticket = worker.flush().unwrap();
    wait_committed(&worker, &wakes, ticket);
}

#[test]
fn local_search_cursors_are_bound_to_query_epoch_and_message_revision() {
    let (worker, wakes) = worker();
    let write = worker
        .apply(
            (1..=30)
                .map(|id| {
                    MessageMutation::Upsert(Box::new(Message {
                        id: MessageId(id),
                        ..message()
                    }))
                })
                .collect(),
        )
        .unwrap();
    wait_committed(&worker, &wakes, write);
    for mismatch in 0..4 {
        let request = search_request(10 + mismatch * 2);
        worker.request_search(request.clone()).unwrap();
        let LocalReadReply::Search { result, .. } = wait_local(&worker, &wakes) else {
            panic!("search reply");
        };
        let cursor = result.unwrap().continuation.unwrap();
        let mut next = SearchRequest {
            cursor: Some(cursor),
            ..request
        };
        next.tag.request += 1;
        match mismatch {
            0 => next.query.text = "other".into(),
            1 => next.tag.epoch += 1,
            2 => next.cursor = Some(rune_model::SearchCursor(cursor.0.wrapping_add(999))),
            _ => {
                let write = worker
                    .apply(vec![MessageMutation::Patch(Box::new(
                        rune_model::MessagePatch {
                            content: Some("changed".into()),
                            ..patch()
                        },
                    ))])
                    .unwrap();
                wait_committed(&worker, &wakes, write);
            }
        }
        worker.request_search(next).unwrap();
        let LocalReadReply::Search { result, .. } = wait_local(&worker, &wakes) else {
            panic!("search reply");
        };
        assert_eq!(result, Err(rune_model::LocalReadError::StaleCursor));
    }
}

#[test]
fn local_context_has_exact_identity_bounded_neighbors_and_missing_anchor_error() {
    let (worker, wakes) = worker();
    let write = worker
        .apply(
            (1..=100)
                .map(|id| {
                    MessageMutation::Upsert(Box::new(Message {
                        id: MessageId(id),
                        ..message()
                    }))
                })
                .collect(),
        )
        .unwrap();
    wait_committed(&worker, &wakes, write);
    for anchor in [50, 101] {
        let request = ContextRequest {
            tag: search_request(anchor).tag,
            anchor: MessageId(anchor),
        };
        worker.request_context(request).unwrap();
        let LocalReadReply::Context { tag, result } = wait_local(&worker, &wakes) else {
            panic!("context reply");
        };
        assert_eq!(tag, request.tag);
        if anchor == 101 {
            assert_eq!(result, Err(rune_model::LocalReadError::NotFoundLocal));
            continue;
        }
        let context = result.unwrap();
        assert_eq!(context.anchor, MessageId(50));
        assert_eq!(context.messages.len(), 51);
        assert!(messages_bytes(&context.messages) <= 2 * 1024 * 1024);
        assert!(context.has_older_cached && context.has_newer_cached);
    }
}

fn patch() -> rune_model::MessagePatch {
    rune_model::MessagePatch {
        channel: ChannelId(5),
        id: MessageId(1),
        ..Default::default()
    }
}

#[test]
fn partial_message_changes_count_all_retained_nested_strings() {
    use rune_model::{
        Attachment, AttachmentId, ImageKind, Reaction, RemoteFile, RemoteImage, Sticker, StickerId,
        TextReference, TextTarget,
    };
    let (worker, _) = worker();
    let text = || String::with_capacity(MAX_QUEUED_BYTES / 10);
    let image = || {
        Some(RemoteImage {
            url: text(),
            kind: ImageKind::Artwork,
        })
    };
    let patch = rune_model::MessagePatch {
        content: Some(text()),
        reactions: Some(vec![Reaction {
            emoji: text(),
            count: 1,
            me: false,
            ..Default::default()
        }]),
        attachments: Some(vec![Attachment {
            id: AttachmentId(1),
            filename: text(),
            size_bytes: 0,
            content_type: Some(text()),
            dimensions: None,
            preview: image(),
            download: Some(RemoteFile { url: text() }),
            spoiler: false,
            voice: None,
            explicit: false,
            placeholder: None,
        }]),
        stickers: Some(vec![Sticker {
            id: StickerId(1),
            name: text(),
            image: image(),
            available: true,
        }]),
        text_references: Some(vec![TextReference {
            source: text(),
            target: TextTarget::Emoji {
                name: text(),
                image: None,
                jumbo: None,
                id: None,
            },
        }]),
        ..patch()
    };
    let rejected = worker
        .apply(vec![MessageMutation::Patch(Box::new(patch))])
        .unwrap_err();
    assert_eq!(rejected.kind, StorageWriteError::TooLarge);
}

#[test]
fn partial_message_changes_count_spare_vector_capacity() {
    use rune_model::{Attachment, MessagePatch, Reaction, Sticker, TextReference};
    let (worker, _) = worker();
    for patch in [
        MessagePatch {
            mentions: Some(Vec::with_capacity(
                MAX_QUEUED_BYTES.div_ceil(size_of::<UserId>()),
            )),
            ..patch()
        },
        MessagePatch {
            mention_roles: Some(Vec::with_capacity(
                MAX_QUEUED_BYTES.div_ceil(size_of::<rune_model::RoleId>()),
            )),
            ..patch()
        },
        MessagePatch {
            reactions: Some(Vec::with_capacity(
                MAX_QUEUED_BYTES.div_ceil(size_of::<Reaction>()),
            )),
            ..patch()
        },
        MessagePatch {
            attachments: Some(Vec::with_capacity(
                MAX_QUEUED_BYTES.div_ceil(size_of::<Attachment>()),
            )),
            ..patch()
        },
        MessagePatch {
            stickers: Some(Vec::with_capacity(
                MAX_QUEUED_BYTES.div_ceil(size_of::<Sticker>()),
            )),
            ..patch()
        },
        MessagePatch {
            text_references: Some(Vec::with_capacity(
                MAX_QUEUED_BYTES.div_ceil(size_of::<TextReference>()),
            )),
            ..patch()
        },
        MessagePatch {
            call: Some(rune_model::MessageCall {
                participants: Vec::with_capacity(MAX_QUEUED_BYTES.div_ceil(size_of::<UserId>())),
                ended: None,
            }),
            ..patch()
        },
        MessagePatch {
            attachments: Some(vec![Attachment {
                id: rune_model::AttachmentId(1),
                filename: String::new(),
                size_bytes: 0,
                content_type: None,
                dimensions: None,
                preview: None,
                download: None,
                spoiler: false,
                voice: None,
                explicit: false,
                placeholder: Some(rune_model::RemoteImage {
                    url: String::with_capacity(MAX_QUEUED_BYTES),
                    kind: rune_model::ImageKind::ThumbHash,
                }),
            }]),
            ..patch()
        },
    ] {
        let rejected = worker
            .apply(vec![MessageMutation::Patch(Box::new(patch))])
            .unwrap_err();
        assert_eq!(rejected.kind, StorageWriteError::TooLarge);
    }
}

#[test]
fn partial_message_changes_return_the_same_owned_patch_on_busy_and_commit_after_retry() {
    let (worker, wakes) = worker();
    let seed = worker
        .apply(vec![MessageMutation::Upsert(Box::new(message()))])
        .unwrap();
    wait_committed(&worker, &wakes, seed);
    let release = pause(&worker);
    let mut last = StorageTicket(0);
    for _ in 0..QUEUE {
        last = worker.flush().unwrap();
    }
    let change = Box::new(rune_model::MessagePatch {
        content: Some("accepted after retry".into()),
        ..patch()
    });
    let allocation = &*change as *const rune_model::MessagePatch;
    let rejected = worker
        .apply(vec![MessageMutation::Patch(change)])
        .unwrap_err();
    assert_eq!(rejected.kind, StorageWriteError::Busy);
    let [MessageMutation::Patch(held)] = &rejected.value[..] else {
        panic!("the patch is returned intact");
    };
    assert_eq!(&**held as *const _, allocation);
    release.send(()).unwrap();
    wait_committed(&worker, &wakes, last);
    let accepted = worker.apply(rejected.value).unwrap();
    wait_committed(&worker, &wakes, accepted);
    assert!(worker.request_cached(
        CacheRequest {
            epoch: 1,
            request: 1,
            channel: ChannelId(5)
        },
        1
    ));
    let flush = worker.flush().unwrap();
    wait_committed(&worker, &wakes, flush);
    let (_, messages) = worker.try_cached().unwrap();
    assert_eq!(
        messages,
        vec![Message {
            content: "accepted after retry".into(),
            ..message()
        }]
    );
}

fn message() -> Message {
    Message {
        id: MessageId(1),
        channel: ChannelId(5),
        author: UserId(2),
        content: "cached".into(),
        timestamp: Timestamp(1),
        edited: None,
        kind: MessageKind::Default,
        reply_to: None,
        mentions: Vec::new(),
        reactions: Vec::new(),
        attachments: Vec::new(),
        stickers: Vec::new(),
        text_references: Vec::new(),
        delivery: Delivery::Sent,
        pinned: false,
        embeds: Vec::new(),
        forwarded: None,
        poll: None,
        components: Default::default(),
        call: None,
        mention_roles: Vec::new(),
        mention_everyone: false,
    }
}

fn ephemeral_snapshot() -> Snapshot {
    Snapshot {
        presences: vec![(
            UserId(2),
            rune_model::Presence {
                status: rune_model::Status::Online,
                custom: Some("ephemeral status".into()),
                custom_emoji: None,
                activities: Vec::new(),
                platforms: Default::default(),
            },
        )],
        voice: vec![rune_model::VoiceState {
            user: UserId(2),
            channel: ChannelId(5),
            muted: false,
            deafened: false,
            speaking: true,
            streaming: false,
            server_muted: false,
            server_deafened: false,
            camera: false,
            suppressed: false,
            speak_request: None,
        }],
        last_messages: vec![(ChannelId(5), MessageId(1))],
        ..Snapshot::default()
    }
}

fn worker() -> (StorageWorker, Receiver<()>) {
    let (wake, wakes) = sync_channel(QUEUE * 4);
    let worker = StorageWorker::start(
        crate::tests::memory("a"),
        "a".into(),
        Arc::new(move || {
            let _ = wake.try_send(());
        }),
    );
    (worker, wakes)
}

fn wait_committed(worker: &StorageWorker, wakes: &Receiver<()>, ticket: StorageTicket) {
    let until = Instant::now() + Duration::from_secs(3);
    while !worker.committed(ticket) {
        wakes
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .expect("worker progress");
    }
}

fn pause(worker: &StorageWorker) -> SyncSender<()> {
    let (entered, waiting) = sync_channel(1);
    let (release, released) = sync_channel(1);
    let released = std::sync::Mutex::new(released);
    worker
        .submit(Job::Test(Box::new(move |_| {
            entered.try_send(()).unwrap();
            released.lock().unwrap().recv().unwrap();
            Ok(())
        })))
        .unwrap();
    waiting.recv_timeout(Duration::from_secs(3)).unwrap();
    release
}

#[test]
fn a_full_queue_returns_the_owned_batch_without_waiting_or_acknowledging_it() {
    let (worker, wakes) = worker();
    let release = pause(&worker);
    let mut last = StorageTicket(0);
    for _ in 0..QUEUE {
        last = worker.flush().unwrap();
    }
    while wakes.try_recv().is_ok() {}
    let rejected = worker
        .apply(vec![MessageMutation::Upsert(Box::new(message()))])
        .unwrap_err();
    assert_eq!(rejected.kind, StorageWriteError::Busy);
    assert!(matches!(&rejected.value[..], [MessageMutation::Upsert(held)] if **held == message()));
    assert_eq!(
        worker.set_draft(ChannelId(5), Some("draft".into())),
        Err(StorageWriteError::Busy)
    );
    let snapshot = Box::new(ephemeral_snapshot());
    let allocation = &*snapshot as *const Snapshot;
    let snapshot = worker.save_snapshot_owned(snapshot).unwrap_err();
    assert_eq!(snapshot.kind, StorageWriteError::Busy);
    assert_eq!(&*snapshot.value as *const Snapshot, allocation);
    assert!(snapshot.value.presences.is_empty());
    assert!(snapshot.value.voice.is_empty());
    assert!(
        wakes.try_recv().is_err(),
        "rejecting work frees no accepted capacity"
    );
    assert!(!worker.committed(last));
    release.send(()).unwrap();
    wait_committed(&worker, &wakes, last);
    let accepted = worker.apply(rejected.value).unwrap();
    wait_committed(&worker, &wakes, accepted);
}

#[test]
fn persisted_snapshots_exclude_ephemeral_state_without_changing_the_live_payload() {
    let directory = std::env::temp_dir().join(format!(
        "rune-storage-snapshot-privacy-{}",
        std::process::id()
    ));
    let path = directory.join("rune.db");
    let live = ephemeral_snapshot();
    let (wake, wakes) = sync_channel(QUEUE);
    let worker = StorageWorker::start(
        crate::tests::unlocked(&path, "a"),
        "a".into(),
        Arc::new(move || {
            let _ = wake.try_send(());
        }),
    );
    let ticket = worker.save_snapshot_owned(Box::new(live.clone())).unwrap();
    wait_committed(&worker, &wakes, ticket);
    drop(worker);
    let storage = crate::tests::unlocked(&path, "a");
    let stored = storage.snapshot::<Snapshot>("a").unwrap().unwrap();
    assert!(
        stored.presences.is_empty(),
        "presence must never reach SQLite"
    );
    assert!(
        stored.voice.is_empty(),
        "voice participation must never reach SQLite"
    );
    assert_eq!(stored.last_messages, live.last_messages);
    assert_eq!(
        live,
        ephemeral_snapshot(),
        "storage must not change the live session"
    );
    drop(storage);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn retained_capacity_is_bounded_even_when_the_payload_text_is_empty() {
    let (worker, _) = worker();
    let mut large = message();
    large.content = String::with_capacity(MAX_QUEUED_BYTES);
    let rejected = worker
        .apply(vec![MessageMutation::Upsert(Box::new(large))])
        .unwrap_err();
    assert_eq!(rejected.kind, StorageWriteError::TooLarge);
    assert!(
        matches!(&rejected.value[..], [MessageMutation::Upsert(held)] if held.content.is_empty() && held.content.capacity() == MAX_QUEUED_BYTES)
    );
}

#[test]
fn snapshot_read_versions_count_spare_capacity_before_admission() {
    let (worker, _) = worker();
    let snapshot = Box::new(Snapshot {
        read_versions: Vec::with_capacity(MAX_QUEUED_BYTES / size_of::<(ChannelId, u64)>() + 1),
        ..Snapshot::default()
    });
    let allocation = &*snapshot as *const Snapshot;
    let rejected = worker.save_snapshot_owned(snapshot).unwrap_err();
    assert_eq!(rejected.kind, StorageWriteError::TooLarge);
    assert_eq!(&*rejected.value as *const Snapshot, allocation);
    assert!(rejected.value.read_versions.is_empty());
    assert_eq!(worker.shared.bytes.load(Ordering::Acquire), 0);
}

#[test]
fn snapshot_first_unread_count_spare_capacity_before_admission() {
    let (worker, _) = worker();
    let snapshot = Box::new(Snapshot {
        first_unread: Vec::with_capacity(
            MAX_QUEUED_BYTES / size_of::<(ChannelId, MessageId)>() + 1,
        ),
        ..Snapshot::default()
    });
    let allocation = &*snapshot as *const Snapshot;
    let rejected = worker.save_snapshot_owned(snapshot).unwrap_err();
    assert_eq!(rejected.kind, StorageWriteError::TooLarge);
    assert_eq!(&*rejected.value as *const Snapshot, allocation);
    assert!(rejected.value.first_unread.is_empty());
    assert_eq!(worker.shared.bytes.load(Ordering::Acquire), 0);
}

#[test]
fn the_byte_budget_covers_queued_jobs_together_and_returns_after_commit() {
    let (worker, wakes) = worker();
    let release = pause(&worker);
    let text = String::with_capacity(MAX_QUEUED_BYTES / 2);
    let first = worker.set_draft(ChannelId(5), Some(text)).unwrap();
    let second = String::with_capacity(MAX_QUEUED_BYTES / 2);
    assert_eq!(
        worker.set_draft(ChannelId(6), Some(second)),
        Err(StorageWriteError::Busy)
    );
    release.send(()).unwrap();
    wait_committed(&worker, &wakes, first);
    // A startup read also fences release of the previous job's reservation.
    assert!(worker.drafts().is_empty());
    let accepted = worker
        .set_draft(
            ChannelId(6),
            Some(String::with_capacity(MAX_QUEUED_BYTES / 2)),
        )
        .unwrap();
    wait_committed(&worker, &wakes, accepted);
}

#[test]
fn a_failed_job_keeps_its_place_and_is_not_committed_until_a_retry_succeeds() {
    use std::sync::atomic::AtomicBool;
    let (worker, wakes) = worker();
    let fail = Arc::new(AtomicBool::new(true));
    let failing = fail.clone();
    let (attempted, attempts) = sync_channel(8);
    let first = worker
        .submit(Job::Test(Box::new(move |storage| {
            let _ = attempted.try_send(());
            if failing.load(Ordering::Acquire) {
                return Err(rune_storage::Error::Newer {
                    found: 99,
                    supported: 3,
                });
            }
            storage.set_setting("order", Some("first"))
        })))
        .unwrap();
    attempts.recv_timeout(Duration::from_secs(3)).unwrap();
    while worker.error().is_none() {
        wakes.recv_timeout(Duration::from_secs(3)).unwrap();
    }
    let later = worker.set_setting("order", Some("later".into())).unwrap();
    let flush = worker.flush().unwrap();
    assert!(!worker.committed(first));
    assert!(!worker.committed(later));
    assert!(!worker.committed(flush));
    fail.store(false, Ordering::Release);
    worker.thread.as_ref().unwrap().thread().unpark();
    wait_committed(&worker, &wakes, flush);
    assert!(worker.error().is_none());
    assert_eq!(worker.setting("order").as_deref(), Some("later"));
}

#[test]
fn a_full_cached_answer_queue_does_not_block_commits_and_keeps_request_identity() {
    let (worker, wakes) = worker();
    let write = worker
        .apply(vec![MessageMutation::Upsert(Box::new(message()))])
        .unwrap();
    wait_committed(&worker, &wakes, write);
    for request in 0..CACHED_ANSWERS + 4 {
        assert!(worker.request_cached(
            CacheRequest {
                epoch: 7,
                request: request as u64,
                channel: ChannelId(5)
            },
            1
        ));
    }
    let flush = worker.flush().unwrap();
    wait_committed(&worker, &wakes, flush);
    let mut answers = 0;
    while let Some((request, messages)) = worker.try_cached() {
        assert_eq!(request.epoch, 7);
        assert_eq!(request.channel, ChannelId(5));
        assert_eq!(request.request, answers);
        assert_eq!(messages, vec![message()]);
        answers += 1;
    }
    assert_eq!(answers, CACHED_ANSWERS as u64);
}

#[test]
fn closed_workers_reject_writes_and_startup_reads_without_waiting() {
    let (mut worker, _) = worker();
    worker.jobs.take();
    assert_eq!(worker.flush(), Err(StorageWriteError::Closed));
    assert!(worker.snapshot().is_none());
    assert!(worker.drafts().is_empty());
    assert!(worker.setting("theme").is_none());
}

#[test]
fn forced_drop_cannot_acknowledge_a_write_that_keeps_failing() {
    let (worker, wakes) = worker();
    let failed = worker
        .submit(Job::Test(Box::new(|_| {
            Err(rune_storage::Error::Newer {
                found: 99,
                supported: 3,
            })
        })))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    while worker.error().is_none() {
        wakes
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .unwrap();
    }
    let shared = worker.shared.clone();
    drop(worker);
    assert!(shared.committed.load(Ordering::Acquire) < failed.0);
    assert!(shared.failed.load(Ordering::Acquire));
    assert!(shared.closed.load(Ordering::Acquire));
    assert_eq!(shared.bytes.load(Ordering::Acquire), 0);
}

#[test]
fn memory_only_notice_returns_after_an_earlier_accepted_write_recovers() {
    let (worker, wakes) = worker();
    let fail = Arc::new(AtomicBool::new(true));
    let failing = fail.clone();
    let ticket = worker
        .submit(Job::Test(Box::new(move |storage| {
            if failing.load(Ordering::Acquire) {
                return Err(rune_storage::Error::Newer {
                    found: 99,
                    supported: 3,
                });
            }
            storage.set_setting("recovered", Some("yes"))
        })))
        .unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    while worker.error().is_none() {
        wakes
            .recv_timeout(until.saturating_duration_since(Instant::now()))
            .unwrap();
    }
    let provider = Arc::new(rune_provider::mock::MockProvider::new(
        rune_provider::mock::MockConfig::default(),
    ));
    let mut client = crate::Client::start(
        provider,
        crate::ClientConfig {
            wake: Arc::new(|| {}),
            storage: None,
            keyring: None,
            backoff: crate::Backoff::default(),
            deadlines: crate::Deadlines::default(),
            image_cache: None,
        },
    );
    let (_sender, events) = tokio::sync::mpsc::channel(1);
    client.events = events;
    client.storage = Some(worker);
    client.storage_active = true;
    client.pending_storage = Some(crate::PendingStorage::Snapshot(Box::new(Snapshot {
        users: Vec::with_capacity(MAX_QUEUED_BYTES / size_of::<rune_model::User>() + 1),
        ..Snapshot::default()
    })));
    client.flush_pending_storage().unwrap();
    let notice = client.storage_error().unwrap().to_owned();
    assert!(notice.contains("continúa en memoria"));
    client.poll(0);
    assert!(client
        .storage_error()
        .unwrap()
        .contains("volverá a intentarlo"));
    let worker = client.storage.as_ref().unwrap();
    fail.store(false, Ordering::Release);
    worker.thread.as_ref().unwrap().thread().unpark();
    wait_committed(worker, &wakes, ticket);
    client.poll(0);
    assert_eq!(
        client.storage_error(),
        Some(notice.as_str()),
        "recovery must restore the memory-only policy warning"
    );
    assert!(!client.storage_active);
    let message = message();
    let mut changes = Vec::new();
    client.accept(
        crate::Envelope {
            generation: client.generation,
            event: rune_provider::Event::MessageCreated {
                message,
                nonce: None,
            },
        },
        &mut changes,
    );
    assert!(
        changes.is_empty(),
        "a recovered worker must not reactivate account caching"
    );
}

#[test]
fn a_reply_only_the_account_sees_is_never_kept() {
    let provider = Arc::new(rune_provider::mock::MockProvider::new(
        rune_provider::mock::MockConfig::default(),
    ));
    let mut client = crate::Client::start(
        provider,
        crate::ClientConfig {
            wake: Arc::new(|| {}),
            storage: None,
            keyring: None,
            backoff: crate::Backoff::default(),
            deadlines: crate::Deadlines::default(),
            image_cache: None,
        },
    );
    client.storage_active = true;
    let kept = |client: &mut crate::Client, message: Message| {
        let mut changes = Vec::new();
        for event in [
            rune_provider::Event::MessageCreated {
                message: message.clone(),
                nonce: None,
            },
            rune_provider::Event::MessageUpdated(message),
        ] {
            client.accept(
                crate::Envelope {
                    generation: client.generation,
                    event,
                },
                &mut changes,
            );
        }
        changes.len()
    };
    assert_eq!(kept(&mut client, message()), 2);
    let mut whisper = message();
    whisper.components.ephemeral = true;
    assert_eq!(
        kept(&mut client, whisper),
        0,
        "it lives only in this session"
    );
}
