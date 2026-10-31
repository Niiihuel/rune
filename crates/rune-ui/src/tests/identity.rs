//! Presence, identity and expressions (P5): the cards of what someone does
//! with their times, the marks around names, the badges and accounts of a
//! profile, and the picker with the plan's limits, the packs and the
//! provider's own GIFs. The simulation answers; nothing reaches a service.

use rune_components::identity::{ActivityTime, Platform, RoleMark};
use rune_components::Look;
use rune_model::{
    Activity, ActivityKind, Perks, Platforms, Presence, Status, Sticker, StickerId, StickerPack,
    UserId,
};
use rune_provider::{GifAttribution, GifCategory, GifItem, GifPage, GifQuery, GifResult};

use super::*;

const PERSON: UserId = UserId(17);

fn act(host: &mut Host, id: &str) {
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &limn::UiAction::Clicked { id: id.into() },
        host.now,
    );
    host.run(intents);
}

/// Someone playing for a while and listening to a track, on the web.
fn busy(host: &mut Host) {
    let wall = host.store.wall();
    host.event(Event::PresenceChanged {
        user: PERSON,
        presence: Presence {
            status: Status::Online,
            custom: None,
            custom_emoji: None,
            activities: vec![
                Activity {
                    kind: ActivityKind::Playing,
                    name: "Factorio".into(),
                    detail: Some("Fábrica".into()),
                    state: None,
                    image: None,
                    small_image: None,
                    started: Some(Timestamp(wall.0 - (48 * 60 + 40) * 1000)),
                    ends: None,
                    ..Default::default()
                },
                Activity {
                    kind: ActivityKind::Listening,
                    name: "Radio".into(),
                    detail: Some("Tema".into()),
                    state: Some("Trío".into()),
                    image: None,
                    small_image: None,
                    started: Some(Timestamp(wall.0 - 175_000)),
                    ends: Some(Timestamp(wall.0 + 35_000)),
                    ..Default::default()
                },
            ],
            platforms: Platforms {
                web: true,
                mobile: true,
                ..Default::default()
            },
        },
    });
}

#[test]
fn a_card_shows_what_someone_does_with_its_times_and_where_from() {
    let mut host = Host::new();
    busy(&mut host);
    let card = crate::profile::card_of(&host.ui, &host.store, None, PERSON, &Look::DARK).unwrap();
    let identity = &card.identity;
    assert_eq!(identity.activities.len(), 2);
    assert_eq!(
        identity.activities[0].time,
        Some(ActivityTime::Elapsed("48:40 transcurridos".into()))
    );
    let Some(ActivityTime::Track {
        at,
        length,
        progress,
    }) = &identity.activities[1].time
    else {
        panic!("a track");
    };
    assert_eq!((at.as_str(), length.as_str()), ("2:55", "3:30"));
    assert!((progress - 0.8333).abs() < 0.01);
    assert_eq!(identity.platforms, [Platform::Mobile, Platform::Web]);
    // While it is shown, it moves each second.
    host.ui.profile = Some(PERSON);
    assert!(activity_tick(&host.ui, &host.store).is_some_and(|after| after.as_millis() <= 1000));
    host.ui.profile = None;
    assert_eq!(activity_tick(&host.ui, &host.store), None);
}

#[test]
fn lists_say_what_someone_does_else_their_status_with_its_emoji() {
    let mut host = Host::new();
    busy(&mut host);
    let presence = host.store.presence(PERSON);
    assert_eq!(
        crate::identity::status_line(&host.ui, &presence),
        (Some("Jugando a Factorio".into()), None),
        "what they do first, with no emoji"
    );
    let emoji = rune_model::TextPicture {
        token: "<:ok:1>".into(),
        name: "ok".into(),
        image: rune_model::RemoteImage {
            url: "https://simulacion.invalid/emoji/ok.png".into(),
            kind: Default::default(),
        },
    };
    let status = Presence {
        status: Status::Online,
        custom: Some("de vacaciones".into()),
        custom_emoji: Some(emoji.clone()),
        ..Presence::default()
    };
    let (line, shown) = crate::identity::status_line(&host.ui, &status);
    assert_eq!(line.as_deref(), Some("de vacaciones"));
    assert_eq!(shown.map(|shown| shown.name), Some("ok".into()));
    // An emoji alone.
    let alone = Presence {
        custom: None,
        ..status
    };
    let (line, shown) = crate::identity::status_line(&host.ui, &alone);
    assert!(line.is_none() && shown.is_some());
}

#[test]
fn a_private_rows_line_says_what_they_do_else_their_status_with_its_emoji() {
    let mut host = Host::new();
    host.click("home");
    // The simulation's first private conversation is with Bruno.
    let friend = UserId(11);
    let wall = host.store.wall();
    host.event(Event::PresenceChanged {
        user: friend,
        presence: Presence {
            status: Status::Online,
            activities: vec![Activity {
                kind: ActivityKind::Playing,
                name: "Factorio".into(),
                detail: None,
                state: None,
                image: None,
                small_image: None,
                started: Some(wall),
                ends: None,
                ..Default::default()
            }],
            ..Presence::default()
        },
    });
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "Jugando a Factorio"),
        "{painted:?}"
    );
    let emoji = rune_model::TextPicture {
        token: "<:ok:1>".into(),
        name: "ok".into(),
        image: rune_model::RemoteImage {
            url: "https://simulacion.invalid/emoji/ok.png".into(),
            kind: Default::default(),
        },
    };
    host.event(Event::PresenceChanged {
        user: friend,
        presence: Presence {
            status: Status::Online,
            custom: Some("de vacaciones".into()),
            custom_emoji: Some(emoji.clone()),
            ..Presence::default()
        },
    });
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "de vacaciones"),
        "{painted:?}"
    );
    assert!(wanted_images(&host.ui, &host.store, &host.tree).contains(&emoji.image));
}

#[test]
fn an_emoji_opens_its_card_which_says_what_it_is_and_copies_it() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let messages = host.store.timeline(channel).unwrap().messages();
    let index = messages
        .iter()
        .position(|message| message.content.starts_with("Quedó andando"))
        .expect("the simulation's line with server emoji");
    let said = messages[index].clone();
    let token = said.text_references[0].source.clone();
    host.tree.scroll_to_index(&timeline_id(channel), index);
    // A click on a server's emoji: its card, under it.
    let id = format!("msg:{}:emoji:0:{token}", said.id);
    host.click(&id);
    // Placed under it on a layout, shown on the next.
    host.frame();
    let painted = host.painted().join(" ");
    assert!(painted.contains(":chispa:"), "{painted}");
    assert!(painted.contains("Un emoji de un servidor."), "{painted}");
    assert!(painted.contains("no está en esta"), "{painted}");
    // Once a server of the session has it, by its id, under the name it
    // has now (2026-10-18: the token's, with its old name, was compared).
    let mut origin = host.store.servers()[1].clone();
    origin.emojis.push(rune_model::CustomEmoji {
        id: rune_model::EmojiId(900_001),
        server: origin.id,
        name: "chispa_nueva".into(),
        insert: ":chispa_nueva:".into(),
        animated: true,
        available: true,
        roles: Vec::new(),
        image: None,
    });
    let from = rune_i18n::tr!("De {server}", server = origin.name);
    host.event(Event::ServerChanged(origin));
    let painted = host.painted().join(" ");
    assert!(painted.contains(&from), "{painted}");
    assert!(painted.contains(":chispa_nueva:"), "{painted}");
    // Copying it copies it as written, and closes it.
    let copied = update(
        &mut host.ui,
        &mut host.store,
        &limn::UiAction::Clicked {
            id: rune_components::emoji_card::ids::COPY.into(),
        },
        host.now,
    );
    assert!(copied.contains(&Intent::Copy(token)), "{copied:?}");
    assert!(!host
        .painted()
        .join(" ")
        .contains("Un emoji de un servidor."));
    // A secondary click on a reaction: a standard emoji's card, there.
    let reacted = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .iter()
        .find_map(|message| Some((message.id, message.reactions.first()?.emoji.clone())))
        .expect("a reaction in the simulation");
    let intents = secondary_click(
        &mut host.ui,
        &host.store,
        &format!("react:{}:{}", reacted.0, reacted.1),
        Point::new(300.0, 300.0),
    )
    .expect("the reaction's card");
    host.run(intents);
    let painted = host.painted().join(" ");
    assert!(painted.contains("emoji estándar"), "{painted}");
    assert!(painted.contains(&rune_components::emoji::code(&reacted.1)));
    // Another conversation closes it (2026-10-18: it came back there).
    let other = host.channel_named("ayuda");
    host.click(&format!("channel:{other}"));
    host.frame();
    assert!(host.ui.emoji_card.is_none());
}

#[test]
fn marks_follow_the_member_and_their_roles_and_badges_open_their_links() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let server = host.ui.server.unwrap();
    // Ada wears a decoration, a nameplate and a tag; her role has an emoji.
    let ada = UserId(10);
    let marks = crate::identity::marks(&host.ui, &host.store, Some(server), ada, &Look::DARK);
    assert_eq!(
        marks.tag.as_ref().map(|tag| tag.text.as_str()),
        Some("RUST")
    );
    assert!(marks.plate.is_some());
    if host
        .store
        .member_of(server, ada)
        .is_some_and(|m| !m.roles.is_empty())
    {
        assert!(marks.role.is_some());
    }
    let emoji_role = host
        .store
        .server(server)
        .unwrap()
        .roles
        .iter()
        .find(|role| role.style.emoji.is_some())
        .cloned()
        .unwrap();
    assert!(matches!(
        crate::identity::role_mark(&host.ui, &emoji_role),
        Some(RoleMark::Emoji { emoji, .. }) if emoji == "🛡️"
    ));
    // A gradient follows the role's colors only while role colors show.
    let gradient = host
        .store
        .server(server)
        .unwrap()
        .roles
        .iter()
        .any(|role| role.style.gradient.len() == 2);
    assert!(gradient, "a role of three colors");
    // Her profile, read, shows her badges; one opens its link.
    host.ui.profile = Some(ada);
    assert!(host.store.begin_profile(ada, Some(server), 7).allowed());
    for event in host.provider.manage(Command::LoadProfile {
        request: 7,
        user: ada,
        server: Some(server),
        effect: true,
    }) {
        host.event(event);
    }
    let card =
        crate::profile::card_of(&host.ui, &host.store, Some(server), ada, &Look::DARK).unwrap();
    assert_eq!(card.identity.badges.len(), 2);
    assert!(card.identity.badges[0].link);
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &limn::UiAction::Clicked {
            id: "pp:badge=0".into(),
        },
        host.now,
    );
    assert!(matches!(&intents[..], [Intent::OpenUrl(url)] if url.contains("insignias/pionera")));
    // Her connected accounts are in the full profile.
    let accounts = crate::identity::connections(&host.store, ada, Some(server));
    assert_eq!(accounts[0].label, "GitHub");
    assert!(accounts[0].verified && accounts[0].link);
    let _ = channel;
}

#[test]
fn the_picker_offers_other_servers_and_packs_with_the_plans_reasons() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    // Before the plan is known only this server's expressions are offered.
    host.click("composer:emoji");
    assert!(
        crate::expressions::custom_items(&host.ui, &host.store, channel)
            .iter()
            .all(|item| item.section.is_none())
    );
    host.event(Event::Perks(Perks {
        emoji_anywhere: true,
        animated_emoji: true,
        stickers_anywhere: false,
        upgrade: Some("Plus".into()),
        ..Perks::default()
    }));
    let emoji = crate::expressions::custom_items(&host.ui, &host.store, channel);
    let elsewhere: Vec<_> = emoji.iter().filter(|item| item.section.is_some()).collect();
    assert!(!elsewhere.is_empty(), "the owned server's emoji");
    assert!(
        elsewhere.iter().any(|item| item.unavailable.is_none()),
        "usable anywhere with the plan"
    );
    // Stickers of another server need the plan; the tab reads the packs.
    let from = host.sent.len();
    act(&mut host, "composer:emoji:tab=stickers");
    assert!(host.sent[from..]
        .iter()
        .any(|command| matches!(command, Command::LoadStickerPacks)));
    host.event(Event::StickerPacks(vec![StickerPack {
        id: 1,
        name: "Saludos".into(),
        stickers: vec![Sticker {
            id: StickerId(9_301),
            name: "Hola".into(),
            image: None,
            available: true,
        }],
    }]));
    let stickers = crate::expressions::sticker_items(&host.ui, &host.store, channel);
    let other = stickers
        .iter()
        .find(|item| item.id == 9_100)
        .expect("the owned server's sticker");
    assert_eq!(
        other.unavailable.as_deref(),
        Some("Necesitas Plus para usar stickers de otros servidores aquí")
    );
    let pack = stickers.iter().find(|item| item.id == 9_301).unwrap();
    assert_eq!(pack.section.as_deref(), Some("Saludos"));
    assert!(pack.unavailable.is_none(), "a pack's goes anywhere");
    // Chosen and sent: the pack's sticker leaves as itself.
    act(&mut host, "composer:emoji:sticker=9301");
    act(&mut host, "composer:sticker:send");
    assert!(host.sent.iter().any(|command| matches!(
        command,
        Command::SendSticker { sticker, .. } if *sticker == StickerId(9_301)
    )));
    // An animated emoji without the plan says why.
    host.event(Event::Perks(Perks::default()));
    let emoji = crate::expressions::custom_items(&host.ui, &host.store, channel);
    assert!(emoji
        .iter()
        .filter(|item| item.section.is_some())
        .all(|item| item.unavailable.is_some()));
}

#[test]
fn the_gif_tab_searches_the_provider_with_categories_and_suggestions() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    host.click("composer:emoji");
    let from = host.sent.len();
    act(&mut host, "composer:emoji:tab=gif");
    let trending = host.sent[from..].iter().find_map(|command| match command {
        Command::SearchGifs {
            request,
            query: GifQuery::Trending,
            ..
        } => Some(*request),
        _ => None,
    });
    let request = trending.expect("trending on opening the tab");
    assert!(host.sent[from..]
        .iter()
        .any(|command| matches!(command, Command::SuggestGifs { query, .. } if query.is_empty())));
    let item = |name: &str| GifItem {
        id: name.into(),
        title: name.into(),
        creator: None,
        url: format!("https://simulacion.invalid/gifs/{name}.gif"),
        preview: rune_model::RemoteImage {
            url: format!("https://simulacion.invalid/gifs/{name}.png"),
            kind: Default::default(),
        },
        thumbnail: None,
    };
    host.event(Event::GifResults(GifResult {
        request,
        query: GifQuery::Trending,
        offset: 0,
        result: Ok(GifPage {
            items: vec![item("hola")],
            next_offset: None,
            attribution: GifAttribution {
                label: "GIF simulados".into(),
                url: String::new(),
                image: None,
            },
            categories: vec![GifCategory {
                name: "gatos".into(),
                image: None,
            }],
        }),
    }));
    let view = crate::gifs::view(&host.ui, &host.store);
    assert_eq!(view.items.len(), 1);
    assert_eq!(view.categories[0].0, "gatos");
    assert!(view.attribution.is_none(), "no address to credit");
    // A category searches its name.
    let from = host.sent.len();
    act(&mut host, "composer:emoji:gif:category=0");
    assert!(host.sent[from..].iter().any(|command| matches!(
        command,
        Command::SearchGifs { query: GifQuery::Search(text), .. } if text == "gatos"
    )));
    // Writing asks what to search for; the answer for it shows.
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &limn::UiAction::InputChanged {
            id: crate::gifs::QUERY.into(),
            value: "ga".into(),
        },
        host.now,
    );
    let suggest = intents.iter().find_map(|intent| match intent {
        Intent::Command(Command::SuggestGifs { request, .. }) => Some(*request),
        _ => None,
    });
    host.run(intents);
    host.event(Event::GifSuggestions {
        request: suggest.unwrap(),
        query: "ga".into(),
        terms: vec!["gato".into(), "gatos".into()],
    });
    assert_eq!(
        crate::gifs::view(&host.ui, &host.store).suggestions,
        ["gato", "gatos"]
    );
    let _ = channel;
}
