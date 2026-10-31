//! The explicit content filter by where a picture was posted and who
//! posted it (contenido-de-mensajes).

use rune_model::{ContentFilter, MessagingPrivacy, Relationship, RelationshipKind};

use super::*;

const BRUNO: UserId = UserId(3);
const DIRECT: ChannelId = ChannelId(20);
const GROUP: ChannelId = ChannelId(21);

/// Ada is a friend; Bruno is not. A conversation with Ada, and a group of both.
fn store() -> Store {
    let mut store = Store::new();
    let mut snapshot = snapshot();
    snapshot.users.push(user(BRUNO, "bruno"));
    let mut direct = channel(DIRECT, ChannelKind::Direct, "");
    direct.server = None;
    direct.recipients = vec![ADA];
    let mut group = channel(GROUP, ChannelKind::Group, "grupo");
    group.server = None;
    group.recipients = vec![ADA, BRUNO];
    snapshot.channels.extend([direct, group]);
    snapshot.relationships = Some(vec![Relationship {
        user: ADA,
        kind: RelationshipKind::Friend,
        since: None,
        nickname: None,
        spam: false,
        ignored: false,
    }]);
    store.apply(Event::Ready(Box::new(snapshot)), Instant::now());
    store
}

#[test]
fn unchosen_levels_show_servers_and_friends_and_hide_everyone_else() {
    let store = store();
    assert!(!store.obscures_explicit(GENERAL, ADA));
    assert!(!store.obscures_explicit(DIRECT, ADA));
    assert!(!store.obscures_explicit(GROUP, ADA));
    assert!(store.obscures_explicit(GROUP, BRUNO));
    // The account's own, and a conversation RUNE does not know, never.
    assert!(!store.obscures_explicit(GROUP, ME));
    assert!(!store.obscures_explicit(ChannelId(99), BRUNO));
}

#[test]
fn levels_not_read_yet_blur_everything_explicit_until_they_arrive() {
    let mut store = store();
    store.provider = Some(ProviderInfo {
        name: "p".into(),
        kind: rune_provider::ProviderKind::Simulated,
        capabilities: rune_provider::Capabilities {
            messaging_privacy: true,
            ..Default::default()
        },
        notice: String::new(),
    });
    assert!(store.obscures_explicit(GENERAL, ADA));
    assert!(store.obscures_explicit(DIRECT, ADA));
    assert!(
        !store.obscures_explicit(GENERAL, ME),
        "the account's own, never"
    );
    store.apply(
        Event::MessagingPrivacy(MessagingPrivacy::default()),
        Instant::now(),
    );
    assert!(!store.obscures_explicit(GENERAL, ADA));
    assert!(!store.obscures_explicit(DIRECT, ADA));
    assert!(store.obscures_explicit(GROUP, BRUNO));
}

#[test]
fn each_kind_of_conversation_follows_its_own_level() {
    let mut store = store();
    let privacy = MessagingPrivacy {
        explicit_servers: ContentFilter::Blur,
        explicit_friends: ContentFilter::Block,
        explicit_others: ContentFilter::Show,
        ..MessagingPrivacy::default()
    };
    store.apply(Event::MessagingPrivacy(privacy), Instant::now());
    assert!(store.obscures_explicit(GENERAL, ADA));
    assert!(store.obscures_explicit(GENERAL, BRUNO));
    assert!(store.obscures_explicit(DIRECT, ADA), "blocking blurs here");
    assert!(!store.obscures_explicit(GROUP, BRUNO));
    assert!(!store.obscures_explicit(GENERAL, ME));
}
