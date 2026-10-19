use super::*;

struct Released(sync::Sender<()>);

impl Drop for Released {
    fn drop(&mut self) {
        let _ = self.0.send(());
    }
}

struct ObservedOwner {
    // Fields drop in this order: observe only after the actual buffer lease.
    bytes: bytes::Bytes,
    _released: Released,
}

impl AsRef<[u8]> for ObservedOwner {
    fn as_ref(&self) -> &[u8] {
        self.bytes.as_ref()
    }
}

#[test]
fn completed_cancelled_preparation_releases_memory_without_another_pick() {
    let h = Harness::new(8);
    let (finished, answer) = tokio::sync::oneshot::channel();
    let (installed, ready) = sync::channel();
    h.uploads
        .jobs
        .try_send(Job::RetainCancelledRead { answer, installed })
        .ok()
        .unwrap();
    ready.recv_timeout(Duration::from_secs(3)).unwrap();
    let budget = Arc::new(prepare::Budget::default());
    let (released, dropped) = sync::channel();
    let bytes = bytes::Bytes::from_owner(ObservedOwner {
        bytes: prepare::seal(
            vec![7; 4096].into_boxed_slice(),
            budget.reserve(4096).unwrap(),
        ),
        _released: Released(released),
    });
    let prepared = PreparedUpload::new(
        PendingAttachment {
            token: UploadToken(90),
            filename: "fixture.txt".into(),
            size_bytes: 4096,
        },
        bytes,
    )
    .unwrap();
    assert_eq!(budget.used(), (4096, 1));
    // Fulfil the same retained one-shot that the fixed worker completes after
    // the original request's cancellation. No new dialog/job/poll is submitted.
    finished
        .send((Ok(vec![prepared]), Vec::new()))
        .ok()
        .unwrap();
    dropped
        .recv_timeout(Duration::from_secs(3))
        .expect("idle actor must drain the completed cancelled read");
    assert_eq!(budget.used(), (0, 0));
    assert!(h.commands.is_empty());
}
