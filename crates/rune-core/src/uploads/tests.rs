use super::*;
use rune_model::{ChannelKind, Delivery, Message, MessageKind, Timestamp};
use rune_provider::{
    mock::{MockConfig, MockProvider},
    Snapshot,
};
use std::sync::mpsc as sync;

mod ack_validation;
mod batch;
mod certainty;
mod drain;
mod paths;

struct Harness {
    uploads: Uploads,
    commands: mpsc::Receiver<Command>,
    provider_commands: mpsc::Sender<Command>,
    wake: sync::Receiver<()>,
    snapshot: Snapshot,
    channel: ChannelId,
    account: UserId,
    _runtime: RuntimeOwner,
}

impl Harness {
    fn new(capacity: usize) -> Self {
        let runtime = RuntimeOwner::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .thread_name("upload-test-runtime")
                .build()
                .unwrap(),
        );
        let (commands, receiver) = mpsc::channel(capacity);
        let (wake, woke) = sync::channel();
        let mut uploads = Uploads::new(
            runtime.handle().clone(),
            commands.clone(),
            Some(UploadLimits {
                max_files: 1,
                max_file_bytes: 20 * 1024 * 1024,
                max_message_bytes: 25 * 1024 * 1024,
            }),
            true,
            Arc::new(move || {
                let _ = wake.send(());
            }),
        );
        let snapshot = MockProvider::new(MockConfig::default()).snapshot();
        let account = snapshot.me.as_ref().unwrap().id;
        let channel = snapshot
            .channels
            .iter()
            .find(|channel| {
                channel.kind == ChannelKind::Text && channel.permissions.contains(REQUIRED)
            })
            .unwrap()
            .id;
        uploads.observe(1, &Event::Ready(Box::new(snapshot.clone())));
        Self {
            uploads,
            commands: receiver,
            provider_commands: commands,
            wake: woke,
            snapshot,
            channel,
            account,
            _runtime: runtime,
        }
    }
    fn until(&self, mut predicate: impl FnMut(&Control) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if predicate(&self.uploads.shared.lock().unwrap()) {
                return;
            }
            self.wake
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("the actor must complete without UI polling");
        }
    }
    fn insert(&mut self, token: UploadToken, body: &[u8]) -> Arc<prepare::Budget> {
        let budget = Arc::new(prepare::Budget::default());
        let prepared = PreparedUpload::new(
            PendingAttachment {
                token,
                filename: "fixture.txt".into(),
                size_bytes: body.len() as u64,
            },
            prepare::seal(
                body.to_vec().into_boxed_slice(),
                budget.reserve(body.len()).unwrap(),
            ),
        )
        .unwrap();
        self.uploads
            .jobs
            .try_send(Job::Insert {
                account: self.account,
                channel: self.channel,
                prepared,
            })
            .ok()
            .unwrap();
        self.until(|state| state.tokens.contains_key(&token));
        budget
    }
    fn send(&mut self, request: u64, nonce: u64, token: UploadToken) {
        self.uploads
            .send(
                request,
                self.channel,
                String::new(),
                None,
                false,
                vec![token],
                nonce,
                Duration::from_secs(30),
            )
            .unwrap();
    }
    fn command(&mut self) -> Command {
        self._runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(3), self.commands.recv())
                .await
                .unwrap()
                .unwrap()
        })
    }
    fn failed(&self, nonce: u64) {
        self.until(|state| {
            state
                .sends
                .get(&nonce)
                .is_some_and(|send| matches!(send.phase, Phase::Failed { .. }))
        });
    }
}

fn ack(account: UserId, channel: ChannelId, nonce: u64) -> Event {
    Event::MessageCreated {
        nonce: Some(nonce),
        message: Message {
            id: MessageId(900),
            channel,
            author: account,
            content: String::new(),
            timestamp: Timestamp(1),
            edited: None,
            kind: MessageKind::Default,
            reply_to: None,
            mentions: vec![],
            reactions: vec![],
            attachments: vec![rune_model::Attachment {
                id: rune_model::AttachmentId(902),
                filename: "fixture.txt".into(),
                size_bytes: 4,
                content_type: None,
                dimensions: None,
                preview: None,
                download: None,
                spoiler: false,
                voice: None,
                explicit: false,
                placeholder: None,
            }],
            stickers: vec![],
            text_references: vec![],
            delivery: Delivery::Sent,
            pinned: false,
            embeds: Vec::new(),
            forwarded: None,
            poll: None,
            components: Default::default(),
            call: None,
            mention_roles: Vec::new(),
            mention_everyone: false,
        },
    }
}

#[test]
fn prepared_files_do_not_send_until_an_explicit_file_only_submission() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"sealed fixture");
    assert!(h.commands.try_recv().is_err());
    h.send(11, 21, UploadToken(10));
    let Command::SendFiles {
        scope,
        content,
        files,
        fence,
        ..
    } = h.command()
    else {
        panic!("never fall back to a text command")
    };
    assert_eq!(
        (
            scope.request,
            scope.nonce,
            scope.account,
            scope.channel,
            scope.generation
        ),
        (11, 21, h.account, h.channel, 1)
    );
    assert!(content.is_empty());
    assert_eq!(files[0].bytes().as_ref(), b"sealed fixture");
    assert!(!fence.is_cancelled());
}

#[test]
fn missing_or_foreign_tokens_never_admit_a_text_fallback() {
    let mut h = Harness::new(8);
    assert_eq!(
        h.uploads.send(
            1,
            h.channel,
            "text".into(),
            None,
            false,
            vec![UploadToken(999)],
            2,
            Duration::from_secs(30)
        ),
        Err(UploadError::NotFound)
    );
    h.insert(UploadToken(10), b"file");
    let other = h
        .snapshot
        .channels
        .iter()
        .find(|channel| {
            channel.id != h.channel
                && channel.kind.has_messages()
                && channel.permissions.contains(REQUIRED)
        })
        .unwrap()
        .id;
    assert_eq!(
        h.uploads.send(
            3,
            other,
            "text".into(),
            None,
            false,
            vec![UploadToken(10)],
            4,
            Duration::from_secs(30)
        ),
        Err(UploadError::NotFound)
    );
    assert!(h.commands.try_recv().is_err());
}

#[test]
fn a_full_provider_queue_preserves_the_frozen_send_for_explicit_retry() {
    let mut h = Harness::new(1);
    h.provider_commands.try_send(Command::Cancel(999)).unwrap();
    h.insert(UploadToken(10), b"retry frozen bytes");
    h.send(11, 21, UploadToken(10));
    h.failed(21);
    let mut failures = VecDeque::new();
    h.uploads.failures(&mut failures);
    assert!(matches!(
        failures.pop_front(),
        Some(Event::Failed {
            failure: Failure::SendFiles {
                request: 11,
                nonce: 21,
                ..
            },
            ambiguous: false,
            ..
        })
    ));
    assert_eq!(h.command(), Command::Cancel(999));
    h.uploads.retry(12, 21, Duration::from_secs(30)).unwrap();
    let Command::SendFiles { scope, files, .. } = h.command() else {
        panic!()
    };
    assert_eq!((scope.request, scope.nonce), (12, 21));
    assert_eq!(files[0].bytes().as_ref(), b"retry frozen bytes");
    assert!(!h.uploads.observe(
        1,
        &Event::Failed {
            failure: Failure::SendFiles {
                request: 11,
                channel: h.channel,
                nonce: 21
            },
            reason: "old queue failure".into(),
            ambiguous: false
        }
    ));
}

#[test]
fn acknowledgements_require_the_original_account_channel_and_generation() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    h.send(11, 21, UploadToken(10));
    let _command = h.command();
    assert!(!h
        .uploads
        .observe(1, &ack(UserId(h.account.0 + 1), h.channel, 21)));
    assert!(!h
        .uploads
        .observe(1, &ack(h.account, ChannelId(h.channel.0 + 1), 21)));
    assert!(h
        .uploads
        .shared
        .lock()
        .unwrap()
        .tokens
        .contains_key(&UploadToken(10)));
    assert!(h.uploads.observe(1, &ack(h.account, h.channel, 21)));
    assert!(h.uploads.shared.lock().unwrap().tokens.is_empty());
}

#[test]
fn reconnect_cancels_queued_commands_but_preserves_prepared_bytes() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"same bytes after reconnect");
    h.send(11, 21, UploadToken(10));
    let Command::SendFiles { fence, .. } = h.command() else {
        panic!()
    };
    h.uploads.observe(
        1,
        &Event::Connection(ConnectionState::Reconnecting {
            attempt: 1,
            retry_in_ms: 1,
        }),
    );
    assert!(fence.is_cancelled());
    h.uploads
        .observe(2, &Event::Ready(Box::new(h.snapshot.clone())));
    h.uploads.retry(12, 21, Duration::from_secs(30)).unwrap();
    let Command::SendFiles {
        scope,
        files,
        fence,
        ..
    } = h.command()
    else {
        panic!()
    };
    assert_eq!(scope.generation, 2);
    assert!(!fence.is_cancelled());
    assert_eq!(files[0].bytes().as_ref(), b"same bytes after reconnect");
}

#[test]
fn account_change_and_revocation_invalidate_grants_and_live_fences() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    h.send(11, 21, UploadToken(10));
    let Command::SendFiles { fence, .. } = h.command() else {
        panic!()
    };
    h.uploads
        .observe(1, &Event::ChannelRemoved { channel: h.channel });
    assert!(fence.is_cancelled());
    assert_eq!(
        h.uploads.retry(12, 21, Duration::from_secs(30)),
        Err(UploadError::Denied)
    );
    assert!(h.uploads.shared.lock().unwrap().tokens.is_empty());
    h.uploads
        .observe(1, &Event::Ready(Box::new(h.snapshot.clone())));
    h.insert(UploadToken(20), b"account-local bytes");
    let mut next = h.snapshot.clone();
    next.me.as_mut().unwrap().id = UserId(h.account.0 + 1);
    h.uploads.observe(2, &Event::Ready(Box::new(next)));
    assert!(h.uploads.shared.lock().unwrap().tokens.is_empty());
    assert_eq!(
        h.uploads.send(
            31,
            h.channel,
            String::new(),
            None,
            false,
            vec![UploadToken(20)],
            41,
            Duration::from_secs(30)
        ),
        Err(UploadError::NotFound)
    );
}

#[test]
fn progress_is_scoped_monotonic_and_does_not_acknowledge_delivery() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    h.send(11, 21, UploadToken(10));
    let Command::SendFiles { scope, .. } = h.command() else {
        panic!()
    };
    assert!(h.uploads.observe(
        1,
        &Event::UploadProgress {
            scope,
            sent_bytes: 4,
            total_bytes: 4
        }
    ));
    assert!(!h.uploads.observe(
        1,
        &Event::UploadProgress {
            scope,
            sent_bytes: 3,
            total_bytes: 4
        }
    ));
    assert!(!h.uploads.observe(
        1,
        &Event::UploadProgress {
            scope,
            sent_bytes: 5,
            total_bytes: 5
        }
    ));
    assert!(h
        .uploads
        .shared
        .lock()
        .unwrap()
        .tokens
        .contains_key(&UploadToken(10)));
    assert_eq!(
        h.uploads.retry(12, 21, Duration::from_secs(30)),
        Err(UploadError::Busy)
    );
}

#[test]
fn a_post_carries_its_prepared_files_and_keeps_them_until_it_is_made() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    let post = |request| Command::CreatePost {
        request,
        forum: h.channel,
        title: "¿Este lente?".into(),
        tags: Vec::new(),
        content: String::new(),
        files: Vec::new(),
    };
    let (first, second) = (post(31), post(32));
    assert_eq!(
        h.uploads.post(first.clone(), vec![UploadToken(99)]),
        Err(UploadError::NotFound)
    );
    h.uploads.post(first, vec![UploadToken(10)]).unwrap();
    let Command::CreatePost { request, files, .. } = h.command() else {
        panic!()
    };
    assert_eq!(request, 31);
    assert_eq!(files.len(), 1);
    assert_eq!(&files[0].bytes()[..], b"file");
    // One post at a time per file.
    assert_eq!(
        h.uploads.post(second.clone(), vec![UploadToken(10)]),
        Err(UploadError::Busy)
    );
    // Refused: the file stays for another try, which takes it.
    h.uploads.observe(
        1,
        &Event::Failed {
            failure: Failure::NewThread { request: 31 },
            reason: "no".into(),
            ambiguous: false,
        },
    );
    h.uploads.post(second, vec![UploadToken(10)]).unwrap();
    assert!(matches!(
        h.command(),
        Command::CreatePost { request: 32, .. }
    ));
    h.uploads.observe(
        1,
        &Event::Created {
            request: 32,
            created: rune_provider::Created::Channel(ChannelId(62)),
        },
    );
    assert!(!h
        .uploads
        .shared
        .lock()
        .unwrap()
        .tokens
        .contains_key(&UploadToken(10)));
}

/// An app's modal answered in `channel` as `request`, naming `token`.
fn modal_answer(channel: ChannelId, request: RequestId, token: UploadToken) -> Command {
    Command::Interact {
        request,
        channel,
        interaction: rune_model::interactions::Interaction::Modal {
            id: 5,
            app: rune_model::UserId(9),
            custom_id: "form".into(),
            answers: vec![(
                "f".into(),
                rune_model::interactions::FieldValue::Files(vec![token]),
            )],
        },
        files: Vec::new(),
    }
}

#[test]
fn a_modal_carries_its_files_until_its_app_answers() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    let channel = h.channel;
    let answer = move |request| modal_answer(channel, request, UploadToken(10));
    h.uploads.post(answer(41), vec![UploadToken(10)]).unwrap();
    let Command::Interact { request, files, .. } = h.command() else {
        panic!()
    };
    assert_eq!(request, 41);
    assert_eq!(&files[0].bytes()[..], b"file");
    // The app refused it: the file stays for another try.
    h.uploads.observe(
        1,
        &Event::Failed {
            failure: Failure::Interaction {
                request: 41,
                channel,
            },
            reason: "no".into(),
            ambiguous: false,
        },
    );
    h.uploads.post(answer(42), vec![UploadToken(10)]).unwrap();
    assert!(matches!(h.command(), Command::Interact { request: 42, .. }));
    // Got, it still waits; answered, the file went with it.
    h.uploads.observe(
        1,
        &Event::Interaction {
            request: 42,
            done: false,
        },
    );
    assert!(h.uploads.shared.lock().unwrap().posts.contains_key(&42));
    h.uploads.observe(
        1,
        &Event::Interaction {
            request: 42,
            done: true,
        },
    );
    assert!(!h
        .uploads
        .shared
        .lock()
        .unwrap()
        .tokens
        .contains_key(&UploadToken(10)));
    // A command that carries no files is not one of these.
    assert_eq!(
        h.uploads.post(Command::Cancel(1), vec![UploadToken(10)]),
        Err(UploadError::InvalidFile)
    );
}

#[test]
fn a_modal_no_longer_waited_for_lets_the_next_carry_its_files() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    let channel = h.channel;
    let answer = move |request| modal_answer(channel, request, UploadToken(10));
    // Its app never answered, and the interface stopped waiting: sent
    // again, the files go again rather than being busy.
    h.uploads.post(answer(41), vec![UploadToken(10)]).unwrap();
    assert!(matches!(h.command(), Command::Interact { request: 41, .. }));
    h.uploads.post(answer(42), vec![UploadToken(10)]).unwrap();
    assert!(matches!(h.command(), Command::Interact { request: 42, .. }));
    // The first one's late answer takes nothing of the second's.
    h.uploads.observe(
        1,
        &Event::Interaction {
            request: 41,
            done: true,
        },
    );
    assert!(h
        .uploads
        .shared
        .lock()
        .unwrap()
        .tokens
        .contains_key(&UploadToken(10)));
    // Let go of, nothing holds it any longer.
    h.uploads.discard(UploadToken(10));
    assert!(h.uploads.shared.lock().unwrap().posts.is_empty());
}

#[test]
fn a_conversation_only_read_takes_a_modals_files_but_sends_none() {
    let mut h = Harness::new(8);
    let mut read_only = h
        .snapshot
        .channels
        .iter()
        .find(|channel| channel.id == h.channel)
        .unwrap()
        .clone();
    read_only.permissions = read_only.permissions.without(Permissions::SEND);
    h.uploads.observe(1, &Event::ChannelChanged(read_only));
    h.insert(UploadToken(10), b"file");
    assert_eq!(
        h.uploads.send(
            1,
            h.channel,
            String::new(),
            None,
            false,
            vec![UploadToken(10)],
            7,
            Duration::from_secs(30),
        ),
        Err(UploadError::Denied)
    );
    h.uploads
        .post(
            modal_answer(h.channel, 2, UploadToken(10)),
            vec![UploadToken(10)],
        )
        .unwrap();
    assert!(matches!(h.command(), Command::Interact { request: 2, .. }));
}

#[test]
fn a_send_waiting_on_a_verification_keeps_its_time_and_restarts_progress() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    h.send(11, 21, UploadToken(10));
    let Command::SendFiles { scope, .. } = h.command() else {
        panic!()
    };
    let progress = |sent_bytes| Event::UploadProgress {
        scope,
        sent_bytes,
        total_bytes: 4,
    };
    assert!(h.uploads.observe(1, &progress(4)));
    let now = Instant::now();
    h.uploads
        .challenged(11, now, rune_provider::CHALLENGE_LIFETIME);
    assert!(h.uploads.deadline().unwrap() >= now + rune_provider::CHALLENGE_LIFETIME);
    // Past the first attempt's time, it still waits.
    h.uploads.expire(scope.expires_at + Duration::from_secs(1));
    assert!(h.uploads.shared.lock().unwrap().sends[&21].phase == Phase::Waiting);
    // The retry stages the file again from its start.
    assert!(h.uploads.observe(1, &progress(2)));
}

#[test]
fn cancellation_is_effective_even_when_the_provider_queue_is_full() {
    let mut h = Harness::new(1);
    h.insert(UploadToken(10), b"file");
    h.send(11, 21, UploadToken(10));
    h.until(|state| {
        state
            .sends
            .get(&21)
            .is_some_and(|send| send.phase == Phase::Waiting)
    });
    h.uploads.cancel(11);
    let Command::SendFiles { fence, .. } = h.command() else {
        panic!()
    };
    assert!(fence.is_cancelled());
    let mut failures = VecDeque::new();
    h.uploads.failures(&mut failures);
    assert!(matches!(
        failures.pop_front(),
        Some(Event::Failed {
            failure: Failure::SendFiles {
                request: 11,
                nonce: 21,
                ..
            },
            ambiguous: true,
            ..
        })
    ));
}

#[test]
fn ambiguous_retries_require_the_provider_idempotency_guarantee() {
    let mut h = Harness::new(8);
    h.uploads.idempotent = false;
    h.insert(UploadToken(10), b"file");
    h.send(11, 21, UploadToken(10));
    let _ = h.command();
    let deadline = h.uploads.deadline().unwrap();
    h.uploads.expire(deadline);
    assert_eq!(
        h.uploads.retry(12, 21, Duration::from_secs(30)),
        Err(UploadError::Unavailable)
    );
    assert!(h.commands.try_recv().is_err());
}

#[test]
fn text_counts_towards_message_limit_without_changing_file_progress_total() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    h.uploads.limits.as_mut().unwrap().max_message_bytes = 4;
    assert_eq!(
        h.uploads.send(
            11,
            h.channel,
            "x".into(),
            None,
            false,
            vec![UploadToken(10)],
            21,
            Duration::from_secs(30)
        ),
        Err(UploadError::TooLarge)
    );
    h.uploads.limits.as_mut().unwrap().max_message_bytes = 100;
    h.uploads
        .send(
            12,
            h.channel,
            "text".into(),
            None,
            false,
            vec![UploadToken(10)],
            22,
            Duration::from_secs(30),
        )
        .unwrap();
    let Command::SendFiles { scope, .. } = h.command() else {
        panic!()
    };
    assert!(h.uploads.observe(
        1,
        &Event::UploadProgress {
            scope,
            sent_bytes: 4,
            total_bytes: 4
        }
    ));
}

#[test]
fn replies_require_read_history_at_admission_and_when_support_is_revoked() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    let mut channel = h
        .snapshot
        .channels
        .iter()
        .find(|channel| channel.id == h.channel)
        .unwrap()
        .clone();
    channel.permissions = channel.permissions.without(Permissions::READ_HISTORY);
    h.uploads
        .observe(1, &Event::ChannelChanged(channel.clone()));
    assert_eq!(
        h.uploads.send(
            11,
            h.channel,
            String::new(),
            Some(MessageId(80)),
            false,
            vec![UploadToken(10)],
            21,
            Duration::from_secs(30)
        ),
        Err(UploadError::Denied)
    );
    channel.permissions = channel.permissions | Permissions::READ_HISTORY;
    h.uploads
        .observe(1, &Event::ChannelChanged(channel.clone()));
    h.uploads
        .send(
            12,
            h.channel,
            String::new(),
            Some(MessageId(80)),
            true,
            vec![UploadToken(10)],
            22,
            Duration::from_secs(30),
        )
        .unwrap();
    // The reply's mention goes with its files.
    let Command::SendFiles {
        fence,
        mention_reply: true,
        ..
    } = h.command()
    else {
        panic!()
    };
    channel.permissions = channel.permissions.without(Permissions::READ_HISTORY);
    h.uploads.observe(1, &Event::ChannelChanged(channel));
    assert!(fence.is_cancelled());
    assert_eq!(
        h.uploads.retry(13, 22, Duration::from_secs(30)),
        Err(UploadError::Denied)
    );
}

#[test]
fn a_cancelled_attempt_does_not_consume_an_incomplete_file_acknowledgement() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    h.send(11, 21, UploadToken(10));
    let _command = h.command();
    h.uploads.cancel(11);
    let mut incomplete = ack(h.account, h.channel, 21);
    let Event::MessageCreated { message, .. } = &mut incomplete else {
        unreachable!()
    };
    message.attachments.clear();
    assert!(!h.uploads.observe(1, &incomplete));
    assert!(h
        .uploads
        .shared
        .lock()
        .unwrap()
        .tokens
        .contains_key(&UploadToken(10)));
}

#[test]
fn a_complete_late_ack_after_cancel_resolves_only_the_original_logical_send() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    h.insert(UploadToken(20), b"new draft");
    h.send(11, 21, UploadToken(10));
    let _command = h.command();
    h.uploads.cancel(11);
    assert!(h.uploads.observe(1, &ack(h.account, h.channel, 21)));
    let state = h.uploads.shared.lock().unwrap();
    assert!(!state.tokens.contains_key(&UploadToken(10)));
    assert!(state.tokens.contains_key(&UploadToken(20)));
    assert!(!state.sends.contains_key(&21));
}

/// What a test waits for, from what one poll brought.
type Until<'a> = dyn FnMut(&mut crate::Client, Vec<crate::Update>) -> bool + 'a;

#[test]
fn a_post_with_files_reaches_the_simulation_whole() {
    use crate::{Backoff, Client, ClientConfig, Deadlines, Update};
    let (wake, woke) = sync::channel();
    let provider = Arc::new(MockProvider::new(MockConfig {
        latency: Duration::ZERO,
        ambient: false,
        replies: false,
        ..MockConfig::default()
    }));
    let snapshot = provider.snapshot();
    let forum = snapshot
        .channels
        .iter()
        .find(|channel| channel.kind == ChannelKind::Forum)
        .unwrap()
        .id;
    let account = snapshot.me.as_ref().unwrap().id;
    let mut client = Client::start(
        provider.clone(),
        ClientConfig {
            storage: None,
            keyring: None,
            wake: Arc::new(move || {
                let _ = wake.send(());
            }),
            backoff: Backoff::default(),
            deadlines: Deadlines::default(),
            image_cache: None,
        },
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    let wait = |client: &mut Client, mut done: Box<Until>| loop {
        let updates = client.poll(64);
        if done(client, updates) {
            return;
        }
        woke.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    };
    wait(
        &mut client,
        Box::new(|client, _| client.uploads.shared.lock().unwrap().account.is_some()),
    );
    let prepared = PreparedUpload::new(
        PendingAttachment {
            token: UploadToken(91),
            filename: "lente.jpg".into(),
            size_bytes: 3,
        },
        bytes::Bytes::from_static(b"jpg"),
    )
    .unwrap();
    client
        .uploads
        .jobs
        .try_send(Job::Insert {
            account,
            channel: forum,
            prepared,
        })
        .ok()
        .unwrap();
    wait(
        &mut client,
        Box::new(|client, _| {
            client
                .uploads
                .shared
                .lock()
                .unwrap()
                .tokens
                .contains_key(&UploadToken(91))
        }),
    );
    client
        .create_post(
            200,
            forum,
            "¿Este lente?".into(),
            Vec::new(),
            String::new(),
            vec![UploadToken(91)],
        )
        .unwrap();
    let mut post = None;
    wait(
        &mut client,
        Box::new(|_, updates| {
            post = post.or(updates.into_iter().find_map(|update| match update {
                Update::Provider(Event::Created {
                    request: 200,
                    created: rune_provider::Created::Channel(post),
                }) => Some(post),
                _ => None,
            }));
            post.is_some()
        }),
    );
    let (first, _) = provider.history(post.unwrap(), None, 10);
    assert_eq!(first[0].attachments[0].filename, "lente.jpg");
    // Made: its file is let go.
    assert!(!client
        .uploads
        .shared
        .lock()
        .unwrap()
        .tokens
        .contains_key(&UploadToken(91)));
}

fn client_sending_file() -> (crate::Client, UserId, ChannelId, u64) {
    use crate::{Backoff, Client, ClientConfig, Deadlines};
    let (wake, woke) = sync::channel();
    let provider = Arc::new(MockProvider::new(MockConfig {
        latency: Duration::ZERO,
        ambient: false,
        replies: false,
        ..MockConfig::default()
    }));
    let snapshot = provider.snapshot();
    let channel = snapshot
        .channels
        .iter()
        .find(|channel| channel.kind == ChannelKind::Text && channel.permissions.contains(REQUIRED))
        .unwrap()
        .id;
    let account = snapshot.me.as_ref().unwrap().id;
    let mut client = Client::start(
        provider,
        ClientConfig {
            storage: None,
            keyring: None,
            wake: Arc::new(move || {
                let _ = wake.send(());
            }),
            backoff: Backoff::default(),
            deadlines: Deadlines::default(),
            image_cache: None,
        },
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while client.uploads.shared.lock().unwrap().account.is_none() {
        client.poll(64);
        if client.uploads.shared.lock().unwrap().account.is_none() {
            woke.recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .unwrap();
        }
    }
    let metadata = PendingAttachment {
        token: UploadToken(90),
        filename: "fixture.txt".into(),
        size_bytes: 4,
    };
    let prepared = PreparedUpload::new(metadata, bytes::Bytes::from_static(b"file")).unwrap();
    client
        .uploads
        .jobs
        .try_send(Job::Insert {
            account,
            channel,
            prepared,
        })
        .ok()
        .unwrap();
    while !client
        .uploads
        .shared
        .lock()
        .unwrap()
        .tokens
        .contains_key(&UploadToken(90))
    {
        woke.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
    let nonce = client
        .send_attachments(
            100,
            channel,
            String::new(),
            None,
            false,
            vec![UploadToken(90)],
        )
        .unwrap();
    while !client
        .uploads
        .shared
        .lock()
        .unwrap()
        .sends
        .get(&nonce)
        .is_some_and(|send| send.phase == Phase::Waiting)
    {
        woke.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
    }
    (client, account, channel, nonce)
}

#[test]
fn client_preserves_foreign_incoming_messages_with_a_colliding_local_nonce() {
    use rune_provider::Envelope;
    let (mut client, account, channel, nonce) = client_sending_file();
    let incoming = ack(UserId(account.0 + 100), channel, nonce);
    let mut uncorrelated = incoming.clone();
    let Event::MessageCreated {
        nonce: accepted_nonce,
        ..
    } = &mut uncorrelated
    else {
        unreachable!()
    };
    *accepted_nonce = None;
    let generation = client.generation;
    assert_eq!(
        client.accept(
            Envelope {
                generation,
                event: incoming.clone()
            },
            &mut Vec::new()
        ),
        Some(uncorrelated)
    );
    assert!(client
        .uploads
        .shared
        .lock()
        .unwrap()
        .sends
        .contains_key(&nonce));
    assert!(client
        .uploads
        .shared
        .lock()
        .unwrap()
        .tokens
        .contains_key(&UploadToken(90)));
}

#[test]
fn client_never_relabels_a_local_or_zero_identity_ack_as_a_remote_message() {
    use rune_provider::Envelope;
    for invalid in 0..3 {
        let (mut client, account, channel, nonce) = client_sending_file();
        // Exercise the pre-persistence mutation list as well as UI delivery.
        // No SQLite worker is needed to prove that this event produces neither.
        client.storage_active = true;
        let mut incoming = ack(account, channel, nonce);
        let Event::MessageCreated { message, .. } = &mut incoming else {
            unreachable!()
        };
        match invalid {
            0 => message.delivery = Delivery::Sending,
            1 => message.id = MessageId(0),
            _ => message.attachments[0].id = rune_model::AttachmentId(0),
        }
        let mut mutations = Vec::new();
        let generation = client.generation;
        assert!(
            client
                .accept(
                    Envelope {
                        generation,
                        event: incoming
                    },
                    &mut mutations
                )
                .is_none(),
            "invalid remote identity must not be converted into an uncorrelated message: {invalid}"
        );
        assert!(mutations.is_empty());
        assert!(client
            .uploads
            .shared
            .lock()
            .unwrap()
            .tokens
            .contains_key(&UploadToken(90)));
    }
}

#[test]
fn client_preserves_a_remote_join_with_a_colliding_file_nonce_without_confirming_it() {
    use rune_provider::Envelope;
    let (mut client, account, channel, nonce) = client_sending_file();
    let mut incoming = ack(account, channel, nonce);
    let Event::MessageCreated { message, .. } = &mut incoming else {
        unreachable!()
    };
    message.kind = MessageKind::Join;
    let mut expected = incoming.clone();
    let Event::MessageCreated {
        nonce: correlation, ..
    } = &mut expected
    else {
        unreachable!()
    };
    *correlation = None;
    let generation = client.generation;
    assert_eq!(
        client.accept(
            Envelope {
                generation,
                event: incoming
            },
            &mut Vec::new()
        ),
        Some(expected)
    );
    assert!(client
        .uploads
        .shared
        .lock()
        .unwrap()
        .tokens
        .contains_key(&UploadToken(90)));
}
