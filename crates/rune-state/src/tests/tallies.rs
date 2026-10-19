use super::*;
use rune_model::Reaction;

fn polled(store: &mut Store, multiselect: bool) {
    let answer = |id, votes| rune_model::PollAnswer {
        id,
        text: format!("respuesta {id}"),
        votes,
        ..Default::default()
    };
    store.apply(
        Event::MessageUpdated(Message {
            poll: Some(rune_model::Poll {
                question: "¿Cuál?".into(),
                answers: vec![answer(1, 3), answer(2, 1)],
                multiselect,
                results_known: true,
                ..Default::default()
            }),
            ..message(4, ADA)
        }),
        Instant::now(),
    );
}

fn votes(store: &Store) -> Vec<(u32, bool)> {
    let shown = store.timeline(GENERAL).unwrap().get(MessageId(4)).unwrap();
    let poll = shown.poll.as_ref().unwrap();
    poll.answers
        .iter()
        .map(|answer| (answer.votes, answer.me))
        .collect()
}

fn vote_event(answer: u32, me: bool, added: bool) -> Event {
    Event::PollVoteChanged {
        channel: GENERAL,
        message: MessageId(4),
        answer,
        me,
        added,
    }
}

fn failed_vote(ambiguous: bool) -> Event {
    Event::Failed {
        failure: Failure::Vote {
            channel: GENERAL,
            message: MessageId(4),
        },
        reason: "falló".into(),
        ambiguous,
    }
}

#[test]
fn live_reactions_count_others_and_confirm_the_accounts_toggle_once() {
    let now = Instant::now();
    let mut store = ready();
    history(&mut store, None, 1..6, true);
    let reaction = |emoji: &str, me, added| Event::ReactionChanged {
        channel: GENERAL,
        message: MessageId(4),
        emoji: emoji.into(),
        me,
        added,
    };
    let shown = |store: &Store| {
        store
            .timeline(GENERAL)
            .unwrap()
            .get(MessageId(4))
            .unwrap()
            .reactions
            .clone()
    };
    let pill = |count, me| Reaction {
        emoji: "🦀".into(),
        count,
        me,
        ..Default::default()
    };
    store.apply(reaction("🦀", false, true), now);
    assert_eq!(shown(&store), vec![pill(1, false)]);
    store.update_local(GENERAL, MessageId(4), |message| {
        message.toggle_reaction("🦀")
    });
    assert_eq!(shown(&store), vec![pill(2, true)]);
    // Someone else while the toggle is pending: rebased, not lost.
    store.apply(reaction("🦀", false, true), now);
    assert_eq!(shown(&store), vec![pill(3, true)]);
    // The service echoes the account's own reaction, maybe twice: once.
    for _ in 0..2 {
        assert_eq!(
            store.apply(reaction("🦀", true, true), now),
            Applied::Nothing
        );
        assert_eq!(shown(&store), vec![pill(3, true)]);
    }
    // The echo confirmed the toggle: a late failure no longer undoes it.
    store.apply(
        Event::Failed {
            failure: Failure::React {
                channel: GENERAL,
                message: MessageId(4),
                emoji: "🦀".into(),
            },
            reason: "falló".into(),
            ambiguous: true,
        },
        now,
    );
    assert_eq!(shown(&store), vec![pill(3, true)]);
    // A super reaction is counted apart, beside the normal ones.
    let burst = Event::BurstReactionChanged {
        channel: GENERAL,
        message: MessageId(4),
        emoji: "🦀".into(),
        me: false,
        added: true,
        colors: vec![rune_model::Rgb(0xff8800)],
    };
    assert!(matches!(store.apply(burst, now), Applied::Messages { .. }));
    assert_eq!(
        shown(&store),
        vec![Reaction {
            burst: 1,
            burst_colors: vec![rune_model::Rgb(0xff8800)],
            ..pill(3, true)
        }]
    );
    store.apply(reaction("🎉", false, true), now);
    store.apply(
        Event::ReactionsCleared {
            channel: GENERAL,
            message: MessageId(4),
            emoji: Some("🦀".into()),
        },
        now,
    );
    assert_eq!(shown(&store).len(), 1);
    store.apply(
        Event::ReactionsCleared {
            channel: GENERAL,
            message: MessageId(4),
            emoji: None,
        },
        now,
    );
    assert!(shown(&store).is_empty());
    // A message that is not loaded is not made up.
    let unknown = Event::ReactionChanged {
        channel: GENERAL,
        message: MessageId(99),
        emoji: "🦀".into(),
        me: false,
        added: true,
    };
    assert_eq!(store.apply(unknown, now), Applied::Nothing);
    assert!(store
        .timeline(GENERAL)
        .unwrap()
        .get(MessageId(99))
        .is_none());
}

#[test]
fn a_refused_vote_is_put_back_with_the_votes_others_cast_meanwhile() {
    let now = Instant::now();
    let mut store = ready();
    history(&mut store, None, 1..6, true);
    polled(&mut store, false);
    store.update_local(GENERAL, MessageId(4), |message| {
        message.poll.as_mut().unwrap().vote(&[2])
    });
    assert_eq!(votes(&store), [(3, false), (2, true)]);
    store.apply(vote_event(1, false, true), now);
    assert_eq!(votes(&store), [(4, false), (2, true)]);
    store.apply(failed_vote(false), now);
    assert_eq!(votes(&store), [(4, false), (1, false)]);
}

#[test]
fn the_accounts_own_vote_events_confirm_a_multiselect_vote_once() {
    let now = Instant::now();
    let mut store = ready();
    history(&mut store, None, 1..6, true);
    polled(&mut store, true);
    store.update_local(GENERAL, MessageId(4), |message| {
        message.poll.as_mut().unwrap().vote(&[1, 2])
    });
    assert_eq!(votes(&store), [(4, true), (2, true)]);
    for _ in 0..2 {
        store.apply(vote_event(1, true, true), now);
        store.apply(vote_event(2, true, true), now);
    }
    assert_eq!(votes(&store), [(4, true), (2, true)]);
    // Both confirmed: nothing is left to undo.
    store.apply(failed_vote(true), now);
    assert_eq!(votes(&store), [(4, true), (2, true)]);
    // Taken back on another device, then someone else leaves.
    store.apply(vote_event(1, true, false), now);
    store.apply(vote_event(2, false, false), now);
    assert_eq!(votes(&store), [(3, false), (1, true)]);
    assert_eq!(
        store.apply(vote_event(2, false, false), now),
        Applied::Nothing,
        "only the account's vote is left"
    );
}
