use super::*;
use crate::testing::{controls, frame, texts};
use limn::{DrawCommand, RasterImage, RasterImageFormat};
use rune_model::{ImageKind, RemoteFile, RemoteImage};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

fn source() -> RemoteImage {
    RemoteImage {
        url: "https://cdn.example.test/attachment.png".into(),
        kind: ImageKind::AttachmentPreview,
    }
}

fn attachment() -> rune_model::Attachment {
    rune_model::Attachment {
        id: AttachmentId(12),
        filename: "imagen de prueba.png".into(),
        size_bytes: 4096,
        content_type: Some("image/png".into()),
        dimensions: Some((800, 450)),
        preview: Some(source()),
        download: Some(RemoteFile {
            url: "https://cdn.example.test/attachment.png".into(),
        }),
        spoiler: false,
        voice: None,
        explicit: false,
        placeholder: None,
    }
}

fn raster() -> RasterImage {
    RasterImage::new(
        "0".repeat(64),
        RasterImageFormat::Png,
        3,
        Arc::from("AAAA"),
        "Imagen de prueba",
    )
    .unwrap()
}

fn laid(
    attachment: &rune_model::Attachment,
    width: f32,
    live: Live<'_>,
    style: MessageStyle,
) -> limn::Frame {
    frame(
        Element::column(vec![file(
            attachment,
            MessageId(8),
            width,
            style,
            live,
            &Look::DARK,
        )]),
        Size::new(width, 600.0),
    )
    .1
}

#[test]
fn loading_failure_and_ready_share_bounds_and_only_ready_paints_the_cached_image() {
    let attachment = attachment();
    let ready = HashMap::from([(source(), raster())]);
    let failed = HashSet::from([source()]);
    let states = [
        (Live::default(), Some("Cargando imagen…")),
        (
            Live {
                attachment_failures: Some(&failed),
                ..Live::default()
            },
            Some("No se pudo cargar la imagen"),
        ),
        (
            Live {
                attachment_images: Some(&ready),
                ..Live::default()
            },
            None,
        ),
    ];
    let mut bounds = None;
    for (live, status) in states {
        let frame = laid(&attachment, 360.0, live, MessageStyle::default());
        let text = texts(&frame);
        assert!(!text.iter().any(|text| text.contains("simulada")));
        if let Some(status) = status {
            assert!(text.iter().any(|text| text == status), "{text:?}");
        }
        let target = frame
            .regions
            .iter()
            .find(|region| region.id == ids::action(MessageId(8), attachment.id, "preview"))
            .unwrap()
            .rect;
        assert_eq!(
            *bounds.get_or_insert(target),
            target,
            "an image status must not shift a measured message row"
        );
        let pictures: Vec<_> = frame
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::RasterImage { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        if status.is_none() {
            assert_eq!(pictures.len(), 1);
            assert!((pictures[0].width / pictures[0].height - 800.0 / 450.0).abs() < 0.001);
            assert!(pictures[0].width <= 360.0 && pictures[0].height <= 260.0);
        } else {
            assert!(pictures.is_empty());
        }
    }
}

#[test]
fn long_filenames_and_extreme_aspects_stay_within_narrow_columns() {
    let mut attachment = attachment();
    attachment.filename = format!("{}.png", "nombre_largo_".repeat(40));
    let ready = HashMap::from([(source(), raster())]);
    for width in [40.0, 120.0, 240.0, 500.0] {
        for dimensions in [(1, 4096), (4096, 1), (0, 0), (800, 450)] {
            attachment.dimensions = Some(dimensions);
            let frame = laid(
                &attachment,
                width,
                Live {
                    attachment_images: Some(&ready),
                    ..Live::default()
                },
                MessageStyle::default(),
            );
            for command in &frame.commands {
                if let DrawCommand::RasterImage { rect, .. } = command {
                    assert!(
                        rect.x >= 0.0 && rect.x + rect.width <= width + 0.5,
                        "{rect:?}"
                    );
                    assert!(rect.height.is_finite() && rect.height <= 260.0);
                }
            }
            assert!(!texts(&frame).contains(&attachment.filename));
            assert!(frame
                .access
                .iter()
                .any(|node| node.name.contains(&attachment.filename)));
        }
    }
}

#[test]
fn ordinary_files_are_named_cards_and_only_real_links_enable_opening() {
    let mut attachment = attachment();
    attachment.preview = None;
    attachment.content_type = Some("application/zip".into());
    for width in [40.0, 200.0, 600.0] {
        let open = laid(&attachment, width, Live::default(), MessageStyle::default());
        assert!(controls(&open).contains(&ids::action(MessageId(8), attachment.id, "open")));
        assert!(!open
            .commands
            .iter()
            .any(|command| matches!(command, DrawCommand::RasterImage { .. })));
    }
    attachment.download = None;
    let closed = laid(&attachment, 240.0, Live::default(), MessageStyle::default());
    assert!(!controls(&closed)
        .iter()
        .any(|id| id.starts_with("attachment:")));
    assert!(texts(&closed).iter().any(|text| text.contains("4.0 KB")));
}

#[test]
fn pdf_cards_request_a_first_page_without_opening_an_external_browser() {
    let mut attachment = attachment();
    attachment.preview = None;
    attachment.filename = "reporte.PDF".into();
    attachment.content_type = None;
    assert!(is_pdf(&attachment));
    for width in [40.0, 200.0, 600.0] {
        let frame = laid(&attachment, width, Live::default(), MessageStyle::default());
        assert!(controls(&frame).contains(&ids::action(MessageId(8), attachment.id, "pdf")));
        assert!(!controls(&frame).contains(&ids::action(MessageId(8), attachment.id, "open")));
    }
    attachment.filename = "sin-extension".into();
    attachment.content_type = Some("APPLICATION/PDF; version=1.4".into());
    assert!(is_pdf(&attachment));
    attachment.download = None;
    let frame = laid(&attachment, 240.0, Live::default(), MessageStyle::default());
    assert!(!controls(&frame).iter().any(|id| id.ends_with(":pdf")));
}

#[test]
fn disabling_previews_or_rendering_a_settings_sample_does_not_create_a_live_preview_action() {
    let attachment = attachment();
    let disabled = laid(
        &attachment,
        360.0,
        Live::default(),
        MessageStyle {
            image_previews: false,
            ..MessageStyle::default()
        },
    );
    assert!(!controls(&disabled).contains(&ids::action(MessageId(8), attachment.id, "preview")));
    let sample = laid(
        &attachment,
        360.0,
        Live::default(),
        MessageStyle {
            sample: true,
            ..MessageStyle::default()
        },
    );
    assert!(!controls(&sample)
        .iter()
        .any(|id| id.starts_with("attachment:")));
}

#[test]
fn a_hidden_spoiler_reserves_the_preview_box_and_paints_nothing_of_the_file() {
    let mut attachment = attachment();
    attachment.spoiler = true;
    let ready = HashMap::from([(source(), raster())]);
    let hidden = laid(
        &attachment,
        360.0,
        Live {
            attachment_images: Some(&ready),
            ..Live::default()
        },
        MessageStyle::default(),
    );
    assert!(!hidden
        .commands
        .iter()
        .any(|command| matches!(command, DrawCommand::RasterImage { .. })));
    assert!(texts(&hidden).iter().any(|text| text == "SPOILER"));
    let reveal = ids::action(MessageId(8), attachment.id, "reveal");
    let target = hidden
        .regions
        .iter()
        .find(|region| region.id == reveal)
        .expect("one explicit reveal");
    assert!(!controls(&hidden).contains(&ids::action(MessageId(8), attachment.id, "preview")));
    let revealed = HashSet::from([attachment.id]);
    let animated = HashSet::from([raster().sha256().to_owned()]);
    let shown = laid(
        &attachment,
        360.0,
        Live {
            attachment_images: Some(&ready),
            revealed_files: Some(&revealed),
            animated: Some(&animated),
            ..Live::default()
        },
        MessageStyle::default(),
    );
    let surface = shown
        .commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Surface { rect, id, .. } => Some((*rect, id.clone())),
            _ => None,
        })
        .expect("a playing animation is a surface");
    assert_eq!(
        surface.1,
        crate::message::animation_surface(raster().sha256())
    );
    assert!(
        target.rect.height >= surface.0.height,
        "same box before and after"
    );
}

// Message content (contenido-de-mensajes).

fn pictures(frame: &limn::Frame) -> usize {
    frame
        .commands
        .iter()
        .filter(|command| matches!(command, DrawCommand::RasterImage { .. }))
        .count()
}

#[test]
fn a_thumbhash_stands_in_while_the_picture_loads_or_fails() {
    let mut attachment = attachment();
    let hash = RemoteImage {
        url: "oPcNHZpwd3dxeHiHh3eHh9jQDPh4".into(),
        kind: ImageKind::ThumbHash,
    };
    attachment.placeholder = Some(hash.clone());
    let only_hash = HashMap::from([(hash.clone(), raster())]);
    let loading = laid(
        &attachment,
        360.0,
        Live {
            attachment_images: Some(&only_hash),
            ..Live::default()
        },
        MessageStyle::default(),
    );
    assert_eq!(pictures(&loading), 1, "its ThumbHash");
    assert!(texts(&loading)
        .iter()
        .any(|text| text == "Cargando imagen…"));
    // Failed, it stays under the mark that says so.
    let failed = HashSet::from([source()]);
    let failing = laid(
        &attachment,
        360.0,
        Live {
            attachment_images: Some(&only_hash),
            attachment_failures: Some(&failed),
            ..Live::default()
        },
        MessageStyle::default(),
    );
    assert_eq!(pictures(&failing), 1, "its ThumbHash");
    assert!(texts(&failing)
        .iter()
        .any(|text| text == "No se pudo cargar la imagen"));
    // Not decoded yet: the box alone.
    assert_eq!(
        pictures(&laid(
            &attachment,
            360.0,
            Live::default(),
            MessageStyle::default()
        )),
        0
    );
}

#[test]
fn an_explicit_picture_the_filter_hides_shows_blurred_until_revealed() {
    let mut attachment = attachment();
    attachment.explicit = true;
    let blurred = HashMap::from([(source().obscured(), raster())]);
    let live = Live {
        attachment_images: Some(&blurred),
        obscure_explicit: true,
        ..Live::default()
    };
    let hidden = laid(&attachment, 360.0, live, MessageStyle::default());
    assert_eq!(pictures(&hidden), 1, "the blurred picture");
    assert!(texts(&hidden)
        .iter()
        .any(|text| text == "CONTENIDO EXPLÍCITO"));
    let action = |verb| ids::action(MessageId(8), attachment.id, verb);
    assert!(controls(&hidden).contains(&action("reveal")));
    assert!(!controls(&hidden).contains(&action("preview")));
    // Where the filter does not apply, or once revealed, it is the picture.
    let revealed = HashSet::from([attachment.id]);
    for live in [
        Live {
            obscure_explicit: false,
            ..live
        },
        Live {
            revealed_files: Some(&revealed),
            ..live
        },
    ] {
        let shown = laid(&attachment, 360.0, live, MessageStyle::default());
        assert!(controls(&shown).contains(&action("preview")));
        assert!(!texts(&shown)
            .iter()
            .any(|text| text == "CONTENIDO EXPLÍCITO"));
    }
    // A video shows nothing of itself until revealed.
    attachment.preview = None;
    attachment.filename = "clip.mp4".into();
    attachment.content_type = Some("video/mp4".into());
    let video = laid(&attachment, 360.0, live, MessageStyle::default());
    assert!(controls(&video).contains(&action("reveal")));
    assert!(texts(&video)
        .iter()
        .any(|text| text == "CONTENIDO EXPLÍCITO"));
    assert_eq!(pictures(&video), 0);
}

#[test]
fn an_explicit_spoiler_says_it_is_explicit_before_its_one_reveal() {
    let mut attachment = attachment();
    attachment.spoiler = true;
    attachment.explicit = true;
    let blurred = HashMap::from([(source().obscured(), raster())]);
    let live = Live {
        attachment_images: Some(&blurred),
        obscure_explicit: true,
        ..Live::default()
    };
    let hidden = laid(&attachment, 360.0, live, MessageStyle::default());
    let said = texts(&hidden);
    assert!(said.iter().any(|text| text == "CONTENIDO EXPLÍCITO"));
    assert!(!said.iter().any(|text| text == "SPOILER"));
    assert_eq!(pictures(&hidden), 0, "nothing of a spoiler");
    assert!(controls(&hidden).contains(&ids::action(MessageId(8), attachment.id, "reveal")));
    // Where the filter does not apply, it is a spoiler.
    let spoiler = laid(
        &attachment,
        360.0,
        Live {
            obscure_explicit: false,
            ..live
        },
        MessageStyle::default(),
    );
    assert!(texts(&spoiler).iter().any(|text| text == "SPOILER"));
}

#[test]
fn pictures_and_videos_can_be_copied_and_other_files_cannot() {
    let mut attachment = attachment();
    assert_eq!(copyable(&attachment), Some(Copied::Image));
    attachment.preview = None;
    attachment.filename = "clip.mp4".into();
    attachment.content_type = Some("video/mp4".into());
    assert_eq!(copyable(&attachment), Some(Copied::Video));
    // Too large to play here, it is still a video; a sound, never.
    attachment.size_bytes = rune_model::Attachment::MAX_PLAYED_BYTES + 1;
    assert_eq!(copyable(&attachment), Some(Copied::Video));
    attachment.filename = "tema.mp3".into();
    attachment.content_type = Some("audio/mpeg".into());
    assert_eq!(copyable(&attachment), None);
    attachment.filename = "datos.zip".into();
    attachment.content_type = Some("application/zip".into());
    assert_eq!(copyable(&attachment), None);
}
