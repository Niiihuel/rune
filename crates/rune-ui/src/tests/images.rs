use super::*;
use rune_model::RemoteImage;

fn source(id: u64) -> RemoteImage {
    RemoteImage {
        url: format!("https://cdn.example.test/{id}.png"),
        kind: Default::default(),
    }
}

fn populated() -> Host {
    let mut host = Host::new();
    let mut snapshot = host.provider.snapshot();
    for user in &mut snapshot.users {
        user.avatar = Some(source(user.id.0));
    }
    if let Some(me) = &mut snapshot.me {
        me.avatar = Some(source(me.id.0));
    }
    let template = snapshot.servers[0].clone();
    for index in 1000..1300 {
        let mut server = template.clone();
        server.id = ServerId(index);
        server.name = format!("Server {index}");
        server.icon = Some(source(index));
        snapshot.servers.push(server);
    }
    host.event(Event::Ready(Box::new(snapshot)));
    host.frame();
    host
}

#[test]
fn artwork_requests_follow_clipped_navigation_and_visible_timeline_rows() {
    let mut host = populated();
    let first = wanted_images(&host.ui, &host.store, &host.tree);
    assert!(first.contains(&source(host.store.me().unwrap().0)));
    assert!(first.contains(&source(1000)));
    assert!(!first.contains(&source(1299)));
    assert!(
        first.len() < 50,
        "offscreen servers and history must not be eagerly fetched"
    );
    host.tree.scroll_to_end("rail");
    host.frame();
    let last = wanted_images(&host.ui, &host.store, &host.tree);
    assert!(last.contains(&source(1299)));
    assert!(!last.contains(&source(1000)));
}

#[test]
fn loaded_avatars_reach_limn_and_logout_clears_the_ui_cache() {
    let mut host = populated();
    for source in wanted_images(&host.ui, &host.store, &host.tree) {
        let image = limn::RasterImage::new(
            "0".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "Imagen",
        )
        .unwrap();
        host.ui.images.insert(source, image);
    }
    let frame = host.frame();
    assert!(
        frame
            .commands
            .iter()
            .filter(|command| matches!(command, limn::DrawCommand::RasterImage { .. }))
            .count()
            >= 2
    );
    host.event(Event::Authentication(
        rune_provider::AuthenticationState::SignedOut,
    ));
    assert!(host.ui.images.is_empty());
}

#[test]
fn a_reply_to_emoji_alone_quotes_them_at_a_lines_size() {
    use rune_model::{TextReference, TextTarget};
    let mut host = Host::new();
    let channel = host.open_channel("limn-core");
    let sized = |size: u32| RemoteImage {
        url: format!("https://cdn.example.test/emojis/1.webp?size={size}"),
        kind: Default::default(),
    };
    let last = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    let said = rune_model::Message {
        id: MessageId(9_000_000_000_000_101),
        content: "<:wave:1>".into(),
        text_references: vec![TextReference {
            source: "<:wave:1>".into(),
            target: TextTarget::Emoji {
                name: "wave".into(),
                image: Some(sized(48)),
                jumbo: Some(sized(96)),
                id: None,
            },
        }],
        reply_to: None,
        ..last.clone()
    };
    let reply = rune_model::Message {
        id: MessageId(9_000_000_000_000_102),
        content: "¡hola!".into(),
        text_references: Vec::new(),
        reply_to: Some(said.id),
        ..last
    };
    for message in [said, reply] {
        host.event(Event::MessageCreated {
            message,
            nonce: None,
        });
    }
    host.tree.scroll_to_end(&timeline_id(channel));
    host.frame();
    let wanted = wanted_images(&host.ui, &host.store, &host.tree);
    assert!(wanted.contains(&sized(48)) && wanted.contains(&sized(96)));
    for (size, digest) in [(48, 'a'), (96, 'b')] {
        let image = limn::RasterImage::new(
            digest.to_string().repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "wave",
        )
        .unwrap();
        host.ui.images.insert(sized(size), image);
    }
    let frame = host.frame();
    let drawn: Vec<char> = frame
        .commands
        .iter()
        .filter_map(|command| match command {
            limn::DrawCommand::RasterImage { image, .. } if image.alt() == "wave" => {
                image.sha256().chars().next()
            }
            _ => None,
        })
        .collect();
    // The message, larger; the line that answers it, at a line's size
    // (2026-10-18: it read `:wave:`).
    assert!(drawn.contains(&'b') && drawn.contains(&'a'), "{drawn:?}");
}

#[test]
fn staged_sticker_keeps_its_image_after_picker_closes_and_releases_it_on_removal() {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    host.store.provider.as_mut().unwrap().capabilities.stickers = true;
    let server = host.store.channel(channel).unwrap().server.unwrap();
    let mut community = host.store.server(server).unwrap().clone();
    let source = source(998);
    community.stickers = vec![rune_model::Sticker {
        id: rune_model::StickerId(998),
        name: "Saludo".into(),
        image: Some(source.clone()),
        available: true,
    }];
    host.event(Event::ServerChanged(community));
    host.click("composer:emoji");
    host.click("composer:emoji:tab=stickers");
    host.click("composer:emoji:sticker=998");
    host.frame();
    assert!(!host.ui.emoji_open);
    assert!(wanted_images(&host.ui, &host.store, &host.tree).contains(&source));
    host.ui.images.insert(
        source.clone(),
        limn::RasterImage::new(
            "0".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "Sticker de prueba",
        )
        .unwrap(),
    );
    let frame = host.frame();
    assert!(frame
        .commands
        .iter()
        .any(|command| matches!(command, limn::DrawCommand::RasterImage { .. })));
    assert!(host
        .sent
        .iter()
        .all(|command| !matches!(command, Command::SendSticker { .. })));
    host.click("composer:sticker:remove");
    host.frame();
    assert!(!wanted_images(&host.ui, &host.store, &host.tree).contains(&source));
}

#[test]
fn profile_banner_requests_follow_open_profiles_not_visible_accounts() {
    use rune_model::{ImageKind, ProfileBanner};
    let mut host = populated();
    let channel = host.open_channel("general");
    let users: Vec<_> = host.store.users().cloned().collect();
    for mut user in users {
        let mut art = source(user.id.0 + 10_000);
        art.kind = ImageKind::Banner;
        user.banner = ProfileBanner::Image(art);
        host.event(Event::UserChanged(user));
    }
    let banners = |host: &Host| {
        wanted_images(&host.ui, &host.store, &host.tree)
            .into_iter()
            .filter(|source| source.kind == ImageKind::Banner)
            .collect::<Vec<_>>()
    };
    host.frame();
    assert!(
        banners(&host).is_empty(),
        "timeline authors and members do not fetch banners"
    );
    let own = host
        .store
        .me_user()
        .unwrap()
        .banner
        .image()
        .unwrap()
        .clone();
    host.click("me:status");
    assert_eq!(banners(&host), vec![own.clone()]);
    host.key(Key::Escape, false, false);
    host.frame();
    assert!(banners(&host).is_empty());
    host.click("me:status");
    host.click("profile:edit");
    assert_eq!(banners(&host), vec![own]);
    host.click("profile:close");
    host.frame();
    assert!(banners(&host).is_empty());
    let message = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .iter()
        .find(|message| Some(message.author) != host.store.me())
        .unwrap()
        .clone();
    let author = host
        .store
        .user(message.author)
        .unwrap()
        .banner
        .image()
        .unwrap()
        .clone();
    host.ui.profiles.author = Some((message.id, false));
    host.frame();
    assert_eq!(banners(&host), vec![author.clone()]);
    host.ui.profiles.author = None;
    host.ui.profile = Some(message.author);
    host.frame();
    assert_eq!(banners(&host), vec![author]);
    host.ui.profile = None;
    host.frame();
    assert!(banners(&host).is_empty());
}
