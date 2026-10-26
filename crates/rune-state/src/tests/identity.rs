//! Presence, identity and expressions (P5): what the plan lets the account
//! use where, the packs, and the marks a member wears over their account's.

use super::*;
use rune_model::{
    CustomEmoji, EmojiId, Identity, ImageKind, Perks, RemoteImage, Sticker, StickerId, StickerPack,
};
use rune_provider::mock::{MockConfig, MockProvider};
use rune_provider::Provider;

/// The simulation's store, and a text channel of its first server.
fn simulated() -> (Store, ChannelId) {
    simulated_with(Permissions::NONE)
}

/// The same, without `taken` in that channel.
fn simulated_with(taken: Permissions) -> (Store, ChannelId) {
    let provider = MockProvider::new(MockConfig {
        seed: 11,
        history_end: Timestamp(1_790_000_000_000),
        clock: || Timestamp(1_790_000_100_000),
        latency: Duration::ZERO,
        ambient: false,
        replies: false,
    });
    let mut store = Store::new();
    store.provider = Some(provider.info());
    let mut snapshot = provider.snapshot();
    let channel = snapshot
        .channels
        .iter_mut()
        .find(|channel| channel.server == Some(ServerId(1)) && channel.kind.has_messages())
        .unwrap();
    channel.permissions = channel.permissions.without(taken);
    let here = channel.id;
    store.apply(Event::Ready(Box::new(snapshot)), Instant::now());
    (store, here)
}

fn picture(name: &str) -> RemoteImage {
    RemoteImage {
        url: format!("https://cdn.example.test/{name}.png"),
        kind: ImageKind::Artwork,
    }
}

#[test]
fn the_plan_decides_other_servers_and_animated_expressions_and_packs_go_anywhere() {
    let (mut store, here) = simulated();
    let emoji = |server: u64, animated: bool| CustomEmoji {
        id: EmojiId(5),
        server: ServerId(server),
        name: "ok".into(),
        insert: ":ok:".into(),
        animated,
        available: true,
        roles: Vec::new(),
        image: None,
    };
    // Unknown perks: only the server's own, as before.
    assert!(store.emoji_perk(here, &emoji(1, true)).allowed());
    assert_eq!(
        store.emoji_perk(here, &emoji(2, false)).reason(),
        Some("Usalo dentro de su servidor")
    );
    let applied = store.apply(Event::Perks(Perks::default()), Instant::now());
    assert_eq!(applied, Applied::Perks);
    assert_eq!(
        store.emoji_perk(here, &emoji(1, true)).reason(),
        Some("Tu cuenta no puede usar emojis animados")
    );
    store.apply(
        Event::Perks(Perks {
            emoji_anywhere: true,
            animated_emoji: true,
            stickers_anywhere: false,
            upgrade: Some("Plus".into()),
            ..Perks::default()
        }),
        Instant::now(),
    );
    assert!(store.emoji_perk(here, &emoji(2, true)).allowed());
    assert_eq!(
        store.sticker_perk(here, Some(ServerId(2))).reason(),
        Some("Necesitas Plus para usar stickers de otros servidores aquí")
    );
    assert!(store.sticker_perk(here, Some(ServerId(1))).allowed());
    assert!(store.sticker_perk(here, None).allowed(), "a pack's");
    // Packs are kept within bounds and found by their stickers.
    store.apply(
        Event::StickerPacks(vec![StickerPack {
            id: 1,
            name: "Saludos".into(),
            stickers: vec![
                Sticker {
                    id: StickerId(77),
                    name: "Hola".into(),
                    image: None,
                    available: true,
                };
                rune_model::identity::MAX_PACK_STICKERS + 5
            ],
        }]),
        Instant::now(),
    );
    assert_eq!(
        store.sticker_packs().unwrap()[0].stickers.len(),
        rune_model::identity::MAX_PACK_STICKERS
    );
    assert!(matches!(
        store.known_sticker(StickerId(77)),
        Some((None, _))
    ));
    assert!(matches!(
        store.known_sticker(StickerId(9_100)),
        Some((Some(ServerId(2)), _))
    ));
    // Another account starts over.
    let mut other = Snapshot {
        me: store.me_user().cloned(),
        ..Default::default()
    };
    if let Some(me) = other.me.as_mut() {
        me.id = UserId(999);
    }
    store.apply(Event::Ready(Box::new(other)), Instant::now());
    assert!(store.perks().is_none() && store.sticker_packs().is_none());
}

#[test]
fn without_the_channels_permission_another_servers_expressions_say_why_first() {
    let (mut store, here) =
        simulated_with(Permissions::EXTERNAL_EMOJI | Permissions::EXTERNAL_STICKERS);
    store.apply(
        Event::Perks(Perks {
            emoji_anywhere: true,
            animated_emoji: true,
            stickers_anywhere: true,
            upgrade: None,
            ..Perks::default()
        }),
        Instant::now(),
    );
    let emoji = |server: u64| CustomEmoji {
        id: EmojiId(5),
        server: ServerId(server),
        name: "ok".into(),
        insert: ":ok:".into(),
        animated: false,
        available: true,
        roles: Vec::new(),
        image: None,
    };
    assert!(
        store.emoji_perk(here, &emoji(1)).allowed(),
        "its own server's"
    );
    let reason = store.emoji_perk(here, &emoji(2));
    assert!(
        reason
            .reason()
            .is_some_and(|reason| reason.contains("usar emojis de otros servidores")),
        "{reason:?}"
    );
    assert!(store.sticker_perk(here, Some(ServerId(1))).allowed());
    assert!(store.sticker_perk(here, None).allowed(), "a pack's");
    assert!(store
        .sticker_perk(here, Some(ServerId(2)))
        .reason()
        .is_some_and(|reason| reason.contains("usar stickers de otros servidores")));
    // With them, the plan decides as before.
    let (mut open, here) = simulated();
    open.apply(Event::Perks(Perks::default()), Instant::now());
    assert_eq!(
        open.emoji_perk(here, &emoji(2)).reason(),
        Some("Tu cuenta no puede usar emojis de otros servidores aquí")
    );
}

#[test]
fn from_the_local_store_another_servers_permission_is_unknown() {
    let (mut store, here) = simulated_with(Permissions::EXTERNAL_STICKERS);
    store.apply(
        Event::Perks(Perks {
            stickers_anywhere: true,
            ..Perks::default()
        }),
        Instant::now(),
    );
    assert!(!store.sticker_perk(here, Some(ServerId(2))).allowed());
    // A cache may be older than the bit: the plan decides (2026-10-18).
    store.from_cache = true;
    assert!(store.sticker_perk(here, Some(ServerId(2))).allowed());
}

#[test]
fn a_members_marks_cover_their_accounts_and_the_highest_role_with_a_mark_shows() {
    let (mut store, _) = simulated();
    let server = ServerId(1);
    let ada = store
        .members(server)
        .iter()
        .find(|member| member.user != rune_provider::mock::ME)
        .unwrap()
        .user;
    let mut account = store.user(ada).unwrap().clone();
    account.identity = Identity {
        decoration: Some(picture("cuenta")),
        ..Default::default()
    };
    store.apply(Event::UserChanged(account), Instant::now());
    assert_eq!(
        store.identity_in(Some(server), ada).decoration,
        Some(picture("cuenta"))
    );
    let mut member = store.member_of(server, ada).unwrap().clone();
    member.identity.decoration = Some(picture("servidor"));
    member.roles = store
        .server(server)
        .unwrap()
        .assignable_roles()
        .iter()
        .map(|role| role.id)
        .collect();
    store.apply(Event::MemberChanged(member), Instant::now());
    assert_eq!(
        store.identity_in(Some(server), ada).decoration,
        Some(picture("servidor"))
    );
    assert_eq!(
        store.identity_in(None, ada).decoration,
        Some(picture("cuenta")),
        "outside the server, the account's"
    );
    // Moderación (3) has an emoji, Colaboración (2) a picture: the higher shows.
    let mark = store.role_mark(ada, server).unwrap();
    assert_eq!(mark.name, "Moderación");
}
