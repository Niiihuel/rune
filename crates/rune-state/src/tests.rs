use super::*;
use rune_provider::Command;
mod context;
// Commands and interactions (P3).
mod interactions;
// Presence, identity and expressions (P5).
mod channel_index;
mod identity;
mod message_patches;
mod profile_edit;
mod profiles;
mod reads;
mod relationships;
mod remote;
mod tallies;
mod threads;
mod uploads;
// Message actions and moderation (P4).
mod message_actions;
// Private calls.
mod private_calls;
// Message content (contenido-de-mensajes).
mod content_filter;
// Voice protocol (voz-protocolo).
mod voice_started;
// Rules and onboarding (2026-10-18).
mod onboarding;
// Scheduled events (eventos-programados).
mod events;
use rune_model::{CallQuality, MessageKind, Permissions, Rgb, Status};

const ME: UserId = UserId(1);
const ADA: UserId = UserId(2);
const GENERAL: ChannelId = ChannelId(10);
const VOICE: ChannelId = ChannelId(11);

#[test]
fn invitation_acknowledgements_never_create_or_remove_membership_or_permissions() {
    let now = Instant::now();
    let mut store = Store::new();
    store.apply(Event::Ready(Box::new(snapshot())), now);
    let original_servers = store.servers.clone();
    let original_channels = store.channels.clone();
    let original_members = store.members.clone();
    let original_relationships = store.relationships.clone();
    let unknown = ServerId(999);
    let to = rune_provider::InvitationDestination::Server;
    for (request, destination, result) in [
        (10, to(unknown), Ok(())),
        (11, to(unknown), Err(InvitationJoinError::Ambiguous)),
        (12, to(ServerId(1)), Err(InvitationJoinError::Denied)),
        (
            13,
            to(unknown),
            Err(InvitationJoinError::VerificationRequired),
        ),
        (
            14,
            rune_provider::InvitationDestination::Group(ChannelId(998)),
            Ok(()),
        ),
        (
            15,
            rune_provider::InvitationDestination::Friend(UserId(997)),
            Ok(()),
        ),
    ] {
        let applied = store.apply(
            Event::InvitationAcceptance {
                request,
                destination,
                result,
            },
            now,
        );
        assert_eq!(
            applied,
            Applied::InvitationAcceptance {
                request,
                destination,
                result
            }
        );
        assert_eq!(store.servers, original_servers);
        assert_eq!(store.channels, original_channels);
        assert_eq!(store.members, original_members);
        assert_eq!(store.relationships, original_relationships);
        assert!(store.server(unknown).is_none());
        assert_eq!(store.me(), Some(ME));
    }
}

#[test]
fn ending_authorization_removes_the_previous_accounts_domain_state() {
    for authentication in [
        AuthenticationState::SignedOut,
        AuthenticationState::Failed {
            reason: "La autorización venció.".into(),
        },
    ] {
        let now = Instant::now();
        let mut store = Store::new();
        let provider = rune_provider::ProviderInfo {
            name: "Proveedor autorizado de prueba".into(),
            kind: rune_provider::ProviderKind::Official,
            capabilities: rune_provider::Capabilities {
                authentication: true,
                ..Default::default()
            },
            notice: "Identidad limitada".into(),
        };
        store.provider = Some(provider.clone());
        store.apply(Event::Ready(Box::new(snapshot())), now);
        store.apply(
            Event::MessageCreated {
                message: message(42, ME),
                nonce: None,
            },
            now,
        );
        let revision = store.revision();
        let applied = store.apply(Event::Authentication(authentication.clone()), now);
        assert_eq!(applied, Applied::Authentication);
        assert_eq!(store.authentication, authentication);
        assert_eq!(store.provider, Some(provider));
        assert_eq!(store.me(), None);
        assert_eq!(store.users().count(), 0);
        assert!(store.servers().is_empty());
        assert!(store.channel(GENERAL).is_none());
        assert!(store.timeline(GENERAL).is_none());
        assert!(store.call().is_none());
        assert!(!store.from_cache);
        assert!(store.revision() > revision);

        let other = user(UserId(900), "otra cuenta");
        store.apply(
            Event::Ready(Box::new(Snapshot {
                me: Some(other.clone()),
                users: vec![other.clone()],
                ..Default::default()
            })),
            now,
        );
        assert_eq!(store.me(), Some(other.id));
        assert!(store.user(ME).is_none());
        assert!(store.timeline(GENERAL).is_none());
    }
}

fn message(id: u64, author: UserId) -> Message {
    Message {
        id: MessageId(id),
        channel: GENERAL,
        author,
        content: format!("mensaje {id}"),
        timestamp: Timestamp(id as i64 * 60_000),
        edited: None,
        kind: MessageKind::Default,
        reply_to: None,
        mentions: Vec::new(),
        reactions: Vec::new(),
        attachments: Vec::new(),
        stickers: Vec::new(),
        text_references: Vec::new(),
        delivery: Delivery::Sent,
        pinned: false,
        embeds: Vec::new(),
        forwarded: None,
        poll: None,
        components: Default::default(),
        call: None,
        mention_roles: Vec::new(),
        mention_everyone: false,
    }
}

fn user(id: UserId, name: &str) -> User {
    User {
        identity: Default::default(),
        avatar: None,
        banner: Default::default(),
        id,
        name: name.into(),
        display_name: None,
        accent: Rgb(0),
        bot: false,
        pronouns: None,
        about: None,
        default_avatar: None,
    }
}

fn channel(id: ChannelId, kind: ChannelKind, name: &str) -> Channel {
    Channel {
        id,
        server: Some(ServerId(1)),
        kind,
        name: name.into(),
        topic: None,
        topic_pictures: Vec::new(),
        parent: None,
        position: 0,
        recipients: Vec::new(),
        owner: None,
        permissions: Permissions::MEMBER | Permissions::READ_HISTORY,
        overwrites: Vec::new(),
        slowmode_secs: 0,
        nsfw: false,
        icon: None,
        thread: None,
        tags: Vec::new(),
        forum: Default::default(),
    }
}

fn snapshot() -> Snapshot {
    Snapshot {
        me: Some(user(ME, "yo")),
        users: vec![user(ME, "yo"), user(ADA, "ada")],
        servers: vec![Server {
            stickers: Vec::new(),
            emojis: Vec::new(),
            icon: None,
            id: ServerId(1),
            name: "S".into(),
            accent: Rgb(0),
            roles: Vec::new(),
            owner: Some(ADA),
            description: None,
            options: Default::default(),
        }],
        channels: vec![
            channel(GENERAL, ChannelKind::Text, "general"),
            channel(VOICE, ChannelKind::Voice, "sala"),
        ],
        members: vec![
            Member {
                user: ME,
                server: ServerId(1),
                nick: None,
                roles: Vec::new(),
                ..Default::default()
            },
            Member {
                user: ADA,
                server: ServerId(1),
                nick: Some("Ada L.".into()),
                roles: Vec::new(),
                ..Default::default()
            },
        ],
        presences: vec![(
            ADA,
            Presence {
                status: Status::Online,
                custom: None,
                custom_emoji: None,
                activities: Vec::new(),
                platforms: Default::default(),
            },
        )],
        voice: Vec::new(),
        last_messages: vec![(GENERAL, MessageId(5))],
        read_states: vec![(GENERAL, Some(MessageId(3)), 1)],
        read_versions: Vec::new(),
        first_unread: Vec::new(),
        relationships: None,
    }
}

fn ready() -> Store {
    let mut store = Store::new();
    store.apply(Event::Ready(Box::new(snapshot())), Instant::now());
    store
}

fn history(store: &mut Store, before: Option<u64>, ids: std::ops::Range<u64>, reached_start: bool) {
    store.apply(
        Event::HistoryLoaded {
            request: 1,
            channel: GENERAL,
            before: before.map(MessageId),
            messages: ids.map(|id| message(id, ADA)).collect(),
            reached_start,
        },
        Instant::now(),
    );
}

fn ids(store: &Store) -> Vec<u64> {
    store
        .timeline(GENERAL)
        .unwrap()
        .messages()
        .iter()
        .map(|message| message.id.0)
        .collect()
}

fn delivery(store: &Store, id: MessageId) -> Delivery {
    store
        .timeline(GENERAL)
        .unwrap()
        .get(id)
        .unwrap()
        .delivery
        .clone()
}

#[test]
fn pages_of_history_join_into_one_ordered_timeline() {
    let mut store = ready();
    history(&mut store, None, 50..100, false);
    history(&mut store, Some(50), 0..50, true);
    history(&mut store, None, 90..101, false);
    assert_eq!(ids(&store), (0..101).collect::<Vec<_>>());
    let timeline = store.timeline(GENERAL).unwrap();
    assert!(timeline.reached_start && timeline.latest);
}

#[test]
fn a_sent_message_shows_at_once_and_its_confirmation_replaces_it() {
    let mut store = ready();
    history(&mut store, None, 1..6, true);
    store.add_pending(GENERAL, "hola".into(), None, 7, Timestamp(0));
    assert_eq!(ids(&store).last(), Some(&(u64::MAX - 7)));
    assert_eq!(delivery(&store, MessageId(u64::MAX - 7)), Delivery::Sending);
    let confirmed = Message {
        content: "hola".into(),
        ..message(6, ME)
    };
    store.apply(
        Event::MessageCreated {
            message: confirmed,
            nonce: Some(7),
        },
        Instant::now(),
    );
    assert_eq!(ids(&store), vec![1, 2, 3, 4, 5, 6]);
    assert!(
        store.read(GENERAL).unread(),
        "sending alone does not prove the conversation was visible"
    );
}

#[test]
fn a_refused_send_can_be_retried_and_a_lost_one_says_its_fate_is_unknown() {
    let mut store = ready();
    history(&mut store, None, 1..6, true);
    store.add_pending(GENERAL, "uno".into(), None, 7, Timestamp(0));
    let failure = Failure::Send {
        channel: GENERAL,
        nonce: 7,
    };
    store.apply(
        Event::Failed {
            failure: failure.clone(),
            reason: "rechazado".into(),
            ambiguous: false,
        },
        Instant::now(),
    );
    let shown = MessageId(u64::MAX - 7);
    assert_eq!(
        delivery(&store, shown),
        Delivery::Failed {
            reason: "rechazado".into()
        }
    );
    let unsent = store.unsent(GENERAL, shown).unwrap();
    assert_eq!(
        (unsent.nonce, unsent.content.as_str(), unsent.ambiguous),
        (7, "uno", false)
    );
    // Retried under a new nonce: shown as sending, and its confirmation lands.
    store.resend(GENERAL, shown, 8);
    assert_eq!(delivery(&store, MessageId(u64::MAX - 8)), Delivery::Sending);
    assert!(store.timeline(GENERAL).unwrap().get(shown).is_none());
    store.apply(
        Event::MessageCreated {
            message: message(6, ME),
            nonce: Some(8),
        },
        Instant::now(),
    );
    assert_eq!(ids(&store), vec![1, 2, 3, 4, 5, 6]);
    // A send that got no answer.
    store.add_pending(GENERAL, "dos".into(), None, 9, Timestamp(0));
    let failure = Failure::Send {
        channel: GENERAL,
        nonce: 9,
    };
    store.apply(
        Event::Failed {
            failure,
            reason: "sin respuesta".into(),
            ambiguous: true,
        },
        Instant::now(),
    );
    assert!(
        store
            .unsent(GENERAL, MessageId(u64::MAX - 9))
            .unwrap()
            .ambiguous
    );
    assert_eq!(
        store.discard(GENERAL, MessageId(u64::MAX - 9)).as_deref(),
        Some("dos")
    );
    assert_eq!(ids(&store), vec![1, 2, 3, 4, 5, 6]);
}

#[test]
fn a_refused_edit_or_reaction_is_put_back() {
    let mut store = ready();
    history(&mut store, None, 1..6, true);
    store.update_local(GENERAL, MessageId(3), |message| {
        message.toggle_reaction("🦀")
    });
    store.update_local(GENERAL, MessageId(3), |message| {
        message.content = "editado".into()
    });
    let failure = Failure::React {
        channel: GENERAL,
        message: MessageId(3),
        emoji: "🦀".into(),
    };
    store.apply(
        Event::Failed {
            failure,
            reason: "no".into(),
            ambiguous: false,
        },
        Instant::now(),
    );
    let restored = store
        .timeline(GENERAL)
        .unwrap()
        .get(MessageId(3))
        .unwrap()
        .clone();
    assert!(
        restored.reactions.is_empty() && restored.content == "mensaje 3",
        "back to how it was before both"
    );
    // A confirmation keeps the change.
    store.update_local(GENERAL, MessageId(4), |message| {
        message.content = "nuevo".into()
    });
    let confirmed = Message {
        content: "nuevo".into(),
        ..message(4, ADA)
    };
    store.apply(Event::MessageUpdated(confirmed), Instant::now());
    let failure = Failure::Edit {
        channel: GENERAL,
        message: MessageId(4),
    };
    store.apply(
        Event::Failed {
            failure,
            reason: "tarde".into(),
            ambiguous: false,
        },
        Instant::now(),
    );
    assert_eq!(
        store
            .timeline(GENERAL)
            .unwrap()
            .get(MessageId(4))
            .unwrap()
            .content,
        "nuevo"
    );
}

#[test]
fn a_snapshot_held_by_the_interface_does_not_change_under_it() {
    let mut store = ready();
    history(&mut store, None, 1..6, true);
    let held = store.timeline(GENERAL).unwrap().messages();
    store.apply(
        Event::MessageCreated {
            message: message(6, ADA),
            nonce: None,
        },
        Instant::now(),
    );
    assert_eq!(held.len(), 5);
    assert_eq!(store.timeline(GENERAL).unwrap().len(), 6);
    // The messages themselves are shared, not copied.
    assert!(Arc::ptr_eq(
        &held[0],
        &store.timeline(GENERAL).unwrap().messages()[0]
    ));
}

#[test]
fn read_marker_commands_require_explicit_provider_support_but_always_update_local_state() {
    for supported in [false, true] {
        let mut store = ready();
        store.provider = Some(rune_provider::ProviderInfo {
            name: "Fixture".into(),
            kind: rune_provider::ProviderKind::Simulated,
            capabilities: rune_provider::Capabilities {
                read_state: supported,
                ..Default::default()
            },
            notice: "Datos simulados".into(),
        });
        assert_eq!(store.read_sync(), supported);
        assert!(
            matches!(
                store.mark_read_command(GENERAL),
                Some(Command::MarkRead {
                    account: ME,
                    channel: GENERAL,
                    message: MessageId(5),
                    ..
                })
            ) == supported
        );
        assert!(!store.read(GENERAL).unread());
        assert_eq!(store.read(GENERAL).mentions, 0);
        assert_eq!(store.mark_read_command(GENERAL), None);

        assert_eq!(store.mark_unread_command(GENERAL, MessageId(4)), None);
        assert_eq!(store.read(GENERAL).last_read, Some(MessageId(5)));
        assert_eq!(store.read(GENERAL).unread_from, Some(MessageId(4)));
        assert_eq!(store.read(GENERAL).last_message, Some(MessageId(5)));
        assert!(store.read(GENERAL).unread());
    }

    let mut store = ready();
    assert!(!store.read_sync());
    assert_eq!(store.mark_read_command(GENERAL), None);
    assert!(!store.read(GENERAL).unread());
}

#[test]
fn unread_and_mentions_count_until_read() {
    let mut store = ready();
    assert!(store.read(GENERAL).unread());
    assert_eq!(store.server_unread(ServerId(1)), (true, 1));
    let mention = Message {
        mentions: vec![ME],
        ..message(6, ADA)
    };
    store.apply(
        Event::MessageCreated {
            message: mention,
            nonce: None,
        },
        Instant::now(),
    );
    assert_eq!(store.read(GENERAL).mentions, 2);
    assert_eq!(store.mark_read(GENERAL), Some(MessageId(6)));
    assert_eq!(store.server_unread(ServerId(1)), (false, 0));
    assert_eq!(store.mark_read(GENERAL), None, "nothing new to report");
    // A manual reminder names the selected message without inventing a cursor.
    assert_eq!(store.mark_unread(GENERAL, MessageId(6)), MessageId(6));
    assert!(store.read(GENERAL).unread());
    assert_eq!(store.mark_read(GENERAL), Some(MessageId(6)));
}

#[test]
fn a_message_for_a_conversation_not_loaded_to_its_end_waits_for_it() {
    let mut store = ready();
    let applied = store.apply(
        Event::MessageCreated {
            message: message(9, ADA),
            nonce: None,
        },
        Instant::now(),
    );
    assert_eq!(
        applied,
        Applied::Messages {
            channel: GENERAL,
            added: 0
        }
    );
    assert!(store.timeline(GENERAL).unwrap().is_empty());
    assert!(store.read(GENERAL).unread());
}

#[test]
fn typing_shows_for_a_while_ends_with_the_message_and_is_bounded() {
    let mut store = ready();
    history(&mut store, None, 1..6, true);
    let start = Instant::now();
    store.apply(
        Event::Typing {
            channel: GENERAL,
            user: ADA,
        },
        start,
    );
    assert_eq!(store.typing(GENERAL, start), vec![ADA]);
    assert_eq!(
        store.typing(GENERAL, start + TYPING_SHOWN),
        Vec::<UserId>::new()
    );
    store.apply(
        Event::MessageCreated {
            message: message(6, ADA),
            nonce: None,
        },
        start,
    );
    assert!(store.typing(GENERAL, start).is_empty());
    for someone in 100..120 {
        store.apply(
            Event::Typing {
                channel: GENERAL,
                user: UserId(someone),
            },
            start,
        );
    }
    assert_eq!(store.typing(GENERAL, start).len(), TYPING_SLOTS);
}

#[test]
fn trimming_bounds_every_conversation_but_the_open_one() {
    let mut store = ready();
    history(&mut store, None, 0..500, true);
    store.trim(None, 100);
    let timeline = store.timeline(GENERAL).unwrap();
    assert_eq!(timeline.len(), 100);
    assert_eq!(timeline.oldest(), Some(MessageId(400)));
    assert!(
        !timeline.reached_start,
        "what was dropped can be loaded again"
    );
}

#[test]
fn names_follow_the_nick_in_the_server_and_invisible_reads_as_offline() {
    let mut store = ready();
    assert_eq!(store.display_name(ADA, Some(ServerId(1))), "Ada L.");
    assert_eq!(store.display_name(ADA, None), "ada");
    let groups = store.member_groups(ServerId(1));
    assert_eq!(groups[0].title, "En línea");
    assert_eq!(groups[0].members, vec![ADA]);
    assert_eq!(groups[1].members, vec![ME], "no presence reads as offline");
    let invisible = Presence {
        status: Status::Invisible,
        custom: None,
        custom_emoji: None,
        activities: Vec::new(),
        platforms: Default::default(),
    };
    store.apply(
        Event::PresenceChanged {
            user: ADA,
            presence: invisible,
        },
        Instant::now(),
    );
    assert_eq!(
        store.member_groups(ServerId(1)).len(),
        1,
        "everyone is in the offline group"
    );
}

#[test]
fn a_call_goes_from_joining_to_connected_and_through_a_reconnection() {
    let mut store = ready();
    let start = Instant::now();
    store.begin_call(VOICE, true, false);
    assert_eq!(store.call().unwrap().phase, CallPhase::Joining);
    let quality = Some(CallQuality {
        rtt_ms: 40,
        loss_permille: 3,
    });
    store.apply(
        Event::CallPhase {
            channel: VOICE,
            phase: CallPhase::Connected,
            quality,
        },
        start,
    );
    let call = store.call().unwrap();
    assert_eq!(
        (call.phase.clone(), call.muted, call.quality),
        (CallPhase::Connected, true, quality)
    );
    assert_eq!(
        store.call_duration(start + Duration::from_secs(3)),
        Some(Duration::from_secs(3))
    );
    store.apply(
        Event::Connection(ConnectionState::Reconnecting {
            attempt: 2,
            retry_in_ms: 1_000,
        }),
        start,
    );
    assert_eq!(
        store.call().unwrap().phase,
        CallPhase::Reconnecting { attempt: 2 }
    );
    // The service is back and the call never changed: as it was, not
    // «reconnecting» until its next change.
    store.apply(Event::Connection(ConnectionState::Connected), start);
    assert_eq!(store.call().unwrap().phase, CallPhase::Connected);
    store.apply(
        Event::Connection(ConnectionState::Reconnecting {
            attempt: 1,
            retry_in_ms: 1_000,
        }),
        start,
    );
    let failure = Failure::Voice {
        channel: Some(VOICE),
    };
    store.apply(
        Event::Failed {
            failure,
            reason: "sin servidor".into(),
            ambiguous: false,
        },
        start,
    );
    assert_eq!(
        store.call().unwrap().phase,
        CallPhase::Failed {
            reason: "sin servidor".into()
        }
    );
    store.apply(
        Event::CallEnded {
            channel: VOICE,
            reason: None,
        },
        start,
    );
    assert!(store.call().is_none());
}

#[test]
fn speaking_marks_the_person_in_the_channel() {
    let mut store = ready();
    let state = VoiceState {
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
    };
    store.apply(Event::VoiceChanged(state), Instant::now());
    store.apply(
        Event::Speaking {
            channel: VOICE,
            user: ADA,
            speaking: true,
        },
        Instant::now(),
    );
    assert!(store.voice_in(VOICE)[0].speaking);
}

#[test]
fn voice_by_server_follows_moves_leaves_other_servers_and_the_own_call() {
    let mut store = ready();
    let other = ChannelId(21);
    store.apply(
        Event::ChannelChanged(Channel {
            server: Some(ServerId(2)),
            ..channel(other, ChannelKind::Voice, "otra")
        }),
        Instant::now(),
    );
    let state = |user, channel, streaming| VoiceState {
        user,
        channel,
        muted: false,
        deafened: false,
        speaking: false,
        streaming,
        server_muted: false,
        server_deafened: false,
        camera: false,
        suppressed: false,
        speak_request: None,
    };
    let people = |store: &Store, server| -> Vec<(UserId, bool)> {
        store
            .voice_by_server()
            .get(&ServerId(server))
            .map(|voice| voice.people.iter().map(|s| (s.user, s.streaming)).collect())
            .unwrap_or_default()
    };
    store.apply(Event::VoiceChanged(state(ADA, VOICE, true)), Instant::now());
    store.apply(
        Event::VoiceChanged(state(UserId(3), other, false)),
        Instant::now(),
    );
    assert_eq!(people(&store, 1), [(ADA, true)]);
    assert_eq!(people(&store, 2), [(UserId(3), false)]);
    assert!(!store.voice_by_server()[&ServerId(1)].own);
    // Ada moves to the other server and stops streaming; 3 leaves.
    store.apply(
        Event::VoiceChanged(state(ADA, other, false)),
        Instant::now(),
    );
    store.apply(
        Event::VoiceLeft {
            user: UserId(3),
            channel: other,
        },
        Instant::now(),
    );
    assert_eq!(people(&store, 1), []);
    assert_eq!(people(&store, 2), [(ADA, false)]);
    // The account's call marks its server while joining, before its state.
    store.begin_call(VOICE, false, false);
    let voice = store.voice_by_server();
    assert!(voice[&ServerId(1)].own && voice[&ServerId(1)].people.is_empty());
    assert!(!voice[&ServerId(2)].own);
    store.apply(Event::VoiceChanged(state(ME, VOICE, false)), Instant::now());
    store.end_call();
    assert!(
        store.voice_by_server()[&ServerId(1)].own,
        "its state says so too"
    );
    // A server that goes takes its voice; a new session starts from its own.
    store.apply(
        Event::ServerRemoved {
            server: ServerId(1),
            reason: None,
        },
        Instant::now(),
    );
    assert!(!store.voice_by_server().contains_key(&ServerId(1)));
    store.apply(Event::Ready(Box::new(snapshot())), Instant::now());
    assert!(store.voice_by_server().is_empty());
}

#[test]
fn cached_messages_are_shown_before_the_newest_page_arrives() {
    let mut store = ready();
    assert_eq!(
        store.apply_cached(GENERAL, vec![message(1, ADA)]),
        Applied::Messages {
            channel: GENERAL,
            added: 1,
        }
    );
    assert_eq!(ids(&store), vec![1]);
    assert!(!store.timeline(GENERAL).unwrap().latest);
    history(&mut store, None, 0..0, true);
    assert!(ids(&store).is_empty());
}

#[test]
fn late_cached_messages_do_not_replace_an_authoritative_empty_page() {
    let mut store = ready();
    history(&mut store, None, 0..0, true);
    let revision = store.revision();
    assert_eq!(
        store.apply_cached(GENERAL, vec![message(1, ADA)]),
        Applied::Nothing
    );
    assert!(ids(&store).is_empty());
    assert_eq!(store.revision(), revision);
}

#[test]
fn a_cached_snapshot_shows_the_servers_but_nobody_around_until_ready() {
    let mut store = Store::new();
    store.apply_cached_snapshot(snapshot());
    assert!(store.from_cache && store.connection == ConnectionState::Connecting);
    assert_eq!(store.servers().len(), 1);
    assert_eq!(
        store.presence(ADA).status,
        Status::Offline,
        "presence is not known yet"
    );
    store.apply(Event::Ready(Box::new(snapshot())), Instant::now());
    assert!(!store.from_cache);
    assert_eq!(store.presence(ADA).status, Status::Online);
    assert_eq!(
        store.apply_cached_snapshot(snapshot()),
        Applied::Nothing,
        "never over a live session"
    );
}

#[test]
fn abilities_join_the_provider_and_the_channel() {
    let mut store = ready();
    assert!(
        store.abilities(GENERAL).unwrap().write.reason().is_some(),
        "no provider, nothing allowed"
    );
    store.provider = Some(ProviderInfo {
        name: "p".into(),
        kind: rune_provider::ProviderKind::Simulated,
        capabilities: rune_provider::Capabilities {
            send_messages: true,
            ..Default::default()
        },
        notice: String::new(),
    });
    assert!(store.abilities(GENERAL).unwrap().write.allowed());
    assert!(store.abilities(ChannelId(999)).is_none());
}

#[test]
fn servers_channels_and_members_change_and_go() {
    let mut store = ready();
    let now = Instant::now();
    let mut renamed = store.server(ServerId(1)).unwrap().clone();
    renamed.name = "Nuevo nombre".into();
    assert_eq!(
        store.apply(Event::ServerChanged(renamed), now),
        Applied::Server(ServerId(1))
    );
    assert_eq!(store.server(ServerId(1)).unwrap().name, "Nuevo nombre");

    let mut hidden = channel(ChannelId(30), ChannelKind::Text, "secreto");
    hidden.permissions = Permissions::NONE;
    store.apply(Event::ChannelChanged(hidden), now);
    let listed: Vec<ChannelId> = store
        .channel_tree(ServerId(1))
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .map(|channel| channel.id)
        .collect();
    assert!(
        !listed.contains(&ChannelId(30)),
        "a channel it cannot view is left out"
    );

    store.apply(
        Event::MemberChanged(Member {
            user: ADA,
            server: ServerId(1),
            nick: Some("Ada".into()),
            roles: Vec::new(),
            ..Default::default()
        }),
        now,
    );
    assert_eq!(store.display_name(ADA, Some(ServerId(1))), "Ada");
    store.apply(
        Event::MemberRemoved {
            server: ServerId(1),
            user: ADA,
        },
        now,
    );
    assert!(store.member_of(ServerId(1), ADA).is_none());
    assert!(
        store.member_of(ServerId(1), ME).is_some(),
        "the index stays right"
    );

    store.apply(Event::ChannelRemoved { channel: GENERAL }, now);
    assert!(store.channel(GENERAL).is_none() && store.timeline(GENERAL).is_none());
    store.apply(
        Event::ServerRemoved {
            server: ServerId(1),
            reason: None,
        },
        now,
    );
    assert!(store.server(ServerId(1)).is_none() && store.channel(VOICE).is_none());
}

#[test]
fn server_abilities_follow_roles_and_ownership() {
    let mut store = ready();
    store.provider = Some(ProviderInfo {
        name: "p".into(),
        kind: rune_provider::ProviderKind::Simulated,
        capabilities: rune_provider::Capabilities {
            manage_servers: true,
            manage_channels: true,
            moderation: true,
            leave_servers: true,
            ..Default::default()
        },
        notice: String::new(),
    });
    let abilities = store.server_abilities(ServerId(1)).unwrap();
    assert!(!abilities.manage_roles.allowed(), "a plain member");
    assert!(abilities.leave.allowed() && !abilities.delete.allowed());
    let mut server = store.server(ServerId(1)).unwrap().clone();
    server.owner = Some(ME);
    store.apply(Event::ServerChanged(server), Instant::now());
    let abilities = store.server_abilities(ServerId(1)).unwrap();
    assert!(abilities.manage_roles.allowed() && abilities.delete.allowed());
    assert!(!abilities.leave.allowed());
    assert_eq!(store.my_rank(ServerId(1)), i32::MAX);
}

fn pins_ready() -> Store {
    let mut store = ready();
    store.provider = Some(ProviderInfo {
        name: "Fixture de fijados".into(),
        kind: rune_provider::ProviderKind::Simulated,
        capabilities: rune_provider::Capabilities {
            pins: true,
            ..Default::default()
        },
        notice: String::new(),
    });
    let mut channel = store.channel(GENERAL).unwrap().clone();
    channel.permissions = channel.permissions | Permissions::READ_HISTORY;
    store.apply(Event::ChannelChanged(channel), Instant::now());
    store
}

#[test]
fn pins_snapshot_does_not_resurrect_a_deleted_message() {
    let mut store = pins_ready();
    let now = Instant::now();
    let pinned = Message {
        pinned: true,
        ..message(4, ADA)
    };
    assert!(store.begin_pins(GENERAL, 1).allowed());
    store.apply(
        Event::MessageDeleted {
            channel: GENERAL,
            message: pinned.id,
        },
        now,
    );
    store.apply(
        Event::PinsLoaded {
            request: 1,
            channel: GENERAL,
            messages: vec![pinned],
        },
        now,
    );
    assert!(
        store.pins(GENERAL).unwrap().is_empty(),
        "a fetched snapshot cannot undo deletion"
    );
}

#[test]
fn pins_snapshot_does_not_undo_an_unpin_observed_during_the_load() {
    let mut store = pins_ready();
    let now = Instant::now();
    let pinned = Message {
        pinned: true,
        ..message(4, ADA)
    };
    assert!(store.begin_pins(GENERAL, 1).allowed());
    store.apply(Event::MessageUpdated(message(4, ADA)), now);
    store.apply(
        Event::PinsLoaded {
            request: 1,
            channel: GENERAL,
            messages: vec![pinned],
        },
        now,
    );
    assert!(
        store.pins(GENERAL).unwrap().is_empty(),
        "a fetched snapshot cannot undo unpinning"
    );
}

#[test]
fn pins_baseline_is_provisional_and_live_pin_unpin_pin_preserves_newer_content() {
    let mut store = pins_ready();
    let now = Instant::now();
    assert!(store.begin_pins(GENERAL, 1).allowed());
    let mut current = Message {
        pinned: true,
        ..message(4, ADA)
    };
    current.content = "edición reciente".into();
    current.edited = Some(Timestamp(200));
    store.apply(Event::MessageUpdated(current.clone()), now);
    assert!(
        store.pins(GENERAL).is_none(),
        "live data is not a completed snapshot"
    );
    store.apply(
        Event::MessageUpdated(Message {
            pinned: false,
            ..current.clone()
        }),
        now,
    );
    store.apply(Event::MessageUpdated(current.clone()), now);
    store.apply(
        Event::MessageUpdated(Message {
            content: "edición vieja".into(),
            edited: Some(Timestamp(100)),
            ..current.clone()
        }),
        now,
    );
    let applied = store.apply(
        Event::PinsLoaded {
            request: 1,
            channel: GENERAL,
            messages: Vec::new(),
        },
        now,
    );
    assert_eq!(
        applied,
        Applied::Pins {
            request: 1,
            channel: GENERAL
        }
    );
    assert_eq!(store.pins(GENERAL).unwrap(), &[current]);
}

#[test]
fn pins_delete_is_terminal_before_and_during_a_read() {
    let mut store = pins_ready();
    let now = Instant::now();
    store.apply(
        Event::MessageDeleted {
            channel: GENERAL,
            message: MessageId(3),
        },
        now,
    );
    assert!(store.begin_pins(GENERAL, 1).allowed());
    store.apply(
        Event::MessageDeleted {
            channel: GENERAL,
            message: MessageId(4),
        },
        now,
    );
    store.apply(
        Event::MessageUpdated(Message {
            pinned: true,
            ..message(4, ADA)
        }),
        now,
    );
    store.apply(
        Event::PinsLoaded {
            request: 1,
            channel: GENERAL,
            messages: vec![
                Message {
                    pinned: true,
                    ..message(3, ADA)
                },
                Message {
                    pinned: true,
                    ..message(4, ADA)
                },
            ],
        },
        now,
    );
    assert!(store.pins(GENERAL).unwrap().is_empty());
}

#[test]
fn pins_delta_budget_is_by_distinct_id_and_overflow_has_a_terminal_failure() {
    let mut store = pins_ready();
    let now = Instant::now();
    assert!(store.begin_pins(GENERAL, 1).allowed());
    for id in 1..=256 {
        store.apply(
            Event::MessageUpdated(Message {
                pinned: true,
                ..message(id, ADA)
            }),
            now,
        );
    }
    for _ in 0..1000 {
        store.apply(
            Event::MessageUpdated(Message {
                pinned: true,
                ..message(1, ADA)
            }),
            now,
        );
    }
    assert_eq!(
        store.apply(
            Event::PinsLoaded {
                request: 1,
                channel: GENERAL,
                messages: Vec::new()
            },
            now
        ),
        Applied::Pins {
            request: 1,
            channel: GENERAL
        }
    );
    assert_eq!(store.pins(GENERAL).unwrap().len(), 256);
    assert!(store.begin_pins(GENERAL, 2).allowed());
    for id in 1..=257 {
        store.apply(Event::MessageUpdated(message(id, ADA)), now);
    }
    let applied = store.apply(
        Event::PinsLoaded {
            request: 2,
            channel: GENERAL,
            messages: Vec::new(),
        },
        now,
    );
    assert!(matches!(
        applied,
        Applied::Failed {
            failure: Failure::Pins {
                channel: GENERAL,
                request: 2
            },
            ambiguous: false,
            ..
        }
    ));
    assert_eq!(
        store.apply(
            Event::PinsLoaded {
                request: 2,
                channel: GENERAL,
                messages: Vec::new()
            },
            now
        ),
        Applied::Nothing,
        "overflow never falls back to legacy response acceptance"
    );
    assert!(store.begin_pins(GENERAL, 3).allowed());
    assert_eq!(
        store.apply(
            Event::PinsLoaded {
                request: 3,
                channel: GENERAL,
                messages: Vec::new()
            },
            now
        ),
        Applied::Pins {
            channel: GENERAL,
            request: 3
        }
    );
    assert!(store.pins(GENERAL).unwrap().is_empty());
}

#[test]
fn pins_snapshot_uses_a_newer_timeline_version_retained_before_begin() {
    let mut store = pins_ready();
    let now = Instant::now();
    history(&mut store, None, 1..6, true);
    let current = Message {
        content: "edición previa vigente".into(),
        edited: Some(Timestamp(200)),
        pinned: true,
        ..message(4, ADA)
    };
    store.apply(Event::MessageUpdated(current.clone()), now);
    assert!(store.begin_pins(GENERAL, 1).allowed());
    store.apply(
        Event::PinsLoaded {
            request: 1,
            channel: GENERAL,
            messages: vec![Message {
                content: "texto anterior".into(),
                edited: Some(Timestamp(100)),
                ..current.clone()
            }],
        },
        now,
    );
    assert_eq!(store.pins(GENERAL).unwrap(), &[current]);
}

#[test]
fn pins_cancel_wrong_failures_and_revocation_cannot_commit_or_cancel_newer_reads() {
    let mut store = pins_ready();
    let now = Instant::now();
    assert!(store.begin_pins(GENERAL, 1).allowed());
    assert!(store.begin_pins(GENERAL, 2).allowed());
    store.cancel_pins(1);
    assert_eq!(
        store.apply(
            Event::PinsLoaded {
                request: 1,
                channel: GENERAL,
                messages: Vec::new()
            },
            now
        ),
        Applied::Nothing
    );
    assert_eq!(
        store.apply(
            Event::Failed {
                failure: Failure::Pins {
                    request: 2,
                    channel: VOICE
                },
                reason: "otra conversación".into(),
                ambiguous: false
            },
            now
        ),
        Applied::Nothing
    );
    assert_eq!(
        store.apply(
            Event::PinsLoaded {
                request: 2,
                channel: GENERAL,
                messages: Vec::new()
            },
            now
        ),
        Applied::Pins {
            channel: GENERAL,
            request: 2
        }
    );
    assert!(store.begin_pins(GENERAL, 3).allowed());
    let mut revoked = store.channel(GENERAL).unwrap().clone();
    revoked.permissions = Permissions::VIEW;
    store.apply(Event::ChannelChanged(revoked), now);
    assert_eq!(
        store.apply(
            Event::PinsLoaded {
                request: 3,
                channel: GENERAL,
                messages: Vec::new()
            },
            now
        ),
        Applied::Nothing
    );
    assert!(store.pins(GENERAL).is_none());
}

#[test]
fn pins_scope_cleanup_discards_unverified_results_and_rechecks_the_session() {
    let now = Instant::now();
    for end in [
        Event::Ready(Box::new(snapshot())),
        Event::Authentication(AuthenticationState::SignedOut),
        Event::ChannelRemoved { channel: GENERAL },
        Event::ServerRemoved {
            server: ServerId(1),
            reason: None,
        },
        Event::Failed {
            failure: Failure::Pins {
                channel: GENERAL,
                request: 1,
            },
            reason: "falló".into(),
            ambiguous: false,
        },
    ] {
        let mut store = pins_ready();
        assert!(store.begin_pins(GENERAL, 1).allowed());
        store.apply(
            Event::MessageUpdated(Message {
                pinned: true,
                ..message(4, ADA)
            }),
            now,
        );
        store.apply(end, now);
        assert!(store.pins(GENERAL).is_none());
        assert_eq!(
            store.apply(
                Event::PinsLoaded {
                    request: 1,
                    channel: GENERAL,
                    messages: Vec::new()
                },
                now
            ),
            Applied::Nothing
        );
    }
    let mut store = pins_ready();
    store.from_cache = true;
    assert!(!store.begin_pins(GENERAL, 1).allowed());
    store.from_cache = false;
    assert!(store.begin_pins(GENERAL, 1).allowed());
    store.from_cache = true;
    assert!(matches!(
        store.apply(
            Event::PinsLoaded {
                request: 1,
                channel: GENERAL,
                messages: Vec::new()
            },
            now
        ),
        Applied::Failed {
            failure: Failure::Pins {
                channel: GENERAL,
                request: 1
            },
            ..
        }
    ));
    assert!(store.pins(GENERAL).is_none());
}

#[test]
fn pins_missing_live_baseline_and_cross_channel_payloads_fail_instead_of_showing_a_false_list() {
    let mut store = pins_ready();
    let now = Instant::now();
    assert!(store.begin_pins(GENERAL, 1).allowed());
    store.apply(
        Event::MessageUpdated(Message {
            pinned: true,
            ..message(4, ADA)
        }),
        now,
    );
    store.pins.remove(&GENERAL);
    assert!(matches!(
        store.apply(
            Event::PinsLoaded {
                request: 1,
                channel: GENERAL,
                messages: Vec::new()
            },
            now
        ),
        Applied::Failed {
            failure: Failure::Pins {
                channel: GENERAL,
                request: 1
            },
            ..
        }
    ));
    assert!(store.pins(GENERAL).is_none());
    assert!(store.begin_pins(GENERAL, 2).allowed());
    assert!(matches!(
        store.apply(
            Event::PinsLoaded {
                request: 2,
                channel: GENERAL,
                messages: vec![Message {
                    channel: VOICE,
                    pinned: true,
                    ..message(4, ADA)
                }]
            },
            now
        ),
        Applied::Failed {
            failure: Failure::Pins {
                channel: GENERAL,
                request: 2
            },
            ..
        }
    ));
    assert!(store.pins(GENERAL).is_none());
}

#[test]
fn an_older_pins_response_cannot_replace_the_latest_request() {
    let mut store = pins_ready();
    let now = Instant::now();
    assert!(store.begin_pins(GENERAL, 2).allowed());
    for (request, id) in [(2, 4), (1, 2)] {
        store.apply(
            Event::PinsLoaded {
                request,
                channel: GENERAL,
                messages: vec![Message {
                    pinned: true,
                    ..message(id, ADA)
                }],
            },
            now,
        );
    }
    assert_eq!(store.pins(GENERAL).unwrap()[0].id, MessageId(4));
}

#[test]
fn pins_arrive_listed_and_follow_each_change_newest_first() {
    let mut store = pins_ready();
    let now = Instant::now();
    history(&mut store, None, 1..6, true);
    assert_eq!(store.pins(GENERAL), None, "not asked for yet");
    assert!(store.begin_pins(GENERAL, 1).allowed());
    let pinned = |id| Message {
        pinned: true,
        ..message(id, ADA)
    };
    assert_eq!(
        store.apply(
            Event::PinsLoaded {
                request: 1,
                channel: GENERAL,
                messages: vec![pinned(4), pinned(2)],
            },
            now
        ),
        Applied::Pins {
            channel: GENERAL,
            request: 1
        }
    );
    let listed = |store: &Store| -> Vec<u64> {
        store
            .pins(GENERAL)
            .unwrap()
            .iter()
            .map(|message| message.id.0)
            .collect()
    };
    assert_eq!(listed(&store), vec![4, 2]);
    // Pinning arrives as an update: in its place, and in the timeline too.
    store.apply(Event::MessageUpdated(pinned(3)), now);
    assert_eq!(listed(&store), vec![4, 3, 2]);
    assert!(
        store
            .timeline(GENERAL)
            .unwrap()
            .get(MessageId(3))
            .unwrap()
            .pinned
    );
    store.apply(Event::MessageUpdated(message(4, ADA)), now);
    assert_eq!(listed(&store), vec![3, 2], "unpinned, it leaves the list");
    store.apply(
        Event::MessageDeleted {
            channel: GENERAL,
            message: MessageId(2),
        },
        now,
    );
    assert_eq!(listed(&store), vec![3]);
    store.apply(Event::ChannelRemoved { channel: GENERAL }, now);
    assert_eq!(store.pins(GENERAL), None);
}

#[test]
fn pinning_takes_the_pin_bit_in_a_server_and_a_provider_that_pins() {
    let mut store = ready();
    let capabilities = |pins| rune_provider::Capabilities {
        pins,
        ..Default::default()
    };
    store.provider = Some(ProviderInfo {
        name: "p".into(),
        kind: rune_provider::ProviderKind::Simulated,
        capabilities: capabilities(false),
        notice: String::new(),
    });
    assert_eq!(
        store.abilities(GENERAL).unwrap().pin.reason(),
        Some("Este proveedor no permite fijar mensajes")
    );
    store.provider.as_mut().unwrap().capabilities = capabilities(true);
    assert_eq!(
        store.abilities(GENERAL).unwrap().pin.reason(),
        Some("No tienes permiso para fijar mensajes en #general")
    );
    // Managing messages alone does not pin, as in the official client.
    let mut moderated = store.channel(GENERAL).unwrap().clone();
    moderated.permissions =
        Permissions::MEMBER | Permissions::READ_HISTORY | Permissions::MANAGE_MESSAGES;
    store.apply(Event::ChannelChanged(moderated.clone()), Instant::now());
    assert!(!store.abilities(GENERAL).unwrap().pin.allowed());
    moderated.permissions =
        Permissions::MEMBER | Permissions::READ_HISTORY | Permissions::PIN_MESSAGES;
    store.apply(Event::ChannelChanged(moderated), Instant::now());
    assert!(store.abilities(GENERAL).unwrap().pin.allowed());
}

#[test]
fn the_account_custom_status_arrives_as_its_presence_and_a_refusal_keeps_it() {
    let mut store = ready();
    let custom = Presence {
        status: Status::Idle,
        custom: Some("de viaje".into()),
        custom_emoji: None,
        activities: Vec::new(),
        platforms: Default::default(),
    };
    let applied = store.apply(
        Event::PresenceChanged {
            user: ME,
            presence: custom.clone(),
        },
        Instant::now(),
    );
    assert_eq!(applied, Applied::Presence(ME));
    assert_eq!(store.presence(ME), custom);
    // Nothing is shown before the provider answers, so a refusal undoes nothing.
    let failure = Failure::of(&rune_provider::Command::SetCustomStatus {
        text: None,
        emoji: None,
        until: None,
    });
    assert_eq!(failure, Some(Failure::Status));
    store.apply(
        Event::Failed {
            failure: failure.unwrap(),
            reason: "no".into(),
            ambiguous: false,
        },
        Instant::now(),
    );
    assert_eq!(store.presence(ME).custom.as_deref(), Some("de viaje"));
}

#[test]
fn sticker_failure_and_retry_keep_the_sticker_without_becoming_empty_text() {
    let mut store = ready();
    let sticker = rune_model::Sticker {
        id: rune_model::StickerId(501),
        name: "Wave".into(),
        image: None,
        available: true,
    };
    store.add_pending_sticker(GENERAL, sticker.clone(), 77, Timestamp(1));
    store.apply(
        Event::Failed {
            failure: Failure::Send {
                channel: GENERAL,
                nonce: 77,
            },
            reason: "test rejection".into(),
            ambiguous: false,
        },
        Instant::now(),
    );
    let pending = store
        .timeline(GENERAL)
        .unwrap()
        .messages()
        .iter()
        .find(|message| {
            matches!(
                message.delivery,
                Delivery::Failed { .. } | Delivery::Unknown { .. }
            )
        })
        .unwrap()
        .clone();
    let unsent = store.unsent(GENERAL, pending.id).unwrap();
    assert_eq!(unsent.sticker, Some(sticker.id));
    assert!(unsent.content.is_empty());
    store.resend(GENERAL, pending.id, 78);
    let retry = store
        .timeline(GENERAL)
        .unwrap()
        .messages()
        .iter()
        .find(|message| message.delivery == Delivery::Sending)
        .unwrap()
        .clone();
    assert_eq!(retry.stickers, vec![sticker]);
}

#[test]
fn profile_banners_survive_partial_accounts_and_allow_explicit_removal() {
    use rune_model::{ImageKind, ProfileBanner, RemoteImage};
    let mut store = ready();
    let now = Instant::now();
    let image = |name: &str| {
        ProfileBanner::Image(RemoteImage {
            url: format!("https://cdn.example.test/{name}.png"),
            kind: ImageKind::Banner,
        })
    };
    let mut initial = user(ADA, "ada");
    initial.banner = image("first");
    store.apply(Event::UserChanged(initial), now);
    let mut partial = user(ADA, "new-handle");
    partial.display_name = Some("Ada".into());
    store.apply(Event::UserChanged(partial.clone()), now);
    assert_eq!(store.user(ADA).unwrap().banner, image("first"));
    assert_eq!(store.user(ADA).unwrap().name, "new-handle");
    store.apply(
        Event::MessageCreated {
            message: message(42, ADA),
            nonce: None,
        },
        now,
    );
    assert_eq!(store.user(ADA).unwrap().banner, image("first"));
    partial.banner = image("replacement");
    store.apply(Event::UserChanged(partial.clone()), now);
    assert_eq!(store.user(ADA).unwrap().banner, image("replacement"));
    partial.banner = ProfileBanner::Absent;
    store.apply(Event::UserChanged(partial.clone()), now);
    partial.banner = ProfileBanner::Unknown;
    store.apply(Event::UserChanged(partial), now);
    assert_eq!(store.user(ADA).unwrap().banner, ProfileBanner::Absent);

    let mut initial = user(ADA, "ada");
    initial.banner = image("previous-account");
    store.apply(Event::UserChanged(initial), now);
    store.apply(Event::Authentication(AuthenticationState::SignedOut), now);
    store.apply(Event::Ready(Box::new(snapshot())), now);
    assert_eq!(store.user(ADA).unwrap().banner, ProfileBanner::Unknown);
}

#[test]
fn cameras_are_seen_only_with_the_capability_and_in_the_call() {
    let mut store = ready();
    let provider = |watch_cameras| ProviderInfo {
        name: "p".into(),
        kind: rune_provider::ProviderKind::Simulated,
        capabilities: rune_provider::Capabilities {
            voice: rune_provider::VoiceSupport::Simulated,
            watch_cameras,
            ..Default::default()
        },
        notice: String::new(),
    };
    store.provider = Some(provider(false));
    store.begin_call(VOICE, false, false);
    assert!(!store.abilities(VOICE).unwrap().watch_camera.allowed());
    store.provider = Some(provider(true));
    assert!(store.abilities(VOICE).unwrap().watch_camera.allowed());
    store.end_call();
    assert_eq!(
        store.abilities(VOICE).unwrap().watch_camera.reason(),
        Some("Entra a la llamada para ver las cámaras")
    );
}

#[test]
fn a_stream_is_watched_only_in_the_call_and_goes_with_it() {
    use rune_model::StreamPhase;
    let mut store = ready();
    let now = Instant::now();
    store.provider = Some(ProviderInfo {
        name: "p".into(),
        kind: rune_provider::ProviderKind::Simulated,
        capabilities: rune_provider::Capabilities {
            voice: rune_provider::VoiceSupport::Simulated,
            watch_streams: true,
            ..Default::default()
        },
        notice: String::new(),
    });
    assert_eq!(
        store.abilities(VOICE).unwrap().watch_stream.reason(),
        Some("Entra a la llamada para ver la transmisión")
    );
    store.begin_call(VOICE, false, false);
    assert!(store.abilities(VOICE).unwrap().watch_stream.allowed());
    store.begin_watch(VOICE, ADA);
    let phase = |phase| Event::StreamPhase {
        channel: VOICE,
        owner: ADA,
        phase,
    };
    store.apply(phase(StreamPhase::Live), now);
    assert_eq!(store.watching()[0].phase, StreamPhase::Live);
    // Someone else's stream is not this one.
    store.apply(
        Event::StreamPhase {
            channel: VOICE,
            owner: UserId(99),
            phase: StreamPhase::Ended(None),
        },
        now,
    );
    assert_eq!(store.watching()[0].phase, StreamPhase::Live);
    // Another at once keeps its own phase, and goes on its own.
    store.begin_watch(VOICE, UserId(98));
    assert_eq!(store.watching()[1].phase, StreamPhase::Connecting);
    store.stop_watch(UserId(98));
    assert_eq!(store.watching().len(), 1);
    // A refusal ends it with why; the call ending lets go of it.
    store.apply(
        Event::Failed {
            failure: Failure::Stream {
                channel: VOICE,
                owner: ADA,
            },
            reason: "Esa persona no está transmitiendo en tu llamada.".into(),
            ambiguous: false,
        },
        now,
    );
    assert!(matches!(
        &store.watching()[0].phase,
        StreamPhase::Ended(Some(_))
    ));
    store.apply(
        Event::CallEnded {
            channel: VOICE,
            reason: None,
        },
        now,
    );
    assert!(store.watching().is_empty());
}

#[test]
fn a_watched_stream_waits_while_its_owner_is_not_streaming_and_connects_again_when_they_do() {
    use rune_model::StreamPhase;
    let mut store = ready();
    let now = Instant::now();
    store.begin_call(VOICE, false, false);
    store.begin_watch(VOICE, ADA);
    store.apply(
        Event::StreamPhase {
            channel: VOICE,
            owner: ADA,
            phase: StreamPhase::Live,
        },
        now,
    );
    let voice = |streaming| VoiceState {
        user: ADA,
        channel: VOICE,
        muted: false,
        deafened: false,
        speaking: false,
        streaming,
        server_muted: false,
        server_deafened: false,
        camera: false,
        suppressed: false,
        speak_request: None,
    };
    // Stopped, still in the call: waiting, the entry kept.
    store.apply(Event::VoiceChanged(voice(false)), now);
    assert_eq!(store.watching()[0].phase, StreamPhase::Waiting);
    // Streaming again: connecting, and the provider's connection follows.
    store.apply(Event::VoiceChanged(voice(true)), now);
    assert_eq!(store.watching()[0].phase, StreamPhase::Connecting);
    // A failed stream is not changed by its owner's voice state.
    store.apply(
        Event::StreamPhase {
            channel: VOICE,
            owner: ADA,
            phase: StreamPhase::Failed("No respondió.".into()),
        },
        now,
    );
    store.apply(Event::VoiceChanged(voice(false)), now);
    assert!(matches!(store.watching()[0].phase, StreamPhase::Failed(_)));
}

#[test]
fn the_own_screen_is_tracked_apart_from_the_stream_watched_and_goes_with_the_call() {
    use rune_model::StreamPhase;
    let mut store = ready();
    let now = Instant::now();
    let me = store.me().unwrap();
    store.begin_call(VOICE, false, false);
    store.begin_watch(VOICE, ADA);
    store.begin_share(VOICE);
    assert_eq!(store.sharing().unwrap().owner, me);
    let phase = |owner, phase| Event::StreamPhase {
        channel: VOICE,
        owner,
        phase,
    };
    store.apply(phase(me, StreamPhase::Live), now);
    assert_eq!(store.sharing().unwrap().phase, StreamPhase::Live);
    assert_eq!(store.watching()[0].phase, StreamPhase::Connecting);
    store.apply(
        Event::Failed {
            failure: Failure::Share { channel: VOICE },
            reason: "Discord rechazó la transmisión.".into(),
            ambiguous: false,
        },
        now,
    );
    assert!(matches!(
        &store.sharing().unwrap().phase,
        StreamPhase::Ended(Some(_))
    ));
    store.stop_share();
    assert!(store.sharing().is_none());
    store.begin_share(VOICE);
    store.apply(
        Event::CallEnded {
            channel: VOICE,
            reason: None,
        },
        now,
    );
    assert!(store.sharing().is_none() && store.watching().is_empty());
}

#[test]
fn a_stream_picture_is_kept_only_while_its_owner_streams_there() {
    let mut store = ready();
    let now = Instant::now();
    let image = rune_model::RemoteImage {
        url: "https://cdn.example/streams/1.png".into(),
        kind: rune_model::ImageKind::Banner,
    };
    let preview = |image| Event::StreamPreview {
        channel: VOICE,
        owner: ADA,
        image,
        retry_after: None,
    };
    let state = |streaming| VoiceState {
        user: ADA,
        channel: VOICE,
        muted: false,
        deafened: false,
        speaking: false,
        streaming,
        server_muted: false,
        server_deafened: false,
        camera: false,
        suppressed: false,
        speak_request: None,
    };
    // Not streaming there: nothing is kept.
    store.apply(preview(Some(image.clone())), now);
    assert_eq!(store.stream_preview(VOICE, ADA), None);
    store.apply(Event::VoiceChanged(state(true)), now);
    store.apply(preview(Some(image.clone())), now);
    assert_eq!(store.stream_preview(VOICE, ADA), Some(Some(&image)));
    // None to show is kept as such; another channel's is not this one.
    store.apply(preview(None), now);
    assert_eq!(store.stream_preview(VOICE, ADA), Some(None));
    assert_eq!(store.stream_preview(ChannelId(1), ADA), None);
    // How long a refusal asked to wait is kept with it.
    let wait = std::time::Duration::from_secs(30);
    store.apply(
        Event::StreamPreview {
            channel: VOICE,
            owner: ADA,
            image: None,
            retry_after: Some(wait),
        },
        now,
    );
    assert_eq!(store.stream_preview_retry(ADA), Some(wait));
    store.apply(preview(None), now);
    assert_eq!(store.stream_preview_retry(ADA), None);
    // Once they stop, the next picture that arrives lets go of theirs.
    store.apply(Event::VoiceChanged(state(false)), now);
    store.apply(preview(Some(image)), now);
    assert_eq!(store.stream_preview(VOICE, ADA), None);
}

#[test]
fn a_channel_is_private_when_everyone_may_not_see_it() {
    use rune_model::{Overwrite, OverwriteTarget, RoleId};
    let mut general = channel(GENERAL, ChannelKind::Text, "general");
    assert!(!Store::private(&general));
    let everyone = |allow, deny| Overwrite {
        target: OverwriteTarget::Role(RoleId(1)),
        allow,
        deny,
    };
    // Another role's denial, or @everyone's of something else, leaves it open.
    general.overwrites = vec![
        Overwrite {
            target: OverwriteTarget::Role(RoleId(7)),
            ..everyone(Permissions::NONE, Permissions::VIEW)
        },
        everyone(Permissions::NONE, Permissions::SEND),
    ];
    assert!(!Store::private(&general));
    general.overwrites[1] = everyone(Permissions::NONE, Permissions::VIEW | Permissions::SEND);
    assert!(Store::private(&general));
}

#[test]
fn a_providers_placeholders_follow_the_language_they_are_shown_in() {
    let mut snapshot = snapshot();
    snapshot.servers[0].name = Server::UNNAMED.into();
    let mut nobody = channel(ChannelId(30), ChannelKind::Group, Channel::UNNAMED);
    nobody.server = None;
    snapshot.channels.push(nobody);
    // A server's channel never has the placeholder: that name is its own.
    snapshot
        .channels
        .push(channel(ChannelId(31), ChannelKind::Text, Channel::UNNAMED));
    // The plan, not the channel's permission, is what holds the sticker back.
    let general = snapshot
        .channels
        .iter_mut()
        .find(|found| found.id == GENERAL)
        .unwrap();
    general.permissions = general.permissions | Permissions::EXTERNAL_STICKERS;
    let mut store = Store::new();
    store.apply(Event::Ready(Box::new(snapshot)), Instant::now());
    store.apply(
        Event::Perks(rune_model::Perks {
            upgrade: Some("Plus (simulado)".into()),
            ..Default::default()
        }),
        Instant::now(),
    );
    let shown = |store: &Store| {
        let server = store.server(ServerId(1)).unwrap();
        (
            server.shown_name().to_owned(),
            server.initials(),
            store.channel_title(ChannelId(30)),
            store
                .sticker_perk(GENERAL, Some(ServerId(2)))
                .reason()
                .unwrap()
                .to_owned(),
        )
    };
    assert_eq!(
        shown(&store),
        (
            "Servidor no disponible".into(),
            "SN".into(),
            "Conversación".into(),
            "Necesitas Plus (simulado) para usar stickers de otros servidores aquí".into()
        )
    );
    // The language changes after they arrived: they follow it.
    let _english = rune_i18n::scoped(rune_i18n::Language::English);
    let (server, initials, conversation, perk) = shown(&store);
    assert_eq!(
        (server.as_str(), initials.as_str(), conversation.as_str()),
        ("Server unavailable", "SU", "Conversation")
    );
    assert!(perk.contains("Plus (simulated)"), "{perk}");
    // A name of its own is content: as it is in any language.
    assert_eq!(store.channel_title(GENERAL), "general");
    assert_eq!(store.channel_title(ChannelId(31)), "Conversación");
}
