use super::*;

#[tokio::test]
async fn multiple_grants_preserve_order_and_do_not_read_or_reopen_either_file() {
    let fixture = Fixture::new();
    let first = fixture.file("first.txt", b"one");
    let second = fixture.file("second.txt", b"second");
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    let selected = worker
        .open(vec![first.clone(), second.clone()], 2, 8, 9, signal)
        .await
        .unwrap();
    assert_eq!(
        selected
            .iter()
            .map(|file| file.info().display_name.as_str())
            .collect::<Vec<_>>(),
        ["first.txt", "second.txt"]
    );
    assert_eq!(
        selected
            .iter()
            .map(|file| file.info().size_bytes)
            .sum::<u64>(),
        9
    );
    std::fs::rename(&first, fixture.0.path().join("old-first")).unwrap();
    std::fs::rename(&second, fixture.0.path().join("old-second")).unwrap();
    std::fs::write(&first, b"replacement").unwrap();
    std::fs::write(&second, b"replacement").unwrap();
    for (file, expected) in selected
        .into_iter()
        .zip([b"one".as_slice(), b"second".as_slice()])
    {
        let (mut descriptor, _, _) = file.into_parts();
        assert_eq!(descriptor.stream_position().unwrap(), 0);
        assert!(descriptor.write_all(b"changed").is_err());
        let mut bytes = Vec::new();
        descriptor.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }
}

#[tokio::test]
async fn a_batch_budget_counts_every_file_and_accepts_empty_files_at_zero_total() {
    let fixture = Fixture::new();
    let first = fixture.file("first", b"1234");
    let second = fixture.file("second", b"5678");
    let empty_a = fixture.file("empty-a", b"");
    let empty_b = fixture.file("empty-b", b"");
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    assert_eq!(
        worker
            .open(vec![first.clone(), second.clone()], 2, 4, 7, signal.clone())
            .await
            .unwrap_err(),
        PickError::TooLarge
    );
    assert_eq!(
        worker
            .open(vec![first, second], 2, 4, 8, signal.clone())
            .await
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        worker
            .open(vec![empty_a, empty_b], 2, 1, 0, signal)
            .await
            .unwrap()
            .len(),
        2
    );
}

#[tokio::test]
async fn every_uri_and_decoded_duplicate_is_checked_before_the_first_file_is_opened() {
    let fixture = Fixture::new();
    let directory = fixture.0.path().join("alias");
    std::fs::create_dir(&directory).unwrap();
    let uri = url::Url::from_file_path(&directory).unwrap().to_string();
    let alias = uri.strip_suffix("alias").unwrap().to_owned() + "%61lias";
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    for uris in [
        vec![uri.clone(), "https://example.invalid/file".into()],
        vec![uri.clone(), alias],
    ] {
        assert_eq!(
            worker
                .open_uris(&uris, 2, 1024, 2048, signal.clone())
                .await
                .unwrap_err(),
            PickError::InvalidResponse,
            "opening the directory first would incorrectly return NotRegularFile"
        );
        assert!(
            worker.pending.is_none(),
            "invalid batches never acquire the file worker"
        );
    }
}

#[tokio::test]
async fn excess_or_empty_batches_are_rejected_before_missing_paths_can_be_opened() {
    let fixture = Fixture::new();
    let paths: Vec<_> = (0..9)
        .map(|id| fixture.0.path().join(format!("missing-{id}")))
        .collect();
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    assert_eq!(
        worker
            .open(paths, 8, 1024, 8192, signal.clone())
            .await
            .unwrap_err(),
        PickError::TooMany
    );
    assert_eq!(
        worker
            .open(Vec::new(), 8, 1024, 8192, signal)
            .await
            .unwrap_err(),
        PickError::InvalidResponse
    );
    assert!(worker.pending.is_none());
}

#[tokio::test]
async fn failure_of_a_later_file_returns_no_partial_grant_and_the_worker_remains_usable() {
    let fixture = Fixture::new();
    let valid = fixture.file("valid", b"fixture");
    let missing = fixture.0.path().join("missing");
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    assert_eq!(
        worker
            .open(vec![valid.clone(), missing], 2, 1024, 2048, signal.clone())
            .await
            .unwrap_err(),
        PickError::Failed
    );
    assert_eq!(
        worker
            .open(
                vec![valid.clone(), fixture.0.path().to_owned()],
                2,
                1024,
                2048,
                signal.clone()
            )
            .await
            .unwrap_err(),
        PickError::NotRegularFile
    );
    let selected = worker.open(vec![valid], 1, 7, 7, signal).await.unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].info().display_name, "valid");
}

#[tokio::test]
async fn cancellation_keeps_the_entire_batch_in_the_same_worker_slot_until_drained() {
    let fixture = Fixture::new();
    let first = fixture.file("first", b"one");
    let second = fixture.file("second", b"two");
    let mut worker = FileWorker::new().unwrap();
    let jobs = worker.jobs.clone();
    let paused = pause(&jobs);
    let (cancel, signal) = watch::channel(false);
    let mut opening = Box::pin(worker.open(vec![first.clone(), second.clone()], 2, 3, 6, signal));
    poll_pending(opening.as_mut()).await;
    cancel.send_replace(true);
    assert_eq!(opening.await.unwrap_err(), PickError::Cancelled);
    let (_next_cancel, next_signal) = watch::channel(false);
    let mut next = Box::pin(worker.open(vec![second], 1, 3, 3, next_signal));
    poll_pending(next.as_mut()).await;
    paused.resume();
    let selected = next.await.unwrap();
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].info().display_name, "second");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn encoded_slashes_cannot_hide_a_dot_segment_from_uri_validation() {
    let fixture = Fixture::new();
    let chosen = fixture.0.path().join("chosen");
    let actual = fixture.0.path().join("actual");
    std::fs::create_dir_all(&chosen).unwrap();
    std::fs::create_dir_all(actual.join("nested")).unwrap();
    std::fs::write(actual.join("target"), b"original selection").unwrap();
    std::fs::write(chosen.join("target"), b"different normalized file").unwrap();
    std::os::unix::fs::symlink(actual.join("nested"), chosen.join("link")).unwrap();
    let base = url::Url::from_directory_path(&chosen).unwrap().to_string();
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();

    // A literal percent escape in a real filename must remain selectable.
    let literal = fixture.file("literal%2F.txt", b"literal filename");
    let literal_uri = url::Url::from_file_path(&literal).unwrap().to_string();
    assert!(literal_uri.contains("%252F"));
    let selected = worker
        .open_uris(&[literal_uri], 1, 1024, 1024, signal.clone())
        .await
        .unwrap();
    assert_eq!(selected[0].info().display_name, "literal%2F.txt");

    for slash in ["%2F", "%2f"] {
        let uri = format!("{base}link{slash}..{slash}target");
        let decoded = url::Url::parse(&uri).unwrap().to_file_path().unwrap();
        assert_eq!(
            decoded,
            chosen.join("link/../target"),
            "this fixture must exercise an accepted encoded separator"
        );
        assert_eq!(std::fs::read(&decoded).unwrap(), b"original selection");
        assert_eq!(
            worker
                .open_uris(&[uri], 1, 1024, 1024, signal.clone())
                .await
                .unwrap_err(),
            PickError::InvalidResponse,
            "a URI segment cannot introduce filesystem separators after validation"
        );
    }
}
