//! Message content (contenido-de-mensajes): explicit pictures behind the
//! account's filter, their ThumbHash, copying a picture, and the
//! simulation's records of calls and forward.

use super::attachments::{populated, source};
use super::*;
use rune_components::message::attachment_ids;
use rune_model::components::Media;
use rune_model::{
    AttachmentId, Component, Components, ContentFilter, Embed, EmbedImage, EmbedKind, Forwarded,
    ImageKind, Message, MessagingPrivacy, RemoteFile, RemoteImage, TextReference, TextTarget,
};

fn hash() -> RemoteImage {
    RemoteImage {
        url: "oPcNHZpwd3dxeHiHh3eHh9jQDPh4".into(),
        kind: ImageKind::ThumbHash,
    }
}

/// The fixture's message after `change`, and the account's filter
/// blurring explicit pictures in servers.
fn changed(change: impl FnOnce(&mut Message)) -> (Host, MessageId, AttachmentId) {
    let (mut host, message, attachment) = populated();
    let channel = host.ui.channel().unwrap();
    let mut updated = host
        .store
        .timeline(channel)
        .unwrap()
        .get(message)
        .unwrap()
        .as_ref()
        .clone();
    change(&mut updated);
    host.event(Event::MessageUpdated(updated));
    host.event(Event::MessagingPrivacy(MessagingPrivacy {
        explicit_servers: ContentFilter::Blur,
        ..MessagingPrivacy::default()
    }));
    host.frame();
    (host, message, attachment)
}

/// The fixture's picture marked explicit, with its ThumbHash.
fn explicit() -> (Host, MessageId, AttachmentId) {
    changed(|said| {
        said.attachments[0].explicit = true;
        said.attachments[0].placeholder = Some(hash());
    })
}

fn picture(name: &str) -> RemoteImage {
    RemoteImage {
        url: format!("https://media.discordapp.net/external/x/{name}.png"),
        kind: ImageKind::AttachmentPreview,
    }
}

fn wanted(host: &Host) -> Vec<RemoteImage> {
    wanted_images(&host.ui, &host.store, &host.tree)
}

#[test]
fn an_explicit_card_loads_blurred_and_opens_only_once_revealed() {
    let (mut host, message, _) = changed(|said| {
        said.attachments.clear();
        said.embeds = vec![Embed {
            kind: EmbedKind::Image,
            image: Some(EmbedImage {
                source: picture("a"),
                dimensions: (400, 300),
            }),
            explicit: true,
            ..Embed::default()
        }];
    });
    assert!(wanted(&host).contains(&picture("a").obscured()));
    assert!(!wanted(&host).contains(&picture("a")));
    let base = format!("msg:{message}");
    let open = |host: &mut Host| {
        update(
            &mut host.ui,
            &mut host.store,
            &UiAction::Clicked {
                id: rune_components::message::card_picture(&base, false, 0, 0),
            },
            host.now,
        );
        host.ui.attachment_viewer.is_some()
    };
    assert!(!open(&mut host), "the viewer waits for the reveal");
    let reveal = rune_components::message::card_spoiler(false, 0);
    host.click(&format!("{base}:spoiler:{reveal}"));
    host.frame();
    assert!(wanted(&host).contains(&picture("a")));
    assert!(!wanted(&host).contains(&picture("a").obscured()));
    assert!(open(&mut host));
}

#[test]
fn an_apps_explicit_picture_loads_blurred_until_its_reveal() {
    let (mut host, message, _) = changed(|said| {
        said.attachments.clear();
        said.components = Components {
            parts: vec![Component::Gallery(vec![Media {
                image: Some(EmbedImage {
                    source: picture("b"),
                    dimensions: (64, 64),
                }),
                description: None,
                spoiler: false,
                explicit: true,
            }])],
            whole: true,
            ..Components::default()
        };
    });
    assert!(wanted(&host).contains(&picture("b").obscured()));
    assert!(!wanted(&host).contains(&picture("b")));
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "CONTENIDO EXPLÍCITO"));
    host.click(&format!("msg:{message}:component:0:reveal"));
    host.frame();
    assert!(wanted(&host).contains(&picture("b")));
    assert!(!wanted(&host).contains(&picture("b").obscured()));
}

#[test]
fn the_emoji_a_forward_names_load_with_it() {
    let wave = RemoteImage {
        url: "https://cdn.discordapp.com/emojis/1.webp".into(),
        kind: ImageKind::Artwork,
    };
    let (host, _, _) = changed(|said| {
        said.attachments.clear();
        said.forwarded = Some(Forwarded {
            content: "<:wave:1>".into(),
            text_references: vec![TextReference {
                source: "<:wave:1>".into(),
                target: TextTarget::Emoji {
                    name: "wave".into(),
                    image: Some(wave.clone()),
                    jumbo: None,
                    id: None,
                },
            }],
            ..Forwarded::default()
        });
    });
    assert!(wanted(&host).contains(&wave));
}

#[test]
fn an_explicit_picture_loads_blurred_and_opens_only_once_revealed() {
    let (mut host, message, attachment) = explicit();
    let action = |verb| attachment_ids::action(message, attachment, verb);
    let wanted = wanted_images(&host.ui, &host.store, &host.tree);
    assert!(wanted.contains(&source().obscured()));
    assert!(wanted.contains(&hash()), "its ThumbHash while it loads");
    assert!(
        !wanted.contains(&source()),
        "the picture waits for the click"
    );
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "CONTENIDO EXPLÍCITO"));
    // Neither the viewer nor a copy reach it before the click.
    let preview = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: action("preview"),
        },
        host.now,
    );
    assert!(preview.is_empty() && host.ui.attachment_viewer.is_none());
    let copy = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: action("copy-image"),
        },
        host.now,
    );
    assert!(copy.is_empty());

    host.click(&action("reveal"));
    host.frame();
    let wanted = wanted_images(&host.ui, &host.store, &host.tree);
    assert!(wanted.contains(&source()) && !wanted.contains(&source().obscured()));
    // Loaded, its ThumbHash is no longer asked for.
    host.ui.images.insert(source(), super::attachments::image());
    host.frame();
    assert!(!wanted_images(&host.ui, &host.store, &host.tree).contains(&hash()));
    assert!(!host
        .painted()
        .iter()
        .any(|text| text == "CONTENIDO EXPLÍCITO"));
    let copy = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: action("copy-image"),
        },
        host.now,
    );
    assert_eq!(
        copy,
        vec![Intent::CopyFile {
            source: RemoteFile { url: source().url },
            filename: "fixture.png".into(),
            size_bytes: 12_288,
            image: true,
        }]
    );
    // A picture is not copied as a video.
    let video = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: action("copy-video"),
        },
        host.now,
    );
    assert!(video.is_empty());
    host.click(&action("preview"));
    assert!(host.ui.attachment_viewer.is_some());
}

#[test]
fn the_attachment_menu_offers_copying_the_picture() {
    let (mut host, message, attachment) = populated();
    host.tree
        .focus(&attachment_ids::action(message, attachment, "preview"));
    host.send(UiEvent::ContextMenuKeyboard);
    assert!(host.tree.context_menu_open());
    let painted = host.painted();
    assert!(painted.iter().any(|text| text == "Copiar imagen"));
    assert!(!painted.iter().any(|text| text == "Copiar video"));
}

#[test]
fn the_simulation_records_a_call_joined_and_one_missed_with_ada() {
    let mut host = Host::new();
    let ada = UserId(10);
    let direct = host
        .store
        .direct_with(ada)
        .expect("a conversation with Ada");
    let intents = open(&mut host.ui, &mut host.store, None, direct);
    host.run(intents);
    host.frame();
    host.tree.scroll_to_end(&timeline_id(direct));
    let painted = host.painted().concat();
    assert!(
        painted.contains("inició una llamada que duró 12 minutos."),
        "{painted}"
    );
    assert!(painted.contains("Te perdiste una llamada de"), "{painted}");
    assert!(painted.contains("que duró un minuto."), "{painted}");
}
