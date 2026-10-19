use super::*;
use crate::{Backoff, Client, ClientConfig, Deadlines, Update};
use std::path::PathBuf;

struct PathsClient {
    client: Client,
    woke: sync::Receiver<()>,
    account: UserId,
    channel: ChannelId,
}

impl PathsClient {
    fn new() -> Self {
        Self::with_storage(None)
    }

    fn with_storage(storage: Option<PathBuf>) -> Self {
        let provider = Arc::new(MockProvider::new(MockConfig {
            latency: Duration::ZERO,
            ambient: false,
            replies: false,
            ..MockConfig::default()
        }));
        let snapshot = provider.snapshot();
        let account = snapshot.me.as_ref().unwrap().id;
        let channel = snapshot
            .channels
            .iter()
            .find(|channel| {
                channel.kind == ChannelKind::Text && channel.permissions.contains(REQUIRED)
            })
            .unwrap()
            .id;
        let (wake, woke) = sync::channel();
        let mut client = Client::start(
            provider,
            ClientConfig {
                storage,
                keyring: Some(Arc::new(crate::tests::DerivedKeys)),
                wake: Arc::new(move || {
                    let _ = wake.send(());
                }),
                backoff: Backoff::default(),
                deadlines: Deadlines::default(),
                image_cache: None,
            },
        );
        // As the window starts one: the store's reads find the key.
        client.wait_for_key();
        let mut h = Self {
            client,
            woke,
            account,
            channel,
        };
        h.until(|client| {
            client.poll(64);
            (client.uploads.shared.lock().unwrap().account == Some(account)).then_some(())
        });
        h
    }

    fn until<T>(&mut self, mut find: impl FnMut(&mut Client) -> Option<T>) -> T {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(result) = find(&mut self.client) {
                return result;
            }
            self.woke
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .expect("the bounded service operation must finish without sleeps or UI I/O");
        }
    }

    fn prepared(&mut self, request: RequestId) -> Result<Vec<PendingAttachment>, UploadError> {
        let update = self.until(|client| {
            client.poll(64);
            client.poll_upload()
        });
        let UploadUpdate::Prepared {
            account,
            channel,
            request: found,
            result,
        } = update
        else {
            panic!("preparation must deliver one metadata terminal")
        };
        assert_eq!(
            (account, channel, found),
            (self.account, self.channel, request)
        );
        result
    }
}

fn fixtures() -> (tempfile::TempDir, Vec<PathBuf>) {
    let root = tempfile::tempdir().unwrap();
    let paths = vec![
        root.path().join("first.txt"),
        root.path().join("second.txt"),
    ];
    std::fs::write(&paths[0], b"first").unwrap();
    std::fs::write(&paths[1], b"second").unwrap();
    (root, paths)
}

#[test]
fn paths_prepare_a_frozen_batch_without_sending_before_explicit_submission() {
    let (_root, paths) = fixtures();
    let mut h = PathsClient::new();
    h.client
        .prepare_attachment_paths(100, h.channel, paths.clone())
        .unwrap();
    let files = h.prepared(100).unwrap();
    assert_eq!(
        files
            .iter()
            .map(|file| (file.filename.as_str(), file.size_bytes))
            .collect::<Vec<_>>(),
        vec![("first.txt", 5), ("second.txt", 6)]
    );
    assert!(h.client.uploads.shared.lock().unwrap().sends.is_empty());
    // Changing every path after preparation must not change the frozen body
    // or turn the explicit send into a fresh filesystem operation.
    for path in &paths {
        std::fs::write(path, b"changed after preparation; never reopen").unwrap();
    }
    let nonce = h
        .client
        .send_attachments(
            101,
            h.channel,
            String::new(),
            None,
            false,
            files.iter().map(|file| file.token).collect(),
        )
        .unwrap();
    let message = h.until(|client| {
        client.poll(64).into_iter().find_map(|update| match update {
            Update::Provider(Event::MessageCreated {
                message,
                nonce: Some(found),
            }) if found == nonce => Some(message),
            _ => None,
        })
    });
    assert_eq!((message.author, message.channel), (h.account, h.channel));
    assert_eq!(
        message
            .attachments
            .iter()
            .map(|file| (file.filename.as_str(), file.size_bytes))
            .collect::<Vec<_>>(),
        vec![("first.txt", 5), ("second.txt", 6)]
    );
    assert!(h.client.uploads.shared.lock().unwrap().tokens.is_empty());
}

#[test]
fn invalid_path_batches_fail_admission_without_reserving_a_picker_or_tokens() {
    let (_root, paths) = fixtures();
    let mut h = PathsClient::new();
    for (paths, expected) in [
        (Vec::new(), UploadError::InvalidFile),
        (
            vec![PathBuf::from("relative.txt")],
            UploadError::InvalidFile,
        ),
        (
            vec![paths[0].clone(), paths[0].clone()],
            UploadError::InvalidFile,
        ),
        (
            (0..11)
                .map(|index| PathBuf::from(format!("/unused-fixture-{index}")))
                .collect(),
            UploadError::TooMany,
        ),
        (
            vec![PathBuf::from(format!("/{}", "x".repeat(4096)))],
            UploadError::InvalidFile,
        ),
    ] {
        let token = h.client.uploads.next_token;
        assert_eq!(
            h.client.prepare_attachment_paths(100, h.channel, paths),
            Err(expected)
        );
        let state = h.client.uploads.shared.lock().unwrap();
        assert!(state.pick.is_none());
        assert!(state.tokens.is_empty());
        assert_eq!(h.client.uploads.next_token, token);
    }
}

#[test]
fn second_path_failure_is_atomic_and_does_not_prevent_the_next_import() {
    let (_root, paths) = fixtures();
    let mut h = PathsClient::new();
    let missing = paths[1].with_file_name("missing.txt");
    h.client
        .prepare_attachment_paths(100, h.channel, vec![paths[0].clone(), missing])
        .unwrap();
    assert!(h.prepared(100).is_err());
    assert!(h.client.uploads.shared.lock().unwrap().tokens.is_empty());
    assert_eq!(h.client.uploads.budget.used(), (0, 0));
    h.client
        .prepare_attachment_paths(101, h.channel, paths)
        .unwrap();
    assert_eq!(h.prepared(101).unwrap().len(), 2);
}

#[test]
fn path_import_and_picker_share_one_cancellable_busy_slot_and_drain_old_reads() {
    let (_root, paths) = fixtures();
    let mut h = PathsClient::new();
    let (finished, answer) = tokio::sync::oneshot::channel();
    let (installed, ready) = sync::channel();
    h.client
        .uploads
        .jobs
        .try_send(Job::RetainCancelledRead { answer, installed })
        .ok()
        .unwrap();
    ready.recv_timeout(Duration::from_secs(3)).unwrap();
    h.client
        .prepare_attachment_paths(100, h.channel, paths.clone())
        .unwrap();
    assert_eq!(
        h.client.pick_attachment(101, h.channel, None),
        Err(UploadError::Busy)
    );
    h.client.cancel_upload(100);
    assert_eq!(h.prepared(100).unwrap_err(), UploadError::Cancelled);
    assert!(h.client.uploads.shared.lock().unwrap().tokens.is_empty());
    // Complete the same previous worker slot, not a replacement thread/job.
    finished
        .send((Err(UploadError::Cancelled), Vec::new()))
        .ok()
        .unwrap();
    h.client
        .prepare_attachment_paths(102, h.channel, paths)
        .unwrap();
    assert_eq!(h.prepared(102).unwrap().len(), 2);
}

#[test]
fn a_kept_draft_comes_back_without_the_files_gone_or_changed() {
    use rune_storage::DraftFile;
    let (root, paths) = fixtures();
    let mut h = PathsClient::new();
    h.client
        .prepare_attachment_paths(100, h.channel, paths.clone())
        .unwrap();
    let tokens: Vec<_> = h
        .prepared(100)
        .unwrap()
        .iter()
        .map(|file| file.token)
        .collect();
    // Kept by where each is and its size, never its bytes.
    let kept = h.client.uploads.draft_files(h.channel, &tokens);
    let saved = |path: &PathBuf, size_bytes| DraftFile {
        path: path.to_str().unwrap().into(),
        size_bytes,
    };
    assert_eq!(kept, vec![saved(&paths[0], 5), saved(&paths[1], 6)]);
    assert!(h
        .client
        .uploads
        .draft_files(ChannelId(1), &tokens)
        .is_empty());
    // A later run: one as it was, one changed since, one gone.
    std::fs::write(&paths[1], b"changed").unwrap();
    let mut later = kept.clone();
    later.push(saved(&root.path().join("gone.txt"), 1));
    h.client.uploads.restore(101, h.channel, later).unwrap();
    let said = h.until(|client| {
        client.poll(64);
        client.poll_upload()
    });
    assert_eq!(
        said,
        UploadUpdate::Skipped {
            account: h.account,
            channel: h.channel,
            request: 101,
            files: vec!["second.txt".into(), "gone.txt".into()],
        }
    );
    let back = h.prepared(101).unwrap();
    assert_eq!(
        back.iter()
            .map(|file| (file.filename.as_str(), file.size_bytes))
            .collect::<Vec<_>>(),
        vec![("first.txt", 5)]
    );
    // It is kept again from where it came.
    assert_eq!(
        h.client.uploads.draft_files(h.channel, &[back[0].token]),
        vec![kept[0].clone()]
    );
    // None left: only said, and nothing comes back.
    std::fs::remove_file(&paths[0]).unwrap();
    h.client
        .uploads
        .restore(102, h.channel, vec![kept[0].clone()])
        .unwrap();
    assert!(matches!(
        h.until(|client| {
            client.poll(64);
            client.poll_upload()
        }),
        UploadUpdate::Skipped { files, .. } if files == ["first.txt"]
    ));
    assert_eq!(h.prepared(102), Err(UploadError::Cancelled));
    // A path that cannot be one is left out alone, named like the rest.
    h.client
        .uploads
        .restore(
            103,
            h.channel,
            vec![DraftFile {
                path: "relativo.txt".into(),
                size_bytes: 1,
            }],
        )
        .unwrap();
    assert!(matches!(
        h.until(|client| {
            client.poll(64);
            client.poll_upload()
        }),
        UploadUpdate::Skipped { files, .. } if files == ["relativo.txt"]
    ));
    assert_eq!(h.prepared(103), Err(UploadError::Cancelled));
}

/// `count` one-byte files, kept as a draft keeps them.
fn kept_files(count: usize) -> (tempfile::TempDir, Vec<rune_storage::DraftFile>) {
    let root = tempfile::tempdir().unwrap();
    let kept = (0..count)
        .map(|index| {
            let path = root.path().join(format!("f{index}.txt"));
            std::fs::write(&path, b"x").unwrap();
            rune_storage::DraftFile {
                path: path.to_str().unwrap().into(),
                size_bytes: 1,
            }
        })
        .collect();
    (root, kept)
}

#[test]
fn kept_files_past_a_message_are_named_and_the_rest_come_back() {
    let (_root, kept) = kept_files(11);
    let mut h = PathsClient::new();
    // Eleven never fit a message of ten: the last is said, ten come back.
    h.client
        .uploads
        .restore(100, h.channel, kept.clone())
        .unwrap();
    assert!(matches!(
        h.until(|client| {
            client.poll(64);
            client.poll_upload()
        }),
        UploadUpdate::Skipped { files, .. } if files == ["f10.txt"]
    ));
    assert_eq!(h.prepared(100).unwrap().len(), 10);
    // Full now, a kept file waits instead of failing.
    assert_eq!(
        h.client.uploads.restore(101, h.channel, kept[..1].to_vec()),
        Err(UploadError::Busy)
    );
}

#[test]
fn a_repeated_kept_path_comes_back_once_and_files_without_room_wait() {
    let (_root, kept) = kept_files(11);
    let mut h = PathsClient::new();
    let twice = vec![kept[0].clone(), kept[0].clone(), kept[1].clone()];
    h.client.uploads.restore(100, h.channel, twice).unwrap();
    assert_eq!(
        h.prepared(100)
            .unwrap()
            .iter()
            .map(|file| file.filename.as_str())
            .collect::<Vec<_>>(),
        ["f0.txt", "f1.txt"],
        "nothing is said to be left out"
    );
    // Nine would fit a message, but two are held: they wait, none lost.
    assert_eq!(
        h.client.uploads.restore(101, h.channel, kept[2..].to_vec()),
        Err(UploadError::Busy)
    );
    assert!(h.client.uploads.shared.lock().unwrap().pick.is_none());
}

#[test]
fn a_drafts_files_are_kept_by_path_and_come_back_once_in_the_next_run() {
    let (root, paths) = fixtures();
    let store = root.path().join("store").join("rune.sqlite");
    {
        let mut h = PathsClient::with_storage(Some(store.clone()));
        h.client
            .prepare_attachment_paths(100, h.channel, paths.clone())
            .unwrap();
        let tokens: Vec<_> = h
            .prepared(100)
            .unwrap()
            .iter()
            .map(|file| file.token)
            .collect();
        h.client.flush_storage().unwrap();
        h.client.set_draft_files(h.channel, &tokens).unwrap();
        // Closing waits for the store's queue.
    }
    {
        // A run without room for them: they wait, and a draft kept again
        // meanwhile still keeps them.
        let (_others, others) = kept_files(9);
        let mut h = PathsClient::with_storage(Some(store.clone()));
        h.client.load_draft_files();
        h.client
            .prepare_attachment_paths(
                100,
                h.channel,
                others
                    .iter()
                    .map(|file| PathBuf::from(&file.path))
                    .collect(),
            )
            .unwrap();
        assert_eq!(h.prepared(100).unwrap().len(), 9);
        assert_eq!(
            h.client.restore_draft_files(101, h.channel),
            Err(UploadError::Busy)
        );
        assert!(h.client.has_draft_files(h.channel), "still waiting");
        h.client.flush_storage().unwrap();
        h.client.set_draft_files(h.channel, &[]).unwrap();
    }
    let mut h = PathsClient::with_storage(Some(store));
    h.client.load_draft_files();
    assert!(h.client.has_draft_files(h.channel));
    assert!(!h.client.has_draft_files(ChannelId(1)));
    h.client.restore_draft_files(101, h.channel).unwrap();
    assert!(!h.client.has_draft_files(h.channel), "once");
    assert_eq!(
        h.prepared(101)
            .unwrap()
            .iter()
            .map(|file| file.filename.as_str())
            .collect::<Vec<_>>(),
        ["first.txt", "second.txt"]
    );
    assert_eq!(
        h.client.restore_draft_files(102, h.channel),
        Err(UploadError::NotFound)
    );
}
