use super::*;
use rune_model::UploadToken;
use std::io::Write;
use tokio::sync::watch;

mod batch;

fn fixture(body: &[u8]) -> (tempfile::TempDir, File, Metadata, PendingAttachment) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("chosen.bin");
    std::fs::write(&path, body).unwrap();
    let file = File::open(&path).unwrap();
    let selected = file.metadata().unwrap();
    let metadata = PendingAttachment {
        token: UploadToken(1),
        filename: "chosen.bin".into(),
        size_bytes: selected.len(),
    };
    (root, file, selected, metadata)
}

#[test]
fn memory_and_entry_credit_cannot_be_overcommitted() {
    let budget = Arc::new(Budget::default());
    let first = budget.reserve(MEMORY_BYTES).unwrap();
    assert!(matches!(budget.reserve(1), Err(UploadError::MemoryLimit)));
    drop(first);
    let leases: Vec<_> = (0..MAX_ENTRIES)
        .map(|_| budget.reserve(0).unwrap())
        .collect();
    assert!(matches!(budget.reserve(0), Err(UploadError::TooMany)));
    drop(leases);
    assert_eq!(budget.used(), (0, 0));
}

#[test]
fn clones_and_slices_keep_memory_credit_until_the_last_owner() {
    let budget = Arc::new(Budget::default());
    let bytes = seal(
        vec![7; 1024].into_boxed_slice(),
        budget.reserve(1024).unwrap(),
    );
    let clone = bytes.clone();
    let slice = bytes.slice(12..13);
    drop(bytes);
    drop(clone);
    assert_eq!(slice.as_ref(), &[7]);
    assert_eq!(budget.used(), (1024, 1));
    drop(slice);
    assert_eq!(budget.used(), (0, 0));
}

// Symbolic links as Unix makes them (windows-base).
#[cfg(unix)]
#[test]
fn reading_freezes_the_original_descriptor_without_reopening_its_path() {
    let (root, original, _, metadata) = fixture(b"original bytes");
    drop(original);
    std::os::unix::fs::symlink(
        root.path().join("chosen.bin"),
        root.path().join("alias.bin"),
    )
    .unwrap();
    let file = File::open(root.path().join("alias.bin")).unwrap();
    let selected = file.metadata().unwrap();
    std::fs::write(root.path().join("replacement.bin"), b"replacement").unwrap();
    std::fs::remove_file(root.path().join("alias.bin")).unwrap();
    std::os::unix::fs::symlink(
        root.path().join("replacement.bin"),
        root.path().join("alias.bin"),
    )
    .unwrap();
    let (cancel, signal) = watch::channel(false);
    let budget = Arc::new(Budget::default());
    let prepared = read_file(
        file,
        selected,
        metadata,
        budget.reserve(14).unwrap(),
        signal,
    )
    .unwrap();
    assert_eq!(prepared.bytes().as_ref(), b"original bytes");
    drop(cancel);
}

#[test]
fn modifying_a_selected_file_before_preparation_is_detected() {
    let (root, file, selected, metadata) = fixture(b"before");
    std::fs::OpenOptions::new()
        .append(true)
        .open(root.path().join("chosen.bin"))
        .unwrap()
        .write_all(b" after")
        .unwrap();
    let (_cancel, signal) = watch::channel(false);
    let budget = Arc::new(Budget::default());
    assert_eq!(
        read_file(file, selected, metadata, budget.reserve(6).unwrap(), signal).unwrap_err(),
        UploadError::Changed
    );
    assert_eq!(budget.used(), (0, 0));
}

#[test]
fn cancellation_never_returns_a_prepared_buffer() {
    let (_root, file, selected, metadata) = fixture(b"secret fixture");
    let (_cancel, signal) = watch::channel(true);
    let budget = Arc::new(Budget::default());
    assert_eq!(
        read_file(
            file,
            selected,
            metadata,
            budget.reserve(14).unwrap(),
            signal
        )
        .unwrap_err(),
        UploadError::Cancelled
    );
    assert_eq!(budget.used(), (0, 0));
}

// Windows opens no folder as a file (windows-base).
#[cfg(unix)]
#[test]
fn a_directory_cannot_be_read_as_an_attachment() {
    let root = tempfile::tempdir().unwrap();
    let file = File::open(root.path()).unwrap();
    let selected = file.metadata().unwrap();
    let metadata = PendingAttachment {
        token: UploadToken(1),
        filename: "directory".into(),
        size_bytes: selected.len(),
    };
    let (_cancel, signal) = watch::channel(false);
    let budget = Arc::new(Budget::default());
    assert_eq!(
        read_file(
            file,
            selected,
            metadata,
            budget.reserve(4096).unwrap(),
            signal
        )
        .unwrap_err(),
        UploadError::InvalidFile
    );
}

#[test]
fn metadata_is_revalidated_after_the_bounded_read_and_eof_check() {
    let (root, file, selected, metadata) = fixture(b"unchanged until EOF");
    let (_cancel, signal) = watch::channel(false);
    let budget = Arc::new(Budget::default());
    let size = metadata.size_bytes as usize;
    let result = read_checked(
        file,
        selected,
        metadata,
        budget.reserve(size).unwrap(),
        signal,
        || {
            std::fs::OpenOptions::new()
                .append(true)
                .open(root.path().join("chosen.bin"))
                .unwrap()
                .write_all(b"changed after EOF")
                .unwrap();
        },
    );
    assert_eq!(result.unwrap_err(), UploadError::Changed);
    assert_eq!(budget.used(), (0, 0));
}

#[cfg(unix)]
#[test]
fn a_same_size_change_with_restored_mtime_still_invalidates_the_selected_file() {
    use std::os::unix::fs::MetadataExt;
    let (root, file, selected, metadata) = fixture(b"original bytes");
    std::fs::write(root.path().join("chosen.bin"), b"tampered bytes").unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(selected.modified().unwrap()))
        .unwrap();
    let changed = file.metadata().unwrap();
    assert_eq!(selected.len(), changed.len());
    assert_eq!(selected.modified().unwrap(), changed.modified().unwrap());
    assert_ne!(
        (selected.ctime(), selected.ctime_nsec()),
        (changed.ctime(), changed.ctime_nsec()),
        "the fixture must exercise a real metadata change"
    );
    let (_cancel, signal) = watch::channel(false);
    let budget = Arc::new(Budget::default());
    assert_eq!(
        read_file(
            file,
            selected,
            metadata,
            budget.reserve(14).unwrap(),
            signal
        )
        .unwrap_err(),
        UploadError::Changed
    );
}

#[test]
fn a_pasted_picture_becomes_one_png_inside_the_budget_and_its_limit() {
    let budget = Arc::new(Budget::default());
    let (_cancel, signal) = watch::channel(false);
    let rgba = [255u8, 0, 0, 255].repeat(4);
    let png = encode_png(2, 2, &rgba, 1024).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let decoded = image::load_from_memory(&png).unwrap().to_rgba8();
    assert_eq!((decoded.width(), decoded.height()), (2, 2));
    assert_eq!(decoded.into_raw(), rgba, "lossless");
    let size = png.len();
    let prepared = sealed_png(png, UploadToken(5), &budget, &signal).unwrap();
    assert_eq!(prepared.len(), 1);
    assert_eq!(prepared[0].metadata().filename, PASTED_IMAGE);
    assert_eq!(prepared[0].metadata().token, UploadToken(5));
    assert_eq!(budget.used(), (size, 1), "held while prepared");
    drop(prepared);
    assert_eq!(budget.used(), (0, 0));
    // Over the account's limit, or as wide as no PNG can say: refused.
    assert_eq!(encode_png(2, 2, &rgba, 8), Err(UploadError::TooLarge));
    assert_eq!(
        encode_png(usize::MAX, 1, &rgba, 1024),
        Err(UploadError::TooLarge)
    );
    let (cancel, signal) = watch::channel(false);
    cancel.send_replace(true);
    let png = encode_png(2, 2, &rgba, 1024).unwrap();
    assert_eq!(
        sealed_png(png, UploadToken(5), &budget, &signal).map(|files| files.len()),
        Err(UploadError::Cancelled)
    );
    assert_eq!(budget.used(), (0, 0), "a cancelled one keeps nothing");
}
