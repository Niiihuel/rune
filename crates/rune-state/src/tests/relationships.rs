//! Relationships and conversations reconcile events without duplicating, and
//! the abilities follow them.

use rune_model::{Relationship, RelationshipKind};
use rune_provider::{Capabilities, ProviderInfo, ProviderKind};

use super::*;

const BRUNO: UserId = UserId(3);
const DIRECT: ChannelId = ChannelId(20);
const GROUP: ChannelId = ChannelId(21);

fn relation(user: UserId, kind: RelationshipKind) -> Relationship {
    Relationship {
        user,
        kind,
        since: None,
        nickname: None,
        spam: false,
    }
}

fn social_store(capabilities: Capabilities, relationships: Option<Vec<Relationship>>) -> Store {
    let mut store = Store::new();
    store.provider = Some(ProviderInfo {
        name: "Prueba".into(),
        kind: ProviderKind::Simulated,
        capabilities,
        notice: String::new(),
    });
    let mut snapshot = snapshot();
    snapshot.users.push(user(BRUNO, "bruno"));
    let mut direct = channel(DIRECT, ChannelKind::Direct, "");
    direct.server = None;
    direct.recipients = vec![ADA];
    let mut group = channel(GROUP, ChannelKind::Group, "grupo");
    group.server = None;
    group.recipients = vec![ADA, BRUNO];
    group.owner = Some(ADA);
    snapshot.channels.extend([direct, group]);
    snapshot.relationships = relationships;
    store.apply(Event::Ready(Box::new(snapshot)), Instant::now());
    store
}

fn everything() -> Capabilities {
    Capabilities {
        relationships: true,
        friend_requests: true,
        block_users: true,
        open_direct: true,
        manage_groups: true,
        group_members: true,
        create_groups: true,
        close_direct: true,
        friend_nicknames: true,
        send_messages: true,
        reactions: true,
        typing: true,
        ..Default::default()
    }
}

#[test]
fn an_unsupported_or_unreported_list_is_not_an_empty_one() {
    let unsupported = social_store(Capabilities::default(), None);
    assert_eq!(
        unsupported.friends_list().reason(),
        Some("Esta conexión no ofrece la lista de amigos")
    );
    let mut waiting = social_store(everything(), None);
    assert!(waiting.relationships().is_none());
    assert!(!waiting.friends_list().allowed());
    let empty = social_store(everything(), Some(Vec::new()));
    assert!(empty.friends_list().allowed());
    assert_eq!(empty.relationships().map(|list| list.len()), Some(0));
    // A lone event never invents a whole list.
    let applied = waiting.apply(
        Event::RelationshipChanged(relation(ADA, RelationshipKind::Friend)),
        Instant::now(),
    );
    assert_eq!(applied, Applied::Nothing);
    assert!(waiting.relationships().is_none());
}

#[test]
fn repeated_and_late_events_replace_instead_of_duplicating() {
    let now = Instant::now();
    let mut store = social_store(everything(), Some(Vec::new()));
    let incoming = relation(BRUNO, RelationshipKind::IncomingRequest);
    for _ in 0..3 {
        store.apply(Event::RelationshipChanged(incoming.clone()), now);
    }
    assert_eq!(store.relationships().unwrap().len(), 1);
    assert!(store.social(BRUNO).accept.allowed());
    store.apply(
        Event::RelationshipChanged(relation(BRUNO, RelationshipKind::Friend)),
        now,
    );
    let list = store.relationships().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].kind, RelationshipKind::Friend);
    assert!(!store.social(BRUNO).accept.allowed());
    assert_eq!(
        store.apply(Event::RelationshipRemoved { user: BRUNO }, now),
        Applied::Relationship(BRUNO)
    );
    // Removed twice: the second is late and changes nothing.
    assert_eq!(
        store.apply(Event::RelationshipRemoved { user: BRUNO }, now),
        Applied::Nothing
    );
    assert!(store.relationships().unwrap().is_empty());
    // The account is never its own friend.
    store.apply(
        Event::RelationshipChanged(relation(ME, RelationshipKind::Friend)),
        now,
    );
    assert!(store.relationships().unwrap().is_empty());
}

#[test]
fn blocking_takes_away_writing_and_reacting_in_their_conversation() {
    let now = Instant::now();
    let mut store = social_store(
        everything(),
        Some(vec![relation(ADA, RelationshipKind::Friend)]),
    );
    assert!(store.abilities(DIRECT).unwrap().write.allowed());
    assert!(store.social(ADA).block.allowed());
    store.apply(
        Event::RelationshipChanged(relation(ADA, RelationshipKind::Blocked)),
        now,
    );
    let abilities = store.abilities(DIRECT).unwrap();
    assert!(abilities
        .write
        .reason()
        .unwrap()
        .starts_with("Bloqueaste a ada"));
    assert!(!abilities.react.allowed() && !abilities.typing.allowed());
    let social = store.social(ADA);
    assert!(social.unblock.allowed() && !social.block.allowed());
    // The existing conversation still opens; a new one would not.
    assert!(social.message.allowed());
    store.apply(
        Event::RelationshipChanged(relation(BRUNO, RelationshipKind::Blocked)),
        now,
    );
    assert!(store
        .open_direct(BRUNO)
        .reason()
        .unwrap()
        .starts_with("Bloqueaste"));
    // A group is not a direct conversation: blocking one member does not mute it.
    assert!(store.abilities(GROUP).unwrap().write.allowed());
}

#[test]
fn listing_groups_does_not_grant_managing_them() {
    let listed = social_store(
        Capabilities {
            direct_messages: true,
            ..Default::default()
        },
        Some(Vec::new()),
    );
    let group = listed.group_abilities(GROUP).unwrap();
    assert!(!group.rename.allowed() && !group.leave.allowed());
    assert!(listed.group_abilities(DIRECT).is_none());
    let managed = social_store(everything(), Some(Vec::new()));
    let group = managed.group_abilities(GROUP).unwrap();
    assert!(group.rename.allowed() && group.add.allowed() && group.leave.allowed());
    assert_eq!(
        group.remove.reason(),
        Some("Solo quien es dueño del grupo puede quitar personas")
    );
}

#[test]
fn friends_can_join_a_group_they_are_not_in_and_an_existing_conversation_is_found() {
    let store = social_store(
        everything(),
        Some(vec![
            relation(ADA, RelationshipKind::Friend),
            relation(UserId(4), RelationshipKind::Friend),
            relation(BRUNO, RelationshipKind::OutgoingRequest),
        ]),
    );
    assert_eq!(store.group_candidates(GROUP), vec![UserId(4)]);
    assert_eq!(store.direct_with(ADA), Some(DIRECT));
    assert_eq!(store.direct_with(BRUNO), None);
    assert!(store.open_direct(BRUNO).allowed());
    assert!(!store.open_direct(ME).allowed());
    // By name: someone not known yet has none and comes first.
    assert_eq!(store.friends(), vec![UserId(4), ADA]);
}

#[test]
fn a_friends_nickname_titles_their_direct_conversation_only() {
    let shown = social_store(everything(), Some(Vec::new())).channel_title(DIRECT);
    let mut store = social_store(
        everything(),
        Some(vec![Relationship {
            nickname: Some("Adita".into()),
            ..relation(ADA, RelationshipKind::Friend)
        }]),
    );
    assert_eq!(store.channel_title(DIRECT), "Adita");
    // A group keeps its own name, Ada in it or not.
    assert_eq!(store.channel_title(GROUP), "grupo");
    // Taken away: the conversation's own name again.
    store.apply(
        Event::RelationshipChanged(relation(ADA, RelationshipKind::Friend)),
        Instant::now(),
    );
    assert_eq!(store.channel_title(DIRECT), shown);
}

#[test]
fn only_the_owner_hands_a_group_over_and_only_with_support() {
    let mut store = social_store(everything(), Some(Vec::new()));
    assert_eq!(
        store.group_abilities(GROUP).unwrap().owner.reason(),
        Some("Esta conexión no permite pasar la propiedad de un grupo")
    );
    store = social_store(
        Capabilities {
            group_owner: true,
            ..everything()
        },
        Some(Vec::new()),
    );
    assert_eq!(
        store.group_abilities(GROUP).unwrap().owner.reason(),
        Some("Solo quien es dueño del grupo puede pasarlo")
    );
    let mut group = store.channel(GROUP).unwrap().clone();
    group.owner = store.me();
    store.apply(Event::ChannelChanged(group), Instant::now());
    let abilities = store.group_abilities(GROUP).unwrap();
    assert!(abilities.owner.allowed() && abilities.remove.allowed());
}

#[test]
fn managing_a_group_and_changing_its_people_are_offered_apart() {
    let store = social_store(
        Capabilities {
            group_members: false,
            create_groups: false,
            ..everything()
        },
        Some(Vec::new()),
    );
    let group = store.group_abilities(GROUP).unwrap();
    assert!(group.rename.allowed() && group.icon.allowed() && group.leave.allowed());
    assert_eq!(
        group.add.reason(),
        Some("Esta conexión no permite añadir ni quitar personas de un grupo")
    );
    assert!(!group.remove.allowed());
    assert_eq!(
        store.create_group().reason(),
        Some("Esta conexión no permite crear grupos")
    );
    // Only a direct conversation is closed; a group is left.
    assert!(store.close_direct(DIRECT).allowed());
    assert!(!store.close_direct(GROUP).allowed());
}

#[test]
fn only_friends_get_a_nickname_and_spam_waits_apart() {
    let mut spam = relation(UserId(4), RelationshipKind::IncomingRequest);
    spam.spam = true;
    let store = social_store(
        everything(),
        Some(vec![
            relation(ADA, RelationshipKind::Friend),
            relation(BRUNO, RelationshipKind::IncomingRequest),
            spam,
        ]),
    );
    assert!(store.social(ADA).nickname.allowed());
    assert_eq!(
        store.social(BRUNO).nickname.reason(),
        Some("Solo tus amigos pueden tener un apodo")
    );
    assert_eq!(store.pending_requests(), 1);
    // Spam is still a request: it can be accepted or ignored.
    assert!(store.social(UserId(4)).accept.allowed());
}
