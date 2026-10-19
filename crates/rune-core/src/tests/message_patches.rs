//! Reliable partial updates keep their order all the way to reopened SQLite.

use super::*;
use rune_model::MessagePatch;

#[test]
fn partial_message_changes_reach_sqlite_without_reconstructing_unknown_messages() {
    for max in [1, 64] {
        let directory = std::env::temp_dir().join(format!(
            "rune-core-partial-message-{}-{max}",
            std::process::id()
        ));
        let path = directory.join("rune.db");
        let provider = mock();
        let mut client = client(Arc::new(mock()), Some(path.clone()));
        let channel = ready(&mut client)
            .iter()
            .find(|channel| channel.name == "general")
            .unwrap()
            .id;
        let messages: Vec<_> = provider
            .history(channel, None, PAGE)
            .0
            .into_iter()
            .filter(|message| message.kind == rune_model::MessageKind::Default)
            .take(2)
            .collect();
        assert_eq!(messages.len(), 2);
        client.storage.as_ref().unwrap().save(messages.clone());
        let kept = &messages[0];
        let deleted = &messages[1];
        let patch = |id| MessagePatch {
            channel,
            id,
            ..Default::default()
        };
        let (sender, events) = mpsc::channel(16);
        client.events = events;
        for event in [
            Event::MessagePatched(MessagePatch {
                content: Some("edited by a partial event".into()),
                edited: Some(Some(Timestamp(1_790_000_200_000))),
                pinned: Some(false),
                ..patch(kept.id)
            }),
            Event::MessagePatched(MessagePatch {
                content: Some("older edit must not win".into()),
                edited: Some(Some(Timestamp(1))),
                ..patch(kept.id)
            }),
            Event::MessagePatched(MessagePatch {
                content: Some("missing baseline stays missing".into()),
                ..patch(MessageId(u64::MAX - 1))
            }),
            Event::MessageDeleted {
                channel,
                message: deleted.id,
            },
            Event::MessagePatched(MessagePatch {
                content: Some("deleted stays deleted".into()),
                ..patch(deleted.id)
            }),
        ] {
            sender
                .try_send(Envelope {
                    generation: client.generation,
                    event,
                })
                .unwrap();
        }
        while !client.events.is_empty() {
            client.poll(max);
        }
        let account = client.account().to_owned();
        drop(client);
        let storage = unlocked(&path, &account);
        let expected = Message {
            content: "edited by a partial event".into(),
            edited: Some(Timestamp(1_790_000_200_000)),
            pinned: false,
            text_references: Vec::new(),
            ..kept.clone()
        };
        assert_eq!(
            storage.recent(&account, channel, PAGE).unwrap(),
            vec![expected],
            "poll size {max}"
        );
        drop(storage);
        let _ = std::fs::remove_dir_all(directory);
    }
}
