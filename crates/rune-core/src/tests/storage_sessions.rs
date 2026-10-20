use super::*;

struct NamedProvider {
    inner: MockProvider,
    name: &'static str,
    identity: Option<StorageIdentity>,
    legacy: Option<&'static str>,
}

impl Provider for NamedProvider {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            name: self.name.into(),
            ..self.inner.info()
        }
    }

    fn storage_identity(&self) -> Option<StorageIdentity> {
        self.identity
    }

    fn legacy_storage_namespace(&self) -> Option<&'static str> {
        self.legacy
    }

    fn connect<'a>(
        &'a self,
        commands: &'a mut mpsc::Receiver<Command>,
        events: &'a EventSink,
    ) -> rune_provider::BoxFuture<'a, Result<(), ProviderError>> {
        self.inner.connect(commands, events)
    }
}

fn named(name: &'static str, provider: &'static str, account: u64) -> Arc<NamedProvider> {
    Arc::new(NamedProvider {
        inner: mock(),
        name,
        identity: Some(StorageIdentity {
            provider,
            account: rune_model::UserId(account),
        }),
        legacy: None,
    })
}

fn directory(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rune-storage-sessions-{name}-{}",
        std::process::id()
    ))
}

#[test]
fn local_read_choices_survive_reopen_and_cannot_cross_the_admitted_account() {
    use rune_model::{ReadMark, UserId};
    let directory = directory("read-choices");
    let path = directory.join("rune.db");
    let mut first = client(named("First", "provider.reads", 1), Some(path.clone()));
    let channel = text_channels(&ready(&mut first))[0];
    let mark = ReadMark {
        through: Some(MessageId(100)),
        unread_from: Some(MessageId(90)),
        revision: 7,
    };
    first.set_read_mark(UserId(2), channel, mark).unwrap();
    assert!(first.read_marks().unwrap().is_empty());
    first.set_read_mark(UserId(1), channel, mark).unwrap();
    let ticket = first.flush_storage().unwrap().unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    while !first.storage_committed(ticket) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(first.storage_committed(ticket));
    drop(first);
    let mut reopened = client(named("Renamed", "provider.reads", 1), Some(path.clone()));
    assert_eq!(reopened.storage_account(), Some(UserId(1)));
    assert_eq!(reopened.read_marks().unwrap(), vec![(channel, mark)]);
    ready(&mut reopened);
    let mut wrong = mock().snapshot();
    wrong.me.as_mut().unwrap().id = UserId(2);
    reopened.accept(
        Envelope {
            generation: reopened.generation,
            event: Event::Ready(Box::new(wrong)),
        },
        &mut Vec::new(),
    );
    assert_eq!(reopened.storage_account(), None);
    reopened
        .set_read_mark(
            UserId(2),
            channel,
            ReadMark {
                revision: 9,
                ..mark
            },
        )
        .unwrap();
    assert_eq!(
        reopened.read_marks(),
        Err(rune_model::LocalReadError::Unavailable)
    );
    drop(reopened);
    let disk = unlocked(&path, "14:provider.reads:1");
    assert_eq!(
        disk.read_marks("14:provider.reads:1").unwrap(),
        vec![(channel, mark)]
    );
    drop(disk);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_failed_read_restore_keeps_new_choices_in_ram_until_the_baseline_is_known() {
    use rune_model::{ReadMark, UserId};
    let directory = directory("read-restore-failed");
    let path = directory.join("rune.db");
    let mut client = client(named("Reads", "provider.reads", 1), Some(path));
    let channel = text_channels(&ready(&mut client))[0];
    let old = ReadMark {
        through: Some(MessageId(100)),
        unread_from: Some(MessageId(80)),
        revision: 20,
    };
    assert!(client.read_marks().unwrap().is_empty());
    client.set_read_mark(UserId(1), channel, old).unwrap();
    let storage = client.storage.as_ref().unwrap();
    storage.save(mock().history(channel, None, 1).0);
    for request in 0..storage::CACHED_ANSWERS {
        assert!(storage.request_cached(
            CacheRequest {
                epoch: 0,
                request: request as u64,
                channel
            },
            1
        ));
    }
    let ticket = storage.flush().unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    while !storage.committed(ticket) && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(storage.committed(ticket));
    assert!(
        client.read_marks().is_err(),
        "occupied answer slots make this restore fail explicitly"
    );
    assert!(client.storage_error().unwrap().contains("sólo en memoria"));
    let newer = ReadMark {
        unread_from: Some(MessageId(90)),
        revision: 21,
        ..old
    };
    client.set_read_mark(UserId(1), channel, newer).unwrap();
    let storage = client.storage.as_ref().unwrap();
    while storage.try_cached().is_some() {}
    assert_eq!(
        storage.read_marks().unwrap(),
        vec![(channel, old)],
        "an unknown baseline cannot admit new marker writes"
    );
    assert_eq!(client.read_marks().unwrap(), vec![(channel, old)]);
    assert!(client.storage_error().is_none());
    client.set_read_mark(UserId(1), channel, newer).unwrap();
    assert_eq!(client.read_marks().unwrap(), vec![(channel, newer)]);
    drop(client);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn returning_to_a_storage_account_needs_a_known_read_baseline_or_explicit_forget() {
    use rune_model::{ReadMark, UserId};
    let directory = directory("read-account-return");
    let path = directory.join("rune.db");
    let provider = named("Reads", "provider.reads", 1);
    let mut client = client(provider.clone(), Some(path));
    let channel = text_channels(&ready(&mut client))[0];
    let mark = ReadMark {
        through: Some(MessageId(100)),
        unread_from: Some(MessageId(80)),
        revision: 20,
    };
    client.read_marks().unwrap();
    client.set_read_mark(UserId(1), channel, mark).unwrap();
    let mut snapshot = provider.inner.snapshot();
    snapshot.me.as_mut().unwrap().id = UserId(2);
    client.accept(
        Envelope {
            generation: client.generation,
            event: Event::Ready(Box::new(snapshot)),
        },
        &mut Vec::new(),
    );
    // Nothing of the account in use is kept, and the other one's namespace
    // is not its to erase (the marks read back below are still there); the
    // request is done, so the window's barrier and settings go on.
    client.forget_local_data().unwrap();
    client.accept(
        Envelope {
            generation: client.generation,
            event: Event::Ready(Box::new(provider.inner.snapshot())),
        },
        &mut Vec::new(),
    );
    assert!(client.storage_error().unwrap().contains("sólo en memoria"));
    let fresh = ReadMark {
        revision: 1,
        unread_from: Some(MessageId(90)),
        ..mark
    };
    client.set_read_mark(UserId(1), channel, fresh).unwrap();
    assert_eq!(
        client.storage.as_ref().unwrap().read_marks().unwrap(),
        vec![(channel, mark)]
    );
    client.forget_local_data().unwrap();
    client.set_read_mark(UserId(1), channel, fresh).unwrap();
    assert_eq!(client.read_marks().unwrap(), vec![(channel, fresh)]);
    drop(client);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn storage_identity_survives_a_display_name_change_and_isolates_provider_and_account() {
    let directory = directory("namespace");
    let path = directory.join("rune.db");
    let mut first = client(named("First display", "provider.a", 1), Some(path.clone()));
    ready(&mut first);
    first
        .set_draft(ChannelId(5), Some("first account".into()))
        .unwrap();
    let namespace = first.account().to_owned();
    drop(first);

    let renamed = client(
        named("Renamed display", "provider.a", 1),
        Some(path.clone()),
    );
    assert_eq!(renamed.account(), namespace);
    assert_eq!(
        renamed.drafts(),
        vec![(ChannelId(5), "first account".into())]
    );
    drop(renamed);
    let other_provider = client(named("First display", "provider.b", 1), Some(path.clone()));
    assert_ne!(other_provider.account(), namespace);
    assert!(other_provider.drafts().is_empty());
    assert!(other_provider.cached_snapshot().is_none());
    drop(other_provider);
    let other_account = client(named("First display", "provider.a", 2), Some(path));
    assert_ne!(other_account.account(), namespace);
    assert!(other_account.drafts().is_empty());
    assert!(other_account.cached_snapshot().is_none());
    drop(other_account);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_provider_without_an_explicit_storage_identity_keeps_settings_and_nothing_of_the_account() {
    let directory = directory("no-identity");
    let path = directory.join("rune.db");
    let provider = || {
        Arc::new(NamedProvider {
            inner: mock(),
            name: "Simulación local",
            identity: None,
            legacy: Some("Simulación local"),
        })
    };
    let mut first = client(provider(), Some(path.clone()));
    assert!(
        first.storage.is_some(),
        "settings are kept with any provider"
    );
    assert!(
        first.storage_error().is_none(),
        "keeping only settings is no failure"
    );
    let channel = text_channels(&ready(&mut first))[0];
    first
        .set_setting("account:x:1:server", Some("7".into()))
        .unwrap();
    first.set_draft(channel, Some("borrador".into())).unwrap();
    first.forget_local_data().unwrap();
    let ticket = first.flush_storage().unwrap().unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !first.storage_committed(ticket) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(first);
    let second = client(provider(), Some(path.clone()));
    assert_eq!(
        second.settings(),
        [("account:x:1:server".to_owned(), "7".to_owned())]
    );
    assert!(second.drafts().is_empty());
    assert!(second.cached_snapshot().is_none());
    drop(second);
    // A display name never opts a provider into keeping the account's data:
    // no row at all, sealed or not.
    assert_eq!(rows(&path, ""), 0);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_ready_in_the_same_generation_invalidates_already_queued_cache_answers() {
    let directory = directory("ready-epoch");
    let path = directory.join("rune.db");
    let provider = Arc::new(mock());
    let mut client = client(provider.clone(), Some(path));
    let channel = text_channels(&ready(&mut client))[2];
    let messages = provider.history(channel, None, 2).0;
    client.storage.as_ref().unwrap().save(messages);
    client.load_latest(channel);
    assert!(client.cache_requests.contains_key(&channel));
    client.storage.as_ref().unwrap().snapshot();
    let old_epoch = client.cache_epoch;
    let (sender, events) = mpsc::channel(4);
    client.events = events;
    sender
        .try_send(Envelope {
            generation: client.generation,
            event: Event::Ready(Box::new(provider.snapshot())),
        })
        .unwrap();
    let updates = client.poll(64);
    assert!(updates
        .iter()
        .any(|update| matches!(update, Update::Provider(Event::Ready(_)))));
    assert!(updates
        .iter()
        .all(|update| !matches!(update, Update::Cached { .. })));
    assert_ne!(client.cache_epoch, old_epoch);
    assert!(client.cache_requests.is_empty());
    drop(client);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_ready_for_another_account_disables_message_and_draft_persistence() {
    let directory = directory("ready-mismatch");
    let path = directory.join("rune.db");
    let provider = Arc::new(mock());
    let mut client = client(provider.clone(), Some(path.clone()));
    let channel = text_channels(&ready(&mut client))[2];
    client
        .set_draft(channel, Some("original account".into()))
        .unwrap();
    let mut wrong = provider.snapshot();
    wrong.me.as_mut().unwrap().id = rune_model::UserId(2);
    let message = provider.history(channel, None, 1).0.remove(0);
    let (sender, events) = mpsc::channel(4);
    client.events = events;
    for event in [
        Event::Ready(Box::new(wrong)),
        Event::MessageCreated {
            message,
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
    client.poll(64);
    assert!(!client.storage_active);
    assert!(client.storage_error().is_some());
    client
        .set_draft(channel, Some("different account".into()))
        .unwrap();
    let account = client.account().to_owned();
    drop(client);
    let storage = unlocked(&path, &account);
    assert!(storage.recent(&account, channel, 10).unwrap().is_empty());
    assert_eq!(
        storage.drafts(&account).unwrap(),
        vec![(channel, "original account".into())]
    );
    assert_eq!(
        storage
            .snapshot::<Snapshot>(&account)
            .unwrap()
            .unwrap()
            .me
            .unwrap()
            .id,
        rune_model::UserId(1)
    );
    drop(storage);
    let _ = std::fs::remove_dir_all(directory);
}

/// The legacy namespace as a version before 5 kept it: in plain text.
fn legacy_fixture(path: &std::path::Path, wrong_account: bool) -> (Snapshot, ChannelId, Message) {
    let provider = mock();
    let mut snapshot = provider.snapshot();
    if wrong_account {
        snapshot.me.as_mut().unwrap().id = rune_model::UserId(2);
    }
    let channel = text_channels(&snapshot.channels)[2];
    let message = provider.history(channel, None, 1).0.remove(0);
    drop(Storage::open(path).unwrap());
    let old = rusqlite::Connection::open(path).unwrap();
    old.execute(
        "INSERT INTO snapshots (account, body) VALUES ('Simulación local', ?1)",
        [serde_json::to_string(&snapshot).unwrap()],
    )
    .unwrap();
    old.execute(
        "INSERT INTO drafts (account, channel, text) VALUES ('Simulación local', ?1, 'legacy draft')",
        [channel.0 as i64],
    )
    .unwrap();
    old.execute(
        "INSERT INTO messages (account, channel, id, body) VALUES ('Simulación local', ?1, ?2, ?3)",
        rusqlite::params![
            channel.0 as i64,
            message.id.0 as i64,
            serde_json::to_string(&message).unwrap()
        ],
    )
    .unwrap();
    (snapshot, channel, message)
}

/// Rows of `account` in every table, sealed or not.
fn rows(path: &std::path::Path, account: &str) -> i64 {
    rusqlite::Connection::open(path)
        .unwrap()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM messages WHERE account = ?1)
                  + (SELECT COUNT(*) FROM drafts WHERE account = ?1)
                  + (SELECT COUNT(*) FROM snapshots WHERE account = ?1)
                  + (SELECT COUNT(*) FROM read_marks WHERE account = ?1)
                  + (SELECT COUNT(*) FROM found WHERE account = ?1)",
            [account],
            |row| row.get(0),
        )
        .unwrap()
}

#[test]
fn a_legacy_cache_moves_only_after_its_account_matches_the_explicit_identity() {
    let directory = directory("legacy-match");
    let path = directory.join("rune.db");
    let (snapshot, channel, message) = legacy_fixture(&path, false);
    let client = client(Arc::new(mock()), Some(path.clone()));
    assert_eq!(client.cached_snapshot(), Some(snapshot));
    assert_eq!(client.drafts(), vec![(channel, "legacy draft".into())]);
    let account = client.account().to_owned();
    assert_eq!(account, "12:rune.mock.v1:1");
    drop(client);
    let storage = unlocked(&path, &account);
    assert_eq!(
        storage.recent(&account, channel, 10).unwrap(),
        vec![message]
    );
    drop(storage);
    assert_eq!(rows(&path, "Simulación local"), 0);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_legacy_cache_of_a_different_account_is_not_migrated_or_exposed_and_goes() {
    let directory = directory("legacy-mismatch");
    let path = directory.join("rune.db");
    let (_, channel, _) = legacy_fixture(&path, true);
    let client = client(Arc::new(mock()), Some(path.clone()));
    assert!(client.cached_snapshot().is_none());
    assert!(client.drafts().is_empty());
    let account = client.account().to_owned();
    drop(client);
    let storage = unlocked(&path, &account);
    assert!(storage.recent(&account, channel, 10).unwrap().is_empty());
    drop(storage);
    // In plain text, and no key would ever seal it: it is not left behind.
    assert_eq!(rows(&path, "Simulación local"), 0);
    let _ = std::fs::remove_dir_all(directory);
}
