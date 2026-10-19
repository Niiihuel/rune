use super::*;
use rune_model::{ImageKind, Permissions, RemoteImage};
use rune_provider::{
    mock::{MockConfig, MockProvider},
    Command, Event, GifAttribution, GifItem, GifThumbnail, Provider,
};

fn world() -> (Ui, Store, ChannelId) {
    let provider = MockProvider::new(MockConfig::default());
    let snapshot = provider.snapshot();
    let channel = snapshot
        .channels
        .iter()
        .find(|channel| {
            channel.kind.has_messages() && channel.permissions.contains(Permissions::SEND)
        })
        .unwrap();
    let (channel, server) = (channel.id, channel.server);
    let mut store = Store::new();
    let mut info = provider.info();
    // These are GIPHY's: a provider without its own search (P5).
    info.capabilities.gif_search = false;
    store.provider = Some(info);
    store.apply(Event::Ready(Box::new(snapshot)), Instant::now());
    let mut ui = Ui {
        server,
        emoji_open: true,
        ..Ui::default()
    };
    ui.channels.insert(server, channel);
    ui.messages.expression_tab = PickerTab::Gif;
    ui.messages.gifs.available = true;
    ui.set_draft(channel, Some("Hola".into()));
    (ui, store, channel)
}

fn page() -> GifPage {
    GifPage {
        items: vec![GifItem {
            id: "one".into(),
            title: "Saludo".into(),
            creator: Some("artist".into()),
            url: "https://giphy.com/gifs/saludo-one?source=fixture".into(),
            preview: RemoteImage {
                url: "https://media.giphy.com/media/one/100w_s.gif".into(),
                kind: ImageKind::Artwork,
            },
            thumbnail: Some(GifThumbnail {
                sha256: "a".repeat(64),
                size_bytes: 3,
                width: 1,
                height: 1,
                png_base64: "AAAA".into(),
            }),
        }],
        next_offset: Some(20),
        attribution: GifAttribution {
            label: "Powered By GIPHY".into(),
            url: "https://giphy.com/".into(),
            image: None,
        },
        categories: Vec::new(),
    }
}

fn complete(ui: &mut Ui, store: &Store) {
    let request = ui.messages.gifs.request.as_ref().unwrap();
    let result = GifResult {
        request: request.id,
        query: request.query.clone(),
        offset: request.offset,
        result: Ok(page()),
    };
    assert_eq!(gif_result(ui, store, result), vec![Intent::Repaint]);
}

#[test]
fn explicit_search_keeps_exact_input_and_selection_only_inserts_into_draft() {
    let (mut ui, store, channel) = world();
    let actions = search(
        &mut ui,
        &store,
        GifQuery::Search("@artist café & hola".into()),
        0,
    );
    let request = ui.messages.gifs.request.as_ref().unwrap().id;
    assert_eq!(
        actions,
        vec![Intent::SearchGifs {
            request,
            query: GifQuery::Search("@artist café & hola".into()),
            offset: 0
        }]
    );
    complete(&mut ui, &store);
    assert_eq!(view(&ui, &store).items.len(), 1);
    assert!(
        ui.images.is_empty(),
        "the reusable image cache must stay untouched"
    );
    let actions = update(
        &mut ui,
        &store,
        &UiAction::Clicked {
            id: format!("{PREFIX}pick={request}:0"),
        },
        Instant::now(),
    )
    .unwrap();
    assert_eq!(
        ui.drafts[&channel],
        "Hola https://giphy.com/gifs/saludo-one?source=fixture"
    );
    assert!(actions.iter().all(|action| matches!(
        action,
        Intent::SetInput { .. } | Intent::FocusInput { .. } | Intent::CancelGifSearch { .. }
    )));
    assert!(!ui.emoji_open);
    assert!(ui.messages.gifs.page.is_none() && ui.messages.gifs.images.is_empty());
    assert!(
        ui.messages.gif_recent.is_empty(),
        "GIPHY results are not retained as recents"
    );
}

#[test]
fn pagination_replaces_page_and_stale_tile_cannot_insert_another_result() {
    let (mut ui, store, channel) = world();
    search(&mut ui, &store, GifQuery::Trending, 0);
    complete(&mut ui, &store);
    let first = ui.messages.gifs.request.as_ref().unwrap().id;
    let actions = update(
        &mut ui,
        &store,
        &UiAction::Clicked {
            id: format!("{PREFIX}next"),
        },
        Instant::now(),
    )
    .unwrap();
    let second = ui.messages.gifs.request.as_ref().unwrap().id;
    assert_eq!(
        actions,
        vec![
            Intent::CancelGifSearch { request: first },
            Intent::SearchGifs {
                request: second,
                query: GifQuery::Trending,
                offset: 20
            }
        ]
    );
    assert!(ui.messages.gifs.page.is_none() && ui.messages.gifs.images.is_empty());
    complete(&mut ui, &store);
    // The same page again: the service ignored the offset, so it is the last.
    assert!(!view(&ui, &store).has_next);
    assert!(update(
        &mut ui,
        &store,
        &UiAction::Clicked {
            id: format!("{PREFIX}pick={first}:0")
        },
        Instant::now()
    )
    .unwrap()
    .is_empty());
    assert_eq!(ui.drafts[&channel], "Hola");
    // Another page goes on offering the next.
    ui.messages.gifs.left = Some("before".into());
    let request = ui.messages.gifs.request.as_ref().unwrap();
    let result = GifResult {
        request: request.id,
        query: request.query.clone(),
        offset: request.offset,
        result: Ok(page()),
    };
    gif_result(&mut ui, &store, result);
    assert!(view(&ui, &store).has_next);
}

#[test]
fn close_channel_account_permission_and_tab_changes_drop_pixels_and_cancel() {
    for change in 0..5 {
        let (mut ui, mut store, _) = world();
        search(&mut ui, &store, GifQuery::Trending, 0);
        complete(&mut ui, &store);
        let request = ui.messages.gifs.request.as_ref().unwrap().id;
        match change {
            0 => ui.emoji_open = false,
            1 => {
                ui.channels.insert(ui.server, ChannelId(u64::MAX));
            }
            2 => store = Store::new(),
            3 => ui.messages.gifs.available = false,
            _ => ui.messages.expression_tab = PickerTab::Stickers,
        }
        assert_eq!(
            sync_gif_search(&mut ui, &store),
            vec![Intent::CancelGifSearch { request }]
        );
        assert!(ui.messages.gifs.page.is_none() && ui.messages.gifs.images.is_empty());
        assert!(gif_result(
            &mut ui,
            &store,
            GifResult {
                request,
                query: GifQuery::Trending,
                offset: 0,
                result: Ok(page())
            }
        )
        .is_empty());
    }
}

#[test]
fn stale_response_and_missing_config_never_search_or_populate_results() {
    let (mut ui, store, _) = world();
    ui.messages.gifs.available = false;
    assert!(search(&mut ui, &store, GifQuery::Trending, 0).is_empty());
    ui.messages.gifs.available = true;
    assert!(search(&mut ui, &store, GifQuery::Search("x".repeat(51)), 0).is_empty());
    search(&mut ui, &store, GifQuery::Trending, 0);
    let request = ui.messages.gifs.request.as_ref().unwrap().id;
    for (request, query, offset) in [
        (request + 1, GifQuery::Trending, 0),
        (request, GifQuery::Search("other".into()), 0),
        (request, GifQuery::Trending, 20),
    ] {
        assert!(gif_result(
            &mut ui,
            &store,
            GifResult {
                request,
                query,
                offset,
                result: Ok(page())
            }
        )
        .is_empty());
        assert!(ui.messages.gifs.page.is_none());
    }
}

#[test]
fn keyboard_search_and_escape_restore_composer_focus_without_sending() {
    let (mut ui, store, channel) = world();
    update(
        &mut ui,
        &store,
        &UiAction::InputChanged {
            id: QUERY.into(),
            value: "cats".into(),
        },
        Instant::now(),
    );
    assert!(matches!(
        &key(&mut ui, &store, Key::Enter, Some(QUERY)).unwrap()[..],
        [Intent::SearchGifs { .. }]
    ));
    assert_eq!(
        key(&mut ui, &store, Key::Escape, Some(QUERY))
            .unwrap()
            .last(),
        Some(&Intent::Focus(editor_id(channel)))
    );
    assert!(!ui.emoji_open && ui.messages.gifs.request.is_none());
    assert!(!format!("{:?}", ui.messages.gifs).contains("cats"));
}

#[test]
fn empty_and_long_titles_keep_the_thumbnail_with_a_bounded_accessible_alt() {
    for title in [String::new(), "á".repeat(200)] {
        let (mut ui, store, _) = world();
        search(&mut ui, &store, GifQuery::Trending, 0);
        let request = ui.messages.gifs.request.as_ref().unwrap().id;
        let mut page = page();
        page.items[0].title = title.clone();
        gif_result(
            &mut ui,
            &store,
            GifResult {
                request,
                query: GifQuery::Trending,
                offset: 0,
                result: Ok(page),
            },
        );
        let view = view(&ui, &store);
        assert_eq!(view.items[0].title, title);
        assert!(view.items[0].image.is_some());
    }
}

// Administration (19): favorites and where they are kept.

fn click(ui: &mut Ui, store: &mut Store, id: &str) -> Vec<Intent> {
    crate::gif_favorites::update(ui, store, &UiAction::Clicked { id: id.into() })
        .expect("a favorites action")
}

#[test]
fn a_local_favorite_is_kept_on_this_device_and_says_so() {
    let (mut ui, mut store, _) = world();
    search(&mut ui, &store, GifQuery::Trending, 0);
    complete(&mut ui, &store);
    let request = ui.messages.gifs.request.as_ref().unwrap().id;
    assert_eq!(
        crate::gif_favorites::scope(&store),
        "Guardados sólo en este equipo"
    );
    let heart = format!("composer:emoji:gif:fav={request}:0");
    let saved = click(&mut ui, &mut store, &heart);
    assert!(
        matches!(&saved[..], [Intent::SaveSetting { key, value: Some(_) }] if key == "gif_favorites")
    );
    assert!(view(&ui, &store).items[0].favorite);
    // Kept and taken back as it was.
    let kept = crate::gif_favorites::kept(&ui);
    let mut other = Ui::default();
    crate::gif_favorites::restore(&mut other, kept);
    assert_eq!(other.messages.gif_favorites, ui.messages.gif_favorites);
    assert_eq!(
        other.messages.gif_favorites[0].url,
        "https://giphy.com/gifs/saludo-one?source=fixture"
    );
    // The same heart takes it out; so does the list's own.
    click(&mut ui, &mut store, &heart);
    assert!(ui.messages.gif_favorites.is_empty());
    click(&mut ui, &mut store, &heart);
    let gone = click(&mut ui, &mut store, "composer:emoji:gif-unfavorite=0");
    assert!(matches!(
        &gone[..],
        [Intent::SaveSetting { value: None, .. }]
    ));
    // A stale heart or a bad kept line does nothing.
    assert!(click(&mut ui, &mut store, "composer:emoji:gif:fav=1:0").is_empty());
    let mut other = Ui::default();
    crate::gif_favorites::restore(&mut other, Some("javascript:alert(1)\t-\t0\t0".into()));
    assert!(other.messages.gif_favorites.is_empty());
}

#[test]
fn synced_favorites_are_asked_for_and_changed_through_the_provider() {
    let (mut ui, mut store, _) = world();
    store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .synced_gif_favorites = true;
    assert_eq!(
        crate::gif_favorites::load(&store),
        [Intent::Command(Command::LoadGifFavorites)]
    );
    assert!(crate::gif_favorites::scope(&store).starts_with("Sincronizados con tu cuenta"));
    search(&mut ui, &store, GifQuery::Trending, 0);
    complete(&mut ui, &store);
    let request = ui.messages.gifs.request.as_ref().unwrap().id;
    let heart = format!("composer:emoji:gif:fav={request}:0");
    // Not loaded yet: nothing to change.
    assert!(click(&mut ui, &mut store, &heart).is_empty());
    store.apply(Event::GifFavorites(Vec::new()), Instant::now());
    assert!(crate::gif_favorites::load(&store).is_empty());
    let sent = click(&mut ui, &mut store, &heart);
    assert!(matches!(
        &sent[..],
        [Intent::Command(Command::SetGifFavorite {
            favorite: true,
            ..
        })]
    ));
    // Shown at once, and nothing is kept on this device.
    assert!(view(&ui, &store).items[0].favorite);
    assert!(ui.messages.gif_favorites.is_empty());
}
