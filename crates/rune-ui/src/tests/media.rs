//! Spoiler gating, animation playback rules and PDF paging.

use super::attachments::{image, populated, populated_pdf, source};
use super::*;
use rune_components::message::{animation_surface, attachment_ids};

fn mark_spoiler(host: &mut Host, message: MessageId) {
    let channel = host.ui.channel().unwrap();
    let mut updated = host
        .store
        .timeline(channel)
        .unwrap()
        .get(message)
        .unwrap()
        .as_ref()
        .clone();
    updated.attachments[0].spoiler = true;
    host.event(Event::MessageUpdated(updated));
    host.frame();
}

fn click(host: &mut Host, id: impl Into<String>) -> Vec<Intent> {
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked { id: id.into() },
        host.now,
    )
}

fn arrow(host: &mut Host, key: Key) -> Vec<Intent> {
    crate::documents::intercept(
        &mut host.ui,
        &UiEvent::Key {
            key,
            shift: false,
            ctrl: false,
            alt: false,
        },
    )
    .expect("the viewer takes its arrows")
}

#[test]
fn a_spoiler_requests_no_preview_until_revealed_then_behaves_normally() {
    let (mut host, message, attachment) = populated();
    mark_spoiler(&mut host, message);
    assert!(!wanted_images(&host.ui, &host.store, &host.tree).contains(&source()));
    assert!(!playable_images(&host.ui, &host.store, &host.tree, false).contains(&source()));
    let painted = host.painted();
    assert!(painted.iter().any(|text| text == "SPOILER"), "{painted:?}");
    assert!(!painted.iter().any(|text| text == "Cargando imagen…"));
    // Its preview action does nothing while hidden.
    click(
        &mut host,
        attachment_ids::action(message, attachment, "preview"),
    );
    assert!(host.ui.attachment_viewer.is_none());

    host.click(&attachment_ids::action(message, attachment, "reveal"));
    host.frame();
    assert!(wanted_images(&host.ui, &host.store, &host.tree).contains(&source()));
    assert!(playable_images(&host.ui, &host.store, &host.tree, false).contains(&source()));
    assert!(!host.painted().iter().any(|text| text == "SPOILER"));
    click(
        &mut host,
        attachment_ids::action(message, attachment, "preview"),
    );
    assert!(host.ui.attachment_viewer.is_some());

    host.event(Event::Authentication(
        rune_provider::AuthenticationState::SignedOut,
    ));
    assert!(host.ui.messages.revealed_files.is_empty());
}

#[test]
fn a_spoiler_document_is_not_rendered_before_its_reveal() {
    let (mut host, message, attachment) = populated_pdf();
    mark_spoiler(&mut host, message);
    assert!(!host.painted().iter().any(|text| text.contains("Ver PDF")));
    let intents = click(
        &mut host,
        attachment_ids::action(message, attachment, "pdf"),
    );
    assert!(!intents
        .iter()
        .any(|intent| matches!(intent, Intent::PreviewDocument { .. })));
    assert!(host.ui.document_viewer.is_none());
    click(
        &mut host,
        attachment_ids::action(message, attachment, "reveal"),
    );
    let intents = click(
        &mut host,
        attachment_ids::action(message, attachment, "pdf"),
    );
    assert!(intents
        .iter()
        .any(|intent| matches!(intent, Intent::PreviewDocument { .. })));
}

#[test]
fn an_apps_button_emoji_plays_in_view_without_reduced_motion() {
    use rune_model::components::Button;
    use rune_model::{Component, Components, RemoteImage};
    let (mut host, message, _) = populated();
    let emoji = RemoteImage {
        url: "https://cdn.example.test/emojis/22.webp?size=128&animated=true".into(),
        kind: Default::default(),
    };
    let channel = host.ui.channel().unwrap();
    let mut updated = host
        .store
        .timeline(channel)
        .unwrap()
        .get(message)
        .unwrap()
        .as_ref()
        .clone();
    updated.components = Components {
        parts: vec![Component::Row(vec![Component::Button(Button {
            label: Some("Fiesta".into()),
            emoji: Some(":fiesta:".into()),
            picture: Some(emoji.clone()),
            ..Button::default()
        })])],
        ..Components::default()
    };
    host.event(Event::MessageUpdated(updated));
    host.frame();
    let playable = |host: &Host, reduced| {
        playable_images(&host.ui, &host.store, &host.tree, reduced).contains(&emoji)
    };
    assert!(playable(&host, false));
    assert!(!playable(&host, true), "reduced motion keeps it still");
}

#[test]
fn animations_play_in_view_or_in_their_viewer_without_reduced_motion() {
    let (mut host, message, attachment) = populated();
    let playable = |host: &Host, reduced| {
        playable_images(&host.ui, &host.store, &host.tree, reduced).contains(&source())
    };
    assert!(playable(&host, false));
    assert!(!playable(&host, true), "reduced motion keeps it still");
    click(
        &mut host,
        attachment_ids::action(message, attachment, "preview"),
    );
    host.frame();
    assert!(
        playable(&host, false),
        "the viewer plays the picture it shows"
    );
    assert!(
        !playable(&host, true),
        "still in the viewer with reduced motion"
    );
    host.ui.images.insert(source(), image());
    host.ui.animated.insert(image().sha256().to_owned());
    let surfaces: Vec<_> = host
        .frame()
        .commands
        .iter()
        .filter_map(|command| match command {
            limn::DrawCommand::Surface { id, fit, .. } => Some((id.clone(), *fit)),
            _ => None,
        })
        .collect();
    // Never past its pixels in the viewer, as the still picture it replaces.
    assert!(
        surfaces.contains(&(
            animation_surface(image().sha256()),
            limn::SurfaceFit::ScaleDown
        )),
        "the viewer draws the playing surface: {surfaces:?}"
    );
    host.ui.animated.clear();
    click(&mut host, "attachment-viewer:close");
    host.frame();
    assert!(playable(&host, false));
    host.ui.prefs.image_previews = false;
    host.frame();
    assert!(!playable(&host, false), "no previews, nothing to play");
    host.ui.prefs.image_previews = true;
    let channel = host.ui.channel().unwrap();
    host.tree.scroll_to_start(&timeline_id(channel));
    host.frame();
    assert!(!playable(&host, false), "scrolled away, it stops");
}

#[test]
fn a_playing_animation_draws_its_surface_and_signing_out_stops_it() {
    let (mut host, _, _) = populated();
    host.ui.images.insert(source(), image());
    let surfaces = |host: &mut Host| {
        host.frame()
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::Surface { id, .. } => Some(id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(
        surfaces(&mut host).is_empty(),
        "still until the host plays it"
    );
    host.ui.animated.insert(image().sha256().to_owned());
    assert_eq!(
        surfaces(&mut host),
        vec![animation_surface(image().sha256())]
    );
    host.event(Event::Authentication(
        rune_provider::AuthenticationState::SignedOut,
    ));
    assert!(host.ui.animated.is_empty());
}

#[test]
fn pdf_pages_render_on_demand_and_only_a_few_stay_kept() {
    let (mut host, message, attachment) = populated_pdf();
    let intents = click(
        &mut host,
        attachment_ids::action(message, attachment, "pdf"),
    );
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PreviewDocument { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap();
    host.run(intents);
    document_result(
        &mut host.ui,
        &host.store,
        request,
        1,
        9,
        Vec::new(),
        Ok(image()),
    );
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "reporte.pdf · Página 1 de 9"));
    assert!(
        arrow(&mut host, Key::ArrowLeft).is_empty(),
        "no page before 1"
    );

    assert_eq!(
        click(&mut host, "document:next"),
        vec![Intent::DocumentPage {
            request,
            page: 2,
            marks: Vec::new()
        }]
    );
    assert_eq!(
        arrow(&mut host, Key::ArrowRight),
        vec![Intent::DocumentPage {
            request,
            page: 3,
            marks: Vec::new()
        }],
        "turning on before page 2 arrived replaces its render"
    );
    // A late page 2 is kept, but page 3 stays shown, still loading.
    document_result(
        &mut host.ui,
        &host.store,
        request,
        2,
        9,
        Vec::new(),
        Ok(image()),
    );
    let viewer = host.ui.document_viewer.as_ref().unwrap();
    assert_eq!(viewer.page, 3);
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "reporte.pdf · Página 3 de 9"));
    document_result(
        &mut host.ui,
        &host.store,
        request,
        3,
        9,
        Vec::new(),
        Err("No se pudo mostrar este PDF.".into()),
    );
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "No se pudo mostrar este PDF."));
    assert!(
        arrow(&mut host, Key::ArrowLeft).is_empty(),
        "page 2 is already rendered"
    );
    for page in 3..=9 {
        click(&mut host, "document:next");
        document_result(
            &mut host.ui,
            &host.store,
            request,
            page,
            9,
            Vec::new(),
            Ok(image()),
        );
    }
    let viewer = host.ui.document_viewer.as_ref().unwrap();
    assert_eq!(viewer.page, 9);
    assert!(
        arrow(&mut host, Key::ArrowRight).is_empty(),
        "no page after 9"
    );
    let kept: Vec<_> = host
        .ui
        .document_viewer
        .as_ref()
        .unwrap()
        .rendered()
        .collect();
    assert_eq!(kept, vec![6, 7, 8, 9], "a bounded set of recent pages");
    assert!(click(&mut host, "document:close").contains(&Intent::CancelDocument { request }));
    assert!(host.ui.document_viewer.is_none());
    // A page landing after closing reopens nothing.
    document_result(
        &mut host.ui,
        &host.store,
        request,
        5,
        9,
        Vec::new(),
        Ok(image()),
    );
    assert!(host.ui.document_viewer.is_none());
}

#[test]
fn a_document_is_searched_by_its_text_and_a_result_opens_its_page() {
    let (mut host, message, attachment) = populated_pdf();
    let intents = click(
        &mut host,
        attachment_ids::action(message, attachment, "pdf"),
    );
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PreviewDocument { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap();
    document_result(
        &mut host.ui,
        &host.store,
        request,
        1,
        3,
        Vec::new(),
        Ok(image()),
    );
    host.frame();
    // Its text is read once, when the search opens.
    let opened = click(&mut host, "document:search");
    assert!(opened.contains(&Intent::DocumentText { request }));
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Leyendo el texto del documento…"));
    let pages = words(&["Uno", "El reproductor\nde video", "Más VIDEO aquí"]);
    document_text(&mut host.ui, request, Ok(pages));
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::InputChanged {
            id: "document:search:query".into(),
            value: "video".into(),
        },
        host.now,
    );
    let search = host
        .ui
        .document_viewer
        .as_ref()
        .unwrap()
        .search
        .clone()
        .unwrap();
    let results = search.results();
    assert_eq!(
        results.iter().map(|(page, _)| *page).collect::<Vec<_>>(),
        [2, 3]
    );
    assert!(results[0].1.contains("reproductor de video"), "{results:?}");
    let painted = host.painted();
    assert!(
        painted.iter().any(|text| text == "En 2 páginas"),
        "{painted:?}"
    );
    // A result goes to its page, highlighted where it says it, and closes
    // the search.
    let went = click(&mut host, "document:search:page:3");
    let asked: Vec<_> = went
        .iter()
        .filter(|intent| matches!(intent, Intent::DocumentPage { .. }))
        .collect();
    assert_eq!(
        asked,
        [&Intent::DocumentPage {
            request,
            page: 3,
            marks: vec![[0.7, 0.0, 1.2, 0.1]]
        }],
        "only the page it goes to"
    );
    let viewer = host.ui.document_viewer.as_ref().unwrap();
    assert_eq!(viewer.page, 3);
    assert!(!viewer.search.as_ref().unwrap().open);
    // Opened again, it keeps what it read; Esc closes it, not the viewer.
    assert!(!click(&mut host, "document:search")
        .iter()
        .any(|intent| matches!(intent, Intent::DocumentText { .. })));
    host.key(Key::Escape, false, false);
    let viewer = host.ui.document_viewer.as_ref().unwrap();
    assert!(!viewer.search.as_ref().unwrap().open);
    // A document of pictures alone says there is nothing to search.
    click(&mut host, "document:search");
    document_text(&mut host.ui, request, Ok(words(&["", " \n"])));
    assert!(host
        .painted()
        .iter()
        .any(|text| text.contains("no tiene texto para buscar")));
}

/// Pages of `texts`, each word five tenths of the page wide and one tall,
/// a word after another and a line under another.
fn words(texts: &[&str]) -> std::sync::Arc<[Vec<rune_model::PdfWord>]> {
    texts
        .iter()
        .map(|text| {
            text.lines()
                .enumerate()
                .flat_map(|(row, line)| {
                    line.split_whitespace()
                        .enumerate()
                        .map(move |(column, word)| {
                            let (left, top) = (0.2 + column as f32 * 0.5, row as f32 * 0.1);
                            rune_model::PdfWord {
                                text: word.to_owned(),
                                bounds: [left, top, left + 0.5, top + 0.1],
                            }
                        })
                })
                .collect()
        })
        .collect()
}

#[test]
fn a_search_highlights_what_it_found_on_the_page_once_it_closes() {
    let (mut host, message, attachment) = populated_pdf();
    let intents = click(
        &mut host,
        attachment_ids::action(message, attachment, "pdf"),
    );
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PreviewDocument { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap();
    document_result(
        &mut host.ui,
        &host.store,
        request,
        1,
        3,
        Vec::new(),
        Ok(image()),
    );
    click(&mut host, "document:search");
    document_text(
        &mut host.ui,
        request,
        Ok(words(&["Dos videos: video y VIDEOS", "", "nada"])),
    );
    let search = |host: &Host| {
        host.ui
            .document_viewer
            .as_ref()
            .unwrap()
            .search
            .clone()
            .unwrap()
    };
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::InputChanged {
            id: "document:search:query".into(),
            value: "  VIDEO ".into(),
        },
        host.now,
    );
    // Each place, narrowed to its letters: "videos:" is seven letters wide.
    let marks = search(&host).marks(1);
    assert_eq!(marks.len(), 3, "{marks:?}");
    assert!(
        (marks[0][0] - 0.7).abs() < 1e-6 && (marks[0][2] - (0.7 + 0.5 * 5.0 / 7.0)).abs() < 1e-6
    );
    assert_eq!(marks[1], [1.2, 0.0, 1.7, 0.1], "a whole word");
    assert!(search(&host).marks(2).is_empty() && search(&host).marks(9).is_empty());
    // A place over two words marks both; a page full of places, the first 256.
    let other = |query: &str, text: &str| crate::documents::DocumentSearch {
        query: query.into(),
        pages: Some(Ok(words(&[text]))),
        open: false,
    };
    assert_eq!(
        other("video y", "Dos videos: video y VIDEOS").marks(1),
        [[1.2, 0.0, 1.7, 0.1], [1.7, 0.0, 2.2, 0.1]]
    );
    let full = other("a", &"a ".repeat(400)).marks(1);
    assert_eq!((full.len(), full[255][0]), (256, 0.2 + 255.0 * 0.5));
    // Typing asks for nothing; closing shows the page as found.
    let closed = click(&mut host, "document:search:close");
    assert!(closed.contains(&Intent::DocumentPage {
        request,
        page: 1,
        marks: marks.clone()
    }));
    // Until it is drawn, the page shows as it is; then highlighted.
    let plain = image();
    let marked = limn::RasterImage::new(
        "b".repeat(64),
        limn::RasterImageFormat::Png,
        3,
        std::sync::Arc::from("cG5n"),
        "Página 1 resaltada",
    )
    .unwrap();
    let shown = |host: &mut Host| {
        host.frame()
            .commands
            .into_iter()
            .find_map(|command| match command {
                limn::DrawCommand::RasterImage { image, .. } => Some(image.sha256().to_owned()),
                _ => None,
            })
    };
    assert_eq!(shown(&mut host), Some(plain.sha256().to_owned()));
    document_result(
        &mut host.ui,
        &host.store,
        request,
        1,
        3,
        marks.clone(),
        Ok(marked.clone()),
    );
    assert_eq!(shown(&mut host), Some(marked.sha256().to_owned()));
    // Asked again with nothing new, nothing is rendered again.
    assert!(!click(&mut host, "document:search:close")
        .iter()
        .any(|intent| matches!(intent, Intent::DocumentPage { .. })));
    // Without a query, the page shows as it is, kept from before.
    click(&mut host, "document:search");
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::InputChanged {
            id: "document:search:query".into(),
            value: String::new(),
        },
        host.now,
    );
    assert!(!click(&mut host, "document:search:close")
        .iter()
        .any(|intent| matches!(intent, Intent::DocumentPage { .. })));
    assert_eq!(shown(&mut host), Some(plain.sha256().to_owned()));
}
