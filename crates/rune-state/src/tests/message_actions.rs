//! Links to messages, ending polls, where to forward, voice moderation and
//! leaving a server (P4).

use super::*;
use rune_provider::{Capabilities, ConnectionState};

fn with(capabilities: Capabilities) -> Store {
    let mut store = ready();
    store.provider = Some(rune_provider::ProviderInfo {
        name: "Acciones de prueba".into(),
        kind: rune_provider::ProviderKind::Simulated,
        capabilities,
        notice: "Fixture".into(),
    });
    store.apply(
        Event::Connection(ConnectionState::Connected),
        Instant::now(),
    );
    store
}

fn all() -> Capabilities {
    Capabilities {
        send_messages: true,
        history: true,
        forward_messages: true,
        create_polls: true,
        reaction_lists: true,
        poll_voters: true,
        voice: rune_provider::VoiceSupport::Simulated,
        voice_moderation: true,
        leave_servers: true,
        channel_links: Some("https://chat.test/channels"),
        ..Default::default()
    }
}

fn direct() -> Channel {
    Channel {
        server: None,
        kind: ChannelKind::Direct,
        recipients: vec![ADA],
        ..channel(ChannelId(30), ChannelKind::Direct, "")
    }
}

#[test]
fn a_message_link_names_its_server_or_none() {
    let mut store = with(all());
    store.apply(Event::ChannelChanged(direct()), Instant::now());
    assert_eq!(
        store.message_link(GENERAL, MessageId(5)).as_deref(),
        Some("https://chat.test/channels/1/10/5")
    );
    assert_eq!(
        store.message_link(ChannelId(30), MessageId(6)).as_deref(),
        Some("https://chat.test/channels/@me/30/6")
    );
    let store = with(Capabilities {
        channel_links: None,
        ..all()
    });
    assert_eq!(store.message_link(GENERAL, MessageId(5)), None);
}

#[test]
fn only_its_author_ends_an_open_poll() {
    let mut store = with(all());
    let poll = |author, expiry| {
        let mut found = message(7, author);
        found.poll = Some(rune_model::Poll {
            question: "¿?".into(),
            answers: Vec::new(),
            expiry: Some(Timestamp(expiry)),
            ..Default::default()
        });
        found
    };
    history(&mut store, None, 1..8, true);
    store.apply(Event::MessageUpdated(poll(ME, 1_000)), Instant::now());
    assert!(store
        .end_poll(GENERAL, MessageId(7), Timestamp(10))
        .allowed());
    assert_eq!(
        store
            .end_poll(GENERAL, MessageId(7), Timestamp(2_000))
            .reason(),
        Some("La encuesta ya terminó")
    );
    store.apply(Event::MessageUpdated(poll(ADA, 1_000)), Instant::now());
    assert!(!store
        .end_poll(GENERAL, MessageId(7), Timestamp(10))
        .allowed());
}

#[test]
fn polls_ask_for_their_permission_only_in_servers() {
    let mut store = with(all());
    store.apply(Event::ChannelChanged(direct()), Instant::now());
    let general = store.abilities(GENERAL).unwrap();
    assert_eq!(
        general.create_poll.reason(),
        Some("No tienes permiso para crear encuestas en #general")
    );
    assert!(general.forward.allowed() && general.reaction_list.allowed());
    assert!(store
        .abilities(ChannelId(30))
        .unwrap()
        .create_poll
        .allowed());
    let mut open = channel(GENERAL, ChannelKind::Text, "general");
    open.permissions = open.permissions | Permissions::SEND_POLLS;
    store.apply(Event::ChannelChanged(open), Instant::now());
    assert!(store.abilities(GENERAL).unwrap().create_poll.allowed());
    let store = with(Capabilities {
        create_polls: false,
        ..all()
    });
    assert!(!store.abilities(GENERAL).unwrap().create_poll.allowed());
}

#[test]
fn forwarding_lists_where_the_account_writes_by_name() {
    let mut store = with(all());
    store.apply(Event::ChannelChanged(direct()), Instant::now());
    let all = store.forward_targets("", 16);
    assert!(all.contains(&GENERAL) && all.contains(&ChannelId(30)));
    assert!(!all.contains(&VOICE), "a voice channel without its chat");
    assert_eq!(store.forward_targets("ADA", 16), vec![ChannelId(30)]);
    assert_eq!(store.forward_targets("gene", 16), vec![GENERAL]);
    assert_eq!(store.forward_targets("", 1).len(), 1);
}

fn owned_with_rooms() -> Store {
    let mut store = with(all());
    let mut server = store.server(ServerId(1)).unwrap().clone();
    server.owner = Some(ME);
    store.apply(Event::ServerChanged(server), Instant::now());
    let mut room = channel(VOICE, ChannelKind::Voice, "sala");
    room.permissions = Permissions::ALL;
    store.apply(Event::ChannelChanged(room.clone()), Instant::now());
    room.id = ChannelId(12);
    room.name = "otra sala".into();
    store.apply(Event::ChannelChanged(room), Instant::now());
    store.apply(
        Event::VoiceChanged(rune_model::VoiceState {
            user: ADA,
            channel: VOICE,
            muted: false,
            deafened: false,
            speaking: false,
            streaming: false,
            server_muted: false,
            server_deafened: false,
            camera: false,
            suppressed: false,
            speak_request: None,
        }),
        Instant::now(),
    );
    store
}

#[test]
fn the_owner_moderates_voice_and_moves_to_the_other_rooms() {
    let store = owned_with_rooms();
    let can = store.voice_moderation(ServerId(1), ADA).unwrap();
    assert_eq!(can.channel, VOICE);
    assert!(can.mute.allowed() && can.deafen.allowed() && can.disconnect.allowed());
    assert!(can.move_to.allowed());
    assert_eq!(can.rooms, vec![ChannelId(12)]);
    assert!(store.voice_moderation(ServerId(1), ME).is_none());
    assert!(store.voice_moderation(ServerId(1), UserId(77)).is_none());
}

#[test]
fn voice_moderation_needs_the_permission_and_a_higher_rank() {
    let mut store = owned_with_rooms();
    let mut server = store.server(ServerId(1)).unwrap().clone();
    server.owner = Some(UserId(99));
    server.roles = vec![
        rune_model::Role::everyone(ServerId(1), Permissions::EVERYONE),
        rune_model::Role {
            id: rune_model::RoleId(5),
            name: "Moderación".into(),
            position: 2,
            permissions: Permissions::MUTE_MEMBERS,
            ..rune_model::Role::everyone(ServerId(1), Permissions::NONE)
        },
    ];
    store.apply(Event::ServerChanged(server), Instant::now());
    let role = |user, roles: Vec<rune_model::RoleId>| Member {
        user,
        server: ServerId(1),
        roles,
        ..Default::default()
    };
    store.apply(
        Event::MemberChanged(role(ME, vec![rune_model::RoleId(5)])),
        Instant::now(),
    );
    let can = store.voice_moderation(ServerId(1), ADA).unwrap();
    assert!(can.mute.allowed());
    assert!(!can.deafen.allowed() && !can.move_to.allowed() && !can.disconnect.allowed());
    // As in Discord, a role at or above the account's does not stop voice moderation.
    store.apply(
        Event::MemberChanged(role(ADA, vec![rune_model::RoleId(5)])),
        Instant::now(),
    );
    let can = store.voice_moderation(ServerId(1), ADA).unwrap();
    assert!(can.mute.allowed());
    assert!(!can.deafen.allowed() && !can.move_to.allowed());
}

#[test]
fn leaving_needs_no_management_of_servers() {
    let store = with(Capabilities {
        manage_servers: false,
        ..all()
    });
    assert!(store.server_abilities(ServerId(1)).unwrap().leave.allowed());
    let store = with(Capabilities {
        leave_servers: false,
        manage_servers: true,
        ..all()
    });
    assert!(!store.server_abilities(ServerId(1)).unwrap().leave.allowed());
}

#[test]
fn people_and_conversations_pass_to_whoever_asked() {
    let mut store = with(all());
    assert_eq!(
        store.apply(
            Event::PeopleLoaded {
                request: 4,
                people: vec![ADA],
                reached_end: true,
            },
            Instant::now(),
        ),
        Applied::People {
            request: 4,
            people: vec![ADA],
            reached_end: true,
        }
    );
    assert_eq!(
        store.apply(
            Event::ChannelLoaded {
                request: 5,
                channel: GENERAL,
            },
            Instant::now(),
        ),
        Applied::ChannelLoaded {
            request: 5,
            channel: GENERAL,
        }
    );
}
