use super::*;

// Retain the actual admission queue without driving the picker: these tests
// inspect admission/capacity, never connect to the session portal.
fn admission() -> (Uploads, mpsc::Receiver<Job>, ChannelId, UserId) {
    let (jobs, queue) = mpsc::channel(MAX_JOBS);
    let mut uploads = Uploads {
        shared: Arc::new(Mutex::new(Control::default())),
        changed: Arc::new(Notify::new()),
        jobs,
        limits: Some(UploadLimits {
            max_files: 10,
            max_file_bytes: 20 * 1024 * 1024,
            max_message_bytes: 25 * 1024 * 1024,
        }),
        idempotent: true,
        next_token: 1,
        budget: Arc::new(prepare::Budget::default()),
    };
    let snapshot = MockProvider::new(MockConfig::default()).snapshot();
    let account = snapshot.me.as_ref().unwrap().id;
    let channel = snapshot
        .channels
        .iter()
        .find(|channel| channel.kind.has_messages() && channel.permissions.contains(REQUIRED))
        .unwrap()
        .id;
    uploads.observe(1, &Event::Ready(Box::new(snapshot)));
    (uploads, queue, channel, account)
}

fn token(uploads: &Uploads, account: UserId, channel: ChannelId, id: u64) {
    uploads.shared.lock().unwrap().tokens.insert(
        UploadToken(id),
        TokenControl {
            account,
            channel,
            metadata: PendingAttachment {
                token: UploadToken(id),
                filename: format!("fixture-{id}.txt"),
                size_bytes: 4,
            },
            path: None,
        },
    );
}

#[test]
fn picker_reserves_an_entire_token_range_and_never_reuses_unused_tokens() {
    let (mut uploads, mut queue, channel, _) = admission();
    uploads.pick(10, channel, None).unwrap();
    assert_eq!(uploads.next_token, 11);
    assert!(matches!(
        queue.try_recv(),
        Ok(Job::Pick {
            token: UploadToken(1),
            ..
        })
    ));
    // A failed/empty portal result still consumes the reserved token range.
    uploads.shared.lock().unwrap().pick = None;
    uploads.pick(11, channel, None).unwrap();
    assert_eq!(uploads.next_token, 21);
    assert!(matches!(
        queue.try_recv(),
        Ok(Job::Pick {
            token: UploadToken(11),
            ..
        })
    ));
}

#[test]
fn a_full_draft_rejects_picker_without_enqueuing_more_files() {
    let (mut uploads, mut queue, channel, account) = admission();
    uploads.limits.as_mut().unwrap().max_files = 2;
    token(&uploads, account, channel, 50);
    token(&uploads, account, channel, 51);
    assert_eq!(uploads.pick(10, channel, None), Err(UploadError::TooMany));
    assert!(queue.try_recv().is_err());
    assert_eq!(uploads.next_token, 1);
}

#[test]
fn a_whole_token_range_must_fit_before_the_picker_is_enqueued() {
    let (mut uploads, mut queue, channel, _) = admission();
    uploads.next_token = u64::MAX - 2;
    assert_eq!(uploads.pick(10, channel, None), Err(UploadError::Failed));
    assert!(queue.try_recv().is_err());
    assert_eq!(uploads.next_token, u64::MAX - 2);
}

#[test]
fn pending_send_tokens_do_not_fill_the_next_draft_but_still_use_global_slots() {
    let (mut uploads, mut queue, channel, account) = admission();
    uploads.limits.as_mut().unwrap().max_files = 2;
    token(&uploads, account, channel, 50);
    token(&uploads, account, channel, 51);
    uploads
        .send(
            10,
            channel,
            String::new(),
            None,
            false,
            vec![UploadToken(50), UploadToken(51)],
            20,
            Duration::from_secs(30),
        )
        .unwrap();
    assert!(matches!(queue.try_recv(), Ok(Job::Send { .. })));
    uploads.pick(11, channel, None).unwrap();
    assert!(matches!(
        queue.try_recv(),
        Ok(Job::Pick { capacity: 2, .. })
    ));
}

#[test]
fn live_byte_owners_limit_picker_capacity_even_after_their_tokens_are_gone() {
    let (mut uploads, mut queue, channel, _) = admission();
    let leases = uploads
        .budget
        .reserve_many(&[0; prepare::MAX_ENTRIES - 1])
        .unwrap();
    uploads.pick(10, channel, None).unwrap();
    assert!(matches!(
        queue.try_recv(),
        Ok(Job::Pick { capacity: 1, .. })
    ));
    uploads.shared.lock().unwrap().pick = None;
    drop(leases);
    uploads.pick(11, channel, None).unwrap();
    assert!(matches!(
        queue.try_recv(),
        Ok(Job::Pick {
            capacity: prepare::MAX_ENTRIES,
            ..
        })
    ));
}

#[test]
fn existing_draft_bytes_constrain_the_whole_selection_and_allow_only_empty_files_at_zero() {
    let (mut uploads, mut queue, channel, account) = admission();
    token(&uploads, account, channel, 50);
    uploads
        .shared
        .lock()
        .unwrap()
        .tokens
        .get_mut(&UploadToken(50))
        .unwrap()
        .metadata
        .size_bytes = 25 * 1024 * 1024;
    uploads.pick(10, channel, None).unwrap();
    assert!(matches!(
        queue.try_recv(),
        Ok(Job::Pick {
            capacity: 9,
            max_total_bytes: 0,
            ..
        })
    ));
}

fn prepared_batch(budget: &Arc<prepare::Budget>) -> Vec<PreparedUpload> {
    [b"first".as_slice(), b"second".as_slice()]
        .into_iter()
        .enumerate()
        .map(|(index, body)| {
            PreparedUpload::new(
                PendingAttachment {
                    token: UploadToken(1 + index as u64),
                    filename: format!("fixture-{index}.txt"),
                    size_bytes: body.len() as u64,
                },
                prepare::seal(
                    body.to_vec().into_boxed_slice(),
                    budget.reserve(body.len()).unwrap(),
                ),
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn batch_publication_validates_every_file_before_changing_the_registry() {
    let (mut uploads, _queue, channel, _) = admission();
    uploads.pick(10, channel, None).unwrap();
    let budget = Arc::new(prepare::Budget::default());
    let mut result = Ok(prepared_batch(&budget));
    let mut entries = HashMap::new();
    let mut state = uploads.shared.lock().unwrap();
    // The first file fits; the second must reject the complete batch.
    let mut limits = uploads.limits.unwrap();
    limits.max_file_bytes = 5;
    assert_eq!(
        actor::publish_batch(&mut state, &mut entries, &mut result, limits),
        Err(UploadError::TooLarge)
    );
    assert!(entries.is_empty());
    assert!(state.tokens.is_empty());
    assert_eq!(budget.used(), (11, 2));
    drop(state);
    drop(result);
    assert_eq!(budget.used(), (0, 0));
}

#[test]
fn a_missing_token_or_cancellation_after_ready_releases_the_entire_batch() {
    for cancel in [false, true] {
        let (mut uploads, _queue, channel, _) = admission();
        uploads.pick(10, channel, None).unwrap();
        let budget = Arc::new(prepare::Budget::default());
        let mut result = Ok(prepared_batch(&budget));
        let mut entries = HashMap::new();
        {
            let mut state = uploads.shared.lock().unwrap();
            let metadata = actor::publish_batch(
                &mut state,
                &mut entries,
                &mut result,
                uploads.limits.unwrap(),
            )
            .unwrap();
            state.pick.as_mut().unwrap().result = Some(Ok(metadata));
        }
        if cancel {
            uploads.cancel(10);
        } else {
            uploads.discard(UploadToken(2));
        }
        assert!(matches!(
            uploads.poll(),
            Some(UploadUpdate::Prepared {
                request: 10,
                result: Err(UploadError::Cancelled),
                ..
            })
        ));
        assert!(uploads.shared.lock().unwrap().tokens.is_empty());
        // Registry cleanup is performed on the actor, never on the UI poll.
        assert_eq!(budget.used(), (11, 2));
        entries.clear();
        assert_eq!(budget.used(), (0, 0));
    }
}

#[test]
fn a_multi_file_retry_keeps_the_complete_frozen_order_and_bytes() {
    let mut h = Harness::new(1);
    h.uploads.limits.as_mut().unwrap().max_files = 8;
    h.insert(UploadToken(10), b"first frozen body");
    h.insert(UploadToken(20), b"second frozen body");
    h.uploads
        .send(
            1,
            h.channel,
            "text".into(),
            None,
            false,
            vec![UploadToken(10), UploadToken(20)],
            30,
            Duration::from_secs(30),
        )
        .unwrap();
    let Command::SendFiles { files: first, .. } = h.command() else {
        panic!("files required")
    };
    h.uploads.observe(
        1,
        &Event::Failed {
            failure: Failure::SendFiles {
                request: 1,
                channel: h.channel,
                nonce: 30,
            },
            reason: "simulated retry".into(),
            ambiguous: false,
        },
    );
    h.uploads.retry(2, 30, Duration::from_secs(30)).unwrap();
    let Command::SendFiles {
        scope,
        content,
        files,
        ..
    } = h.command()
    else {
        panic!("files required")
    };
    assert_eq!((scope.request, scope.nonce), (2, 30));
    assert_eq!(content, "text");
    assert_eq!(files.len(), 2);
    for (retried, original) in files.iter().zip(&first) {
        assert_eq!(retried.metadata(), original.metadata());
        assert_eq!(retried.bytes(), original.bytes());
        assert_eq!(retried.bytes().as_ptr(), original.bytes().as_ptr());
    }
    assert_eq!(files[0].bytes().as_ref(), b"first frozen body");
    assert_eq!(files[1].bytes().as_ref(), b"second frozen body");
}

#[test]
fn account_change_during_a_pick_delivers_one_scoped_cancelled_terminal() {
    for signed_out in [true, false] {
        let (mut uploads, _queue, channel, account) = admission();
        uploads.pick(10, channel, None).unwrap();
        if signed_out {
            uploads.observe(1, &Event::Authentication(AuthenticationState::SignedOut));
        } else {
            let mut replacement = MockProvider::new(MockConfig::default()).snapshot();
            replacement.me.as_mut().unwrap().id = UserId(account.0 + 1);
            uploads.observe(2, &Event::Ready(Box::new(replacement)));
        }
        // Completion of the cancelled worker must retire the old UI picker,
        // even though its account is no longer the active one. No metadata
        // from that account may be returned as a successful preparation.
        uploads.shared.lock().unwrap().pick.as_mut().unwrap().result =
            Some(Err(UploadError::Cancelled));
        assert!(matches!(uploads.poll(), Some(UploadUpdate::Prepared {
            account: source, channel: target, request: 10,
            result: Err(UploadError::Cancelled),
        }) if source == account && target == channel));
        assert!(uploads.poll().is_none());
        assert!(uploads.shared.lock().unwrap().tokens.is_empty());
        if !signed_out {
            uploads.pick(11, channel, None).unwrap();
        }
    }
}
