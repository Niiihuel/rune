use super::*;
use crate::file_dialogs::open_local_files;
#[cfg(unix)]
use crate::file_dialogs::validate_local_paths;

// Unix paths and file names (windows-base).
#[cfg(unix)]
#[test]
fn local_paths_validate_the_whole_bounded_batch_without_requiring_existing_files() {
    let valid = PathBuf::from("/not-opened/local-file");
    assert_eq!(
        validate_local_paths(std::slice::from_ref(&valid), 1),
        Ok(())
    );
    assert_eq!(
        validate_local_paths(&[], 1),
        Err(PickError::InvalidResponse)
    );
    for max_files in [0, 11, usize::MAX] {
        assert_eq!(
            validate_local_paths(std::slice::from_ref(&valid), max_files),
            Err(PickError::InvalidRequest)
        );
    }
    let exact: Vec<_> = (0..10)
        .map(|id| PathBuf::from(format!("/{id}{}", "x".repeat(4094))))
        .collect();
    assert_eq!(validate_local_paths(&exact, 10), Ok(()));
    let eleven: Vec<_> = (0..11)
        .map(|id| PathBuf::from(format!("/missing-{id}")))
        .collect();
    assert_eq!(validate_local_paths(&eleven, 10), Err(PickError::TooMany));
    assert_eq!(
        validate_local_paths(&[valid.clone(), valid], 2),
        Err(PickError::InvalidResponse)
    );
    for path in [
        PathBuf::from("relative"),
        PathBuf::from("/nul\0byte"),
        PathBuf::from(format!("/{}", "x".repeat(4096))),
    ] {
        assert_eq!(
            validate_local_paths(&[path], 1),
            Err(PickError::InvalidRequest)
        );
    }
}

#[test]
fn local_paths_invalid_second_path_is_rejected_before_opening_the_first_directory() {
    let fixture = Fixture::new();
    let directory = fixture.0.path().to_owned();
    let (_cancel, signal) = watch::channel(false);
    for invalid in [PathBuf::from("relative"), PathBuf::from("/nul\0byte")] {
        assert_eq!(
            open_local_files(&[directory.clone(), invalid], 2, 32, 64, &signal).unwrap_err(),
            PickError::InvalidRequest,
            "opening the first path would incorrectly report NotRegularFile"
        );
    }
    assert_eq!(
        open_local_files(&[directory.clone(), directory], 2, 32, 64, &signal).unwrap_err(),
        PickError::InvalidResponse
    );
    let eleven: Vec<_> = (0..11)
        .map(|id| fixture.0.path().join(format!("missing-{id}")))
        .collect();
    assert_eq!(
        open_local_files(&eleven, 10, 32, 256, &signal).unwrap_err(),
        PickError::TooMany
    );
}

#[cfg(target_os = "linux")]
#[test]
fn local_paths_return_ordered_read_only_unconsumed_descriptors_and_never_reopen_paths() {
    let fixture = Fixture::new();
    let first = fixture.file("first", b"one");
    let second = fixture.file("second", b"two");
    let link = fixture.0.path().join("selected-link");
    std::os::unix::fs::symlink(&first, &link).unwrap();
    let (_cancel, signal) = watch::channel(false);
    let selected = open_local_files(&[link.clone(), second.clone()], 2, 3, 6, &signal).unwrap();
    assert_eq!(selected[0].info().display_name, "selected-link");
    assert_eq!(selected[1].info().display_name, "second");
    std::fs::remove_file(&link).unwrap();
    std::os::unix::fs::symlink(&second, &link).unwrap();
    std::fs::rename(&second, fixture.0.path().join("original-second")).unwrap();
    std::fs::write(&second, b"replacement").unwrap();
    for (selected, expected) in selected.into_iter().zip([b"one", b"two"]) {
        let (mut file, metadata, info) = selected.into_parts();
        assert_eq!(metadata.len(), 3);
        assert_eq!(info.size_bytes, 3);
        assert_eq!(file.stream_position().unwrap(), 0);
        assert!(file.write_all(b"mutation").is_err());
        let mut body = Vec::new();
        file.read_to_end(&mut body).unwrap();
        assert_eq!(body, expected);
    }
}

#[cfg(target_os = "linux")]
#[test]
fn local_paths_enforce_individual_and_aggregate_budgets_and_preserve_zero_total_empty_files() {
    let fixture = Fixture::new();
    let first = fixture.file("first", b"1234");
    let second = fixture.file("second", b"5678");
    let empty = fixture.file("empty", b"");
    let (_cancel, signal) = watch::channel(false);
    assert_eq!(
        open_local_files(&[first.clone(), second.clone()], 2, 4, 7, &signal).unwrap_err(),
        PickError::TooLarge
    );
    assert_eq!(
        open_local_files(&[first.clone(), second.clone()], 2, 3, 8, &signal).unwrap_err(),
        PickError::TooLarge
    );
    assert_eq!(
        open_local_files(&[first, second], 2, 4, 8, &signal)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        open_local_files(std::slice::from_ref(&empty), 1, 1, 0, &signal).unwrap()[0]
            .info()
            .size_bytes,
        0
    );
    assert_eq!(
        open_local_files(&[empty], 1, 0, 0, &signal).unwrap_err(),
        PickError::InvalidRequest
    );
}

#[cfg(target_os = "linux")]
#[test]
fn local_paths_failure_of_a_later_file_never_publishes_a_partial_batch() {
    let fixture = Fixture::new();
    let valid = fixture.file("valid", b"fixture");
    let (_cancel, signal) = watch::channel(false);
    assert_eq!(
        open_local_files(
            &[valid.clone(), fixture.0.path().join("missing")],
            2,
            7,
            14,
            &signal
        )
        .unwrap_err(),
        PickError::Failed
    );
    assert_eq!(
        open_local_files(
            &[valid.clone(), fixture.0.path().to_owned()],
            2,
            7,
            14,
            &signal
        )
        .unwrap_err(),
        PickError::NotRegularFile
    );
    assert_eq!(
        open_local_files(&[valid], 1, 7, 7, &signal).unwrap().len(),
        1
    );
}

#[test]
fn local_paths_cancelled_or_dropped_owners_cannot_open_even_a_valid_directory() {
    let fixture = Fixture::new();
    let paths = [fixture.0.path().to_owned()];
    let (_cancel, signal) = watch::channel(true);
    assert_eq!(
        open_local_files(&paths, 1, 32, 32, &signal).unwrap_err(),
        PickError::Cancelled
    );
    let (cancel, signal) = watch::channel(false);
    drop(cancel);
    assert_eq!(
        open_local_files(&paths, 1, 32, 32, &signal).unwrap_err(),
        PickError::Cancelled
    );
}

#[cfg(target_os = "linux")]
#[test]
fn local_paths_fifo_rejection_cannot_wait_for_a_writer() {
    let fixture = Fixture::new();
    let fifo = fixture.0.path().join("fifo");
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &fifo,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    let (_cancel, signal) = watch::channel(false);
    let (reply, answer) = sync_channel(1);
    let worker = std::thread::spawn(move || {
        reply
            .send(open_local_files(&[fifo], 1, 32, 32, &signal).map(|files| files.len()))
            .unwrap();
    });
    assert_eq!(
        answer
            .recv_timeout(Duration::from_secs(2))
            .expect("a service worker cannot block on a FIFO"),
        Err(PickError::NotRegularFile)
    );
    worker.join().unwrap();
}
