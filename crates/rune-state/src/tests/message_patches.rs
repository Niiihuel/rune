use super::*;
use rune_model::{MessagePatch, Reaction};

fn patch(id: u64) -> MessagePatch {
    MessagePatch {
        channel: GENERAL,
        id: MessageId(id),
        ..Default::default()
    }
}

#[test]
fn partial_update_changes_a_retained_timeline_without_erasing_absent_fields() {
    let mut store = ready();
    history(&mut store, None, 1..520, true);
    let before = store
        .timeline(GENERAL)
        .unwrap()
        .get(MessageId(4))
        .unwrap()
        .clone();
    let applied = store.apply(
        Event::MessagePatched(MessagePatch {
            content: Some("edición remota retenida".into()),
            edited: Some(Some(Timestamp(200))),
            ..patch(4)
        }),
        Instant::now(),
    );
    assert_eq!(
        applied,
        Applied::Messages {
            channel: GENERAL,
            added: 0
        }
    );
    let current = store.timeline(GENERAL).unwrap().get(MessageId(4)).unwrap();
    assert_eq!(current.content, "edición remota retenida");
    assert_eq!(current.author, before.author);
    assert_eq!(current.timestamp, before.timestamp);
    assert_eq!(current.reply_to, before.reply_to);
    assert_eq!(store.timeline(GENERAL).unwrap().len(), 519);
}

#[test]
fn partial_update_reaches_a_pin_retained_outside_the_timeline() {
    let mut store = pins_ready();
    let now = Instant::now();
    assert!(store.begin_pins(GENERAL, 1).allowed());
    store.apply(
        Event::PinsLoaded {
            channel: GENERAL,
            request: 1,
            messages: vec![Message {
                pinned: true,
                ..message(700, ADA)
            }],
        },
        now,
    );
    store.apply(
        Event::MessagePatched(MessagePatch {
            content: Some("fijado actualizado".into()),
            ..patch(700)
        }),
        now,
    );
    assert_eq!(
        store.pins(GENERAL).unwrap()[0].content,
        "fijado actualizado"
    );
    assert!(store.timeline(GENERAL).is_none());
}

#[test]
fn remote_reactions_survive_failure_of_an_unrelated_optimistic_edit() {
    let mut store = ready();
    history(&mut store, None, 1..6, true);
    store.update_local(GENERAL, MessageId(4), |message| {
        message.content = "edición pendiente".into()
    });
    let reactions = vec![Reaction {
        emoji: "🦀".into(),
        count: 8,
        me: false,
        ..Default::default()
    }];
    store.apply(
        Event::MessagePatched(MessagePatch {
            reactions: Some(reactions.clone()),
            ..patch(4)
        }),
        Instant::now(),
    );
    assert_eq!(
        store
            .timeline(GENERAL)
            .unwrap()
            .get(MessageId(4))
            .unwrap()
            .content,
        "edición pendiente"
    );
    store.apply(
        Event::Failed {
            failure: Failure::Edit {
                channel: GENERAL,
                message: MessageId(4),
            },
            reason: "falló".into(),
            ambiguous: false,
        },
        Instant::now(),
    );
    let current = store.timeline(GENERAL).unwrap().get(MessageId(4)).unwrap();
    assert_eq!(current.content, "mensaje 4");
    assert_eq!(current.reactions, reactions);
}

#[test]
fn newer_canonical_pin_rebases_undo_without_losing_a_pending_edit() {
    let mut store = pins_ready();
    let now = Instant::now();
    history(&mut store, None, 1..6, true);
    let old = Message {
        content: "versión cinco".into(),
        edited: Some(Timestamp(5)),
        pinned: true,
        ..message(4, ADA)
    };
    store.apply(Event::MessageUpdated(old), now);
    store.update_local(GENERAL, MessageId(4), |message| {
        message.content = "edición pendiente".into()
    });
    assert!(store.begin_pins(GENERAL, 1).allowed());
    store.apply(
        Event::PinsLoaded {
            channel: GENERAL,
            request: 1,
            messages: vec![Message {
                content: "versión diez".into(),
                edited: Some(Timestamp(10)),
                pinned: true,
                text_references: vec![rune_model::TextReference {
                    source: "versión diez".into(),
                    target: rune_model::TextTarget::Invitation,
                }],
                ..message(4, ADA)
            }],
        },
        now,
    );
    let reactions = vec![Reaction {
        emoji: "🦀".into(),
        count: 8,
        me: false,
        ..Default::default()
    }];
    store.apply(
        Event::MessagePatched(MessagePatch {
            reactions: Some(reactions.clone()),
            ..patch(4)
        }),
        now,
    );
    assert_eq!(
        store
            .timeline(GENERAL)
            .unwrap()
            .get(MessageId(4))
            .unwrap()
            .content,
        "edición pendiente"
    );
    assert!(store
        .timeline(GENERAL)
        .unwrap()
        .get(MessageId(4))
        .unwrap()
        .text_references
        .is_empty());
    store.apply(
        Event::Failed {
            failure: Failure::Edit {
                channel: GENERAL,
                message: MessageId(4),
            },
            reason: "falló".into(),
            ambiguous: false,
        },
        now,
    );
    let current = store.timeline(GENERAL).unwrap().get(MessageId(4)).unwrap();
    assert_eq!(current.content, "versión diez");
    assert_eq!(current.edited, Some(Timestamp(10)));
    assert_eq!(current.reactions, reactions);
    assert_eq!(current.text_references[0].source, "versión diez");
}

#[test]
fn older_pins_snapshot_never_promotes_a_visible_optimistic_edit_to_confirmed() {
    let mut store = pins_ready();
    let now = Instant::now();
    history(&mut store, None, 1..6, true);
    let older = Message {
        content: "confirmado cinco".into(),
        edited: Some(Timestamp(5)),
        pinned: true,
        ..message(4, ADA)
    };
    store.apply(Event::MessageUpdated(older.clone()), now);
    let confirmed = Message {
        content: "confirmado diez".into(),
        edited: Some(Timestamp(10)),
        ..older.clone()
    };
    store.apply(Event::MessageUpdated(confirmed.clone()), now);
    store.update_local(GENERAL, MessageId(4), |message| {
        message.content = "edición pendiente".into()
    });
    assert!(store.begin_pins(GENERAL, 1).allowed());
    store.apply(
        Event::PinsLoaded {
            channel: GENERAL,
            request: 1,
            messages: vec![older],
        },
        now,
    );
    assert_eq!(
        store.pins(GENERAL).unwrap(),
        std::slice::from_ref(&confirmed)
    );
    let reactions = vec![Reaction {
        emoji: "🦀".into(),
        count: 8,
        me: false,
        ..Default::default()
    }];
    store.apply(
        Event::MessagePatched(MessagePatch {
            reactions: Some(reactions.clone()),
            ..patch(4)
        }),
        now,
    );
    assert_eq!(
        store
            .timeline(GENERAL)
            .unwrap()
            .get(MessageId(4))
            .unwrap()
            .content,
        "edición pendiente"
    );
    store.apply(
        Event::Failed {
            failure: Failure::Edit {
                channel: GENERAL,
                message: MessageId(4),
            },
            reason: "falló".into(),
            ambiguous: false,
        },
        now,
    );
    let current = store.timeline(GENERAL).unwrap().get(MessageId(4)).unwrap();
    assert_eq!(current.content, confirmed.content);
    assert_eq!(current.edited, confirmed.edited);
    assert_eq!(current.reactions, reactions);
    assert_eq!(store.pins(GENERAL).unwrap()[0], **current);
}

#[test]
fn refreshed_metadata_does_not_replace_a_newer_pins_snapshot_with_older_content() {
    let mut store = pins_ready();
    let now = Instant::now();
    assert!(store.begin_pins(GENERAL, 1).allowed());
    store.apply(
        Event::PinsLoaded {
            channel: GENERAL,
            request: 1,
            messages: vec![Message {
                content: "versión cinco".into(),
                edited: Some(Timestamp(5)),
                pinned: true,
                ..message(700, ADA)
            }],
        },
        now,
    );
    assert!(store.begin_pins(GENERAL, 2).allowed());
    store.apply(
        Event::MessagePatched(MessagePatch {
            reactions: Some(vec![Reaction {
                emoji: "🦀".into(),
                count: 8,
                me: false,
                ..Default::default()
            }]),
            ..patch(700)
        }),
        now,
    );
    let snapshot = Message {
        content: "versión diez".into(),
        edited: Some(Timestamp(10)),
        pinned: true,
        ..message(700, ADA)
    };
    store.apply(
        Event::PinsLoaded {
            channel: GENERAL,
            request: 2,
            messages: vec![snapshot.clone()],
        },
        now,
    );
    assert_eq!(store.pins(GENERAL).unwrap(), &[snapshot]);
}

#[test]
fn metadata_patch_during_pins_read_does_not_assert_membership_missing_from_snapshot() {
    let now = Instant::now();
    for listed in [false, true] {
        let mut store = pins_ready();
        assert!(store.begin_pins(GENERAL, 1).allowed());
        let original = Message {
            pinned: true,
            ..message(700, ADA)
        };
        store.apply(
            Event::PinsLoaded {
                channel: GENERAL,
                request: 1,
                messages: vec![original.clone()],
            },
            now,
        );
        assert!(store.begin_pins(GENERAL, 2).allowed());
        let reactions = vec![Reaction {
            emoji: "🦀".into(),
            count: 8,
            me: false,
            ..Default::default()
        }];
        store.apply(
            Event::MessagePatched(MessagePatch {
                reactions: Some(reactions.clone()),
                ..patch(700)
            }),
            now,
        );
        store.apply(
            Event::PinsLoaded {
                channel: GENERAL,
                request: 2,
                messages: if listed { vec![original] } else { Vec::new() },
            },
            now,
        );
        if listed {
            assert_eq!(store.pins(GENERAL).unwrap()[0].reactions, reactions);
        } else {
            assert!(
                store.pins(GENERAL).unwrap().is_empty(),
                "an absent pinned field cannot add snapshot membership"
            );
        }
    }
}

#[test]
fn an_unknown_unpin_during_read_cannot_be_undone_by_its_snapshot() {
    let mut store = pins_ready();
    let now = Instant::now();
    assert!(store.begin_pins(GENERAL, 1).allowed());
    store.apply(
        Event::MessagePatched(MessagePatch {
            pinned: Some(false),
            ..patch(700)
        }),
        now,
    );
    assert!(store.pins(GENERAL).is_none());
    store.apply(
        Event::PinsLoaded {
            channel: GENERAL,
            request: 1,
            messages: vec![Message {
                pinned: true,
                ..message(700, ADA)
            }],
        },
        now,
    );
    assert!(store.pins(GENERAL).unwrap().is_empty());
}

#[test]
fn unknown_partial_update_requires_a_real_baseline_on_retry() {
    let now = Instant::now();
    for update in [
        MessagePatch {
            pinned: Some(true),
            ..patch(700)
        },
        MessagePatch {
            author: Some(ADA),
            pinned: Some(true),
            ..patch(700)
        },
        MessagePatch {
            content: Some("texto nuevo".into()),
            edited: Some(Some(Timestamp(20))),
            ..patch(700)
        },
        MessagePatch {
            reactions: Some(vec![Reaction {
                emoji: "🦀".into(),
                count: 8,
                me: false,
                ..Default::default()
            }]),
            ..patch(700)
        },
    ] {
        let mut store = pins_ready();
        assert!(store.begin_pins(GENERAL, 1).allowed());
        store.apply(Event::MessagePatched(update), now);
        let applied = store.apply(
            Event::PinsLoaded {
                channel: GENERAL,
                request: 1,
                messages: Vec::new(),
            },
            now,
        );
        match applied {
            Applied::Failed {
                failure:
                    Failure::Pins {
                        channel: GENERAL,
                        request: 1,
                    },
                reason,
                ambiguous: false,
            } => {
                assert!(reason.contains("Vuelve a intentarlo"));
                assert!(
                    !reason.contains("demasiado"),
                    "an unknown baseline is not a delta overflow"
                );
            }
            applied => panic!("expected a retryable pins failure, got {applied:?}"),
        }
        assert!(store.pins(GENERAL).is_none());
        assert!(store.timeline(GENERAL).is_none());
        assert!(store.begin_pins(GENERAL, 2).allowed());
        let baseline = Message {
            pinned: true,
            ..message(700, ADA)
        };
        assert_eq!(
            store.apply(
                Event::PinsLoaded {
                    channel: GENERAL,
                    request: 2,
                    messages: vec![baseline.clone()]
                },
                now
            ),
            Applied::Pins {
                channel: GENERAL,
                request: 2
            }
        );
        assert_eq!(store.pins(GENERAL).unwrap(), &[baseline]);
    }
}

#[test]
fn remote_content_survives_failure_of_an_unrelated_optimistic_reaction() {
    let mut store = ready();
    history(&mut store, None, 1..6, true);
    store.update_local(GENERAL, MessageId(4), |message| {
        message.reactions.push(Reaction {
            emoji: "🦀".into(),
            count: 1,
            me: true,
            ..Default::default()
        })
    });
    store.apply(
        Event::MessagePatched(MessagePatch {
            content: Some("contenido confirmado".into()),
            edited: Some(Some(Timestamp(200))),
            ..patch(4)
        }),
        Instant::now(),
    );
    let shown = store.timeline(GENERAL).unwrap().get(MessageId(4)).unwrap();
    assert_eq!(shown.content, "contenido confirmado");
    assert!(shown.reactions[0].me);
    store.apply(
        Event::Failed {
            failure: Failure::React {
                channel: GENERAL,
                message: MessageId(4),
                emoji: "🦀".into(),
            },
            reason: "falló".into(),
            ambiguous: false,
        },
        Instant::now(),
    );
    let current = store.timeline(GENERAL).unwrap().get(MessageId(4)).unwrap();
    assert_eq!(current.content, "contenido confirmado");
    assert_eq!(current.edited, Some(Timestamp(200)));
    assert!(current.reactions.is_empty());
}

#[test]
fn optimistic_reaction_rebases_its_membership_on_newer_remote_counts() {
    let now = Instant::now();
    for (originally_me, confirmed_me) in
        [(false, false), (false, true), (true, false), (true, true)]
    {
        let mut store = pins_ready();
        history(&mut store, None, 1..6, true);
        let reaction = |count, me| Reaction {
            emoji: "🦀".into(),
            count,
            me,
            ..Default::default()
        };
        store.apply(
            Event::MessageUpdated(Message {
                edited: Some(Timestamp(5)),
                reactions: vec![reaction(2, originally_me)],
                pinned: true,
                ..message(4, ADA)
            }),
            now,
        );
        store.update_local(GENERAL, MessageId(4), |message| {
            message.toggle_reaction("🦀")
        });
        assert!(store.begin_pins(GENERAL, 1).allowed());
        store.apply(
            Event::PinsLoaded {
                channel: GENERAL,
                request: 1,
                messages: vec![Message {
                    edited: Some(Timestamp(10)),
                    reactions: vec![reaction(5, confirmed_me)],
                    pinned: true,
                    ..message(4, ADA)
                }],
            },
            now,
        );
        store.apply(
            Event::MessagePatched(MessagePatch {
                pinned: Some(true),
                ..patch(4)
            }),
            now,
        );
        let shown = store.timeline(GENERAL).unwrap().get(MessageId(4)).unwrap();
        let count = if confirmed_me != originally_me {
            5
        } else if originally_me {
            4
        } else {
            6
        };
        assert_eq!(shown.reactions, vec![reaction(count, !originally_me)]);
        store.apply(
            Event::Failed {
                failure: Failure::React {
                    channel: GENERAL,
                    message: MessageId(4),
                    emoji: "🦀".into(),
                },
                reason: "falló".into(),
                ambiguous: false,
            },
            now,
        );
        assert_eq!(
            store
                .timeline(GENERAL)
                .unwrap()
                .get(MessageId(4))
                .unwrap()
                .reactions,
            vec![reaction(5, confirmed_me)]
        );
    }
}

#[test]
fn stale_or_mismatched_partial_updates_do_not_mutate_retained_state() {
    let now = Instant::now();
    for update in [
        MessagePatch {
            edited: Some(Some(Timestamp(9))),
            ..patch(4)
        },
        MessagePatch {
            author: Some(ME),
            ..patch(4)
        },
        MessagePatch {
            channel: VOICE,
            ..patch(4)
        },
    ] {
        let mut store = pins_ready();
        history(&mut store, None, 1..6, true);
        let baseline = Message {
            edited: Some(Timestamp(10)),
            pinned: true,
            ..message(4, ADA)
        };
        store.apply(Event::MessageUpdated(baseline.clone()), now);
        assert!(store.begin_pins(GENERAL, 1).allowed());
        store.apply(
            Event::PinsLoaded {
                channel: GENERAL,
                request: 1,
                messages: vec![baseline.clone()],
            },
            now,
        );
        assert_eq!(
            store.apply(
                Event::MessagePatched(MessagePatch {
                    content: Some("no debe entrar".into()),
                    pinned: Some(false),
                    ..update
                }),
                now
            ),
            Applied::Nothing
        );
        assert_eq!(
            store
                .timeline(GENERAL)
                .unwrap()
                .get(MessageId(4))
                .unwrap()
                .as_ref(),
            &baseline
        );
        assert_eq!(store.pins(GENERAL).unwrap(), &[baseline]);
    }
}

#[test]
fn partial_updates_cannot_resurrect_deleted_or_signed_out_messages() {
    let now = Instant::now();
    let mut store = pins_ready();
    history(&mut store, None, 1..6, true);
    store.apply(
        Event::MessageDeleted {
            channel: GENERAL,
            message: MessageId(4),
        },
        now,
    );
    assert_eq!(
        store.apply(
            Event::MessagePatched(MessagePatch {
                content: Some("resucitado".into()),
                pinned: Some(true),
                ..patch(4)
            }),
            now
        ),
        Applied::Nothing
    );
    assert!(store.timeline(GENERAL).unwrap().get(MessageId(4)).is_none());
    store.apply(Event::Authentication(AuthenticationState::SignedOut), now);
    assert_eq!(
        store.apply(
            Event::MessagePatched(MessagePatch {
                content: Some("sesión anterior".into()),
                ..patch(3)
            }),
            now
        ),
        Applied::Nothing
    );
    assert!(store.timeline(GENERAL).is_none());
    assert!(store.pins(GENERAL).is_none());
}
