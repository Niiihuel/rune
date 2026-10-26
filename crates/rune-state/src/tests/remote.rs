//! Reminders at the provider, whole server reads, passive activity, searches
//! and members found by name (P2).

use super::*;

fn with(capabilities: rune_provider::Capabilities) -> Store {
    let mut store = ready();
    store.provider = Some(rune_provider::ProviderInfo {
        name: "Lecturas remotas de prueba".into(),
        kind: rune_provider::ProviderKind::Simulated,
        capabilities,
        notice: "Fixture".into(),
    });
    store
}

fn remote() -> Store {
    with(rune_provider::Capabilities {
        read_state: true,
        remote_unread: true,
        server_read: true,
        message_search: true,
        member_search: true,
        ..Default::default()
    })
}

#[test]
fn a_reminder_goes_to_the_provider_only_when_it_takes_them() {
    let mut store = with(rune_provider::Capabilities {
        read_state: true,
        ..Default::default()
    });
    history(&mut store, None, 1..6, true);
    assert_eq!(store.mark_unread_command(GENERAL, MessageId(4)), None);
    let mut store = remote();
    history(&mut store, None, 1..6, true);
    let mut mention = message(5, ADA);
    mention.mentions = vec![ME];
    store.apply(Event::MessageUpdated(mention), Instant::now());
    assert_eq!(
        store.mark_unread_command(GENERAL, MessageId(4)),
        Some(Command::MarkUnread {
            account: ME,
            channel: GENERAL,
            message: MessageId(4),
            mentions: 1,
        })
    );
    assert_eq!(store.read(GENERAL).unread_from, Some(MessageId(4)));
}

#[test]
fn another_devices_reminder_falls_on_the_first_loaded_message_after_its_cursor() {
    let mut store = remote();
    history(&mut store, None, 1..6, true);
    store.mark_read(GENERAL);
    let held = |through, version| Event::MarkedUnread {
        account: ME,
        channel: GENERAL,
        through: Some(MessageId(through)),
        anchor: None,
        mentions: 2,
        version,
    };
    assert_eq!(
        store.apply(held(3, 5), Instant::now()),
        Applied::ReadState(GENERAL)
    );
    let read = store.read(GENERAL);
    assert_eq!(read.unread_from, Some(MessageId(4)));
    assert_eq!(read.confirmed_read, Some(MessageId(3)));
    assert_eq!(read.mentions, 2);
    assert!(read.unread());
    // An older version changes nothing; a newer read confirms forward again.
    assert_eq!(store.apply(held(1, 4), Instant::now()), Applied::Nothing);
    store.apply(
        Event::ReadStateConfirmed {
            account: ME,
            request: None,
            channel: GENERAL,
            message: MessageId(5),
            version: 6,
        },
        Instant::now(),
    );
    assert_eq!(store.read(GENERAL).confirmed_read, Some(MessageId(5)));
    // Without the capability, the provider's reminders are not taken.
    let mut local = with(rune_provider::Capabilities {
        read_state: true,
        ..Default::default()
    });
    assert_eq!(local.apply(held(3, 9), Instant::now()), Applied::Nothing);
}

#[test]
fn a_server_is_marked_read_in_one_command_or_channel_by_channel() {
    let mut store = remote();
    assert!(store.server_unread(ServerId(1)).0);
    assert_eq!(
        store.mark_server_read_commands(ServerId(1)),
        vec![Command::MarkServerRead {
            account: ME,
            server: ServerId(1),
        }]
    );
    assert!(!store.server_unread(ServerId(1)).0);
    assert!(store.mark_server_read_commands(ServerId(1)).is_empty());
    let mut each = with(rune_provider::Capabilities {
        read_state: true,
        ..Default::default()
    });
    assert!(matches!(
        each.mark_server_read_commands(ServerId(1))[..],
        [Command::MarkRead {
            channel: GENERAL,
            ..
        }]
    ));
    let mut local = ready();
    assert!(local.mark_server_read_commands(ServerId(1)).is_empty());
    assert!(!local.server_unread(ServerId(1)).0, "marked here anyway");
}

#[test]
fn passive_activity_marks_channels_unread_without_their_messages() {
    let mut store = remote();
    store.mark_read(GENERAL);
    assert!(!store.read(GENERAL).unread());
    assert_eq!(
        store.apply(
            Event::LatestMessages(vec![
                (GENERAL, MessageId(9)),
                (ChannelId(999), MessageId(1))
            ]),
            Instant::now()
        ),
        Applied::Activity
    );
    assert!(store.read(GENERAL).unread());
    assert_eq!(store.read(GENERAL).last_message, Some(MessageId(9)));
    assert_eq!(
        store.apply(
            Event::LatestMessages(vec![(GENERAL, MessageId(8))]),
            Instant::now()
        ),
        Applied::Nothing
    );
}

#[test]
fn lookups_say_why_not_and_found_members_join_once() {
    let store = ready();
    assert!(!store.remote_search(GENERAL).allowed());
    assert!(!store.member_search(ServerId(1)).allowed());
    let mut store = remote();
    assert!(store.remote_search(GENERAL).allowed());
    assert!(store.member_search(ServerId(1)).allowed());
    assert!(!store.member_search(ServerId(9)).allowed());
    store.connection = ConnectionState::Connecting;
    assert_eq!(
        store.remote_search(GENERAL).reason(),
        Some("Espera a que la conexión esté disponible")
    );
    store.connection = ConnectionState::Connected;
    let known = store.members(ServerId(1)).len();
    let found = Member {
        user: UserId(77),
        server: ServerId(1),
        ..Default::default()
    };
    store.apply(
        Event::UserChanged(user(UserId(77), "pablo")),
        Instant::now(),
    );
    for _ in 0..2 {
        assert_eq!(
            store.apply(
                Event::MembersFound {
                    server: ServerId(1),
                    query: "pa".into(),
                    members: vec![found.clone()],
                },
                Instant::now()
            ),
            Applied::MembersFound {
                server: ServerId(1),
                query: "pa".into()
            }
        );
    }
    assert_eq!(store.members(ServerId(1)).len(), known + 1);
    // A member of another server is not taken.
    assert_eq!(
        store.apply(
            Event::MembersFound {
                server: ServerId(1),
                query: "x".into(),
                members: vec![Member {
                    server: ServerId(2),
                    ..found
                }],
            },
            Instant::now()
        ),
        Applied::Nothing
    );
}

#[test]
fn automatic_reading_keeps_one_confirmation_in_flight_per_channel() {
    let mut store = remote();
    history(&mut store, None, 1..6, true);
    let Some(Command::MarkRead {
        request,
        message: MessageId(5),
        ..
    }) = store.mark_read_visible_command(GENERAL)
    else {
        panic!("the first reading goes out");
    };
    // A message arrives while it is in flight: read here, not sent yet.
    store.apply(
        Event::MessageCreated {
            message: message(6, ADA),
            nonce: None,
        },
        Instant::now(),
    );
    assert_eq!(store.mark_read_visible_command(GENERAL), None);
    assert_eq!(store.read(GENERAL).last_read, Some(MessageId(6)));
    assert_eq!(store.read_follow_up(GENERAL), None, "still in flight");
    store.apply(
        Event::ReadStateConfirmed {
            account: ME,
            request: Some(request),
            channel: GENERAL,
            message: MessageId(5),
            version: 1,
        },
        Instant::now(),
    );
    assert!(matches!(
        store.read_follow_up(GENERAL),
        Some(Command::MarkRead {
            message: MessageId(6),
            ..
        })
    ));
    assert_eq!(store.read_follow_up(GENERAL), None, "in flight again");
}
