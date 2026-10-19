use super::*;
use rune_model::{ChannelKind, Timestamp};
use rune_provider::mock::{MockConfig, MockControl, MockProvider};

mod context;
// Encrypted local cache (2026-10-18).
mod encrypted_storage;
mod local_history;
mod message_patches;
mod pins;
mod read_states;
mod storage_sessions;
// Reactions, pins and search (2026-10-18).
mod tallies;
// AutoMod (automod).
mod automod;

#[test]
fn backoff_doubles_up_to_its_ceiling_and_jitters_by_a_fifth() {
    let backoff = Backoff {
        base: Duration::from_millis(500),
        max: Duration::from_secs(8),
    };
    let delays: Vec<u128> = (1..=7)
        .map(|attempt| backoff.delay(attempt, 0.0).as_millis())
        .collect();
    assert_eq!(delays, vec![500, 1000, 2000, 4000, 8000, 8000, 8000]);
    assert_eq!(backoff.delay(1, 1.0).as_millis(), 600);
    assert_eq!(backoff.delay(1, -1.0).as_millis(), 400);
    assert!((-1.0..=1.0).contains(&jitter(3)) && jitter(3) != jitter(4));
}

fn mock() -> MockProvider {
    MockProvider::new(MockConfig {
        seed: 3,
        history_end: Timestamp(1_790_000_000_000),
        clock: || Timestamp(1_790_000_100_000),
        latency: Duration::from_millis(5),
        ambient: false,
        replies: false,
    })
}

/// A keyring that knows every account's key without being told: derived
/// from the account's name, so each client of a test, and a store a test
/// opens itself, read the same rows. Never the person's keyring.
pub(crate) struct DerivedKeys;

/// The key [`DerivedKeys`] gives `account`.
pub(crate) fn derived_key(account: &str) -> rune_storage::Key {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(account.as_bytes());
    let text: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    rune_storage::Key::from_text(&text).unwrap()
}

impl rune_platform::secrets::SecretStore for DerivedKeys {
    fn list<'a>(
        &'a self,
        _: &'a str,
    ) -> rune_platform::secrets::SecretFuture<'a, Vec<rune_platform::secrets::Entry>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn load<'a>(
        &'a self,
        _: &'a str,
        account: &'a str,
    ) -> rune_platform::secrets::SecretFuture<'a, Option<rune_platform::secrets::Saved>> {
        Box::pin(async move {
            Ok(Some(rune_platform::secrets::Saved {
                account: account.into(),
                secret: derived_key(account).to_text(),
            }))
        })
    }

    fn save<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> rune_platform::secrets::SecretFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn delete<'a>(
        &'a self,
        _: &'a str,
        _: &'a str,
    ) -> rune_platform::secrets::SecretFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }
}

/// `path`'s store with `account` unlocked as the client unlocks it.
pub(crate) fn unlocked(path: &std::path::Path, account: &str) -> Storage {
    let mut storage = Storage::open(path).unwrap();
    storage.unlock(account, derived_key(account)).unwrap();
    storage
}

/// A store in memory with `account` unlocked.
pub(crate) fn memory(account: &str) -> Storage {
    let mut storage = Storage::in_memory().unwrap();
    storage.unlock(account, derived_key(account)).unwrap();
    storage
}

/// A client as the window starts one: its startup reads find the key.
fn client_with(
    provider: Arc<dyn Provider>,
    storage: Option<PathBuf>,
    deadlines: Deadlines,
) -> Client {
    let mut client = Client::start(
        provider,
        ClientConfig {
            wake: Arc::new(|| {}),
            storage,
            keyring: Some(Arc::new(DerivedKeys)),
            backoff: Backoff {
                base: Duration::from_millis(10),
                max: Duration::from_millis(40),
            },
            deadlines,
            image_cache: None,
        },
    );
    client.wait_for_key();
    client
}

fn client(provider: Arc<dyn Provider>, storage: Option<PathBuf>) -> Client {
    client_with(provider, storage, Deadlines::default())
}

/// Poll until `find` matches an update, for at most two seconds.
fn wait<T>(client: &mut Client, mut find: impl FnMut(&Update) -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        for update in client.poll(64) {
            if let Some(found) = find(&update) {
                return found;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("nothing matched in time");
}

fn ready(client: &mut Client) -> Vec<rune_model::Channel> {
    wait(client, |update| match update {
        Update::Provider(Event::Ready(snapshot)) => Some(snapshot.channels.clone()),
        _ => None,
    })
}

fn text_channels(channels: &[rune_model::Channel]) -> Vec<ChannelId> {
    channels
        .iter()
        .filter(|channel| channel.kind == ChannelKind::Text)
        .map(|channel| channel.id)
        .collect()
}

fn send_failed(update: &Update) -> Option<(u64, bool, String)> {
    match update {
        Update::Provider(Event::Failed {
            failure: Failure::Send { nonce, .. },
            ambiguous,
            reason,
        }) => Some((*nonce, *ambiguous, reason.clone())),
        _ => None,
    }
}

fn controlled() -> (Arc<MockProvider>, Arc<MockControl>) {
    let provider = Arc::new(mock());
    let control = provider.control();
    (provider, control)
}

#[test]
fn invitation_queue_failures_release_only_their_request_and_logout_cannot_clear_a_live_write() {
    use rune_model::{ServerId, UserId};
    use rune_provider::InvitationTarget;
    let mut client = client(Arc::new(mock()), None);
    ready(&mut client);
    client.info.capabilities.invitation_accept = true;
    let (sender, mut receiver) = mpsc::channel(1);
    client.commands = sender;
    let expires_at = Instant::now() + Duration::from_secs(50);
    let join = |request| Command::AcceptInvitation {
        request,
        destination: rune_provider::InvitationDestination::Server(ServerId(70)),
        account: UserId(1),
        expires_at,
        target: InvitationTarget {
            key: "fixture".into(),
            url: "https://fixture.invalid/invite".into(),
        },
    };
    client.command(Command::Cancel(0));
    client.command(join(1));
    assert!(client.invitation_acceptance.deadline().is_none());
    assert!(matches!(
        client.found.pop_back(),
        Some(Event::Failed {
            failure: Failure::Invitation { request: 1, .. },
            ambiguous: false,
            ..
        })
    ));
    assert_eq!(receiver.try_recv().unwrap(), Command::Cancel(0));
    client.command(join(2));
    let deadline = client.invitation_acceptance.deadline();
    assert!(deadline.is_some());
    client.command(join(2));
    assert!(
        client.found.is_empty(),
        "a duplicate must not fail the original command"
    );
    client.command(Command::SignOut);
    assert_eq!(
        client.invitation_acceptance.deadline(),
        deadline,
        "a rejected logout does not end the session"
    );
    assert_eq!(receiver.try_recv().unwrap(), join(2));
    assert!(receiver.try_recv().is_err());
    client.expire(deadline.unwrap());
    assert!(matches!(
        client.found.pop_back(),
        Some(Event::Failed {
            failure: Failure::Invitation { request: 2, .. },
            ambiguous: true,
            ..
        })
    ));
    client.command(join(3));
    assert!(
        receiver.try_recv().is_err(),
        "timeout does not release a potentially successful write"
    );
    let generation = client.generation;
    let event = Event::InvitationAcceptance {
        request: 2,
        destination: rune_provider::InvitationDestination::Server(ServerId(70)),
        result: Ok(()),
    };
    assert_eq!(
        client.accept(
            Envelope {
                generation,
                event: event.clone()
            },
            &mut Vec::new()
        ),
        Some(event)
    );
    client.command(join(3));
    assert_eq!(receiver.try_recv().unwrap(), join(3));
}

#[test]
fn the_provider_runtime_supports_loopback_network_io() {
    let client = client(Arc::new(mock()), None);
    let (sender, receiver) = std::sync::mpsc::channel();
    client.spawn(async move {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await;
        sender.send(listener.is_ok()).unwrap();
    });
    assert!(receiver.recv_timeout(Duration::from_secs(2)).unwrap());
}

#[test]
fn ending_authorization_discards_pending_history_and_delivery_work() {
    for state in [
        AuthenticationState::SignedOut,
        AuthenticationState::Failed {
            reason: "Autorización vencida".into(),
        },
    ] {
        let provider = Arc::new(mock());
        let mut client = client(provider.clone(), None);
        let channel = text_channels(&ready(&mut client))[0];
        let (messages, _) = provider.history(channel, None, 1);
        let mut save = messages
            .iter()
            .cloned()
            .map(|message| MessageMutation::Upsert(Box::new(message)))
            .collect();
        client.live.insert(900);
        client.loading.insert(
            channel,
            Loading {
                request: 900,
                before: None,
                since: Instant::now(),
            },
        );
        client.sending.insert(901, (channel, Instant::now()));
        let generation = client.generation;
        client.accept(
            Envelope {
                generation,
                event: Event::Authentication(state),
            },
            &mut save,
        );
        assert!(
            save.is_empty(),
            "old account work must not be persisted after sign-out"
        );
        assert!(!client.is_loading(channel));
        assert!(client.next_deadline(Instant::now()).is_none());
        let late = client.accept(
            Envelope {
                generation,
                event: Event::HistoryLoaded {
                    request: 900,
                    channel,
                    before: None,
                    messages,
                    reached_start: false,
                },
            },
            &mut save,
        );
        assert!(
            late.is_none(),
            "late history from the signed-out account is ignored"
        );
    }
}

#[test]
fn a_page_is_asked_once_and_a_cancelled_one_never_arrives() {
    let mut client = client(Arc::new(mock()), None);
    let channels = text_channels(&ready(&mut client));
    let (first, second) = (channels[2], channels[3]);
    client.load_latest(first);
    client.load_latest(first);
    client.load_latest(second);
    client.cancel_history_except(second);
    assert!(!client.is_loading(first) && client.is_loading(second));
    let arrived = wait(&mut client, |update| match update {
        Update::Provider(Event::HistoryLoaded {
            channel, messages, ..
        }) => Some((*channel, messages.len())),
        _ => None,
    });
    assert_eq!(arrived, (second, PAGE));
    assert!(!client.is_loading(second));
    std::thread::sleep(Duration::from_millis(50));
    assert!(
        !client
            .poll(64)
            .iter()
            .any(|update| matches!(update, Update::Provider(Event::HistoryLoaded { .. }))),
        "the cancelled page is dropped"
    );
}

#[test]
fn a_lost_connection_comes_back_by_itself_as_a_newer_session() {
    let (provider, control) = controlled();
    let mut client = client(provider, None);
    ready(&mut client);
    let first = client.generation;
    control.drop_connection();
    let attempt = wait(&mut client, |update| match update {
        Update::Provider(Event::Connection(ConnectionState::Reconnecting { attempt, .. })) => {
            Some(*attempt)
        }
        _ => None,
    });
    assert_eq!(attempt, 1);
    ready(&mut client);
    assert!(
        client.generation > first,
        "the new session has a later number"
    );
}

#[test]
fn a_send_without_an_answer_fails_as_maybe_sent_when_its_time_is_up() {
    let (provider, control) = controlled();
    let deadlines = Deadlines {
        history: Duration::from_secs(20),
        send: Duration::from_millis(60),
    };
    let mut client = client_with(provider, None, deadlines);
    let channel = text_channels(&ready(&mut client))[2];
    control.ignore_next_send();
    let nonce = client.send_message(channel, "¿llegó?".into(), None, true);
    assert!(client
        .next_deadline(Instant::now())
        .is_some_and(|left| left <= Duration::from_millis(60)));
    let (failed, ambiguous, reason) = wait(&mut client, send_failed);
    assert_eq!((failed, ambiguous), (nonce, true));
    assert!(reason.contains("sin confirmación"), "{reason}");
    assert_eq!(
        client.next_deadline(Instant::now()),
        None,
        "nothing waits any more"
    );
}

#[test]
fn a_send_waiting_on_a_verification_gets_its_whole_time() {
    let (provider, control) = controlled();
    let deadlines = Deadlines {
        history: Duration::from_secs(20),
        send: Duration::from_millis(60),
    };
    let mut client = client_with(provider, None, deadlines);
    let channel = text_channels(&ready(&mut client))[2];
    control.ignore_next_send();
    let nonce = client.send_message(channel, "¿llegó?".into(), None, true);
    let challenge = Envelope {
        generation: client.generation,
        event: Event::ChallengeRequired {
            challenge: 1,
            failure: Failure::Send { channel, nonce },
        },
    };
    assert!(client.accept(challenge, &mut Vec::new()).is_some());
    let left = client.next_deadline(Instant::now()).unwrap();
    assert!(left > rune_provider::CHALLENGE_LIFETIME, "{left:?}");
    client.expire(Instant::now() + Duration::from_secs(60));
    assert!(
        !client
            .poll(64)
            .iter()
            .any(|update| send_failed(update).is_some()),
        "the person may still be solving it"
    );
}

#[test]
fn a_send_in_flight_when_the_connection_drops_is_marked_maybe_sent() {
    let (provider, control) = controlled();
    let mut client = client(provider, None);
    let channel = text_channels(&ready(&mut client))[2];
    control.ignore_next_send();
    let nonce = client.send_message(channel, "en vuelo".into(), None, true);
    std::thread::sleep(Duration::from_millis(20));
    control.drop_connection();
    let (failed, ambiguous, _) = wait(&mut client, send_failed);
    assert_eq!((failed, ambiguous), (nonce, true));
}

#[test]
fn a_refused_send_comes_back_as_a_definite_failure() {
    let (provider, control) = controlled();
    let mut client = client(provider, None);
    let channel = text_channels(&ready(&mut client))[2];
    control.fail_next_send();
    let nonce = client.send_message(channel, "no".into(), None, true);
    let (failed, ambiguous, _) = wait(&mut client, send_failed);
    assert_eq!((failed, ambiguous), (nonce, false));
}

#[test]
fn what_arrives_is_kept_and_offered_on_the_next_run() {
    let directory = std::env::temp_dir().join(format!("rune-core-test-{}", std::process::id()));
    let path = directory.join("rune.db");
    let channel = {
        let mut client = client(Arc::new(mock()), Some(path.clone()));
        let channel = text_channels(&ready(&mut client))[2];
        client.load_latest(channel);
        wait(&mut client, |update| {
            matches!(update, Update::Provider(Event::HistoryLoaded { .. })).then_some(())
        });
        client
            .set_draft(channel, Some("a medio escribir".into()))
            .unwrap();
        // Dropping the client lets the store's thread finish writing.
        channel
    };
    let mut client = client(Arc::new(mock()), Some(path));
    let snapshot = client
        .cached_snapshot()
        .expect("the last session's snapshot is kept");
    assert!(snapshot.channels.iter().any(|known| known.id == channel));
    assert_eq!(
        client.drafts(),
        vec![(channel, "a medio escribir".to_owned())]
    );
    ready(&mut client);
    client.load_latest(channel);
    let cached = wait(&mut client, |update| match update {
        Update::Cached { messages, .. } => Some(messages.len()),
        _ => None,
    });
    assert_eq!(cached, PAGE);
    drop(client);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_deleted_message_stays_deleted_in_cache_regardless_of_poll_size() {
    for max in [1, 64] {
        let directory = std::env::temp_dir().join(format!(
            "rune-core-deleted-cache-{max}-{}",
            std::process::id()
        ));
        let path = directory.join("rune.db");
        let provider = Arc::new(mock());
        let mut client = client(provider.clone(), Some(path.clone()));
        let channels = text_channels(&ready(&mut client));
        let channel = channels[2];
        let messages = provider.history(channel, None, 3).0;
        client.storage.as_ref().unwrap().save(messages.clone());

        // The same id in another conversation must survive the deletion.
        let neighbor = Message {
            channel: channels[3],
            ..messages[0].clone()
        };
        // Queue the complete burst before polling, without timing a provider.
        let (sender, events) = mpsc::channel(16);
        client.events = events;
        let request = 999;
        client.live.insert(request);
        let events = [
            Event::MessageCreated {
                message: neighbor.clone(),
                nonce: None,
            },
            Event::MessageCreated {
                message: messages[0].clone(),
                nonce: None,
            },
            Event::MessageUpdated(Message {
                content: "editado antes de borrar".into(),
                edited: Some(Timestamp(1_790_000_100_000)),
                ..messages[1].clone()
            }),
            Event::HistoryLoaded {
                request,
                channel,
                before: None,
                messages: vec![messages[2].clone()],
                reached_start: false,
            },
        ];
        for event in
            events
                .into_iter()
                .chain(messages.iter().map(|message| Event::MessageDeleted {
                    channel,
                    message: message.id,
                }))
        {
            sender
                .try_send(Envelope {
                    generation: client.generation,
                    event,
                })
                .unwrap();
        }
        while !client.events.is_empty() {
            client.poll(max);
        }
        let account = client.account().to_owned();
        drop(client); // Finish the storage queue before reopening the file.
        let storage = unlocked(&path, &account);
        assert!(
            storage.recent(&account, channel, PAGE).unwrap().is_empty(),
            "deleted messages returned after reopening (poll size {max})"
        );
        assert_eq!(
            storage.recent(&account, neighbor.channel, PAGE).unwrap(),
            vec![neighbor]
        );
        drop(storage);
        let _ = std::fs::remove_dir_all(directory);
    }
}

#[test]
fn an_authoritative_history_range_is_reconciled_on_disk() {
    let directory =
        std::env::temp_dir().join(format!("rune-core-history-range-{}", std::process::id()));
    let path = directory.join("rune.db");
    let provider = Arc::new(mock());
    let mut client = client(provider.clone(), Some(path.clone()));
    let channel = text_channels(&ready(&mut client))[2];
    let messages = provider.history(channel, None, 5).0;
    client.storage.as_ref().unwrap().save(messages.clone());
    let request = 900;
    client.live.insert(request);
    let (sender, events) = mpsc::channel(4);
    client.events = events;
    sender
        .try_send(Envelope {
            generation: client.generation,
            event: Event::HistoryLoaded {
                request,
                channel,
                before: Some(messages[4].id),
                messages: vec![messages[1].clone(), messages[3].clone()],
                reached_start: false,
            },
        })
        .unwrap();
    client.poll(64);
    let account = client.account().to_owned();
    drop(client);
    let storage = unlocked(&path, &account);
    assert_eq!(
        storage.recent(&account, channel, 10).unwrap(),
        vec![
            messages[0].clone(),
            messages[1].clone(),
            messages[3].clone(),
            messages[4].clone(),
        ]
    );
    drop(storage);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_cached_reply_cannot_survive_sign_out_within_the_same_provider_generation() {
    let provider = Arc::new(mock());
    let mut client = client(provider.clone(), None);
    let channel = text_channels(&ready(&mut client))[2];
    let messages = provider.history(channel, None, 2).0;
    let mut storage = memory(client.account());
    storage.save_messages(client.account(), &messages).unwrap();
    client.storage = Some(StorageWorker::start(
        storage,
        client.account().to_owned(),
        Arc::new(|| {}),
    ));
    client.storage_active = true;
    client.unlocked = true;
    client.load_latest(channel);
    assert!(client.cache_requests.contains_key(&channel));
    // This startup-only barrier puts the cache reply in its answer queue.
    client.storage.as_ref().unwrap().snapshot();
    let (sender, events) = mpsc::channel(4);
    client.events = events;
    sender
        .try_send(Envelope {
            generation: client.generation,
            event: Event::Authentication(AuthenticationState::SignedOut),
        })
        .unwrap();
    assert!(client
        .poll(64)
        .iter()
        .all(|update| !matches!(update, Update::Cached { .. })));
}

#[test]
fn a_cached_conversation_opens_even_when_the_provider_command_queue_is_closed() {
    let provider = Arc::new(mock());
    let mut client = client(provider.clone(), None);
    let channel = text_channels(&ready(&mut client))[2];
    let messages = provider.history(channel, None, 2).0;
    let mut storage = memory(client.account());
    storage.save_messages(client.account(), &messages).unwrap();
    client.storage = Some(StorageWorker::start(
        storage,
        client.account().to_owned(),
        Arc::new(|| {}),
    ));
    client.storage_active = true;
    client.unlocked = true;
    let (commands, received) = mpsc::channel(1);
    drop(received);
    client.commands = commands;
    client.load_latest(channel);
    client.storage.as_ref().unwrap().snapshot();
    assert!(client.poll(64).iter().any(|update| matches!(
        update, Update::Cached { channel: id, messages: held }
            if *id == channel && held == &messages
    )));
}

#[test]
fn storage_backpressure_does_not_expire_responses_waiting_in_the_provider_queue() {
    let mut client = client(Arc::new(mock()), None);
    ready(&mut client);
    let now = Instant::now();
    client.loading.insert(
        ChannelId(9),
        Loading {
            request: 900,
            before: None,
            since: now - Duration::from_secs(40),
        },
    );
    client.live.insert(900);
    client.storage_paused_at = Some(now - Duration::from_secs(40));
    // The pending write has just been accepted; poll resumes network deadlines.
    client.poll(0);
    assert!(client.loading.contains_key(&ChannelId(9)));
    assert!(client.found.is_empty());
    assert!(client.next_deadline(Instant::now()).unwrap() > Duration::from_secs(19));
}

#[test]
fn a_recovered_storage_error_does_not_survive_a_committed_barrier() {
    let mut client = client(Arc::new(mock()), None);
    ready(&mut client);
    client.storage = Some(StorageWorker::start(
        memory(client.account()),
        client.account().to_owned(),
        Arc::new(|| {}),
    ));
    client.storage_active = true;
    client.unlocked = true;
    client.storage_error = Some("fallo transitorio observado en un poll anterior".into());
    let ticket = client.flush_storage().unwrap().unwrap();
    client.storage.as_ref().unwrap().snapshot();
    client.poll(0);
    assert!(client.storage_committed(ticket));
    assert!(client.storage_error().is_none());
}

#[test]
fn a_rejected_sign_out_keeps_the_active_account_cache_and_draft_writes() {
    for closed in [false, true] {
        let mut client = client(Arc::new(mock()), None);
        let channel = text_channels(&ready(&mut client))[2];
        client.storage = Some(StorageWorker::start(
            memory(client.account()),
            client.account().to_owned(),
            Arc::new(|| {}),
        ));
        client.storage_active = true;
        client.unlocked = true;
        let request = CacheRequest {
            epoch: client.cache_epoch,
            request: 99,
            channel,
        };
        client.cache_requests.insert(channel, request);
        let (commands, received) = mpsc::channel(1);
        let _received = if closed {
            drop(received);
            None
        } else {
            commands.try_send(Command::StartTyping { channel }).unwrap();
            Some(received)
        };
        client.commands = commands;
        client.command(Command::SignOut);
        assert!(
            client.storage_active,
            "a rejected logout must not silently disable persistence"
        );
        assert_eq!(client.cache_requests.get(&channel), Some(&request));
        assert_eq!(client.cache_epoch, request.epoch);
        client
            .set_draft(channel, Some("still this account".into()))
            .unwrap();
        assert_eq!(
            client.storage.as_ref().unwrap().drafts(),
            vec![(channel, "still this account".into())]
        );

        let (commands, _received) = mpsc::channel(1);
        client.commands = commands;
        client.command(Command::SignOut);
        assert!(
            !client.storage_active,
            "an admitted logout fences cache replies immediately"
        );
        assert!(client.cache_requests.is_empty());
    }
}

#[test]
fn an_oversized_snapshot_keeps_the_session_live_with_an_explicit_memory_only_notice() {
    let provider = Arc::new(mock());
    let mut client = client(provider.clone(), None);
    let channel = text_channels(&ready(&mut client))[2];
    client.storage = Some(StorageWorker::start(
        memory(client.account()),
        client.account().to_owned(),
        Arc::new(|| {}),
    ));
    client.storage_active = true;
    client.unlocked = true;
    client
        .set_draft(channel, Some("already accepted".into()))
        .unwrap();
    let mut snapshot = provider.snapshot();
    snapshot.me.as_mut().unwrap().name = "x".repeat(9 * 1024 * 1024);
    let message = provider.history(channel, None, 1).0.remove(0);
    let (sender, events) = mpsc::channel(4);
    client.events = events;
    for event in [
        Event::Ready(Box::new(snapshot)),
        Event::MessageCreated {
            message: message.clone(),
            nonce: None,
        },
    ] {
        sender
            .try_send(Envelope {
                generation: client.generation,
                event,
            })
            .unwrap();
    }
    let updates = client.poll(64);
    assert!(
        updates.iter().any(|update| matches!(
            update, Update::Provider(Event::MessageCreated { message: received, .. })
                if received.id == message.id
        )),
        "a cache limit must not stall provider events"
    );
    assert!(client.pending_storage.is_none());
    assert!(client.storage_paused_at.is_none());
    assert!(!client.storage_active);
    assert!(client.storage_error().unwrap().contains("memoria"));
    assert!(client.cache_requests.is_empty());
    let ticket = client.flush_storage().unwrap().unwrap();
    let worker = client.storage.as_ref().unwrap();
    assert_eq!(worker.drafts(), vec![(channel, "already accepted".into())]);
    assert!(
        client.storage_committed(ticket),
        "accepted writes still commit"
    );
    client.poll(0);
    assert!(
        client.storage_error().is_some(),
        "memory-only status must stay visible"
    );
}
