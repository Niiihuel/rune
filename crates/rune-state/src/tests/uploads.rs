use super::*;
use rune_model::{PendingAttachment, UploadToken};
use rune_provider::{Capabilities, ProviderInfo, ProviderKind, UploadSendScope};

fn file() -> PendingAttachment {
    PendingAttachment {
        token: UploadToken(7),
        filename: "fixture.txt".into(),
        size_bytes: 12,
    }
}

fn populated() -> Store {
    let mut store = Store::new();
    store.provider = Some(ProviderInfo {
        name: "fixture".into(),
        kind: ProviderKind::Simulated,
        capabilities: Capabilities {
            attachments: true,
            send_messages: true,
            ..Default::default()
        },
        notice: String::new(),
    });
    store.apply(Event::Ready(Box::new(snapshot())), Instant::now());
    store.connection = ConnectionState::Connected;
    store.add_pending_attachments(
        GENERAL,
        String::new(),
        None,
        4,
        40,
        vec![file()],
        Timestamp(10),
    );
    store
}

fn fail(store: &mut Store, request: u64) -> Applied {
    store.apply(
        Event::Failed {
            failure: Failure::SendFiles {
                request,
                channel: GENERAL,
                nonce: 4,
            },
            reason: "fixture".into(),
            ambiguous: false,
        },
        Instant::now(),
    )
}

#[test]
fn outgoing_metadata_is_retained_without_inventing_a_remote_attachment() {
    let store = populated();
    let shown = store.timeline(GENERAL).unwrap().get(local_id(4)).unwrap();
    assert!(shown.attachments.is_empty());
    assert_eq!(store.pending_attachments(GENERAL, shown.id), &[file()]);
}

#[test]
fn a_foreign_author_or_channel_ack_cannot_consume_the_pending_send() {
    for (author, channel) in [(ADA, GENERAL), (ME, ChannelId(999))] {
        let mut store = populated();
        let mut ack = message(300, author);
        ack.channel = channel;
        store.apply(
            Event::MessageCreated {
                message: ack,
                nonce: Some(4),
            },
            Instant::now(),
        );
        assert!(
            store.pending.contains_key(&4),
            "scope mismatch must not consume the nonce"
        );
        assert!(store.timeline(GENERAL).unwrap().get(local_id(4)).is_some());
    }
}

#[test]
fn a_retry_keeps_files_and_rejects_an_old_attempt_failure() {
    let mut store = populated();
    fail(&mut store, 40);
    let unsent = store.unsent(GENERAL, local_id(4)).unwrap();
    assert_eq!(unsent.files, vec![file()]);
    assert_eq!(unsent.upload_request, Some(40));
    store.resend_attachments(GENERAL, local_id(4), 41);
    assert_eq!(fail(&mut store, 40), Applied::Nothing);
    assert_eq!(
        store
            .timeline(GENERAL)
            .unwrap()
            .get(local_id(4))
            .unwrap()
            .delivery,
        Delivery::Sending
    );
    fail(&mut store, 41);
    assert_eq!(
        store.unsent(GENERAL, local_id(4)).unwrap().upload_request,
        Some(41)
    );
}

#[test]
fn a_generic_text_retry_cannot_strip_files_from_a_failed_send() {
    let mut store = populated();
    fail(&mut store, 40);
    store.resend(GENERAL, local_id(4), 5);
    assert!(store.unsent(GENERAL, local_id(4)).is_some());
    assert!(!store.pending.contains_key(&5));
}

#[test]
fn body_progress_is_scoped_monotonic_and_never_confirms_the_message() {
    let mut store = populated();
    let scope = UploadSendScope {
        request: 40,
        account: ME,
        channel: GENERAL,
        generation: 1,
        nonce: 4,
        expires_at: Instant::now() + Duration::from_secs(5),
    };
    for sent in [5, 3, 12] {
        store.apply(
            Event::UploadProgress {
                scope,
                sent_bytes: sent,
                total_bytes: 12,
            },
            Instant::now(),
        );
    }
    assert_eq!(
        store.attachment_progress(GENERAL, local_id(4)),
        Some((12, 12))
    );
    assert_eq!(
        store
            .timeline(GENERAL)
            .unwrap()
            .get(local_id(4))
            .unwrap()
            .delivery,
        Delivery::Sending
    );
    store.apply(
        Event::UploadProgress {
            scope: UploadSendScope {
                request: 39,
                ..scope
            },
            sent_bytes: 1,
            total_bytes: 12,
        },
        Instant::now(),
    );
    assert_eq!(
        store.attachment_progress(GENERAL, local_id(4)),
        Some((12, 12))
    );
}

#[test]
fn attaching_requires_a_current_connected_identity_and_all_permissions() {
    let mut store = populated();
    assert!(store.abilities(GENERAL).unwrap().attach.allowed());
    store.connection = ConnectionState::Connecting;
    assert!(!store.abilities(GENERAL).unwrap().attach.allowed());
    store.connection = ConnectionState::Connected;
    store.channels.get_mut(&GENERAL).unwrap().permissions = Permissions::ATTACH;
    assert!(!store.abilities(GENERAL).unwrap().attach.allowed());
}

#[test]
fn an_ack_with_missing_or_changed_file_metadata_is_only_an_uncorrelated_message() {
    use rune_model::{Attachment, AttachmentId};
    for changed in [None, Some(("another.txt", 12)), Some(("fixture.txt", 13))] {
        let mut store = populated();
        store.timelines.get_mut(&GENERAL).unwrap().latest = true;
        store.apply(
            Event::Failed {
                failure: Failure::SendFiles {
                    request: 40,
                    channel: GENERAL,
                    nonce: 4,
                },
                reason: "uncertain".into(),
                ambiguous: true,
            },
            Instant::now(),
        );
        let mut ack = message(400, ME);
        ack.attachments = changed
            .into_iter()
            .map(|(filename, size_bytes)| Attachment {
                id: AttachmentId(7),
                filename: filename.into(),
                size_bytes,
                content_type: None,
                dimensions: None,
                preview: None,
                download: None,
                spoiler: false,
                voice: None,
                explicit: false,
                placeholder: None,
            })
            .collect();
        store.apply(
            Event::MessageCreated {
                message: ack,
                nonce: Some(4),
            },
            Instant::now(),
        );
        assert!(
            store
                .timeline(GENERAL)
                .unwrap()
                .get(MessageId(400))
                .is_some(),
            "a valid incoming message remains visible"
        );
        assert!(
            store.pending.contains_key(&4),
            "a nonce is insufficient without matching file metadata"
        );
        assert!(store.unsent(GENERAL, local_id(4)).unwrap().ambiguous);
    }
}

#[test]
fn revoking_text_sending_also_revokes_attachment_selection_and_retention() {
    let mut store = populated();
    store.provider.as_mut().unwrap().capabilities.send_messages = false;
    assert!(!store.abilities(GENERAL).unwrap().attach.allowed());
    assert!(!store.can_keep_attachments(GENERAL));
}

fn matching_ack() -> Message {
    let mut ack = message(400, ME);
    ack.attachments = vec![rune_model::Attachment {
        id: rune_model::AttachmentId(7),
        filename: file().filename,
        size_bytes: 12,
        content_type: None,
        dimensions: None,
        preview: None,
        download: None,
        spoiler: false,
        voice: None,
        explicit: false,
        placeholder: None,
    }];
    ack
}

#[test]
fn a_file_ack_requires_nonzero_remote_identifiers() {
    for message_id_zero in [true, false] {
        let mut store = populated();
        let mut ack = matching_ack();
        if message_id_zero {
            ack.id = MessageId(0);
        } else {
            ack.attachments[0].id = rune_model::AttachmentId(0);
        }
        store.apply(
            Event::MessageCreated {
                message: ack,
                nonce: Some(4),
            },
            Instant::now(),
        );
        assert!(
            store.pending.contains_key(&4),
            "placeholder identifiers cannot confirm an upload"
        );
    }
}

#[test]
fn a_join_event_or_local_echo_cannot_confirm_a_file_message() {
    for is_join in [true, false] {
        let mut store = populated();
        let mut ack = matching_ack();
        if is_join {
            ack.kind = MessageKind::Join;
        } else {
            ack.delivery = Delivery::Sending;
        }
        store.apply(
            Event::MessageCreated {
                message: ack,
                nonce: Some(4),
            },
            Instant::now(),
        );
        assert!(store.pending.contains_key(&4));
    }
}

#[test]
fn a_valid_late_ack_resolves_the_unknown_send_without_duplicating_it() {
    let mut store = populated();
    store.timelines.get_mut(&GENERAL).unwrap().latest = true;
    store.apply(
        Event::Failed {
            failure: Failure::SendFiles {
                request: 40,
                channel: GENERAL,
                nonce: 4,
            },
            reason: "cancelled after sending".into(),
            ambiguous: true,
        },
        Instant::now(),
    );
    store.apply(
        Event::MessageCreated {
            message: matching_ack(),
            nonce: Some(4),
        },
        Instant::now(),
    );
    assert!(!store.pending.contains_key(&4));
    assert!(store.timeline(GENERAL).unwrap().get(local_id(4)).is_none());
    assert!(store
        .timeline(GENERAL)
        .unwrap()
        .get(MessageId(400))
        .is_some());
}

#[test]
fn certainty_can_refine_the_same_attempt_but_cannot_regress_after_a_definite_failure() {
    let mut store = populated();
    for ambiguous in [true, false, true] {
        store.apply(
            Event::Failed {
                failure: Failure::SendFiles {
                    request: 40,
                    channel: GENERAL,
                    nonce: 4,
                },
                reason: "fixture certainty".into(),
                ambiguous,
            },
            Instant::now(),
        );
    }
    let unsent = store.unsent(GENERAL, local_id(4)).unwrap();
    assert!(
        !unsent.ambiguous,
        "a definitive result must not become uncertain again"
    );
    assert_eq!(unsent.files, vec![file()]);
    assert_eq!(unsent.upload_request, Some(40));
}

#[test]
fn removing_a_channel_or_server_releases_its_unreachable_outgoing_metadata() {
    for event in [
        Event::ChannelRemoved { channel: GENERAL },
        Event::ServerRemoved {
            server: ServerId(1),
            reason: None,
        },
    ] {
        let mut store = populated();
        let mut other = store.channels[&GENERAL].clone();
        other.id = ChannelId(999);
        other.server = None;
        other.kind = ChannelKind::Direct;
        store.channels.insert(other.id, other);
        let mut keep = file();
        keep.token = UploadToken(8);
        store.add_pending_attachments(
            ChannelId(999),
            "keep".into(),
            None,
            8,
            80,
            vec![keep.clone()],
            Timestamp(10),
        );
        store.apply(event, Instant::now());
        assert!(store.timeline(GENERAL).is_none());
        assert!(
            !store.pending.contains_key(&4),
            "a removed timeline cannot own an unreachable pending send"
        );
        assert!(
            !store.pending_files.contains_key(&4),
            "removed file metadata must leave with its channel"
        );
        assert_eq!(
            store.pending_attachments(ChannelId(999), local_id(8)),
            &[keep]
        );
        let mut restored = snapshot();
        restored
            .channels
            .push(store.channels[&ChannelId(999)].clone());
        store.apply(Event::Ready(Box::new(restored)), Instant::now());
        assert!(
            !store.pending.contains_key(&4),
            "a later same-account Ready cannot resurrect the removed attempt"
        );
        assert!(!store.pending_files.contains_key(&4));
    }
}

fn multi_file_ack() -> (Store, Message) {
    let mut store = populated();
    let mut second = file();
    second.token = UploadToken(8);
    second.filename = "second.txt".into();
    store.add_pending_attachments(
        GENERAL,
        String::new(),
        None,
        4,
        40,
        vec![file(), second.clone()],
        Timestamp(10),
    );
    store.timelines.get_mut(&GENERAL).unwrap().latest = true;
    let mut ack = matching_ack();
    let mut remote = ack.attachments[0].clone();
    remote.id = rune_model::AttachmentId(8);
    remote.filename = second.filename;
    ack.attachments.push(remote);
    (store, ack)
}

#[test]
fn duplicate_remote_attachment_ids_cannot_confirm_a_multiple_file_message() {
    let (mut store, mut ack) = multi_file_ack();
    ack.attachments[1].id = ack.attachments[0].id;
    store.apply(
        Event::MessageCreated {
            message: ack,
            nonce: Some(4),
        },
        Instant::now(),
    );
    assert!(
        store.pending.contains_key(&4),
        "two outgoing files require two distinct remote attachment IDs"
    );
    assert_eq!(store.pending_attachments(GENERAL, local_id(4)).len(), 2);
    assert!(store
        .timeline(GENERAL)
        .unwrap()
        .get(MessageId(400))
        .is_some());
}

#[test]
fn distinct_remote_attachment_ids_confirm_a_multiple_file_message() {
    let (mut store, ack) = multi_file_ack();
    store.apply(
        Event::MessageCreated {
            message: ack,
            nonce: Some(4),
        },
        Instant::now(),
    );
    assert!(!store.pending.contains_key(&4));
    assert!(store.pending_attachments(GENERAL, local_id(4)).is_empty());
}
