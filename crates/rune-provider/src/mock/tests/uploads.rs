use super::*;
use crate::{PreparedUpload, UploadFence, UploadSendScope};
use bytes::Bytes;
use rune_model::{PendingAttachment, UploadToken};

mod multiple;

fn files(
    channel: ChannelId,
    request: u64,
    nonce: u64,
    content: &str,
    data: &'static [u8],
) -> (tokio::sync::watch::Sender<bool>, Command) {
    let (control, fence) = UploadFence::new();
    let file = PreparedUpload::new(
        PendingAttachment {
            token: UploadToken(1),
            filename: "fixture.txt".into(),
            size_bytes: data.len() as u64,
        },
        Bytes::from_static(data),
    )
    .unwrap();
    (
        control,
        Command::SendFiles {
            scope: UploadSendScope {
                request,
                account: ME,
                channel,
                generation: 3,
                nonce,
                expires_at: std::time::Instant::now() + Duration::from_secs(30),
            },
            content: content.into(),
            reply_to: None,
            files: vec![file],
            fence,
            mention_reply: false,
        },
    )
}

async fn outcome(session: &mut Session) -> Event {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(1), session.next())
            .await
            .expect("a file send has a bounded terminal result");
        if !matches!(event, Event::UploadProgress { .. }) {
            return event;
        }
    }
}

fn created(event: Event, nonce: u64) -> Message {
    let Event::MessageCreated {
        message,
        nonce: Some(actual),
    } = event
    else {
        panic!("confirmed file message expected: {event:?}")
    };
    assert_eq!(actual, nonce);
    message
}

#[tokio::test(start_paused = true)]
async fn uploads_confirm_file_only_messages_and_retry_the_same_nonce_without_duplication() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let (_first, command) = files(channel, 700, 701, "", b"fixture");
    session.send(command).await;
    let message = created(outcome(&mut session).await, 701);
    assert!(message.content.is_empty());
    assert_eq!((message.author, message.channel), (ME, channel));
    assert_eq!(message.attachments.len(), 1);
    assert_eq!(
        (
            &*message.attachments[0].filename,
            message.attachments[0].size_bytes
        ),
        ("fixture.txt", 7)
    );
    assert!(message.attachments[0].download.is_none());
    assert!(message.attachments[0].preview.is_none());

    let (_retry, command) = files(channel, 702, 701, "", b"fixture");
    session.send(command).await;
    assert_eq!(created(outcome(&mut session).await, 701), message);
    let world = world.lock().unwrap();
    assert_eq!(
        world.sims[&channel]
            .extra
            .iter()
            .filter(|entry| entry.id == message.id)
            .count(),
        1
    );
}

#[tokio::test(start_paused = true)]
async fn uploads_reject_a_retry_that_changes_payload_under_the_same_nonce() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let (_first, command) = files(channel, 710, 711, "original", b"fixture");
    session.send(command).await;
    let original = created(outcome(&mut session).await, 711);
    for (request, text, body) in [
        (712, "changed", &b"fixture"[..]),
        (713, "original", &b"changed"[..]),
    ] {
        let (_retry, command) = files(channel, request, 711, text, body);
        session.send(command).await;
        assert!(
            matches!(outcome(&mut session).await, Event::Failed { failure: Failure::SendFiles { request: found, channel: actual, nonce: 711 }, ambiguous: false, .. } if found == request && actual == channel)
        );
    }
    let world = world.lock().unwrap();
    assert_eq!(world.message(channel, original.id).unwrap(), original);
    assert_eq!(world.sims[&channel].extra.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn uploads_reject_foreign_account_generation_and_expired_commands_without_posts() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    for index in 0..3 {
        let (_control, mut command) = files(channel, 720 + index, 730 + index, "", b"fixture");
        if let Command::SendFiles { scope, .. } = &mut command {
            match index {
                0 => scope.account = UserId(999),
                1 => scope.generation = 2,
                _ => scope.expires_at = std::time::Instant::now() - Duration::from_secs(1),
            }
        }
        session.send(command).await;
        assert!(matches!(
            outcome(&mut session).await,
            Event::Failed {
                failure: Failure::SendFiles { .. },
                ambiguous: false,
                ..
            }
        ));
    }
    assert!(world.lock().unwrap().sims[&channel].extra.is_empty());
}

#[tokio::test(start_paused = true)]
async fn uploads_revalidate_permissions_after_admission_and_cancel_before_commit() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let (control, command) = files(channel, 740, 741, "", b"fixture");
    session.send(command).await;
    assert!(
        matches!(session.next().await, Event::UploadProgress { scope, sent_bytes: 0, .. } if scope.request == 740)
    );
    assert!(
        world.lock().unwrap().sims[&channel].extra.is_empty(),
        "admission is not a commit"
    );
    control.send_replace(true);
    assert!(matches!(
        outcome(&mut session).await,
        Event::Failed {
            failure: Failure::SendFiles { request: 740, .. },
            ambiguous: false,
            ..
        }
    ));
    let (_control, command) = files(channel, 742, 743, "", b"fixture");
    session.send(command).await;
    assert!(
        matches!(session.next().await, Event::UploadProgress { scope, sent_bytes: 0, .. } if scope.request == 742)
    );
    assert!(world.lock().unwrap().sims[&channel].extra.is_empty());
    world
        .lock()
        .unwrap()
        .channels
        .iter_mut()
        .find(|entry| entry.id == channel)
        .unwrap()
        .permissions = Permissions::VIEW | Permissions::SEND;
    assert!(matches!(
        outcome(&mut session).await,
        Event::Failed {
            failure: Failure::SendFiles { request: 742, .. },
            ambiguous: false,
            ..
        }
    ));
    assert!(world.lock().unwrap().sims[&channel].extra.is_empty());
}

#[test]
fn uploads_declare_only_the_tested_simulation_policy() {
    let provider = MockProvider::new(config());
    assert!(provider.info().capabilities.attachments);
    assert_eq!(
        provider.upload_limits(),
        Some(crate::UploadLimits {
            max_files: 10,
            max_file_bytes: 20 * 1024 * 1024,
            max_message_bytes: 25 * 1024 * 1024
        })
    );
}

#[tokio::test(start_paused = true)]
async fn uploads_lost_ack_retries_return_the_committed_message_and_progress_never_claims_an_ack() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    provider.control().ignore_next_send();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let (_control, command) = files(channel, 770, 771, "frozen", b"fixture");
    session.send(command).await;
    let mut progress = 0;
    loop {
        let event = tokio::time::timeout(Duration::from_secs(1), session.next())
            .await
            .unwrap();
        let Event::UploadProgress {
            scope,
            sent_bytes,
            total_bytes,
        } = event
        else {
            panic!("only body progress before the missing acknowledgement: {event:?}")
        };
        assert_eq!(
            (scope.request, scope.nonce, scope.channel),
            (770, 771, channel)
        );
        assert!(sent_bytes <= total_bytes);
        progress += 1;
        assert!(
            progress <= 2,
            "simulation progress is coalesced, not one event per chunk"
        );
        if sent_bytes == total_bytes {
            break;
        }
    }
    assert!(session.events.try_recv().is_err());
    let committed = world.lock().unwrap().sims[&channel].extra[0].id;
    let (_retry, command) = files(channel, 772, 771, "frozen", b"fixture");
    session.send(command).await;
    assert_eq!(created(outcome(&mut session).await, 771).id, committed);
    assert_eq!(world.lock().unwrap().sims[&channel].extra.len(), 1);
}

#[tokio::test(start_paused = true)]
async fn uploads_dropped_control_and_expiry_after_admission_prevent_commit() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let (control, command) = files(channel, 780, 781, "", b"fixture");
    session.send(command).await;
    assert!(
        matches!(session.next().await, Event::UploadProgress { scope, sent_bytes: 0, .. } if scope.request == 780)
    );
    drop(control);
    assert!(matches!(
        outcome(&mut session).await,
        Event::Failed {
            failure: Failure::SendFiles { request: 780, .. },
            ambiguous: false,
            ..
        }
    ));
    let (_control, mut command) = files(channel, 782, 783, "", b"fixture");
    if let Command::SendFiles { scope, .. } = &mut command {
        scope.expires_at = (tokio::time::Instant::now() + Duration::from_millis(10)).into_std();
    }
    session.send(command).await;
    assert!(matches!(
        outcome(&mut session).await,
        Event::Failed {
            failure: Failure::SendFiles { request: 782, .. },
            ambiguous: false,
            ..
        }
    ));
    assert!(world.lock().unwrap().sims[&channel].extra.is_empty());
}

#[tokio::test(start_paused = true)]
async fn uploads_cancel_command_and_aborting_the_session_stop_its_pending_commit() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let (_control, command) = files(channel, 790, 791, "", b"fixture");
    session.send(command).await;
    assert!(
        matches!(session.next().await, Event::UploadProgress { scope, sent_bytes: 0, .. } if scope.request == 790)
    );
    session.send(Command::Cancel(790)).await;
    session
        .send(Command::LoadHistory {
            request: 792,
            channel,
            before: None,
            limit: 1,
        })
        .await;
    let after_cancel = outcome(&mut session).await;
    assert!(
        matches!(after_cancel, Event::HistoryLoaded { request: 792, .. }),
        "unexpected event after cancelling admitted upload: {after_cancel:?}"
    );
    assert!(world.lock().unwrap().sims[&channel].extra.is_empty());

    let (_control, command) = files(channel, 793, 794, "", b"fixture");
    session.send(command).await;
    assert!(
        matches!(session.next().await, Event::UploadProgress { scope, sent_bytes: 0, .. } if scope.request == 793)
    );
    session.task.abort();
    let _ = session.task.await;
    tokio::time::advance(Duration::from_secs(1)).await;
    assert!(world.lock().unwrap().sims[&channel].extra.is_empty());
}

#[tokio::test(start_paused = true)]
async fn uploads_queued_in_an_old_generation_cannot_run_on_the_reused_receiver() {
    let provider = MockProvider::new(config());
    let control = provider.control();
    let world = provider.world.clone();
    let channel = general(&world.lock().unwrap());
    let (sender, mut receiver) = mpsc::channel(16);
    let (event_sender, mut events) = mpsc::channel(32);
    let sink = EventSink::new(
        event_sender,
        crate::LatestValues::default(),
        Arc::new(|| {}),
    );
    let first_sink = sink.for_session(3);
    let first = tokio::spawn(async move {
        let result = provider.connect(&mut receiver, &first_sink).await;
        (provider, receiver, result)
    });
    assert!(matches!(
        events.recv().await.unwrap().event,
        Event::Ready(_)
    ));
    // The account's settings (activity sharing too), its perks (P5), the app in voice and, per
    // server, since when its voice rooms are on (voz-protocolo).
    for _ in 0..6 + crate::mock::SERVERS.len() {
        events.recv().await.unwrap();
    }
    control.drop_connection();
    let (provider, mut receiver, result) = first.await.unwrap();
    assert!(matches!(result, Err(ProviderError::Transient(_))));
    let (_control, command) = files(channel, 800, 801, "", b"fixture");
    sender.send(command).await.unwrap();
    let sink = sink.for_session(4);
    let next = tokio::spawn(async move { provider.connect(&mut receiver, &sink).await });
    assert!(matches!(
        events.recv().await.unwrap().event,
        Event::Ready(_)
    ));
    // The account's settings (activity sharing too), its perks (P5), the app in voice and, per
    // server, since when its voice rooms are on (voz-protocolo).
    for _ in 0..6 + crate::mock::SERVERS.len() {
        events.recv().await.unwrap();
    }
    assert!(matches!(
        events.recv().await.unwrap().event,
        Event::Failed {
            failure: Failure::SendFiles { request: 800, .. },
            ambiguous: false,
            ..
        }
    ));
    assert!(world.lock().unwrap().sims[&channel].extra.is_empty());
    next.abort();
}

#[tokio::test(start_paused = true)]
async fn uploads_allow_an_empty_regular_file_without_a_text_message() {
    let mut session = Session::start(MockProvider::new(config())).await;
    let channel = session.channel("general");
    let (_control, command) = files(channel, 810, 811, "", b"");
    session.send(command).await;
    let message = created(outcome(&mut session).await, 811);
    assert!(message.content.is_empty());
    assert_eq!(message.attachments[0].size_bytes, 0);
}

#[tokio::test(start_paused = true)]
async fn uploads_enforce_file_count_byte_limits_and_reply_history_before_mutation() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let target = session
        .snapshot
        .last_messages
        .iter()
        .find(|entry| entry.0 == channel)
        .unwrap()
        .1;
    for index in 0..4 {
        let (_control, mut command) = files(channel, 750 + index, 760 + index, "", b"fixture");
        if let Command::SendFiles {
            files, reply_to, ..
        } = &mut command
        {
            match index {
                0 => files.clear(),
                1 => files.push(files[0].clone()),
                2 => {
                    files[0] = PreparedUpload::new(
                        PendingAttachment {
                            token: UploadToken(2),
                            filename: "huge".into(),
                            size_bytes: 20 * 1024 * 1024 + 1,
                        },
                        Bytes::from(vec![0; 20 * 1024 * 1024 + 1]),
                    )
                    .unwrap()
                }
                _ => {
                    *reply_to = Some(target);
                    world
                        .lock()
                        .unwrap()
                        .channels
                        .iter_mut()
                        .find(|entry| entry.id == channel)
                        .unwrap()
                        .permissions = Permissions::VIEW | Permissions::SEND | Permissions::ATTACH;
                }
            }
        }
        session.send(command).await;
        assert!(matches!(
            outcome(&mut session).await,
            Event::Failed {
                failure: Failure::SendFiles { .. },
                ambiguous: false,
                ..
            }
        ));
    }
    assert!(world.lock().unwrap().sims[&channel].extra.is_empty());
}
