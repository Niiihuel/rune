//! The card of a member as a real account first saw it (2026-10-17): a
//! server of thousands whose list keeps changing, someone with a
//! decoration, a nameplate and badges, people in voice, a bio with the
//! server's emoji. The simulation answers; nothing reaches a service.

use rune_model::{ListGroup, ListRow, MemberList, ServerId, UserId};

use super::*;

/// Where the member card floats, if it does.
fn card_rect(host: &Host) -> Option<limn::Rect> {
    let surface = format!(
        "popover_surface:{}",
        rune_components::profile::ids::MEMBER_DISMISS
    );
    host.tree
        .frame()
        .regions
        .iter()
        .find(|region| region.id == surface)
        .map(|region| region.rect)
}

/// `#general` with the member list open, and the people it shows.
fn listed(host: &mut Host) -> (ServerId, ChannelId, Vec<UserId>) {
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    host.ui.members_open = true;
    host.frame();
    let people = host
        .store
        .member_groups(server)
        .iter()
        .flat_map(|group| group.members.clone())
        .collect();
    (server, general, people)
}

/// The provider's list of a server of 60 000 people around, `shift` rows
/// down from the top, as people above come and go; `hole` is not sent.
fn busy_list(
    server: ServerId,
    channel: ChannelId,
    people: &[UserId],
    shift: u32,
    hole: Option<UserId>,
) -> MemberList {
    MemberList {
        server,
        channel,
        groups: vec![(ListGroup::Online, 60_000)],
        rows: people
            .iter()
            .enumerate()
            .filter(|(_, user)| Some(**user) != hole)
            .map(|(at, user)| (at as u32 + 1 + shift, ListRow::Member(*user)))
            .collect(),
    }
}

#[test]
fn a_members_card_holds_its_place_while_a_busy_list_moves_under_it() {
    let mut host = Host::new();
    let (server, general, people) = listed(&mut host);
    let ada = people[0];
    host.event(Event::MemberListChanged(busy_list(
        server, general, &people, 0, None,
    )));
    host.click(&format!("member:{ada}"));
    host.frame();
    let opened = card_rect(&host).expect("the card opens beside her row");
    // People above come and go, her row moves, and an update leaves her
    // out for a moment: the card stays where it opened, whole.
    for step in 1..8u32 {
        let hole = (step % 3 == 0).then_some(ada);
        host.event(Event::MemberListChanged(busy_list(
            server,
            general,
            &people,
            step % 2,
            hole,
        )));
        host.frame();
        assert_eq!(card_rect(&host), Some(opened), "step {step}");
    }
    // Closing it lets go of the place: the next card opens at its row.
    host.ui.profile = None;
    host.frame();
    assert_eq!(host.ui.profiles.member_at, None);
    assert_eq!(card_rect(&host), None);
}

#[test]
fn an_authors_card_holds_its_place_while_new_messages_push_the_conversation_up() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let surface = format!(
        "popover_surface:{}",
        rune_components::profile::ids::AUTHOR_DISMISS
    );
    let rect = |host: &Host| {
        host.tree
            .frame()
            .regions
            .iter()
            .find(|region| region.id == surface)
            .map(|region| region.rect)
    };
    // The highest name in view: its card is not held at the bottom edge.
    let me = host.store.me();
    let frame = host.frame();
    let message = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .iter()
        .filter(|message| Some(message.author) != me)
        .filter_map(|message| {
            let name = rune_components::profile::ids::author_name(message.id.0);
            let at = frame.regions.iter().find(|region| region.id == name)?.rect;
            (at.y > 60.0).then_some((at.y, message.clone()))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .unwrap()
        .1;
    host.click(&rune_components::profile::ids::author_name(message.id.0));
    host.frame();
    let opened = rect(&host).expect("the card opens beside the name");
    // Messages keep coming while it shows: the conversation moves up.
    let name = rune_components::profile::ids::author_name(message.id.0);
    let name_at = |host: &Host| {
        host.tree
            .frame()
            .regions
            .iter()
            .find(|region| region.id == name)
            .map(|region| region.rect.y)
    };
    let before = name_at(&host);
    for step in 0..4u64 {
        let newest = host.store.timeline(general).unwrap().newest().unwrap();
        let mut next = (*newest).clone();
        next.id = rune_model::MessageId(newest.id.0 + 1);
        next.author = message.author;
        next.content = format!("mensaje nuevo {step}\nen dos líneas");
        host.event(Event::MessageCreated {
            message: next,
            nonce: None,
        });
        host.frame();
        assert_eq!(rect(&host), Some(opened), "step {step}");
    }
    assert_ne!(name_at(&host), before, "the conversation did move");
}

#[test]
fn a_voice_card_asks_for_the_profile_and_note_and_a_failed_note_says_so() {
    let mut host = Host::new();
    host.open_channel("general");
    let server = host.ui.server.unwrap();
    let me = host.store.me();
    // Someone already in a voice channel of the server.
    let (room, person) = host
        .store
        .channel_tree(server)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .find_map(|channel| {
            let state = host
                .store
                .voice_in(channel.id)
                .into_iter()
                .find(|state| Some(state.user) != me)?;
            Some((channel.id, state.user))
        })
        .expect("someone in voice");
    let row = rune_components::voice::ids::row(person.0);
    // Their face in the column is asked for, as the member list's are.
    host.frame();
    if let Some(face) = host.store.avatar_in(Some(server), person).cloned() {
        assert!(wanted_images(&host.ui, &host.store, &host.tree).contains(&face));
    }
    let from = host.sent.len();
    host.click(&row);
    assert_eq!(host.ui.voice.card, Some((person, room, false)));
    assert!(
        host.sent[from..].iter().any(|command| matches!(
            command,
            Command::LoadProfile { user, server: Some(at), .. } if *user == person && *at == server
        )),
        "{:?}",
        &host.sent[from..]
    );
    assert!(host.sent[from..]
        .iter()
        .any(|command| *command == Command::LoadNote { user: person }));
    // Their presence is kept current while the card shows: in a large
    // server they may be in no row of the list.
    host.frame();
    assert!(host.sent[from..].contains(&Command::WatchPeople {
        server,
        people: vec![person],
    }));
    // On the card the note is an icon that opens the full profile, where
    // it is (2026-10-18). It cannot be read: its place says it, not
    // «Cargando la nota…».
    host.click(&format!(
        "{}{person}",
        rune_components::profile::ids::FULL_NOTE
    ));
    assert_eq!(host.ui.profiles.full.map(|(user, _)| user), Some(person));
    host.event(Event::Failed {
        failure: rune_provider::Failure::Note { user: person },
        reason: "403".into(),
        ambiguous: false,
    });
    let painted = host.painted();
    assert!(
        painted.contains(&"No se pudo cargar la nota.".to_owned()),
        "{painted:?}"
    );
    assert!(!painted.contains(&"Cargando la nota…".to_owned()));
    assert!(host.ui.notices.is_empty(), "no toast: the profile says it");
    host.ui.profiles.full = None;
    // Closed, nobody is watched; opened again, the note is asked for again.
    host.click(&row);
    host.frame();
    assert_eq!(
        host.sent.last(),
        Some(&Command::WatchPeople {
            server,
            people: Vec::new(),
        })
    );
    let from = host.sent.len();
    host.click(&row);
    assert!(host.sent[from..]
        .iter()
        .any(|command| *command == Command::LoadNote { user: person }));
}

#[test]
fn a_bios_emoji_and_an_effect_show_once_their_pictures_arrived() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let server = host.store.channel(general).unwrap().server.unwrap();
    host.ui.members_open = true;
    host.frame();
    // Ada: a bio naming two emoji of the server, and an effect (simulated).
    let ada = UserId(10);
    host.click(&format!("member:{ada}"));
    let request = host
        .sent
        .iter()
        .rev()
        .find_map(|command| match command {
            Command::LoadProfile { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap();
    for event in host.provider.manage(Command::LoadProfile {
        request,
        user: ada,
        server: Some(server),
        effect: true,
    }) {
        host.event(event);
    }
    host.frame();
    let loaded = host.store.profile(ada, Some(server)).unwrap().clone();
    let wanted = wanted_images(&host.ui, &host.store, &host.tree);
    for picture in &loaded.pictures {
        assert!(wanted.contains(&picture.image), "{picture:?}");
    }
    let effect = loaded.effect.clone().unwrap();
    assert!(wanted.contains(&effect.layers[0].image));
    let card = |host: &Host| {
        crate::profile::card_of(
            &host.ui,
            &host.store,
            Some(server),
            ada,
            &rune_components::Look::DARK,
        )
        .unwrap()
    };
    // Nothing of the effect, and the emoji's names, while they load.
    assert_eq!(card(&host).identity.effect, None);
    assert!(card(&host)
        .identity
        .pictures
        .iter()
        .all(|picture| picture.image.is_none()));
    let arrived = super::attachments::image();
    for source in loaded
        .pictures
        .iter()
        .map(|picture| &picture.image)
        .chain([&effect.layers[0].image])
    {
        host.ui.images.insert(source.clone(), arrived.clone());
    }
    let shown = card(&host);
    assert!(shown
        .identity
        .pictures
        .iter()
        .all(|picture| picture.image.is_some()));
    assert_eq!(
        shown.identity.effect.map(|effect| effect.layers.len()),
        Some(1)
    );
    // Both move while the card shows.
    let playing = playable_images(&host.ui, &host.store, &host.tree, false);
    assert!(playing.contains(&loaded.pictures[0].image));
    assert!(playing.contains(&effect.layers[0].image));
}

/// The middle of the region `id` of the kind `kind`.
fn middle(host: &Host, id: &str, kind: limn::HitKind) -> Point {
    let rect = host
        .tree
        .frame()
        .regions
        .iter()
        .find(|region| region.id == id && region.kind == kind)
        .unwrap_or_else(|| panic!("{id}"))
        .rect;
    Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

#[test]
fn a_members_card_closes_when_the_person_scrolls_the_list_not_when_it_moves_by_itself() {
    let mut host = Host::new();
    let (server, general, people) = listed(&mut host);
    let ada = people[0];
    host.event(Event::MemberListChanged(busy_list(
        server, general, &people, 0, None,
    )));
    host.click(&format!("member:{ada}"));
    host.frame();
    let card = card_rect(&host).expect("open");
    // The list moves and rebuilds by itself: the card stays.
    host.event(Event::MemberListChanged(busy_list(
        server, general, &people, 1, None,
    )));
    host.frame();
    assert_eq!(host.ui.profile, Some(ada));
    // The wheel over the card itself (its body scrolls) leaves it open.
    let inside = Point::new(card.x + card.width / 2.0, card.y + card.height / 2.0);
    host.send(UiEvent::Scroll {
        position: inside,
        delta_y: -40.0,
    });
    assert_eq!(host.ui.profile, Some(ada));
    // The person scrolls the list: it closes, as the official popout does.
    let list = middle(&host, crate::navigation::MEMBERS, limn::HitKind::Scroll);
    host.send(UiEvent::Scroll {
        position: list,
        delta_y: -40.0,
    });
    assert_eq!(host.ui.profile, None);
    host.frame();
    assert_eq!(card_rect(&host), None);
}

#[test]
fn an_authors_card_closes_when_the_person_scrolls_the_conversation() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let me = host.store.me();
    let frame = host.frame();
    let message = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .iter()
        .filter(|message| Some(message.author) != me)
        .find(|message| {
            let name = rune_components::profile::ids::author_name(message.id.0);
            frame.regions.iter().any(|region| region.id == name)
        })
        .unwrap()
        .id;
    host.click(&rune_components::profile::ids::author_name(message.0));
    host.frame();
    assert!(host.ui.profiles.author.is_some());
    // Far from the card, which opens to the right of the name.
    let timeline = crate::timeline_id(general);
    let view = host
        .tree
        .frame()
        .regions
        .iter()
        .find(|region| region.id == timeline && region.kind == limn::HitKind::Scroll)
        .unwrap()
        .rect;
    host.send(UiEvent::Scroll {
        position: Point::new(view.x + 4.0, view.y + 4.0),
        delta_y: 40.0,
    });
    assert_eq!(host.ui.profiles.author, None);
}

#[test]
fn a_decoration_and_a_nameplate_move_under_the_pointer_and_stop_away() {
    let mut host = Host::new();
    let (_, _, _) = listed(&mut host);
    // Ada (simulated) wears a decoration and a nameplate that moves.
    let ada = UserId(10);
    let marks = host.store.identity_in(host.ui.server, ada);
    let decoration = marks.decoration.clone().unwrap();
    let moving = marks.nameplate.clone().unwrap().animated.unwrap();
    host.frame();
    assert!(!playable_images(&host.ui, &host.store, &host.tree, false).contains(&decoration));
    let row = middle(&host, &format!("member:{ada}"), limn::HitKind::Button);
    host.send(UiEvent::PointerMove(row));
    host.frame();
    assert_eq!(host.ui.profiles.hovered, Some(ada));
    let playing = playable_images(&host.ui, &host.store, &host.tree, false);
    assert!(playing.contains(&decoration) && playing.contains(&moving));
    assert!(
        wanted_images(&host.ui, &host.store, &host.tree).contains(&moving),
        "the moving nameplate loads only now"
    );
    // Reduced motion: nothing plays.
    assert!(playable_images(&host.ui, &host.store, &host.tree, true).is_empty());
    host.send(UiEvent::PointerLeave);
    host.frame();
    assert_eq!(host.ui.profiles.hovered, None);
    assert!(!playable_images(&host.ui, &host.store, &host.tree, false).contains(&decoration));
    assert!(!wanted_images(&host.ui, &host.store, &host.tree).contains(&moving));
}

#[test]
fn a_messages_author_is_under_the_pointer_over_its_name_and_its_row() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let frame = host.frame();
    let (message, name) = host
        .store
        .timeline(general)
        .unwrap()
        .messages()
        .iter()
        .find_map(|message| {
            let id = rune_components::profile::ids::author_name(message.id.0);
            let at = frame.regions.iter().find(|region| region.id == id)?.rect;
            Some((message.clone(), at))
        })
        .unwrap();
    host.send(UiEvent::PointerMove(Point::new(
        name.x + name.width / 2.0,
        name.y + name.height / 2.0,
    )));
    host.frame();
    assert_eq!(host.ui.profiles.hovered, Some(message.author), "its name");
    // Over the text of its row, where no control is.
    let view = host
        .tree
        .frame()
        .regions
        .iter()
        .find(|region| {
            region.id == crate::timeline_id(general) && region.kind == limn::HitKind::Scroll
        })
        .unwrap()
        .rect;
    host.send(UiEvent::PointerMove(Point::new(
        view.x + view.width - 120.0,
        name.y + name.height + 6.0,
    )));
    host.frame();
    assert_eq!(host.ui.profiles.hovered, Some(message.author), "its row");
}

#[test]
fn hidden_cosmetics_are_neither_drawn_nor_asked_for() {
    use std::sync::atomic::{AtomicI64, Ordering};
    static NOW: AtomicI64 = AtomicI64::new(1_790_000_100_000);
    let mut host = Host::new();
    host.store.clock = Some(|| Timestamp(NOW.load(Ordering::Relaxed)));
    let (server, _, _) = listed(&mut host);
    let ada = UserId(10);
    let marks = host.store.identity_in(Some(server), ada);
    let (decoration, plate) = (
        marks.decoration.clone().unwrap(),
        marks.nameplate.clone().unwrap().image,
    );
    // Her card, with the effect (simulated) she wears over it.
    let asked = |host: &Host, from: usize| {
        host.sent[from..].iter().find_map(|command| match command {
            Command::LoadProfile {
                request,
                user,
                effect,
                ..
            } if *user == ada => Some((*request, *effect)),
            _ => None,
        })
    };
    let from = host.sent.len();
    host.click(&format!("member:{ada}"));
    let (request, effect) = asked(&host, from).unwrap();
    assert!(effect, "shown: its effect is asked for");
    for event in host.provider.manage(Command::LoadProfile {
        request,
        user: ada,
        server: Some(server),
        effect,
    }) {
        host.event(event);
    }
    host.frame();
    let layer = host
        .store
        .profile(ada, Some(server))
        .unwrap()
        .effect
        .as_ref()
        .unwrap()
        .layers[0]
        .image
        .clone();
    let wanted = wanted_images(&host.ui, &host.store, &host.tree);
    assert!(wanted.contains(&decoration) && wanted.contains(&plate) && wanted.contains(&layer));
    assert!(playable_images(&host.ui, &host.store, &host.tree, false).contains(&layer));
    host.click(&format!("member:{ada}"));
    // The switches of «Apariencia»: each one hides and keeps its choice.
    host.click("me:settings");
    host.click("us:page=appearance");
    host.click("us:jump=cosmetics");
    host.frame();
    let from = host.sent.len();
    for cosmetic in ["decorations", "nameplates", "effects"] {
        let id = format!("us:cosmetic:{cosmetic}");
        assert!(
            host.tree
                .frame()
                .regions
                .iter()
                .any(|region| region.id == id),
            "{id} shows"
        );
        let intents = update(
            &mut host.ui,
            &mut host.store,
            &UiAction::Clicked { id },
            host.now,
        );
        assert_eq!(
            intents,
            vec![Intent::SaveSetting {
                key: format!("cosmetic_{cosmetic}"),
                value: Some("false".into()),
            }]
        );
        host.frame();
    }
    assert_eq!(
        host.ui.prefs.cosmetics,
        crate::Cosmetics {
            decorations: false,
            nameplates: false,
            effects: false,
        }
    );
    assert_eq!(host.sent.len(), from, "nothing reaches the provider");
    host.ui.settings = None;
    host.frame();
    let wanted = wanted_images(&host.ui, &host.store, &host.tree);
    assert!(!wanted.contains(&decoration) && !wanted.contains(&plate));
    let row = middle(&host, &format!("member:{ada}"), limn::HitKind::Button);
    host.send(UiEvent::PointerMove(row));
    host.frame();
    assert!(!playable_images(&host.ui, &host.store, &host.tree, false).contains(&decoration));
    // Her card again, a minute on: neither its effect's pictures nor the
    // effect itself are asked for.
    NOW.fetch_add(60_000, Ordering::Relaxed);
    let from = host.sent.len();
    host.click(&format!("member:{ada}"));
    assert_eq!(asked(&host, from).map(|(_, effect)| effect), Some(false));
    host.frame();
    assert!(!wanted_images(&host.ui, &host.store, &host.tree).contains(&layer));
    let playing = playable_images(&host.ui, &host.store, &host.tree, false);
    assert!(!playing.contains(&layer) && !playing.contains(&decoration));
    let card = crate::profile::card_of(
        &host.ui,
        &host.store,
        Some(server),
        ada,
        &rune_components::Look::DARK,
    )
    .unwrap();
    assert!(card.identity.marks.decoration.is_none() && card.identity.effect.is_none());
}

#[test]
fn a_mention_in_a_message_plays_its_author_not_who_it_names() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    let ada = UserId(10);
    let timeline = host.store.timeline(general).unwrap();
    let author = timeline
        .messages()
        .iter()
        .map(|message| message.author)
        .find(|author| *author != ada && Some(*author) != host.store.me())
        .unwrap();
    let mut message = timeline.messages().last().unwrap().as_ref().clone();
    message.id = rune_model::MessageId(9_000_000_000_000_092);
    message.author = author;
    message.kind = rune_model::MessageKind::Default;
    message.content = "hola <@10>".into();
    message.mentions = vec![ada];
    message.text_references = vec![rune_model::TextReference {
        source: "<@10>".into(),
        target: rune_model::TextTarget::User(ada),
    }];
    host.event(Event::MessageCreated {
        message,
        nonce: None,
    });
    host.tree.scroll_to_end(&crate::timeline_id(general));
    let frame = host.frame();
    let view = frame
        .regions
        .iter()
        .find(|region| {
            region.id == crate::timeline_id(general) && region.kind == limn::HitKind::Scroll
        })
        .unwrap()
        .rect;
    let mention = frame
        .regions
        .iter()
        .rev()
        .find(|region| {
            region.id == format!("member:{ada}")
                && view.contains(Point::new(region.rect.x + 1.0, region.rect.y + 1.0))
        })
        .expect("the mention in the conversation")
        .rect;
    host.send(UiEvent::PointerMove(Point::new(
        mention.x + mention.width / 2.0,
        mention.y + mention.height / 2.0,
    )));
    host.frame();
    assert_eq!(
        host.tree.hovered_id(),
        Some(format!("member:{ada}").as_str())
    );
    assert_eq!(host.ui.profiles.hovered, Some(author));
}

#[test]
fn a_members_card_stands_16_px_from_its_row() {
    let mut host = Host::new();
    listed(&mut host);
    let row = format!("member:{}", UserId(10));
    host.click(&row);
    host.frame();
    let row = crate::profile::member_row(&host.tree, &row).expect("her row in the list");
    let card = card_rect(&host).expect("her card");
    let gap = row.x - (card.x + card.width);
    assert!(
        (gap - rune_components::profile::LIST_GAP).abs() < 0.5,
        "{gap}"
    );
}

#[test]
fn the_own_popout_takes_its_banner_from_the_full_profile() {
    let mut host = Host::new();
    host.open_channel("general");
    let me = host.store.me().unwrap();
    host.store.apply(
        Event::UserChanged(rune_model::User {
            banner: rune_model::ProfileBanner::Unknown,
            ..host.store.me_user().unwrap().clone()
        }),
        host.now,
    );
    let from = host.sent.len();
    host.click("me:status");
    let request = host.sent[from..]
        .iter()
        .find_map(|command| match command {
            Command::LoadProfile {
                request,
                user,
                server: None,
                ..
            } if *user == me => Some(*request),
            _ => None,
        })
        .expect("its full profile is asked for");
    let banner = rune_model::RemoteImage {
        url: "https://simulacion.invalid/banners/propio.png".into(),
        kind: rune_model::ImageKind::Banner,
    };
    host.event(Event::ProfileLoaded {
        request,
        profile: rune_model::UserProfile {
            user: me,
            banner: rune_model::ProfileBanner::Image(banner.clone()),
            ..Default::default()
        },
    });
    host.frame();
    assert!(wanted_images(&host.ui, &host.store, &host.tree).contains(&banner));
}

#[test]
fn a_kept_profile_is_shown_and_asked_for_again_after_a_minute() {
    use std::sync::atomic::{AtomicI64, Ordering};
    static NOW: AtomicI64 = AtomicI64::new(1_790_000_100_000);
    let mut host = Host::new();
    host.store.clock = Some(|| Timestamp(NOW.load(Ordering::Relaxed)));
    let (server, _, _) = listed(&mut host);
    let ada = UserId(10);
    let asked = |host: &Host, from: usize| {
        host.sent[from..].iter().find_map(|command| match command {
            Command::LoadProfile { request, user, .. } if *user == ada => Some(*request),
            _ => None,
        })
    };
    let answer = |host: &mut Host, request| {
        for event in host.provider.manage(Command::LoadProfile {
            request,
            user: ada,
            server: Some(server),
            effect: true,
        }) {
            host.event(event);
        }
    };
    let from = host.sent.len();
    host.click(&format!("member:{ada}"));
    let request = asked(&host, from).expect("asked as it opens");
    answer(&mut host, request);
    host.click(&format!("member:{ada}"));
    // Opened again within the minute: the kept one, nothing asked.
    let from = host.sent.len();
    host.click(&format!("member:{ada}"));
    assert_eq!(asked(&host, from), None);
    host.click(&format!("member:{ada}"));
    // A minute later: still shown at once, and asked for again.
    NOW.fetch_add(60_000, Ordering::Relaxed);
    let from = host.sent.len();
    host.click(&format!("member:{ada}"));
    assert!(asked(&host, from).is_some(), "asked again");
    assert!(
        host.store.profile(ada, Some(server)).is_some(),
        "shown meanwhile"
    );
}
