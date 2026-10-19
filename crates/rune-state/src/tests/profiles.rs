use std::sync::atomic::{AtomicI64, Ordering};

use super::*;
use rune_model::{Restriction, UserProfile};
use rune_provider::mock::{MockConfig, MockProvider};
use rune_provider::Provider;

const START: Timestamp = Timestamp(1_790_000_100_000);

fn simulated() -> (MockProvider, Store) {
    let provider = MockProvider::new(MockConfig {
        seed: 11,
        history_end: Timestamp(1_790_000_000_000),
        clock: || START,
        latency: Duration::ZERO,
        ambient: false,
        replies: false,
    });
    let mut store = Store::new();
    store.provider = Some(provider.info());
    store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
    (provider, store)
}

fn channel_named(store: &Store, name: &str) -> ChannelId {
    store
        .channels
        .values()
        .find(|channel| channel.name == name)
        .unwrap()
        .id
}

#[test]
fn a_timeout_blocks_taking_part_with_its_reason_and_lifts_at_its_end() {
    static NOW: AtomicI64 = AtomicI64::new(START.0);
    let (_, mut store) = simulated();
    store.clock = Some(|| Timestamp(NOW.load(Ordering::Relaxed)));
    let charla = channel_named(&store, "charla");
    let server = store.channel(charla).unwrap().server.unwrap();
    let until = START.plus_minutes(95);
    assert_eq!(
        store.restriction(server),
        Some(Restriction::TimedOut { until })
    );
    let abilities = store.abilities(charla).unwrap();
    for permit in [
        &abilities.write,
        &abilities.typing,
        &abilities.react,
        &abilities.vote,
        &abilities.attach,
        &abilities.edit_own,
    ] {
        assert_eq!(
            permit.reason(),
            Some("Estás aislado en Café Nocturno: podés leer, pero no participar")
        );
    }
    assert!(abilities.read_history.allowed() && abilities.delete_own.allowed());
    let voice = store
        .channels
        .values()
        .find(|c| c.server == Some(server) && c.kind == ChannelKind::Voice)
        .unwrap()
        .id;
    assert!(!store.abilities(voice).unwrap().join_voice.allowed());
    let general = channel_named(&store, "general");
    assert!(
        store.abilities(general).unwrap().write.allowed(),
        "elsewhere"
    );
    // Looked at again at the next minute while it lasts, and as it ends.
    assert_eq!(store.restriction_expires(), Some(Duration::from_secs(60)));
    NOW.store(until.0 - 1_000, Ordering::Relaxed);
    assert_eq!(store.restriction_expires(), Some(Duration::from_secs(1)));
    NOW.store(until.0, Ordering::Relaxed);
    assert_eq!(store.restriction(server), None);
    assert!(store.abilities(charla).unwrap().write.allowed());
    assert_eq!(store.restriction_expires(), None);
}

#[test]
fn pending_verification_restricts_the_server_but_leaving_stays() {
    let (provider, mut store) = simulated();
    let general = channel_named(&store, "general");
    let server = store.channel(general).unwrap().server.unwrap();
    let mut member = store.member_of(server, provider.me()).unwrap().clone();
    member.restrictions.pending = true;
    store.apply(Event::MemberChanged(member), Instant::now());
    assert_eq!(store.restriction(server), Some(Restriction::Pending));
    assert_eq!(
        store.abilities(general).unwrap().write.reason(),
        Some("Aceptá las reglas de Taller de Rust para participar")
    );
    assert!(store.abilities(general).unwrap().read_history.allowed());
    let server_abilities = store.server_abilities(server).unwrap();
    assert!(server_abilities
        .change_nick
        .reason()
        .is_some_and(|reason| reason.contains("reglas")));
    assert!(server_abilities
        .leave
        .reason()
        .is_none_or(|reason| !reason.contains("reglas")));
}

#[test]
fn only_the_profile_read_in_flight_and_its_scope_are_kept() {
    let (_, mut store) = simulated();
    let (ada, server) = (UserId(10), Some(ServerId(1)));
    let answer = |request, user, server| Event::ProfileLoaded {
        request,
        profile: UserProfile {
            user,
            server,
            mutual_servers: Some(Vec::new()),
            ..Default::default()
        },
    };
    assert_eq!(
        store.apply(answer(5, ada, server), Instant::now()),
        Applied::Nothing,
        "nobody asked"
    );
    assert!(store.begin_profile(ada, server, 5).allowed());
    assert!(store.profile_loading(ada));
    assert!(!store.begin_profile(ada, server, 5).allowed(), "reused id");
    for stale in [
        answer(4, ada, server),
        answer(5, UserId(11), server),
        answer(5, ada, None),
    ] {
        assert_eq!(store.apply(stale, Instant::now()), Applied::Nothing);
    }
    assert!(store.profile(ada, server).is_none());
    // A newer read replaces the older one: its late answer changes nothing.
    assert!(store.begin_profile(ada, None, 6).allowed());
    assert_eq!(
        store.apply(answer(5, ada, server), Instant::now()),
        Applied::Nothing
    );
    let failure = |request| Event::Failed {
        failure: Failure::ProfileLoad { request, user: ada },
        reason: "falló".into(),
        ambiguous: false,
    };
    assert_eq!(store.apply(failure(5), Instant::now()), Applied::Nothing);
    assert!(store.profile_loading(ada));
    assert_eq!(
        store.apply(answer(6, ada, None), Instant::now()),
        Applied::Profile(ada)
    );
    let loaded = store.profile(ada, None).unwrap();
    assert_eq!(loaded.mutual_servers, Some(Vec::new()), "known: none");
    assert_eq!(loaded.mutual_friends, None, "unknown stays unknown");
    assert!(store.profile(ada, server).is_none(), "another scope");
    assert!(!store.profile_loading(ada));
}

#[test]
fn profiles_are_read_only_with_a_capable_live_provider() {
    let (_, mut store) = simulated();
    store.provider.as_mut().unwrap().capabilities.profiles = false;
    assert!(!store.begin_profile(UserId(10), None, 1).allowed());
    store.provider.as_mut().unwrap().capabilities.profiles = true;
    store.from_cache = true;
    assert!(!store.begin_profile(UserId(10), None, 1).allowed());
    store.from_cache = false;
    assert!(store.begin_profile(UserId(10), None, 1).allowed());
}

#[test]
fn a_member_list_is_kept_only_within_bounds_for_a_known_server_until_the_next_session() {
    use rune_model::{ListGroup, ListRow, MemberList};
    let (provider, mut store) = simulated();
    let server = store.servers()[0].id;
    let list = |server, rows: Vec<(u32, ListRow)>| {
        Event::MemberListChanged(MemberList {
            server,
            channel: ChannelId(1),
            groups: vec![(ListGroup::Online, 2)],
            rows,
        })
    };
    let ada = ListRow::Member(UserId(10));
    assert_eq!(
        store.apply(list(server, vec![(1, ada), (2, ada)]), Instant::now()),
        Applied::Nothing,
        "someone twice"
    );
    assert_eq!(
        store.apply(list(ServerId(999_999), vec![]), Instant::now()),
        Applied::Nothing,
        "an unknown server"
    );
    assert!(store.member_list(server).is_none());
    assert_eq!(
        store.apply(list(server, vec![(1, ada)]), Instant::now()),
        Applied::Members(server)
    );
    assert_eq!(store.member_list(server).map(MemberList::len), Some(3));
    assert!(
        store.member_list(store.servers()[1].id).is_none(),
        "another server"
    );
    store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
    assert!(
        store.member_list(server).is_none(),
        "a new session lists again"
    );
}

#[test]
fn a_kept_profile_is_asked_for_again_after_a_minute() {
    static NOW: AtomicI64 = AtomicI64::new(START.0);
    let (_, mut store) = simulated();
    store.clock = Some(|| Timestamp(NOW.load(Ordering::Relaxed)));
    let (ada, server) = (UserId(10), Some(ServerId(1)));
    assert!(store.profile_stale(ada, server), "never loaded");
    assert!(store.begin_profile(ada, server, 1).allowed());
    let loaded = Event::ProfileLoaded {
        request: 1,
        profile: UserProfile {
            user: ada,
            server,
            ..Default::default()
        },
    };
    store.apply(loaded, Instant::now());
    assert!(!store.profile_stale(ada, server), "fresh");
    assert!(store.profile_stale(ada, None), "another scope");
    NOW.fetch_add(59_999, Ordering::Relaxed);
    assert!(!store.profile_stale(ada, server));
    NOW.fetch_add(1, Ordering::Relaxed);
    assert!(store.profile_stale(ada, server), "a minute old");
    assert!(
        store.profile(ada, server).is_some(),
        "still shown meanwhile"
    );
    // A clock set back before it came: old, not fresh until it catches up.
    NOW.fetch_sub(60_000 + 3_600_000, Ordering::Relaxed);
    assert!(store.profile_stale(ada, server), "the clock went back");
}
