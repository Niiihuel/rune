use super::*;

fn inputs(bodies: &[&[u8]], budget: &Arc<Budget>) -> (Vec<tempfile::TempDir>, Vec<Input>) {
    let leases = budget
        .reserve_many(&bodies.iter().map(|body| body.len()).collect::<Vec<_>>())
        .unwrap();
    let mut roots = Vec::new();
    let files = bodies
        .iter()
        .zip(leases)
        .enumerate()
        .map(|(index, (body, lease))| {
            let (root, file, selected, mut metadata) = fixture(body);
            metadata.token = UploadToken(100 + index as u64);
            roots.push(root);
            Input {
                file,
                selected,
                metadata,
                lease,
            }
        })
        .collect();
    (roots, files)
}

#[test]
fn batch_reservation_is_atomic_for_bytes_entries_and_arithmetic_overflow() {
    let budget = Arc::new(Budget::default());
    assert!(matches!(
        budget.reserve_many(&[MEMORY_BYTES, 1]),
        Err(UploadError::MemoryLimit)
    ));
    assert_eq!(budget.used(), (0, 0));
    assert!(matches!(
        budget.reserve_many(&[usize::MAX, 1]),
        Err(UploadError::MemoryLimit)
    ));
    assert_eq!(budget.used(), (0, 0));
    let held = budget.reserve_many(&[0; MAX_ENTRIES - 1]).unwrap();
    assert!(matches!(
        budget.reserve_many(&[0, 0]),
        Err(UploadError::TooMany)
    ));
    assert_eq!(budget.used(), (0, MAX_ENTRIES - 1));
    drop(held);
    let full = budget
        .reserve_many(&[MEMORY_BYTES / 2, MEMORY_BYTES / 2])
        .unwrap();
    assert_eq!(budget.used(), (MEMORY_BYTES, 2));
    drop(full);
    assert_eq!(budget.used(), (0, 0));
}

#[test]
fn changed_second_file_rolls_back_first_buffer_and_every_reserved_lease() {
    let budget = Arc::new(Budget::default());
    let (roots, files) = inputs(&[b"first file", b"second file"], &budget);
    let (_cancel, signal) = watch::channel(false);
    let mut reads = 0;
    let result = read_batch(files, signal, |index| {
        reads += 1;
        if index == 0 {
            std::fs::write(roots[1].path().join("chosen.bin"), b"changed second file").unwrap();
        }
    });
    assert_eq!(result.unwrap_err(), UploadError::Changed);
    assert_eq!(reads, 1);
    assert_eq!(budget.used(), (0, 0));
}

#[test]
fn cancellation_between_reads_or_after_the_second_file_never_publishes_a_subset() {
    for cancel_after in [0, 1] {
        let budget = Arc::new(Budget::default());
        let (_roots, files) = inputs(&[b"first", b"second"], &budget);
        let (cancel, signal) = watch::channel(false);
        let mut reads = 0;
        let result = read_batch(files, signal, |index| {
            reads += 1;
            if index == cancel_after {
                cancel.send_replace(true);
            }
        });
        assert_eq!(result.unwrap_err(), UploadError::Cancelled);
        assert_eq!(reads, cancel_after + 1);
        assert_eq!(budget.used(), (0, 0));
    }
}

#[test]
fn each_frozen_file_releases_its_own_credit_after_its_last_clone_or_slice() {
    let budget = Arc::new(Budget::default());
    let (roots, files) = inputs(&[b"first", b"second"], &budget);
    let (_cancel, signal) = watch::channel(false);
    let mut prepared = read_batch(files, signal, |_| {}).unwrap();
    assert_eq!(
        prepared
            .iter()
            .map(|file| file.metadata().token)
            .collect::<Vec<_>>(),
        vec![UploadToken(100), UploadToken(101)]
    );
    assert_eq!(budget.used(), (11, 2));
    let first = prepared.remove(0);
    let clone = first.bytes();
    let slice = clone.slice(1..3);
    std::fs::write(roots[0].path().join("chosen.bin"), b"new path content").unwrap();
    assert_eq!(clone.as_ref(), b"first");
    assert_eq!(prepared[0].bytes().as_ref(), b"second");
    drop(first);
    drop(clone);
    drop(prepared);
    assert_eq!(slice.as_ref(), b"ir");
    assert_eq!(budget.used(), (5, 1));
    drop(slice);
    assert_eq!(budget.used(), (0, 0));
}
