use super::*;

#[test]
fn an_exact_definitive_failure_refines_unknown_after_cancel_or_timeout() {
    for timeout in [false, true] {
        let mut h = Harness::new(8);
        h.uploads.idempotent = false;
        h.insert(UploadToken(10), b"file");
        h.send(11, 21, UploadToken(10));
        let _command = h.command();
        if timeout {
            h.uploads.expire(h.uploads.deadline().unwrap());
        } else {
            h.uploads.cancel(11);
        }
        assert_eq!(
            h.uploads.retry(12, 21, Duration::from_secs(30)),
            Err(UploadError::Unavailable)
        );
        let definitive = Event::Failed {
            failure: Failure::SendFiles {
                request: 11,
                channel: h.channel,
                nonce: 21,
            },
            reason: "definitively rejected by the provider".into(),
            ambiguous: false,
        };
        assert!(
            h.uploads.observe(1, &definitive),
            "the same admitted attempt can refine unknown to not sent"
        );
        h.uploads.retry(12, 21, Duration::from_secs(30)).unwrap();
        let Command::SendFiles { scope, files, .. } = h.command() else {
            panic!()
        };
        assert_eq!((scope.request, scope.nonce), (12, 21));
        assert_eq!(files[0].bytes().as_ref(), b"file");
        assert!(
            !h.uploads.observe(1, &definitive),
            "a previous request cannot overwrite the retry"
        );
        assert!(h.uploads.observe(
            1,
            &Event::Failed {
                failure: Failure::SendFiles {
                    request: 12,
                    channel: h.channel,
                    nonce: 21
                },
                reason: "current definitive failure".into(),
                ambiguous: false
            }
        ));
        assert!(
            !h.uploads.observe(
                1,
                &Event::Failed {
                    failure: Failure::SendFiles {
                        request: 12,
                        channel: h.channel,
                        nonce: 21
                    },
                    reason: "less certain delayed failure".into(),
                    ambiguous: true
                }
            ),
            "a later unknown outcome cannot weaken a definitive failure"
        );
        assert!(matches!(
            h.uploads.shared.lock().unwrap().sends[&21].phase,
            Phase::Failed { ambiguous: false }
        ));
    }
}
