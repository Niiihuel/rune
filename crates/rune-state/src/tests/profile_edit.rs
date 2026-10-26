use super::*;
use rune_model::{ImageKind, ProfileBanner, RemoteImage, Restriction, UserProfile};
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
    store.clock = Some(|| START);
    store.provider = Some(provider.info());
    store.apply(Event::Ready(Box::new(provider.snapshot())), Instant::now());
    (provider, store)
}

fn server_named(store: &Store, name: &str) -> ServerId {
    store.servers().iter().find(|s| s.name == name).unwrap().id
}

#[test]
fn the_profile_abilities_follow_the_provider_the_server_and_its_restrictions() {
    let (provider, mut store) = simulated();
    let everywhere = store.profile_abilities(None);
    assert!(everywhere.text.allowed() && everywhere.images.allowed());
    assert_eq!(
        everywhere.nick.reason(),
        Some("El apodo es de cada servidor.")
    );
    // Username, email and password ask for the password: never here.
    assert_eq!(everywhere.credentials.reason(), Some(CREDENTIALS_DENIED));

    let taller = server_named(&store, "Taller de Rust");
    let there = store.profile_abilities(Some(taller));
    assert!(there.text.allowed() && there.images.allowed() && there.nick.allowed());
    // Timed out in Café Nocturno: its profile there waits, with the reason.
    let cafe = server_named(&store, "Café Nocturno");
    assert!(matches!(
        store.restriction(cafe),
        Some(Restriction::TimedOut { .. })
    ));
    let restricted = store.profile_abilities(Some(cafe));
    for permit in [&restricted.text, &restricted.images, &restricted.nick] {
        assert!(
            permit.reason().is_some_and(|r| r.contains("aislado")),
            "{permit:?}"
        );
    }
    // A server the account left, and a provider without the capability.
    assert!(store
        .profile_abilities(Some(ServerId(999)))
        .text
        .reason()
        .is_some());
    let mut info = provider.info();
    info.capabilities.profile_images = false;
    info.capabilities.server_profiles = false;
    store.provider = Some(info);
    assert_eq!(
        store.profile_abilities(None).images.reason(),
        Some("Este proveedor no permite cambiar el avatar ni el banner.")
    );
    let there = store.profile_abilities(Some(taller));
    assert!(there.text.reason().is_some() && there.nick.reason().is_some());
}

#[test]
fn the_plan_says_what_needs_it_before_the_service_refuses() {
    let (_, mut store) = simulated();
    // Unknown, the service answers, as before.
    let everywhere = store.profile_abilities(None);
    assert!(everywhere.banner.allowed() && everywhere.animated.allowed());
    // Without a plan: a still avatar and the text, but no banner, no moving
    // avatar and no profile of its own in a server.
    store.apply(
        Event::Perks(rune_model::Perks {
            upgrade: Some("Nitro".into()),
            ..Default::default()
        }),
        Instant::now(),
    );
    let everywhere = store.profile_abilities(None);
    assert!(everywhere.images.allowed() && everywhere.text.allowed());
    assert_eq!(
        everywhere.banner.reason(),
        Some("Necesitas Nitro para usar un banner")
    );
    assert_eq!(
        everywhere.animated.reason(),
        Some("Necesitas Nitro para usar un avatar animado")
    );
    let taller = server_named(&store, "Taller de Rust");
    let there = store.profile_abilities(Some(taller));
    assert!(there.nick.allowed(), "the name there is free");
    for permit in [&there.text, &there.images, &there.banner, &there.animated] {
        assert_eq!(
            permit.reason(),
            Some("Necesitas Nitro para usar un perfil propio en cada servidor")
        );
    }
    // A plan with a moving avatar and no banner, unnamed.
    store.apply(
        Event::Perks(rune_model::Perks {
            animated_avatar: true,
            ..Default::default()
        }),
        Instant::now(),
    );
    let everywhere = store.profile_abilities(None);
    assert!(everywhere.animated.allowed());
    assert_eq!(
        everywhere.banner.reason(),
        Some("Tu cuenta no puede usar un banner")
    );
    // A provider without pictures says that first.
    let mut info = store.provider.clone().unwrap();
    info.capabilities.profile_images = false;
    store.provider = Some(info);
    assert_eq!(
        store.profile_abilities(None).banner.reason(),
        Some("Este proveedor no permite cambiar el avatar ni el banner.")
    );
}

#[test]
fn a_server_tag_quarantine_restricts_with_its_own_reason() {
    let (provider, mut store) = simulated();
    let taller = server_named(&store, "Taller de Rust");
    let mut member = store.member_of(taller, provider.me()).unwrap().clone();
    member.restrictions.tag_quarantined = true;
    store.apply(Event::MemberChanged(member), Instant::now());
    assert_eq!(store.restriction(taller), Some(Restriction::TagQuarantined));
    let general = store
        .channels
        .values()
        .find(|c| c.server == Some(taller) && c.name == "general")
        .unwrap()
        .id;
    assert!(store
        .abilities(general)
        .unwrap()
        .write
        .reason()
        .is_some_and(|reason| reason.contains("etiqueta de servidor")));
}

#[test]
fn a_saved_profile_replaces_its_scope_and_a_server_avatar_comes_first() {
    let (provider, mut store) = simulated();
    let me = provider.me();
    let taller = server_named(&store, "Taller de Rust");
    // A profile loaded before, then saved across servers and in one.
    store.profiles.push(UserProfile {
        user: me,
        server: Some(taller),
        about: Some("antes".into()),
        server_about: Some("antes aquí".into()),
        mutual_servers: Some(Vec::new()),
        ..Default::default()
    });
    let applied = store.apply(
        Event::ProfileChanged(UserProfile {
            user: me,
            about: Some("ahora".into()),
            banner: ProfileBanner::Absent,
            ..Default::default()
        }),
        Instant::now(),
    );
    assert_eq!(applied, Applied::ProfileSaved { server: None });
    let kept = store.profile(me, Some(taller)).unwrap();
    assert_eq!(kept.about.as_deref(), Some("ahora"));
    assert_eq!(
        kept.server_about.as_deref(),
        Some("antes aquí"),
        "not its scope"
    );
    assert_eq!(kept.mutual_servers, Some(Vec::new()), "kept");
    let applied = store.apply(
        Event::ProfileChanged(UserProfile {
            user: me,
            server: Some(taller),
            server_about: None,
            nick: Some("Lab".into()),
            ..Default::default()
        }),
        Instant::now(),
    );
    assert_eq!(
        applied,
        Applied::ProfileSaved {
            server: Some(taller)
        }
    );
    let kept = store.profile(me, Some(taller)).unwrap();
    assert_eq!(kept.server_about, None, "taken away there");
    assert_eq!(kept.about.as_deref(), Some("ahora"));
    // Someone else's is not the account's to save.
    let someone = UserProfile {
        user: UserId(10),
        ..Default::default()
    };
    assert_eq!(
        store.apply(Event::ProfileChanged(someone), Instant::now()),
        Applied::Nothing
    );

    let picture = |url: &str| RemoteImage {
        url: url.into(),
        kind: ImageKind::Artwork,
    };
    // Without one of their own, the service's default shows everywhere.
    let mut user = store.user(me).unwrap().clone();
    user.avatar = None;
    user.default_avatar = Some(picture("https://a.invalid/predeterminado.png"));
    store.apply(Event::UserChanged(user.clone()), Instant::now());
    assert_eq!(
        store.avatar_in(Some(taller), me).map(|a| a.url.as_str()),
        Some("https://a.invalid/predeterminado.png")
    );
    user.avatar = Some(picture("https://a.invalid/cuenta.png"));
    store.apply(Event::UserChanged(user), Instant::now());
    assert_eq!(
        store.avatar_in(Some(taller), me).map(|a| a.url.as_str()),
        Some("https://a.invalid/cuenta.png")
    );
    let mut member = store.member_of(taller, me).unwrap().clone();
    member.avatar = Some(picture("https://a.invalid/servidor.png"));
    store.apply(Event::MemberChanged(member), Instant::now());
    assert_eq!(
        store.avatar_in(Some(taller), me).map(|a| a.url.as_str()),
        Some("https://a.invalid/servidor.png")
    );
    assert_eq!(
        store.avatar_in(None, me).map(|a| a.url.as_str()),
        Some("https://a.invalid/cuenta.png")
    );
}
