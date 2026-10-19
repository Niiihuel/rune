//! The image and video viewer (visor-de-medios): a message's pictures as a
//! gallery, the zoom the viewer keeps and reports, its keys, its actions,
//! and a video in the same viewer.

use super::attachments::{image, populated, source};
use super::*;
use rune_components::message::attachment_ids;
use rune_model::{Attachment, AttachmentId, RemoteFile, RemoteImage};

/// A message with three pictures, the second larger than its preview, and
/// the viewer open on the first.
fn three() -> (Host, MessageId) {
    let (mut host, message, _) = populated();
    let channel = host.ui.channel().unwrap();
    let mut updated = host
        .store
        .timeline(channel)
        .unwrap()
        .get(message)
        .unwrap()
        .as_ref()
        .clone();
    let picture = |id: u64, name: &str, dimensions| Attachment {
        id: AttachmentId(id),
        filename: name.into(),
        size_bytes: 48_213,
        content_type: Some("image/png".into()),
        dimensions: Some(dimensions),
        preview: Some(RemoteImage {
            url: format!("https://cdn.example.test/{name}"),
            kind: rune_model::ImageKind::AttachmentPreview,
        }),
        download: Some(RemoteFile {
            url: format!("https://cdn.example.test/{name}"),
        }),
        spoiler: false,
        voice: None,
        explicit: false,
        placeholder: None,
    };
    updated.attachments = vec![
        picture(86, "dibujo.png", (300, 340)),
        picture(87, "foto.png", (2400, 1600)),
        picture(88, "logo.png", (512, 512)),
    ];
    host.event(Event::MessageUpdated(updated));
    host.frame();
    host.click(&attachment_ids::action(
        message,
        AttachmentId(86),
        "preview",
    ));
    host.frame();
    (host, message)
}

/// A picture known by `n`: a digest of its own.
pub(super) fn numbered(n: u8) -> limn::RasterImage {
    limn::RasterImage::new(
        format!("{n:064x}"),
        limn::RasterImageFormat::Png,
        3,
        std::sync::Arc::from("AAAA"),
        "Fixture",
    )
    .unwrap()
}

/// Whichever place of the viewer's strip is chosen, the thumbnail at each
/// place draws `pictures` at that place and the viewer shows the chosen
/// one: no thumbnail stands for another picture.
pub(super) fn strip_shows_what_it_opens(host: &mut Host, pictures: &[limn::RasterImage]) {
    for (at, expected) in pictures.iter().enumerate() {
        host.click(&format!("attachment-viewer:show:{at}"));
        let frame = host.frame();
        let inside = |rect: limn::Rect, within: limn::Rect| {
            let (x, y) = (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
            within.contains(Point::new(x, y))
        };
        let region = |found: &dyn Fn(&limn::HitRegion) -> bool| {
            frame
                .regions
                .iter()
                .find(|region| found(region))
                .unwrap()
                .rect
        };
        // Drawn last there: the strip lies over the conversation.
        let drawn = |within: limn::Rect, viewed: bool| {
            frame
                .commands
                .iter()
                .rev()
                .find_map(|command| match command {
                    limn::DrawCommand::RasterImage { rect, image, .. }
                        if inside(*rect, within) && image.is_full_resolution() == viewed =>
                    {
                        Some(image.sha256().to_owned())
                    }
                    _ => None,
                })
        };
        for (place, picture) in pictures.iter().enumerate() {
            let id = format!("attachment-viewer:show:{place}");
            assert_eq!(
                drawn(region(&|found| found.id == id), false).as_deref(),
                Some(picture.sha256()),
                "thumbnail {place} with {at} chosen"
            );
        }
        let room = region(&|found| found.kind == limn::HitKind::Zoom);
        assert_eq!(
            drawn(room, true).as_deref(),
            Some(expected.sha256()),
            "picture {at}"
        );
    }
}

fn viewed(host: &Host) -> Viewed {
    host.ui.attachment_viewer.as_ref().unwrap().viewed.clone()
}

fn node(host: &mut Host, id: &str) -> Option<limn::AccessNode> {
    host.frame().access.into_iter().find(|node| node.id == id)
}

#[test]
fn a_messages_pictures_are_a_gallery_with_a_strip_arrows_and_home_and_end() {
    let (mut host, message) = three();
    assert_eq!(viewed(&host), Viewed::File(AttachmentId(86)));
    // A strip of the three, the first ringed; the line under the title.
    assert_eq!(
        node(&mut host, "attachment-viewer:show:0").and_then(|node| node.selected),
        Some(true)
    );
    assert!(
        host.painted()
            .iter()
            .any(|text| text == "300 × 340 · 47.1 KB · PNG"),
        "{:?}",
        host.painted()
    );
    host.key(Key::ArrowRight, false, false);
    assert_eq!(viewed(&host), Viewed::File(AttachmentId(87)));
    host.key(Key::End, false, false);
    assert_eq!(viewed(&host), Viewed::File(AttachmentId(88)));
    host.key(Key::ArrowRight, false, false);
    assert_eq!(
        viewed(&host),
        Viewed::File(AttachmentId(88)),
        "the last one stays"
    );
    host.key(Key::Home, false, false);
    assert_eq!(viewed(&host), Viewed::File(AttachmentId(86)));
    host.click("attachment-viewer:show:1");
    assert_eq!(viewed(&host), Viewed::File(AttachmentId(87)));
    // A picture past the last is none: the viewer stays where it is.
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "attachment-viewer:show:9".into(),
        },
        host.now,
    );
    assert_eq!(viewed(&host), Viewed::File(AttachmentId(87)));
    // Each preview is wanted, for the strip.
    let wanted = wanted_images(&host.ui, &host.store, &host.tree);
    for name in ["dibujo.png", "foto.png", "logo.png"] {
        assert!(
            wanted.iter().any(|source| source.url.ends_with(name)),
            "{name}"
        );
    }
    // A spoiler among them leaves the gallery until revealed.
    let channel = host.ui.channel().unwrap();
    let mut updated = host
        .store
        .timeline(channel)
        .unwrap()
        .get(message)
        .unwrap()
        .as_ref()
        .clone();
    updated.attachments[2].spoiler = true;
    host.event(Event::MessageUpdated(updated));
    assert!(node(&mut host, "attachment-viewer:show:2").is_none());
}

#[test]
fn each_thumbnail_of_a_messages_pictures_is_the_picture_it_opens() {
    let (mut host, _) = three();
    let pictures: Vec<_> = (1..=3).map(numbered).collect();
    for (name, picture) in ["dibujo.png", "foto.png", "logo.png"].iter().zip(&pictures) {
        let source = RemoteImage {
            url: format!("https://cdn.example.test/{name}"),
            kind: rune_model::ImageKind::AttachmentPreview,
        };
        host.ui.images.insert(source, picture.clone());
    }
    strip_shows_what_it_opens(&mut host, &pictures);
}

#[test]
fn the_viewer_keeps_its_zoom_reports_it_and_wants_the_original_once_past_its_preview() {
    let (mut host, _) = three();
    host.click("attachment-viewer:show:1");
    host.ui.images.insert(
        RemoteImage {
            url: "https://cdn.example.test/foto.png".into(),
            kind: rune_model::ImageKind::AttachmentPreview,
        },
        image(),
    );
    host.frame();
    let viewer = |host: &Host| host.ui.attachment_viewer.as_ref().unwrap().clone();
    assert_eq!((viewer(&host).zoom, viewer(&host).original), (None, false));
    // The tree zooms with the key and reports it; the header shows it.
    host.key(Key::Plus, false, false);
    let (scale, fitted) = viewer(&host).zoom.expect("the zoom was reported");
    assert!(!fitted && scale > 0.0);
    let percent = format!("{} %", limn::zoom_percent(scale));
    assert!(host.painted().contains(&percent), "{percent}");
    // Past what its 768 by 512 preview has of its 2400 by 1600: its own pixels.
    host.key(Key::Num1, false, false);
    assert!(viewer(&host).original);
    let wanted = wanted_images(&host.ui, &host.store, &host.tree);
    assert!(wanted
        .iter()
        .any(|source| source.kind == rune_model::ImageKind::Original));
    // The header's zoom buttons move it too, and nothing else.
    assert!(update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "attachment-viewer:fit".into()
        },
        host.now
    )
    .is_empty());
    host.click("attachment-viewer:fit");
    assert_eq!(viewer(&host).zoom.map(|(_, fitted)| fitted), Some(true));
    // Another picture opens fitted, from its preview.
    host.key(Key::ArrowRight, false, false);
    assert_eq!((viewer(&host).zoom, viewer(&host).original), (None, false));
}

#[test]
fn ctrl_c_and_ctrl_s_copy_and_save_the_picture_and_the_header_offers_both() {
    let (mut host, message) = three();
    let copy = attachment_ids::action(message, AttachmentId(86), "copy-image");
    assert!(node(&mut host, &copy).is_some_and(|node| node.actionable));
    for id in ["open", "save-as"] {
        assert!(
            node(
                &mut host,
                &attachment_ids::action(message, AttachmentId(86), id)
            )
            .is_some(),
            "{id}"
        );
    }
    let keys = |host: &mut Host, key| {
        crate::attachments::intercept(
            &mut host.ui,
            &host.store,
            &UiEvent::Key {
                key,
                shift: false,
                ctrl: true,
                alt: false,
            },
        )
        .expect("the viewer takes it")
    };
    assert!(matches!(
        keys(&mut host, Key::C).as_slice(),
        [Intent::CopyFile { image: true, .. }]
    ));
    keys(&mut host, Key::S);
    assert!(host.ui.save_as.is_some());
    // The zoom keys and Ctrl with an arrow go on to the tree.
    for (key, ctrl) in [
        (Key::Plus, false),
        (Key::Num0, false),
        (Key::ArrowLeft, true),
        (Key::R, true),
    ] {
        let event = UiEvent::Key {
            key,
            shift: false,
            ctrl,
            alt: false,
        };
        assert!(
            crate::attachments::intercept(&mut host.ui, &host.store, &event).is_none(),
            "{key:?}"
        );
    }
}

#[test]
fn forwarding_from_the_viewer_closes_it_and_opens_the_forward_dialog() {
    let (mut host, _) = three();
    assert!(node(&mut host, "attachment-viewer:forward").is_some());
    host.click("attachment-viewer:forward");
    assert!(host.ui.attachment_viewer.is_none());
    assert!(host.ui.message_actions.forward.is_some());
}

#[test]
fn a_click_beside_the_picture_closes_the_viewer() {
    let (mut host, message, attachment) = populated();
    host.ui.images.insert(source(), image());
    host.click(&attachment_ids::action(message, attachment, "preview"));
    let frame = host.frame();
    let room = frame
        .regions
        .iter()
        .find(|region| region.kind == limn::HitKind::Zoom)
        .unwrap()
        .rect;
    // The 800 by 450 picture fits the room: beside it, at the left edge.
    let beside = Point::new(room.x + 4.0, room.y + room.height / 2.0);
    host.send(UiEvent::PointerDown(beside));
    host.send(UiEvent::PointerUp(beside));
    assert!(host.ui.attachment_viewer.is_none());
}

#[test]
fn a_documents_pages_zoom_in_the_viewer_and_it_shows_the_level() {
    let (mut host, message, attachment) = super::attachments::populated_pdf();
    host.click(&attachment_ids::action(message, attachment, "pdf"));
    host.frame();
    assert!(host.ui.document_viewer.is_some());
    // Without its page yet, nothing to zoom; the level arrives as reported.
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Zoomed {
            id: "document:close:view:0:x".into(),
            scale: 1.5,
            fitted: false,
        },
        host.now,
    );
    assert!(host.painted().iter().any(|text| text == "150 %"));
    assert!(update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "document:zoom-in".into()
        },
        host.now
    )
    .is_empty());
}
