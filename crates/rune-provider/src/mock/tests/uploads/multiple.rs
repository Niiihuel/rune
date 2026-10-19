use super::*;
use crate::UploadError;

fn second() -> PreparedUpload {
    PreparedUpload::new(
        PendingAttachment {
            token: UploadToken(2),
            filename: "second.txt".into(),
            size_bytes: 6,
        },
        Bytes::from_static(b"second"),
    )
    .unwrap()
}

#[tokio::test(start_paused = true)]
async fn uploads_multiple_distinct_files_are_one_frozen_message_across_retry() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let (_control, mut command) = files(channel, 900, 901, "", b"fixture");
    let Command::SendFiles { files, .. } = &mut command else {
        unreachable!()
    };
    files.push(second());
    session.send(command.clone()).await;
    let original = created(outcome(&mut session).await, 901);
    assert!(original.content.is_empty());
    assert_eq!(original.attachments.len(), 2);
    assert_eq!(original.attachments[0].filename, "fixture.txt");
    assert_eq!(original.attachments[1].filename, "second.txt");
    assert_eq!(original.attachments[0].size_bytes, 7);
    assert_eq!(original.attachments[1].size_bytes, 6);
    assert_ne!(original.attachments[0].id, original.attachments[1].id);
    assert!(original
        .attachments
        .iter()
        .all(|file| { file.id.0 != 0 && file.download.is_none() && file.preview.is_none() }));

    let Command::SendFiles { scope, .. } = &mut command else {
        unreachable!()
    };
    scope.request = 902;
    session.send(command.clone()).await;
    assert_eq!(created(outcome(&mut session).await, 901), original);

    for change in 0..3 {
        let mut changed = command.clone();
        let Command::SendFiles { scope, files, .. } = &mut changed else {
            unreachable!()
        };
        scope.request = 903 + change;
        match change {
            0 => files.reverse(),
            1 => {
                files.pop();
            }
            _ => {
                files[1] =
                    PreparedUpload::new(second().metadata().clone(), Bytes::from_static(b"alter!"))
                        .unwrap();
            }
        }
        session.send(changed).await;
        assert!(matches!(
            outcome(&mut session).await,
            Event::Failed {
                failure: Failure::SendFiles { request, nonce: 901, .. },
                ambiguous: false,
                ..
            } if request == 903 + change
        ));
    }
    assert_eq!(world.lock().unwrap().sims[&channel].extra.len(), 1);
    assert_eq!(
        world.lock().unwrap().message(channel, original.id),
        Some(original)
    );
}

#[tokio::test(start_paused = true)]
async fn uploads_eleven_distinct_files_are_rejected_atomically() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let (_control, mut command) = files(channel, 910, 911, "", b"fixture");
    let Command::SendFiles { files, .. } = &mut command else {
        unreachable!()
    };
    *files = (1..=11)
        .map(|token| {
            PreparedUpload::new(
                PendingAttachment {
                    token: UploadToken(token),
                    filename: format!("file-{token}.txt"),
                    size_bytes: 1,
                },
                Bytes::from_static(b"x"),
            )
            .unwrap()
        })
        .collect();
    session.send(command).await;
    assert!(matches!(
        outcome(&mut session).await,
        Event::Failed {
            failure: Failure::SendFiles { request: 910, nonce: 911, .. },
            reason,
            ambiguous: false,
        } if reason == UploadError::TooMany.label()
    ));
    assert!(world.lock().unwrap().sims[&channel].extra.is_empty());
}

#[tokio::test(start_paused = true)]
async fn uploads_duplicate_tokens_never_become_two_remote_attachments() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let (_control, mut command) = files(channel, 920, 921, "", b"fixture");
    let Command::SendFiles { files, .. } = &mut command else {
        unreachable!()
    };
    files.push(files[0].clone());
    session.send(command).await;
    let result = outcome(&mut session).await;
    assert!(
        matches!(
            result,
            Event::Failed {
                failure: Failure::SendFiles {
                    request: 920,
                    nonce: 921,
                    ..
                },
                ambiguous: false,
                ..
            }
        ),
        "one prepared grant cannot become two remote files: {result:?}"
    );
    assert!(world.lock().unwrap().sims[&channel].extra.is_empty());
}

#[tokio::test(start_paused = true)]
async fn uploads_ten_distinct_files_fit_the_declared_policy_without_accepting_an_eleventh() {
    let provider = MockProvider::new(config());
    let world = provider.world.clone();
    let mut session = Session::start(provider).await;
    let channel = session.channel("general");
    let (_control, mut command) = files(channel, 930, 931, "", b"fixture");
    let Command::SendFiles { files, .. } = &mut command else {
        unreachable!()
    };
    *files = (1..=10)
        .map(|token| {
            PreparedUpload::new(
                PendingAttachment {
                    token: UploadToken(token),
                    filename: format!("file-{token}.txt"),
                    size_bytes: 1,
                },
                Bytes::from_static(b"x"),
            )
            .unwrap()
        })
        .collect();
    session.send(command.clone()).await;
    let message = created(outcome(&mut session).await, 931);
    assert_eq!(message.attachments.len(), 10);
    assert_eq!(
        message
            .attachments
            .iter()
            .map(|attachment| attachment.filename.as_str())
            .collect::<Vec<_>>(),
        (1..=10)
            .map(|token| format!("file-{token}.txt"))
            .collect::<Vec<_>>()
    );
    let Command::SendFiles { scope, files, .. } = &mut command else {
        unreachable!()
    };
    scope.request = 932;
    scope.nonce = 933;
    files.push(
        PreparedUpload::new(
            PendingAttachment {
                token: UploadToken(11),
                filename: "file-11.txt".into(),
                size_bytes: 1,
            },
            Bytes::from_static(b"x"),
        )
        .unwrap(),
    );
    session.send(command).await;
    assert!(matches!(
        outcome(&mut session).await,
        Event::Failed {
            failure: Failure::SendFiles {
                request: 932,
                nonce: 933,
                ..
            },
            ambiguous: false,
            ..
        }
    ));
    assert_eq!(world.lock().unwrap().sims[&channel].extra.len(), 1);
}
