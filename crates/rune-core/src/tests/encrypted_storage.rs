//! The encrypted local cache (2026-10-18): an account named after its
//! verification, its key from a keyring, what it restores before READY, and
//! forgetting it. Keyrings here are fakes; nothing reaches the person's.

use super::*;
use rune_model::{ReadMark, UserId};
use rune_platform::secrets::{Entry, Saved, SecretError, SecretFuture, SecretStore};

/// A provider whose events the test sends, one session long; it names no
/// account up front, as the unofficial session does not.
struct Scripted {
    inner: MockProvider,
    script: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<Event>>>,
}

impl Provider for Scripted {
    fn info(&self) -> ProviderInfo {
        self.inner.info()
    }

    fn connect<'a>(
        &'a self,
        commands: &'a mut mpsc::Receiver<Command>,
        events: &'a EventSink,
    ) -> rune_provider::BoxFuture<'a, Result<(), ProviderError>> {
        Box::pin(async move {
            let Some(mut script) = self.script.lock().unwrap().take() else {
                return Ok(());
            };
            loop {
                tokio::select! {
                    event = script.recv() => match event {
                        Some(event) => {
                            if events.send(event).await.is_err() {
                                return Ok(());
                            }
                        }
                        None => return Ok(()),
                    },
                    command = commands.recv() => if command.is_none() {
                        return Ok(());
                    },
                }
            }
        })
    }
}

fn scripted() -> (Arc<Scripted>, tokio::sync::mpsc::UnboundedSender<Event>) {
    let (sender, script) = tokio::sync::mpsc::unbounded_channel();
    let provider = Arc::new(Scripted {
        inner: mock(),
        script: std::sync::Mutex::new(Some(script)),
    });
    (provider, sender)
}

/// A keyring in memory that says what was asked of it. With `gate`, keys
/// are given only once the test opens it; with `fail`, nothing is.
#[derive(Default)]
struct Recording {
    saved: std::sync::Mutex<HashMap<String, String>>,
    deleted: std::sync::Mutex<Vec<String>>,
    gate: Option<tokio::sync::Semaphore>,
    fail: bool,
}

impl SecretStore for Recording {
    fn list<'a>(&'a self, _: &'a str) -> SecretFuture<'a, Vec<Entry>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn load<'a>(&'a self, service: &'a str, account: &'a str) -> SecretFuture<'a, Option<Saved>> {
        Box::pin(async move {
            assert_eq!(service, rune_platform::secrets::CACHE_KEYS);
            if let Some(gate) = &self.gate {
                let _ = gate.acquire().await.unwrap();
            }
            if self.fail {
                return Err(SecretError::UNAVAILABLE);
            }
            Ok(self.saved.lock().unwrap().get(account).map(|secret| Saved {
                account: account.into(),
                secret: secret.clone().into(),
            }))
        })
    }

    fn save<'a>(
        &'a self,
        service: &'a str,
        account: &'a str,
        _: &'a str,
        _: &'a str,
        secret: &'a str,
    ) -> SecretFuture<'a, ()> {
        Box::pin(async move {
            assert_eq!(service, rune_platform::secrets::CACHE_KEYS);
            self.saved
                .lock()
                .unwrap()
                .insert(account.into(), secret.into());
            Ok(())
        })
    }

    fn delete<'a>(&'a self, service: &'a str, account: &'a str) -> SecretFuture<'a, ()> {
        Box::pin(async move {
            assert_eq!(service, rune_platform::secrets::CACHE_KEYS);
            self.saved.lock().unwrap().remove(account);
            self.deleted.lock().unwrap().push(account.into());
            Ok(())
        })
    }
}

fn start(
    provider: Arc<dyn Provider>,
    path: &std::path::Path,
    keyring: Option<Arc<dyn SecretStore>>,
) -> Client {
    let mut client = launch(provider, Some(path), keyring);
    client.wait_for_key();
    client
}

/// A client as a change of provider starts one: nothing waits for a key.
fn launch(
    provider: Arc<dyn Provider>,
    path: Option<&std::path::Path>,
    keyring: Option<Arc<dyn SecretStore>>,
) -> Client {
    Client::start(
        provider,
        ClientConfig {
            wake: Arc::new(|| {}),
            storage: path.map(std::path::Path::to_path_buf),
            keyring,
            backoff: Backoff::default(),
            deadlines: Deadlines::default(),
            image_cache: None,
        },
    )
}

const SESSION: &str = "rune.test-session.v1";

fn identity(account: u64) -> StorageIdentity {
    StorageIdentity {
        provider: SESSION,
        account: UserId(account),
    }
}

fn directory(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rune-encrypted-storage-{name}-{}",
        std::process::id()
    ))
}

/// Wait until the committed barrier: everything submitted so far is on disk.
fn settle(client: &mut Client) {
    let ticket = client.flush_storage().unwrap().unwrap();
    let until = Instant::now() + Duration::from_secs(3);
    while !client.storage_committed(ticket) && Instant::now() < until {
        client.poll(64);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(client.storage_committed(ticket));
}

#[test]
fn an_account_named_after_its_verification_shows_what_was_kept_before_ready() {
    let directory = directory("restore");
    let path = directory.join("rune.db");
    let (provider, script) = scripted();
    let kept = provider.inner.snapshot();
    let account = kept.me.as_ref().unwrap().id;
    let channel = text_channels(&kept.channels)[0];
    let namespace = namespace(identity(account.0));
    let mark = ReadMark {
        through: Some(MessageId(10)),
        unread_from: None,
        revision: 4,
    };
    {
        // What an earlier run kept, sealed with the account's key.
        let mut disk = unlocked(&path, &namespace);
        disk.save_snapshot(&namespace, &kept).unwrap();
        disk.set_draft(&namespace, channel, Some("a medio escribir"))
            .unwrap();
        disk.set_read_mark(&namespace, channel, mark).unwrap();
        let messages = provider.inner.history(channel, None, 3).0;
        disk.save_messages(&namespace, &messages).unwrap();
        // Where its draft's files are (envios-y-subidas).
        let file = rune_storage::DraftFile {
            path: "/home/vos/contrato.pdf".into(),
            size_bytes: 3,
        };
        disk.set_draft_files(&namespace, channel, &[file]).unwrap();
    }
    let mut client = start(provider.clone(), &path, Some(Arc::new(DerivedKeys)));
    assert_eq!(client.storage_account(), None, "nothing before the name");
    assert!(!client.has_draft_files(channel));
    script
        .send(Event::StorageIdentity(identity(account.0)))
        .unwrap();
    let restored = wait(&mut client, |update| match update {
        Update::Provider(Event::Ready(_)) => panic!("no Ready was sent"),
        Update::Restored(restored) => Some(restored.clone()),
        _ => None,
    });
    assert_eq!(restored.account, account);
    assert_eq!(restored.snapshot.as_ref(), Some(&kept));
    assert_eq!(restored.drafts, vec![(channel, "a medio escribir".into())]);
    assert_eq!(restored.read_marks, Ok(vec![(channel, mark)]));
    assert_eq!(client.storage_account(), Some(account));
    // Its draft's files stay in Core until the conversation opens.
    assert!(restored.draft_files.is_empty());
    assert!(client.has_draft_files(channel));
    // The conversation opens from the cache while its page is asked.
    client.load_latest(channel);
    wait(&mut client, |update| match update {
        Update::Cached {
            channel: id,
            messages,
        } if *id == channel => Some(messages.len()),
        _ => None,
    });
    // READY confirms it, and its snapshot is kept for the next start.
    let mut fresh = kept.clone();
    fresh.servers.truncate(1);
    script.send(Event::Ready(Box::new(fresh.clone()))).unwrap();
    ready(&mut client);
    settle(&mut client);
    drop(client);
    let disk = unlocked(&path, &namespace);
    assert_eq!(
        disk.snapshot::<Snapshot>(&namespace)
            .unwrap()
            .unwrap()
            .servers,
        fresh.servers
    );
    drop(disk);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn ready_waits_for_the_read_marks_result_before_reporting_a_storage_failure() {
    for corrupt in [false, true] {
        let directory = directory(if corrupt {
            "ready-before-failed-marks"
        } else {
            "ready-before-restored-marks"
        });
        let path = directory.join("rune.db");
        let (provider, _script) = scripted();
        let snapshot = provider.inner.snapshot();
        let account = snapshot.me.as_ref().unwrap().id;
        let channel = text_channels(&snapshot.channels)[0];
        let identity = identity(account.0);
        let namespace = namespace(identity);
        let mark = ReadMark {
            through: Some(MessageId(10)),
            unread_from: None,
            revision: 4,
        };
        {
            let disk = unlocked(&path, &namespace);
            disk.set_read_mark(&namespace, channel, mark).unwrap();
        }
        if corrupt {
            rusqlite::Connection::open(&path)
                .unwrap()
                .execute("UPDATE read_marks SET body='broken'", [])
                .unwrap();
        }
        let mut client = start(provider, &path, None);
        wait(&mut client, |update| {
            matches!(
                update,
                Update::Provider(Event::Connection(ConnectionState::Connecting))
            )
            .then_some(())
        });
        // Deliver the test key directly, then READY before Core consumes the
        // worker's answer. No timing or keyring speed decides this ordering.
        client.adopt(Some(identity));
        client.unlock(derived_key(&namespace), false, true);
        client.flush_pending_storage().unwrap();
        assert_eq!(client.storage_account(), Some(account));
        assert!(client.storage_error().is_none());
        client.accept(
            Envelope {
                generation: client.generation,
                event: Event::Ready(Box::new(snapshot)),
            },
            &mut Vec::new(),
        );
        assert!(
            client.storage_error().is_none(),
            "a pending restore is not a failed restore"
        );
        let restored = wait(&mut client, |update| match update {
            Update::Restored(restored) => Some(restored.clone()),
            _ => None,
        });
        if corrupt {
            assert_eq!(restored.read_marks, Err(rune_model::LocalReadError::Failed));
            assert!(
                client.storage_error().unwrap().contains("sólo en memoria"),
                "a confirmed restore failure must still warn"
            );
        } else {
            assert_eq!(restored.read_marks, Ok(vec![(channel, mark)]));
            assert!(client.storage_error().is_none());
        }
        drop(client);
        let _ = std::fs::remove_dir_all(directory);
    }
}

#[test]
fn a_ready_that_comes_before_the_key_is_kept_once_the_key_is_in() {
    let directory = directory("ready-first");
    let path = directory.join("rune.db");
    let (provider, script) = scripted();
    let snapshot = provider.inner.snapshot();
    let account = snapshot.me.as_ref().unwrap().id;
    let keyring = Arc::new(Recording {
        gate: Some(tokio::sync::Semaphore::new(0)),
        ..Default::default()
    });
    let mut client = start(provider.clone(), &path, Some(keyring.clone()));
    script
        .send(Event::StorageIdentity(identity(account.0)))
        .unwrap();
    script
        .send(Event::Ready(Box::new(snapshot.clone())))
        .unwrap();
    ready(&mut client);
    assert_eq!(client.storage_account(), None, "the key is not in yet");
    keyring.gate.as_ref().unwrap().add_permits(1);
    let deadline = Instant::now() + Duration::from_secs(3);
    while client.storage_account().is_none() && Instant::now() < deadline {
        client.poll(64);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(client.storage_account(), Some(account));
    settle(&mut client);
    drop(client);
    // A key was made once, saved in the keyring, never in the file.
    let namespace = namespace(identity(account.0));
    let saved = keyring
        .saved
        .lock()
        .unwrap()
        .get(&namespace)
        .cloned()
        .unwrap();
    let mut disk = Storage::open(&path).unwrap();
    disk.unlock(&namespace, rune_storage::Key::from_text(&saved).unwrap())
        .unwrap();
    // Kept as any snapshot is: without who is online or in a call.
    let expected = Snapshot {
        presences: Vec::new(),
        voice: Vec::new(),
        ..snapshot
    };
    assert_eq!(
        disk.snapshot::<Snapshot>(&namespace).unwrap(),
        Some(expected)
    );
    drop(disk);
    let bytes = std::fs::read(&path).unwrap();
    assert!(!bytes
        .windows(saved.len())
        .any(|window| window == saved.as_bytes()));
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_ready_of_another_account_before_the_key_keeps_the_store_off() {
    let directory = directory("ready-other");
    let path = directory.join("rune.db");
    let (provider, script) = scripted();
    let snapshot = provider.inner.snapshot();
    let account = snapshot.me.as_ref().unwrap().id;
    let keyring = Arc::new(Recording {
        gate: Some(tokio::sync::Semaphore::new(0)),
        ..Default::default()
    });
    let mut client = start(provider.clone(), &path, Some(keyring.clone()));
    script
        .send(Event::StorageIdentity(identity(account.0)))
        .unwrap();
    let mut other = snapshot.clone();
    other.me.as_mut().unwrap().id = UserId(account.0 + 1);
    script.send(Event::Ready(Box::new(other))).unwrap();
    ready(&mut client);
    keyring.gate.as_ref().unwrap().add_permits(1);
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        client.poll(64);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(client.storage_account(), None);
    // Its own Ready later does not turn it on either: the identity is spent.
    script.send(Event::Ready(Box::new(snapshot))).unwrap();
    ready(&mut client);
    assert_eq!(client.storage_account(), None);
    let channel = text_channels(&provider.inner.snapshot().channels)[0];
    client
        .set_draft(channel, Some("en memoria".into()))
        .unwrap();
    settle(&mut client);
    drop(client);
    let rows: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM drafts) + (SELECT COUNT(*) FROM snapshots)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn forgetting_an_account_deletes_its_rows_and_its_key_and_switching_does_not() {
    let directory = directory("forget");
    let path = directory.join("rune.db");
    let (provider, script) = scripted();
    let keyring = Arc::new(Recording::default());
    let snapshot = provider.inner.snapshot();
    let me = snapshot.me.as_ref().unwrap().id;
    let channel = text_channels(&snapshot.channels)[0];
    let mut client = start(provider.clone(), &path, Some(keyring.clone()));
    let named = |client: &mut Client, account: u64| {
        let deadline = Instant::now() + Duration::from_secs(3);
        while client.storage_account() != Some(UserId(account)) && Instant::now() < deadline {
            client.poll(64);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(client.storage_account(), Some(UserId(account)));
    };
    // Another account, kept from before; then this one.
    script.send(Event::StorageIdentity(identity(7))).unwrap();
    named(&mut client, 7);
    client
        .set_draft(channel, Some("de la otra".into()))
        .unwrap();
    script
        .send(Event::Authentication(AuthenticationState::SignedOut))
        .unwrap();
    script.send(Event::StorageIdentity(identity(me.0))).unwrap();
    named(&mut client, me.0);
    client.set_draft(channel, Some("de esta".into())).unwrap();
    settle(&mut client);
    let other = namespace(identity(7));
    let this = namespace(identity(me.0));
    assert!(
        keyring.deleted.lock().unwrap().is_empty(),
        "switching keeps both"
    );
    let deleted = |client: &mut Client, count: usize| {
        let deadline = Instant::now() + Duration::from_secs(3);
        while keyring.deleted.lock().unwrap().len() < count && Instant::now() < deadline {
            client.poll(64);
            std::thread::sleep(Duration::from_millis(1));
        }
        settle(client);
    };
    // Forgetting the other: its rows and key go; this one stays as it was.
    script.send(Event::ForgetStorage(identity(7))).unwrap();
    deleted(&mut client, 1);
    assert_eq!(client.storage_account(), Some(me));
    // Forgetting this one, as signing out does: the store keeps nothing more.
    script.send(Event::ForgetStorage(identity(me.0))).unwrap();
    deleted(&mut client, 2);
    assert_eq!(client.storage_account(), None);
    client
        .set_draft(channel, Some("en memoria".into()))
        .unwrap();
    settle(&mut client);
    assert_eq!(
        *keyring.deleted.lock().unwrap(),
        vec![other.clone(), this.clone()]
    );
    assert!(keyring.saved.lock().unwrap().is_empty());
    drop(client);
    let rows: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT (SELECT COUNT(*) FROM drafts) + (SELECT COUNT(*) FROM messages)
                  + (SELECT COUNT(*) FROM snapshots) + (SELECT COUNT(*) FROM read_marks)
                  + (SELECT COUNT(*) FROM found)",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(rows, 0, "nothing of either account is left");
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn without_a_keyring_or_with_a_failing_one_an_account_stays_in_memory() {
    for keyring in [
        None,
        Some(Arc::new(Recording {
            fail: true,
            ..Default::default()
        })),
    ] {
        let directory = directory(if keyring.is_none() {
            "no-keyring"
        } else {
            "failing-keyring"
        });
        let path = directory.join("rune.db");
        // An account known from the start, as the simulation's.
        let mut client = start(
            Arc::new(mock()),
            &path,
            keyring
                .clone()
                .map(|keyring| keyring as Arc<dyn SecretStore>),
        );
        let channel = text_channels(&ready(&mut client))[0];
        assert_eq!(client.storage_account(), None);
        assert!(client.storage_error().unwrap().contains("sólo en memoria"));
        client
            .set_draft(channel, Some("en memoria".into()))
            .unwrap();
        client.set_setting("zoom", Some("1.25".into())).unwrap();
        settle(&mut client);
        drop(client);
        if let Some(keyring) = keyring {
            assert!(
                keyring.saved.lock().unwrap().is_empty(),
                "no new key over a failure"
            );
        }
        let disk = Storage::open(&path).unwrap();
        assert_eq!(
            disk.setting("zoom").unwrap().as_deref(),
            Some("1.25"),
            "settings stay"
        );
        let rows: i64 = rusqlite::Connection::open(&path)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM drafts", [], |row| row.get(0))
            .unwrap();
        assert_eq!(rows, 0, "nothing of the account reaches the file");
        drop(disk);
        let _ = std::fs::remove_dir_all(directory);
    }
}

#[test]
fn what_a_search_finds_is_kept_apart_from_the_history() {
    let directory = directory("found");
    let path = directory.join("rune.db");
    let provider = Arc::new(mock());
    let mut client = client(provider.clone(), Some(path.clone()));
    let channel = text_channels(&ready(&mut client))[2];
    let found = provider.history(channel, None, 3).0;
    let mut save = Vec::new();
    client.accept(
        Envelope {
            generation: client.generation,
            event: Event::SearchResults {
                request: 5,
                result: Ok(rune_model::RemotePage {
                    messages: found.clone(),
                    total: 3,
                    offset: 0,
                    partial: false,
                }),
            },
        },
        &mut save,
    );
    assert!(matches!(&save[..], [MessageMutation::Found(kept)] if *kept == found));
    client.pending_storage = Some(PendingStorage::Messages(save));
    settle(&mut client);
    let account = client.account().to_owned();
    drop(client);
    let disk = unlocked(&path, &account);
    assert!(
        disk.recent(&account, channel, 10).unwrap().is_empty(),
        "no conversation's history"
    );
    let query = rune_model::SearchQuery {
        channel,
        from: Some(Timestamp(0)),
        ..Default::default()
    };
    let hits = disk
        .search_chunk(&account, &query, None, 20, &|| false)
        .unwrap();
    assert_eq!(hits.hits.len(), 3, "a search reads them");
    drop(disk);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn without_its_key_an_account_s_local_data_can_still_be_deleted() {
    let directory = directory("forget-without-key");
    let path = directory.join("rune.db");
    let provider = Arc::new(mock());
    let namespace = namespace(provider.storage_identity().unwrap());
    let channel = text_channels(&provider.snapshot().channels)[0];
    {
        // Kept by a run that had the keyring, and a row version 4 left plain.
        let disk = unlocked(&path, &namespace);
        disk.set_draft(&namespace, channel, Some("sellado"))
            .unwrap();
    }
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO drafts (account, channel, text) VALUES (?1, 1, 'de v4')",
            [&namespace],
        )
        .unwrap();
    let mut client = start(provider, &path, None);
    ready(&mut client);
    assert_eq!(client.storage_account(), None, "no keyring: in memory");
    // «Borrar datos locales» is confirmed, and the barrier after it closes.
    client.forget_local_data().unwrap();
    settle(&mut client);
    drop(client);
    let rows: i64 = rusqlite::Connection::open(&path)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM drafts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(rows, 0);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_key_made_anew_drops_what_the_lost_one_sealed_and_no_read_fails_for_it() {
    let directory = directory("lost-key");
    let path = directory.join("rune.db");
    let (provider, script) = scripted();
    let snapshot = provider.inner.snapshot();
    let account = snapshot.me.as_ref().unwrap().id;
    let channel = text_channels(&snapshot.channels)[0];
    let namespace = namespace(identity(account.0));
    {
        // Kept by an earlier run, with a key the keyring no longer holds.
        let mut disk = unlocked(&path, &namespace);
        let messages = provider.inner.history(channel, None, 3).0;
        disk.save_messages(&namespace, &messages).unwrap();
        disk.set_draft(&namespace, channel, Some("perdido"))
            .unwrap();
        let mark = ReadMark {
            through: Some(MessageId(10)),
            unread_from: None,
            revision: 4,
        };
        disk.set_read_mark(&namespace, channel, mark).unwrap();
    }
    let keyring = Arc::new(Recording::default());
    let mut client = start(provider.clone(), &path, Some(keyring.clone()));
    script
        .send(Event::StorageIdentity(identity(account.0)))
        .unwrap();
    let restored = wait(&mut client, |update| match update {
        Update::Restored(restored) => Some(restored.clone()),
        _ => None,
    });
    assert_eq!(
        restored.read_marks,
        Ok(Vec::new()),
        "no error at each start"
    );
    assert!(restored.drafts.is_empty());
    settle(&mut client);
    drop(client);
    let saved = keyring
        .saved
        .lock()
        .unwrap()
        .get(&namespace)
        .cloned()
        .unwrap();
    let mut disk = Storage::open(&path).unwrap();
    disk.unlock(&namespace, rune_storage::Key::from_text(&saved).unwrap())
        .unwrap();
    assert_eq!(disk.count(&namespace).unwrap(), 0);
    let query = rune_model::SearchQuery {
        channel,
        from: Some(Timestamp(0)),
        ..Default::default()
    };
    assert!(disk
        .search_chunk(&namespace, &query, None, 20, &|| false)
        .is_ok());
    drop(disk);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn forgetting_an_account_deletes_its_key_even_with_no_store_open() {
    let (provider, script) = scripted();
    let keyring = Arc::new(Recording::default());
    let mut client = launch(provider, None, Some(keyring.clone()));
    script.send(Event::ForgetStorage(identity(7))).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while keyring.deleted.lock().unwrap().is_empty() && Instant::now() < deadline {
        client.poll(64);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        *keyring.deleted.lock().unwrap(),
        vec![namespace(identity(7))]
    );
}

/// Poll a while, so the events sent so far are taken.
fn drain(client: &mut Client) {
    let until = Instant::now() + Duration::from_millis(200);
    while Instant::now() < until {
        client.poll(64);
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn keyring_operations_run_in_order_so_a_key_is_made_once_and_stays_deleted() {
    let directory = directory("keyring-order");
    let path = directory.join("rune.db");
    let (provider, script) = scripted();
    let snapshot = provider.inner.snapshot();
    let account = snapshot.me.as_ref().unwrap().id;
    let channel = text_channels(&snapshot.channels)[0];
    let kept = namespace(identity(account.0));
    let keyring = Arc::new(Recording {
        gate: Some(tokio::sync::Semaphore::new(0)),
        ..Default::default()
    });
    let mut client = start(provider, &path, Some(keyring.clone()));
    // Named, ended and named again before the keyring answers: asked twice.
    script
        .send(Event::StorageIdentity(identity(account.0)))
        .unwrap();
    script
        .send(Event::Authentication(AuthenticationState::SignedOut))
        .unwrap();
    script
        .send(Event::StorageIdentity(identity(account.0)))
        .unwrap();
    drain(&mut client);
    keyring.gate.as_ref().unwrap().add_permits(1);
    let deadline = Instant::now() + Duration::from_secs(3);
    while client.storage_account() != Some(account) && Instant::now() < deadline {
        client.poll(64);
        std::thread::sleep(Duration::from_millis(1));
    }
    client
        .set_draft(channel, Some("una sola clave".into()))
        .unwrap();
    settle(&mut client);
    drop(client);
    // The key the rows were sealed with is the one the keyring keeps.
    let saved = keyring.saved.lock().unwrap().get(&kept).cloned().unwrap();
    let mut disk = Storage::open(&path).unwrap();
    disk.unlock(&kept, rune_storage::Key::from_text(&saved).unwrap())
        .unwrap();
    assert_eq!(
        disk.drafts(&kept).unwrap(),
        vec![(channel, "una sola clave".to_owned())]
    );
    drop(disk);
    // Forgotten while its key is still being asked: the deletion waits.
    let (provider, script) = scripted();
    let keyring = Arc::new(Recording {
        gate: Some(tokio::sync::Semaphore::new(0)),
        ..Default::default()
    });
    let mut client = start(provider, &path, Some(keyring.clone()));
    script.send(Event::StorageIdentity(identity(9))).unwrap();
    script.send(Event::ForgetStorage(identity(9))).unwrap();
    drain(&mut client);
    keyring.gate.as_ref().unwrap().add_permits(1);
    let deadline = Instant::now() + Duration::from_secs(3);
    while keyring.deleted.lock().unwrap().is_empty() && Instant::now() < deadline {
        client.poll(64);
        std::thread::sleep(Duration::from_millis(1));
    }
    drain(&mut client);
    assert_eq!(
        *keyring.deleted.lock().unwrap(),
        vec![namespace(identity(9))]
    );
    assert!(
        keyring.saved.lock().unwrap().is_empty(),
        "no key outlives its account"
    );
    drop(client);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_change_of_provider_never_waits_for_the_keyring_and_restores_through_poll() {
    let directory = directory("switch");
    let path = directory.join("rune.db");
    let provider = Arc::new(mock());
    let namespace = namespace(provider.storage_identity().unwrap());
    let channel = text_channels(&provider.snapshot().channels)[0];
    {
        let disk = unlocked(&path, &namespace);
        disk.set_draft(&namespace, channel, Some("guardado"))
            .unwrap();
    }
    let keyring = Arc::new(Recording {
        gate: Some(tokio::sync::Semaphore::new(0)),
        ..Default::default()
    });
    keyring.saved.lock().unwrap().insert(
        namespace.clone(),
        derived_key(&namespace).to_text().as_str().to_owned(),
    );
    // The keyring asks for its password and nobody has answered yet.
    let started = Instant::now();
    let mut client = launch(provider, Some(&path), Some(keyring.clone()));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "the window's thread never waits for it"
    );
    keyring.gate.as_ref().unwrap().add_permits(1);
    let restored = wait(&mut client, |update| match update {
        Update::Restored(restored) => Some(restored.clone()),
        _ => None,
    });
    assert_eq!(restored.drafts, vec![(channel, "guardado".to_owned())]);
    drop(client);
    let _ = std::fs::remove_dir_all(directory);
}

// Pictures on disk between runs (imagenes-y-lectura).
#[test]
fn pictures_on_disk_wait_for_the_key_follow_the_verified_session_and_go_with_the_account() {
    let directory = directory("pictures");
    let cache = directory.join("cache");
    let (provider, script) = scripted();
    let keyring = Arc::new(Recording::default());
    let mut client = Client::start(
        provider.clone(),
        ClientConfig {
            wake: Arc::new(|| {}),
            storage: Some(directory.join("rune.db")),
            keyring: Some(keyring),
            backoff: Backoff::default(),
            deadlines: Deadlines::default(),
            image_cache: Some(cache.clone()),
        },
    );
    client.wait_for_key();
    let folders = || std::fs::read_dir(cache.join("images")).map_or(0, |folders| folders.count());
    let until = |client: &mut Client, done: &dyn Fn(&mut Client) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !done(client) && Instant::now() < deadline {
            client.poll(64);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(done(client));
    };
    // No account yet: nothing is opened, whatever the limits.
    client.set_image_cache_limits(ImageCacheLimits::default());
    assert!(client.image_cache_usage().is_none());
    script.send(Event::StorageIdentity(identity(7))).unwrap();
    until(&mut client, &|client| client.image_cache_usage().is_some());
    assert_eq!(folders(), 1);
    assert_eq!(client.image_cache_usage(), Some(ImageCacheUsage::default()));
    // A loopback server that answers each of two pictures with a PNG.
    let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = server.local_addr().unwrap();
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        4,
        4,
        image::Rgba([20, 80, 160, 255]),
    ))
    .write_to(&mut png, image::ImageFormat::Png)
    .unwrap();
    let png = png.into_inner();
    let serving = std::thread::spawn(move || {
        use std::io::{BufRead, Write};
        for socket in server.incoming().take(2) {
            let mut socket = socket.unwrap();
            let mut request = std::io::BufReader::new(socket.try_clone().unwrap());
            let mut line = String::new();
            while request.read_line(&mut line).unwrap() > 2 {
                line.clear();
            }
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                png.len()
            )
            .unwrap();
            socket.write_all(&png).unwrap();
        }
    });
    client.images.loopback = true;
    let picture = |name: &str| rune_model::RemoteImage {
        url: format!("http://{address}/{name}.png"),
        kind: rune_model::ImageKind::Artwork,
    };
    let shown = |client: &mut Client, wanted: &rune_model::RemoteImage| {
        !client.load_images(std::slice::from_ref(wanted)).is_empty()
    };
    let pictures = || {
        std::fs::read_dir(cache.join("images"))
            .into_iter()
            .flatten()
            .flatten()
            .flat_map(|account| ["media", "artwork"].map(|part| account.path().join(part)))
            .filter_map(|part| std::fs::read_dir(part).ok())
            .map(Iterator::count)
            .sum::<usize>()
    };
    // The account's own session keeps what it downloads, sealed.
    let own = picture("own");
    until(&mut client, &|client| shown(client, &own));
    assert_eq!(pictures(), 1);
    // A Ready of another account: that session stays in memory, and so do
    // its pictures, even with the first account's disk open.
    let mut other = provider.inner.snapshot();
    other.me.as_mut().unwrap().id = UserId(8);
    script.send(Event::Ready(Box::new(other))).unwrap();
    ready(&mut client);
    assert_eq!(client.storage_account(), None);
    let theirs = picture("theirs");
    until(&mut client, &|client| shown(client, &theirs));
    serving.join().unwrap();
    assert_eq!(pictures(), 1);
    // Forgotten: its folder goes, and no disk is open for it any more.
    script.send(Event::ForgetStorage(identity(7))).unwrap();
    until(&mut client, &|client| {
        client.image_cache_usage().is_none() && folders() == 0
    });
    drop(client);
    let _ = std::fs::remove_dir_all(directory);
}
