use super::*;
use rune_model::{Attachment, AttachmentId};

fn confirmed_file(account: UserId, channel: ChannelId, nonce: u64) -> Event {
    let mut event = ack(account, channel, nonce);
    let Event::MessageCreated { message, .. } = &mut event else {
        unreachable!()
    };
    message.attachments = vec![Attachment {
        id: AttachmentId(901),
        filename: "fixture.txt".into(),
        size_bytes: 4,
        content_type: None,
        dimensions: None,
        preview: None,
        download: None,
        spoiler: false,
        voice: None,
        explicit: false,
        placeholder: None,
    }];
    event
}

#[test]
fn file_acknowledgements_require_the_frozen_attachment_metadata_before_consuming_grants() {
    for mismatch in 0..4 {
        let mut h = Harness::new(8);
        h.insert(UploadToken(10), b"file");
        h.send(11, 21, UploadToken(10));
        let _command = h.command();
        let mut event = confirmed_file(h.account, h.channel, 21);
        let Event::MessageCreated { message, .. } = &mut event else {
            unreachable!()
        };
        match mismatch {
            0 => message.attachments.clear(),
            1 => message.attachments[0].filename = String::new(),
            2 => message.attachments[0].size_bytes = 5,
            _ => message.attachments.push(message.attachments[0].clone()),
        }
        assert!(
            !h.uploads.observe(1, &event),
            "an incomplete file acknowledgement must not pass to Store: {mismatch}"
        );
        {
            let state = h.uploads.shared.lock().unwrap();
            assert!(state.tokens.contains_key(&UploadToken(10)));
            assert!(matches!(
                state.sends[&21].phase,
                Phase::Failed { ambiguous: true }
            ));
        }
        let mut failures = VecDeque::new();
        h.uploads.failures(&mut failures);
        assert!(matches!(
            failures.pop_front(),
            Some(Event::Failed {
                failure: Failure::SendFiles {
                    request: 11,
                    channel,
                    nonce: 21,
                },
                ambiguous: true,
                ..
            }) if channel == h.channel
        ));
        h.uploads.retry(12, 21, Duration::from_secs(30)).unwrap();
        let Command::SendFiles { scope, files, .. } = h.command() else {
            panic!("retry must retain the file payload")
        };
        assert_eq!((scope.request, scope.nonce), (12, 21));
        assert_eq!(files[0].bytes().as_ref(), b"file");
    }
}

#[test]
fn a_name_the_service_rewrote_still_confirms_the_same_bytes() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    h.send(11, 21, UploadToken(10));
    let _command = h.command();
    let mut event = confirmed_file(h.account, h.channel, 21);
    let Event::MessageCreated { message, .. } = &mut event else {
        unreachable!()
    };
    message.attachments[0].filename = "fixture_renamed.txt".into();
    assert!(h.uploads.observe(1, &event));
    let state = h.uploads.shared.lock().unwrap();
    assert!(!state.tokens.contains_key(&UploadToken(10)) && state.sends.is_empty());
}

#[test]
fn complete_remote_file_acknowledgement_consumes_only_its_original_grants() {
    let mut h = Harness::new(8);
    h.insert(UploadToken(10), b"file");
    h.insert(UploadToken(20), b"new draft");
    h.send(11, 21, UploadToken(10));
    let _command = h.command();
    assert!(h
        .uploads
        .observe(1, &confirmed_file(h.account, h.channel, 21)));
    let state = h.uploads.shared.lock().unwrap();
    assert!(!state.tokens.contains_key(&UploadToken(10)));
    assert!(state.tokens.contains_key(&UploadToken(20)));
    assert!(!state.sends.contains_key(&21));
}

#[test]
fn file_acknowledgements_require_real_remote_attachment_identity_and_delivery() {
    for invalid in 0..3 {
        let mut h = Harness::new(8);
        h.insert(UploadToken(10), b"file");
        h.send(11, 21, UploadToken(10));
        let _command = h.command();
        let mut event = confirmed_file(h.account, h.channel, 21);
        let Event::MessageCreated { message, .. } = &mut event else {
            unreachable!()
        };
        match invalid {
            0 => message.attachments[0].id = AttachmentId(0),
            1 => message.delivery = Delivery::Sending,
            _ => message.kind = MessageKind::Join,
        }
        assert!(
            !h.uploads.observe(1, &event),
            "a fabricated or local attachment result is not confirmation: {invalid}"
        );
        let state = h.uploads.shared.lock().unwrap();
        assert!(state.tokens.contains_key(&UploadToken(10)));
        assert!(matches!(
            state.sends[&21].phase,
            Phase::Failed { ambiguous: true }
        ));
    }
}

#[test]
fn multiple_file_acknowledgements_require_distinct_remote_attachment_ids() {
    for duplicate in [true, false] {
        let mut h = Harness::new(8);
        h.uploads.limits.as_mut().unwrap().max_files = 2;
        h.insert(UploadToken(10), b"file");
        h.insert(UploadToken(11), b"book");
        h.insert(UploadToken(20), b"new draft");
        h.uploads
            .send(
                11,
                h.channel,
                String::new(),
                None,
                false,
                vec![UploadToken(10), UploadToken(11)],
                21,
                Duration::from_secs(30),
            )
            .unwrap();
        let _command = h.command();
        let mut event = confirmed_file(h.account, h.channel, 21);
        let Event::MessageCreated { message, .. } = &mut event else {
            unreachable!()
        };
        let mut second = message.attachments[0].clone();
        if !duplicate {
            second.id = AttachmentId(902);
        }
        message.attachments.push(second);
        assert_eq!(
            h.uploads.observe(1, &event),
            !duplicate,
            "two frozen files need distinct remote identities"
        );
        {
            let state = h.uploads.shared.lock().unwrap();
            for token in [UploadToken(10), UploadToken(11)] {
                assert_eq!(state.tokens.contains_key(&token), duplicate);
            }
            assert!(state.tokens.contains_key(&UploadToken(20)));
            assert_eq!(state.sends.contains_key(&21), duplicate);
        }
        if duplicate {
            h.uploads.retry(12, 21, Duration::from_secs(30)).unwrap();
            let Command::SendFiles { scope, files, .. } = h.command() else {
                panic!("an invalid multiple-file ACK must retain the entire retry")
            };
            assert_eq!((scope.request, scope.nonce), (12, 21));
            assert_eq!(files.len(), 2);
            assert_eq!(files[0].bytes().as_ref(), b"file");
            assert_eq!(files[1].bytes().as_ref(), b"book");
        }
    }
}
