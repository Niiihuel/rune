use super::*;
use rune_model::UploadToken;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn metadata() -> PendingAttachment {
    PendingAttachment {
        token: UploadToken(1),
        filename: "fixture.txt".into(),
        size_bytes: 7,
    }
}

#[test]
fn prepared_upload_rejects_mismatched_or_unsafe_metadata() {
    for invalid in [
        PendingAttachment {
            token: UploadToken(0),
            ..metadata()
        },
        PendingAttachment {
            size_bytes: 8,
            ..metadata()
        },
        PendingAttachment {
            filename: String::new(),
            ..metadata()
        },
        PendingAttachment {
            filename: "secret\r\nheader".into(),
            ..metadata()
        },
        PendingAttachment {
            filename: "folder/file".into(),
            ..metadata()
        },
        PendingAttachment {
            filename: "é".repeat(91),
            ..metadata()
        },
    ] {
        assert_eq!(
            PreparedUpload::new(invalid, Bytes::from_static(b"fixture")).unwrap_err(),
            UploadError::InvalidFile
        );
    }
}

#[tokio::test]
async fn upload_fence_cancels_all_command_clones_when_the_control_is_set_or_dropped() {
    let (control, fence) = UploadFence::new();
    let clone = fence.clone();
    assert!(!fence.is_cancelled());
    assert_eq!(fence, clone);
    control.send_replace(true);
    assert!(fence.is_cancelled());
    assert!(clone.is_cancelled());
    clone.cancelled().await;

    let (control, fence) = UploadFence::new();
    drop(control);
    assert!(
        fence.is_cancelled(),
        "a stale queued command loses its only control owner"
    );
    fence.cancelled().await;
}

struct OwnedBytes {
    data: Vec<u8>,
    drops: Arc<AtomicUsize>,
}
impl AsRef<[u8]> for OwnedBytes {
    fn as_ref(&self) -> &[u8] {
        &self.data
    }
}
impl Drop for OwnedBytes {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn prepared_command_clones_and_transport_slices_keep_the_original_service_owner() {
    let drops = Arc::new(AtomicUsize::new(0));
    let body = Bytes::from_owner(OwnedBytes {
        data: b"fixture".to_vec(),
        drops: drops.clone(),
    });
    let original = PreparedUpload::new(metadata(), body).unwrap();
    let pointer = original.bytes().as_ptr();
    let clone = original.clone();
    let slice = clone.bytes().slice(1..4);
    assert_eq!(clone.bytes().as_ptr(), pointer);
    assert_eq!(slice.as_ref(), b"ixt");
    assert!(!format!("{clone:?}").contains("fixture"));
    drop(original);
    drop(clone);
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(slice);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
}
