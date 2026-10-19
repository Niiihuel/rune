//! Live reactions and votes reach reopened SQLite, and a vote nobody
//! answers fails on the client's own deadline (2026-10-18).

use super::*;
use rune_model::{Poll, PollAnswer, Reaction, Rgb};

#[test]
fn live_reactions_and_votes_reach_sqlite() {
    let directory = std::env::temp_dir().join(format!("rune-core-tallies-{}", std::process::id()));
    let path = directory.join("rune.db");
    let provider = mock();
    let mut client = client(Arc::new(mock()), Some(path.clone()));
    let channel = ready(&mut client)
        .iter()
        .find(|channel| channel.name == "general")
        .unwrap()
        .id;
    let mut kept = provider
        .history(channel, None, PAGE)
        .0
        .into_iter()
        .find(|message| message.kind == rune_model::MessageKind::Default)
        .unwrap();
    kept.reactions.clear();
    let answer = |id, votes| PollAnswer {
        id,
        text: id.to_string(),
        votes,
        ..Default::default()
    };
    kept.poll = Some(Poll {
        question: "¿Sí o no?".into(),
        answers: vec![answer(1, 2), answer(2, 0)],
        results_known: true,
        ..Default::default()
    });
    client.storage.as_ref().unwrap().save(vec![kept.clone()]);
    let (sender, events) = mpsc::channel(16);
    client.events = events;
    for event in [
        Event::ReactionChanged {
            channel,
            message: kept.id,
            emoji: "🦀".into(),
            me: false,
            added: true,
        },
        Event::BurstReactionChanged {
            channel,
            message: kept.id,
            emoji: "🦀".into(),
            me: true,
            added: true,
            colors: vec![Rgb(0xff8800)],
        },
        Event::ReactionChanged {
            channel,
            message: kept.id,
            emoji: "🎉".into(),
            me: false,
            added: true,
        },
        Event::ReactionsCleared {
            channel,
            message: kept.id,
            emoji: Some("🎉".into()),
        },
        // Someone else's vote, and the account's own.
        Event::PollVoteChanged {
            channel,
            message: kept.id,
            answer: 1,
            me: false,
            added: true,
        },
        Event::PollVoteChanged {
            channel,
            message: kept.id,
            answer: 2,
            me: true,
            added: true,
        },
        // A message the store does not keep stays missing.
        Event::ReactionChanged {
            channel,
            message: MessageId(u64::MAX - 1),
            emoji: "🦀".into(),
            me: false,
            added: true,
        },
    ] {
        sender
            .try_send(Envelope {
                generation: client.generation,
                event,
            })
            .unwrap();
    }
    while !client.events.is_empty() {
        client.poll(64);
    }
    let account = client.account().to_owned();
    drop(client);
    let storage = unlocked(&path, &account);
    let expected = Message {
        reactions: vec![Reaction {
            emoji: "🦀".into(),
            count: 1,
            me: false,
            burst: 1,
            me_burst: true,
            burst_colors: vec![Rgb(0xff8800)],
        }],
        poll: Some(Poll {
            answers: vec![
                answer(1, 3),
                PollAnswer {
                    me: true,
                    ..answer(2, 1)
                },
            ],
            ..kept.poll.clone().unwrap()
        }),
        ..kept
    };
    assert_eq!(
        storage.recent(&account, channel, PAGE).unwrap(),
        vec![expected]
    );
    drop(storage);
    let _ = std::fs::remove_dir_all(directory);
}

#[test]
fn a_vote_without_an_answer_fails_as_maybe_done_when_its_time_is_up() {
    let deadlines = Deadlines {
        history: Duration::from_secs(20),
        send: Duration::from_millis(60),
    };
    let mut client = client_with(Arc::new(mock()), None, deadlines);
    let channel = text_channels(&ready(&mut client))[0];
    let message = MessageId(7);
    client.command(Command::VotePoll {
        channel,
        message,
        answers: vec![1],
    });
    assert!(client
        .next_deadline(Instant::now())
        .is_some_and(|left| left <= Duration::from_millis(60)));
    // Before the simulation's own answer (it has no such message) arrives.
    client.expire(Instant::now() + Duration::from_secs(1));
    let failed = client.poll(64).into_iter().find_map(|update| match update {
        Update::Provider(Event::Failed {
            failure:
                Failure::Vote {
                    channel: at,
                    message: id,
                },
            ambiguous: true,
            reason,
        }) => Some((at, id, reason)),
        _ => None,
    });
    let (at, id, reason) = failed.expect("the vote failed as maybe done");
    assert_eq!((at, id), (channel, message));
    assert!(reason.contains("sin confirmación"), "{reason}");
}
