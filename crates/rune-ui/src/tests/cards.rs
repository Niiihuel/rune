//! Pictures of cards: galleries, the viewer, spoilers, forwards and the
//! picture at its own size.

use super::attachments::{image, source};
use super::*;
use rune_components::message::{attachment_ids, card_picture, card_spoiler};
use rune_model::{
    Attachment, AttachmentId, Embed, EmbedImage, EmbedKind, Forwarded, ImageKind, RemoteFile,
    RemoteImage,
};

fn picture(name: &str) -> Option<EmbedImage> {
    Some(EmbedImage {
        source: RemoteImage {
            url: format!("https://media.example.test/{name}.png"),
            kind: ImageKind::AttachmentPreview,
        },
        dimensions: (1600, 900),
    })
}

/// A message after the latest of #general, as `edit` leaves it.
fn posted(edit: impl FnOnce(&mut rune_model::Message)) -> (Host, MessageId) {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let mut message = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    message.id = MessageId(message.id.0 + 1);
    message.content = String::new();
    message.attachments.clear();
    message.reactions.clear();
    message.embeds.clear();
    message.poll = None;
    message.components = Default::default();
    edit(&mut message);
    let id = message.id;
    host.event(Event::MessageCreated {
        message,
        nonce: None,
    });
    host.frame();
    host.tree.scroll_to_end(&timeline_id(channel));
    host.frame();
    (host, id)
}

fn album() -> Vec<Embed> {
    (0..3)
        .map(|index| Embed {
            url: Some("https://example.test/post".into()),
            title: (index == 0).then(|| "Una publicación".into()),
            image: picture(&index.to_string()),
            ..Embed::default()
        })
        .collect()
}

fn wanted(host: &Host) -> Vec<RemoteImage> {
    wanted_images(&host.ui, &host.store, &host.tree)
}

#[test]
fn a_gallery_opens_each_of_its_pictures_in_the_viewer_and_steps_through_them() {
    let (mut host, id) = posted(|message| message.embeds = album());
    for index in 0..3 {
        assert!(wanted(&host).contains(&picture(&index.to_string()).unwrap().source));
    }
    let base = format!("msg:{id}");
    // One card: its title once, three pictures to open.
    let painted = host.painted();
    assert_eq!(
        painted
            .iter()
            .filter(|text| *text == "Una publicación")
            .count(),
        1
    );
    host.click(&card_picture(&base, false, 0, 1));
    host.frame();
    assert_eq!(
        host.ui
            .attachment_viewer
            .as_ref()
            .map(|viewer| viewer.viewed.clone()),
        Some(Viewed::Card {
            forwarded: false,
            first: 0,
            picture: 1
        })
    );
    // Its place in the strip of the three, which one opens directly.
    let strip = |host: &mut Host, at: usize| {
        let frame = host.frame();
        let id = format!("attachment-viewer:show:{at}");
        frame
            .access
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.selected)
    };
    assert_eq!(
        (strip(&mut host, 0), strip(&mut host, 1)),
        (Some(false), Some(true))
    );
    host.click("attachment-viewer:show:0");
    assert_eq!(strip(&mut host, 0), Some(true));
    host.click("attachment-viewer:show:1");
    host.key(Key::ArrowRight, false, false);
    host.key(Key::ArrowRight, false, false);
    let shown = |host: &Host| match host.ui.attachment_viewer.as_ref().unwrap().viewed {
        Viewed::Card { picture, .. } => picture,
        Viewed::File(_) => unreachable!(),
    };
    assert_eq!(shown(&host), 2, "the last one stays");
    host.key(Key::ArrowLeft, false, false);
    assert_eq!(shown(&host), 1);
    // The viewer keeps its picture wanted.
    assert!(wanted(&host).contains(&picture("1").unwrap().source));
    host.key(Key::Escape, false, false);
    host.frame();
    assert!(host.ui.attachment_viewer.is_none());
    assert_eq!(
        host.tree.focused_id(),
        Some(card_picture(&base, false, 0, 1).as_str())
    );
}

#[test]
fn each_thumbnail_of_a_gallery_is_the_picture_it_opens() {
    let (mut host, id) = posted(|message| message.embeds = album());
    let pictures: Vec<_> = (1..=3).map(super::media_viewer::numbered).collect();
    for (index, drawn) in pictures.iter().enumerate() {
        let source = picture(&index.to_string()).unwrap().source;
        host.ui.images.insert(source, drawn.clone());
    }
    host.click(&card_picture(&format!("msg:{id}"), false, 0, 0));
    super::media_viewer::strip_shows_what_it_opens(&mut host, &pictures);
}

#[test]
fn a_card_behind_a_spoiler_shows_and_loads_nothing_until_revealed() {
    let link = "https://example.test/final.png";
    let (mut host, id) = posted(|message| {
        message.content = format!("Mirá ||{link}||");
        message.embeds = vec![
            Embed {
                kind: EmbedKind::Image,
                url: Some(link.into()),
                image: picture("final"),
                ..Embed::default()
            },
            Embed {
                title: Some("Pista".into()),
                description: Some("La respuesta es ||cuarenta y dos||".into()),
                url: Some("https://example.test/pista".into()),
                image: picture("pista"),
                ..Embed::default()
            },
        ];
    });
    let hidden = [picture("final"), picture("pista")].map(|found| found.unwrap().source);
    assert!(hidden.iter().all(|source| !wanted(&host).contains(source)));
    let painted = host.painted();
    assert_eq!(
        painted.iter().filter(|text| *text == "SPOILER").count(),
        2,
        "{painted:?}"
    );
    // Its link does not open what it hides.
    let base = format!("msg:{id}");
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: card_picture(&base, false, 0, 0),
        },
        host.now,
    );
    assert!(host.ui.attachment_viewer.is_none());
    host.click(&format!("{base}:spoiler:{}", card_spoiler(false, 0)));
    host.frame();
    assert!(wanted(&host).contains(&hidden[0]));
    assert!(!wanted(&host).contains(&hidden[1]), "each card on its own");
    host.click(&format!("{base}:spoiler:{}", card_spoiler(false, 1)));
    host.frame();
    assert!(wanted(&host).contains(&hidden[1]));
    // The text's own spoiler is still hidden, by a reveal of its own.
    let frame = host.frame();
    let hidden_text = frame.regions.iter().any(|region| {
        region.id.starts_with(&format!("{base}:spoiler:"))
            && region.id != format!("{base}:spoiler:{}", card_spoiler(false, 1))
    });
    assert!(hidden_text, "the text's spoiler waits for its own click");
}

#[test]
fn a_forwarded_picture_previews_and_opens_as_the_messages_own_do() {
    let (mut host, id) = posted(|message| {
        message.forwarded = Some(Forwarded {
            content: "La foto".into(),
            timestamp: message.timestamp,
            attachments: vec![Attachment {
                id: AttachmentId(400),
                filename: "cartel.png".into(),
                size_bytes: 3,
                content_type: Some("image/png".into()),
                dimensions: Some((800, 450)),
                preview: Some(source()),
                download: Some(RemoteFile { url: source().url }),
                spoiler: false,
                voice: None,
                explicit: false,
                placeholder: None,
            }],
            ..Forwarded::default()
        });
    });
    assert!(wanted(&host).contains(&source()));
    host.ui.images.insert(source(), image());
    let open = attachment_ids::action(id, AttachmentId(400), "preview");
    host.click(&open);
    host.frame();
    assert_eq!(
        host.ui
            .attachment_viewer
            .as_ref()
            .map(|viewer| viewer.viewed.clone()),
        Some(Viewed::File(AttachmentId(400)))
    );
    // «Guardar como…» resolves the forwarded file again.
    assert!(host
        .frame()
        .regions
        .iter()
        .any(|region| region.id == attachment_ids::action(id, AttachmentId(400), "save-as")));
}

#[test]
fn the_picture_at_its_own_size_loads_only_when_asked_and_replaces_the_preview() {
    let (mut host, id) = posted(|message| message.embeds = album());
    host.click(&card_picture(&format!("msg:{id}"), false, 0, 0));
    let original = RemoteImage {
        kind: ImageKind::Original,
        ..picture("0").unwrap().source
    };
    assert!(!wanted(&host).contains(&original));
    host.click(crate::attachments::ORIGINAL);
    assert!(host.ui.attachment_viewer.as_ref().unwrap().original);
    assert!(wanted(&host).contains(&original));
    assert!(
        wanted(&host).contains(&picture("0").unwrap().source),
        "the preview stays until it arrives"
    );
    // Another picture starts from its preview again.
    host.key(Key::ArrowRight, false, false);
    assert!(!host.ui.attachment_viewer.as_ref().unwrap().original);
    assert!(!wanted(&host).contains(&original));
}
