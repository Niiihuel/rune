use super::*;
use crate::file_dialogs::FileInfo;
#[cfg(unix)]
use crate::file_dialogs::{ParentWindow, PickId, PickRequest};
#[cfg(unix)]
use std::num::NonZeroU64;
use std::{
    future::Future,
    io::{Read, Seek, Write},
    sync::mpsc::{sync_channel, Receiver},
    task::Poll,
    time::Duration,
};
use tokio::sync::watch;

mod local_paths;
mod multiple;

// Preserve the original single-descriptor regressions while exercising the
// only production API, which now returns a bounded batch.
async fn open(
    worker: &mut FileWorker,
    path: PathBuf,
    max_bytes: u64,
    cancel: CancelSignal,
) -> Result<SelectedFile, PickError> {
    let mut files = worker
        .open(vec![path], 1, max_bytes, max_bytes, cancel)
        .await?;
    assert_eq!(files.len(), 1);
    Ok(files.pop().unwrap())
}

async fn open_uri(
    worker: &mut FileWorker,
    uri: &str,
    max_bytes: u64,
    cancel: CancelSignal,
) -> Result<SelectedFile, PickError> {
    let mut files = worker
        .open_uris(&[uri.to_owned()], 1, max_bytes, max_bytes, cancel)
        .await?;
    assert_eq!(files.len(), 1);
    Ok(files.pop().unwrap())
}

struct Fixture(tempfile::TempDir);

impl Fixture {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }

    fn file(&self, name: &str, body: &[u8]) -> PathBuf {
        let path = self.0.path().join(name);
        std::fs::write(&path, body).unwrap();
        path
    }
}

#[tokio::test]
async fn choosing_prepares_a_read_only_descriptor_without_consuming_the_file() {
    let fixture = Fixture::new();
    let path = fixture.file("elegido.txt", b"fixture");
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    let selected = open(&mut worker, path, 7, signal).await.unwrap();
    assert_eq!(
        selected.info(),
        &FileInfo {
            display_name: "elegido.txt".into(),
            size_bytes: 7
        }
    );
    let (mut file, metadata, _) = selected.into_parts();
    assert!(metadata.is_file());
    assert_eq!(
        file.stream_position().unwrap(),
        0,
        "selection cannot read the contents"
    );
    assert!(
        file.write_all(b"changed").is_err(),
        "the grant has no write access"
    );
}

#[tokio::test]
async fn a_grant_keeps_the_selected_file_when_its_path_is_replaced() {
    let fixture = Fixture::new();
    let path = fixture.file("seleccionado.txt", b"first");
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    let selected = open(&mut worker, path.clone(), 32, signal).await.unwrap();
    std::fs::rename(&path, fixture.0.path().join("original.txt")).unwrap();
    std::fs::write(&path, b"replacement").unwrap();
    let (mut file, _, _) = selected.into_parts();
    let mut body = String::new();
    file.read_to_string(&mut body).unwrap();
    assert_eq!(
        body, "first",
        "uploads must consume the descriptor instead of reopening a path"
    );
}

#[tokio::test]
async fn selection_enforces_the_exact_byte_limit_and_accepts_empty_regular_files() {
    let fixture = Fixture::new();
    let path = fixture.file("limite.txt", b"12345678");
    let empty = fixture.file("vacio", b"");
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    assert_eq!(
        open(&mut worker, path.clone(), 7, signal.clone())
            .await
            .unwrap_err(),
        PickError::TooLarge
    );
    assert_eq!(
        open(&mut worker, path, 8, signal.clone())
            .await
            .unwrap()
            .info()
            .size_bytes,
        8
    );
    assert_eq!(
        open(&mut worker, empty, 1, signal)
            .await
            .unwrap()
            .info()
            .size_bytes,
        0
    );
}

#[tokio::test]
async fn directories_and_devices_never_become_selected_files() {
    let fixture = Fixture::new();
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    #[cfg(unix)]
    let device = PathBuf::from("/dev/null");
    #[cfg(windows)]
    let device = PathBuf::from(r"\\.\NUL");
    for path in [fixture.0.path().to_owned(), device] {
        assert_eq!(
            open(&mut worker, path, u64::MAX, signal.clone())
                .await
                .unwrap_err(),
            PickError::NotRegularFile
        );
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn a_fifo_without_a_writer_is_rejected_without_waiting() {
    let fixture = Fixture::new();
    let path = fixture.0.path().join("fifo");
    rustix::fs::mkfifoat(
        rustix::fs::CWD,
        &path,
        rustix::fs::Mode::RUSR | rustix::fs::Mode::WUSR,
    )
    .unwrap();
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    let link = fixture.0.path().join("link-to-fifo");
    std::os::unix::fs::symlink(&path, &link).unwrap();
    for path in [path, link] {
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            open(&mut worker, path, 1024, signal.clone()),
        )
        .await;
        assert_eq!(
            result
                .expect("a FIFO cannot wait for a writer")
                .unwrap_err(),
            PickError::NotRegularFile
        );
    }
}

#[tokio::test]
async fn cancellation_or_a_closed_owner_prevents_file_preparation() {
    let fixture = Fixture::new();
    let path = fixture.file("privado", b"fixture");
    let mut worker = FileWorker::new().unwrap();
    let (_cancel, signal) = watch::channel(true);
    assert_eq!(
        open(&mut worker, path.clone(), 1024, signal)
            .await
            .unwrap_err(),
        PickError::Cancelled
    );
    let (cancel, signal) = watch::channel(false);
    drop(cancel);
    assert_eq!(
        open(&mut worker, path, 1024, signal).await.unwrap_err(),
        PickError::Cancelled
    );
}

// Unix paths and file names (windows-base).
#[cfg(unix)]
#[tokio::test]
async fn file_uris_decode_unicode_spaces_and_reserved_characters_without_exposing_the_path() {
    let fixture = Fixture::new();
    let path = fixture.file("único #1?.txt", b"fixture");
    let uri = url::Url::from_file_path(&path).unwrap().to_string();
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    let selected = open_uri(&mut worker, &uri, 7, signal).await.unwrap();
    assert_eq!(selected.info().display_name, "único #1?.txt");
    let debug = format!("{selected:?}");
    assert!(!debug.contains("único"));
    assert!(!debug.contains(&fixture.0.path().to_string_lossy().to_string()));
}

#[tokio::test]
async fn remote_or_ambiguous_uris_are_rejected_before_file_io() {
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    for uri in [
        "https://example.invalid/file",
        "file://example.invalid/etc/passwd",
        "file://user:secret@localhost/file",
        "file:///tmp/file?other",
        "file:///tmp/file#other",
        "file:///tmp/%00file",
        "file:relative",
        "file:/tmp/file",
        "file:///tmp/back\\slash",
        "file:///tmp/line\nbreak",
        "file:///tmp/tab\tname",
        "file:///tmp/other\u{7f}name",
        "file:///tmp/trailing ",
        "file:///tmp/%",
        "file:///tmp/%0",
        "file:///tmp/%GG",
        "file:///tmp/%0G",
        "not a URI",
    ] {
        assert_eq!(
            open_uri(&mut worker, uri, 1024, signal.clone())
                .await
                .unwrap_err(),
            PickError::InvalidResponse
        );
    }
    assert_eq!(
        open_uri(
            &mut worker,
            &format!("file:///{}", "x".repeat(4096)),
            1024,
            signal
        )
        .await
        .unwrap_err(),
        PickError::TooLarge
    );
}

// Unix paths and file names (windows-base).
#[cfg(unix)]
#[tokio::test]
async fn encoded_controls_backslashes_and_percent_are_valid_filename_bytes() {
    let fixture = Fixture::new();
    let path = fixture.file("back\\slash\n%.txt", b"fixture");
    let uri = url::Url::from_file_path(&path).unwrap().to_string();
    assert!(uri.contains("%5C") && uri.contains("%0A") && uri.contains("%25"));
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    let selected = open_uri(&mut worker, &uri, 1024, signal).await.unwrap();
    assert_eq!(selected.info().display_name, "back\\slash%.txt");
    let (mut file, _, _) = selected.into_parts();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"fixture");
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn dot_segments_cannot_normalize_a_symlink_selection_to_another_file() {
    let fixture = Fixture::new();
    let chosen = fixture.0.path().join("chosen");
    let actual = fixture.0.path().join("actual");
    std::fs::create_dir_all(&chosen).unwrap();
    std::fs::create_dir_all(actual.join("nested")).unwrap();
    std::fs::write(actual.join("target"), b"ORIGINAL_SELECTION").unwrap();
    std::fs::write(chosen.join("target"), b"NORMALIZED_DESTINATION").unwrap();
    std::os::unix::fs::symlink(actual.join("nested"), chosen.join("link")).unwrap();
    assert_eq!(
        std::fs::read(chosen.join("link/../target")).unwrap(),
        b"ORIGINAL_SELECTION"
    );
    let base = url::Url::from_directory_path(&chosen).unwrap().to_string();
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    for segment in ["..", "%2e%2E", ".%2e", "%2E.", ".", "%2e", "%2E"] {
        let uri = format!("{base}link/{segment}/target");
        assert_eq!(
            open_uri(&mut worker, &uri, 1024, signal.clone())
                .await
                .unwrap_err(),
            PickError::InvalidResponse,
            "noncanonical segment {segment} must not pick a normalized destination"
        );
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn literal_percent_dot_names_and_longer_dot_names_remain_selectable() {
    let fixture = Fixture::new();
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    for directory in ["%2e", "%2E%2e", "...", "foo.."] {
        let path = fixture.0.path().join(directory).join("target");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"literal filename").unwrap();
        let uri = url::Url::from_file_path(path).unwrap().to_string();
        let (mut file, _, _) = open_uri(&mut worker, &uri, 1024, signal.clone())
            .await
            .unwrap()
            .into_parts();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, b"literal filename");
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn file_uris_preserve_non_utf8_linux_filenames_in_the_descriptor() {
    use std::os::unix::ffi::OsStringExt;
    let fixture = Fixture::new();
    let path = fixture
        .0
        .path()
        .join(std::ffi::OsString::from_vec(b"raw-\xff.bin".to_vec()));
    std::fs::write(&path, b"non-utf8 fixture").unwrap();
    let uri = url::Url::from_file_path(&path).unwrap().to_string();
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    let selected = open_uri(&mut worker, &uri, 1024, signal).await.unwrap();
    assert_eq!(selected.info().display_name, "raw-\u{fffd}.bin");
    let (mut file, _, _) = selected.into_parts();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    assert_eq!(bytes, b"non-utf8 fixture");
}

// Unix paths and file names (windows-base).
#[cfg(unix)]
#[tokio::test]
async fn display_names_remove_controls_and_keep_a_bounded_unicode_prefix() {
    let fixture = Fixture::new();
    let path = fixture.file(&format!("\n{}\r", "é".repeat(100)), b"fixture");
    let (_cancel, signal) = watch::channel(false);
    let mut worker = FileWorker::new().unwrap();
    let selected = open(&mut worker, path, 1024, signal).await.unwrap();
    assert_eq!(selected.info().display_name, "é".repeat(90));
}

// Unix paths and file names (windows-base).
#[cfg(unix)]
#[test]
fn request_validation_keeps_folder_paths_absolute_bounded_and_out_of_debug_output() {
    let valid = PickRequest {
        id: PickId(1),
        max_files: 1,
        max_file_bytes: 1024,
        max_total_bytes: 1024,
        parent: Some(ParentWindow::x11(NonZeroU64::new(0x123).unwrap())),
        current_folder: Some(PathBuf::from("/tmp/private-fixture")),
    };
    assert!(valid.validate().is_ok());
    assert_eq!(valid.parent.as_ref().unwrap().as_str(), "x11:123");
    assert!(!format!("{valid:?}").contains("private-fixture"));
    for request in [
        PickRequest {
            id: PickId(0),
            ..valid.clone()
        },
        PickRequest {
            max_file_bytes: 0,
            ..valid.clone()
        },
        PickRequest {
            max_files: 0,
            ..valid.clone()
        },
        PickRequest {
            max_files: 11,
            ..valid.clone()
        },
        PickRequest {
            current_folder: Some(PathBuf::from("relative")),
            ..valid.clone()
        },
        PickRequest {
            current_folder: Some(PathBuf::from("/tmp/\0bad")),
            ..valid.clone()
        },
        PickRequest {
            current_folder: Some(PathBuf::from(format!("/{}", "x".repeat(4096)))),
            ..valid
        },
    ] {
        assert_eq!(request.validate(), Err(PickError::InvalidRequest));
    }
}

struct Pause(Option<SyncSender<()>>);

impl Pause {
    fn resume(mut self) {
        self.0.take().unwrap().send(()).unwrap();
    }
}

impl Drop for Pause {
    fn drop(&mut self) {
        if let Some(release) = self.0.take() {
            let _ = release.send(());
        }
    }
}

fn pause(jobs: &SyncSender<Job>) -> Pause {
    let (entered, waiting) = sync_channel(1);
    let (release, released) = sync_channel(1);
    jobs.send(Job::Pause {
        entered,
        release: released,
    })
    .map_err(|_| ())
    .unwrap();
    waiting.recv_timeout(Duration::from_secs(3)).unwrap();
    Pause(Some(release))
}

async fn poll_pending(future: std::pin::Pin<&mut impl Future>) {
    let mut future = future;
    std::future::poll_fn(|cx| {
        assert!(
            future.as_mut().poll(cx).is_pending(),
            "the real worker is paused"
        );
        Poll::Ready(())
    })
    .await;
}

#[tokio::test]
async fn a_cancelled_queued_open_is_drained_before_the_next_choice_uses_the_worker() {
    let fixture = Fixture::new();
    let first = fixture.file("primero", b"old");
    let second = fixture.file("segundo", b"new file");
    let mut worker = FileWorker::new().unwrap();
    let jobs = worker.jobs.clone();
    let paused = pause(&jobs);
    let (cancel, signal) = watch::channel(false);
    let mut opening = Box::pin(open(&mut worker, first, 1024, signal));
    poll_pending(opening.as_mut()).await;
    cancel.send(true).unwrap();
    assert_eq!(opening.await.unwrap_err(), PickError::Cancelled);
    let (_next_cancel, next_signal) = watch::channel(false);
    let mut next = Box::pin(open(&mut worker, second, 1024, next_signal));
    poll_pending(next.as_mut()).await;
    paused.resume();
    let selected = next.await.unwrap();
    assert_eq!(selected.info().display_name, "segundo");
    assert_eq!(selected.info().size_bytes, 8);
}

#[tokio::test]
async fn dropping_a_file_open_future_retains_its_single_slot_until_cleanup() {
    let fixture = Fixture::new();
    let path = fixture.file("seleccionado", b"fixture");
    let mut worker = FileWorker::new().unwrap();
    let jobs = worker.jobs.clone();
    let paused = pause(&jobs);
    let (_cancel, signal) = watch::channel(false);
    let mut opening = Box::pin(open(&mut worker, path.clone(), 7, signal.clone()));
    poll_pending(opening.as_mut()).await;
    drop(opening);
    let mut idle = Box::pin(worker.wait_idle(signal.clone()));
    poll_pending(idle.as_mut()).await;
    paused.resume();
    idle.await.unwrap();
    assert_eq!(
        open(&mut worker, path, 7, signal)
            .await
            .unwrap()
            .info()
            .size_bytes,
        7
    );
}

#[tokio::test]
async fn cancellation_wins_over_a_descriptor_which_completed_before_polling() {
    let fixture = Fixture::new();
    let path = fixture.file("seleccionado", b"fixture");
    let mut worker = FileWorker::new().unwrap();
    let jobs = worker.jobs.clone();
    let paused = pause(&jobs);
    let (cancel, signal) = watch::channel(false);
    let mut opening = Box::pin(open(&mut worker, path, 7, signal.clone()));
    poll_pending(opening.as_mut()).await;
    paused.resume();
    // This barrier enters only after the preceding descriptor result is sent.
    let completed = pause(&jobs);
    cancel.send(true).unwrap();
    assert_eq!(opening.await.unwrap_err(), PickError::Cancelled);
    completed.resume();
    let (_next_cancel, next_signal) = watch::channel(false);
    worker.wait_idle(next_signal).await.unwrap();
}

#[test]
fn dropping_the_service_never_joins_a_blocked_file_worker() {
    let worker = FileWorker::new().unwrap();
    let paused = pause(&worker.jobs);
    let (done, waiting): (_, Receiver<()>) = sync_channel(1);
    let dropped = std::thread::spawn(move || {
        drop(worker);
        done.send(()).unwrap();
    });
    let result = waiting.recv_timeout(Duration::from_secs(3));
    paused.resume();
    dropped.join().unwrap();
    result.expect("UI shutdown cannot wait for a filesystem syscall");
}
