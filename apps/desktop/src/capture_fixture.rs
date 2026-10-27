//! Local visual evidence, reachable only from the mock screenshot harness.

use rune_model::{Attachment, AttachmentId, ImageKind, MessageId, RemoteFile, RemoteImage};
use rune_provider::Event;
use rune_state::Store;
use rune_ui::Ui;

/// Deliberately synthetic preview: no intents are dispatched to a provider.
pub fn invitation(store: &mut Store, ui: &mut Ui) -> bool {
    use rune_provider::InvitationResult;
    let Some(mut message) = ui.channel().and_then(|channel| {
        let timeline = store.timeline(channel)?;
        if !timeline.latest {
            return None;
        }
        timeline
            .messages()
            .last()
            .map(|message| message.as_ref().clone())
    }) else {
        return false;
    };
    let Some(provider) = store.provider.as_mut() else {
        return false;
    };
    assert_eq!(provider.kind, rune_provider::ProviderKind::Simulated);
    provider.capabilities.invitation_preview = true;
    provider.capabilities.invitation_accept = true;
    // The modal is the simulation's link it always refuses, shown as the
    // fixture's server: `--action invitation:accept` captures the refusal.
    let (target, destination) = rune_provider::mock::refused_invitation();
    message.timestamp = rune_provider::mock::wall_clock();
    message.kind = rune_model::MessageKind::Default;
    message.delivery = rune_model::Delivery::Sent;
    message.reply_to = None;
    message.edited = None;
    message.reactions.clear();
    message.attachments.clear();
    message.stickers.clear();
    message.mentions.clear();
    let now = std::time::Instant::now();
    // One card per state: unavailable, already a member, and to join amid
    // text with a guest's and a group's; then a friendship (last, in view).
    for (id, content) in [
        (91, format!("{FIXTURE}gone-fixture")),
        (92, format!("{FIXTURE}member-fixture")),
        (
            93,
            format!(
                "Para el sábado: {FIXTURE}local-fixture, la radio {FIXTURE}guest-fixture \
                 y el grupo {FIXTURE}group-fixture"
            ),
        ),
        (94, format!("¿Nos agregamos? {FIXTURE}friend-fixture")),
    ] {
        message.id = MessageId(9_000_000_000_000_000 + id);
        message.text_references = content
            .split_whitespace()
            .map(|word| word.trim_end_matches(','))
            .filter(|word| word.starts_with(FIXTURE))
            .map(|word| rune_model::TextReference {
                source: word.into(),
                target: rune_model::TextTarget::Invitation,
            })
            .collect();
        message.content = content;
        let applied = store.apply(
            Event::MessageCreated {
                message: message.clone(),
                nonce: None,
            },
            now,
        );
        // The capture harness never dispatches this fixture's intents.
        let _ = rune_ui::on_applied(ui, store, &applied, now);
    }
    let intents = rune_ui::open_invitation(ui, store, target.clone());
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            rune_ui::Intent::PreviewInvitation { request, .. } => Some(*request),
            _ => None,
        })
        .expect("mock preview enabled");
    rune_ui::invitation_result(
        ui,
        store,
        InvitationResult {
            request,
            target,
            result: Ok(rune_provider::InvitationPreview {
                destination,
                ..joinable()
            }),
        },
    );
    true
}

const FIXTURE: &str = "https://fixture.invalid/";

fn joinable() -> rune_provider::InvitationPreview {
    rune_provider::InvitationPreview {
        destination: rune_provider::InvitationDestination::Server(rune_model::ServerId(999_999)),
        name: "Jardín de RUNE · simulado".into(),
        description: Some("Un lugar para compartir proyectos y conversar.".into()),
        image: None,
        members: Some(45),
        online: Some(12),
        expires_at: None,
        event: None,
    }
}

/// The fixture's own reading of its links, standing in for an adapter's.
pub fn recognize(url: &str) -> Option<rune_provider::InvitationTarget> {
    let key = url.strip_prefix(FIXTURE)?;
    Some(rune_provider::InvitationTarget {
        key: key.into(),
        url: url.into(),
    })
}

/// The synthetic answer to a card's lookup: the fixture never asks a provider.
pub fn card_answer(
    store: &Store,
    ui: &Ui,
    request: u64,
    target: rune_provider::InvitationTarget,
) -> rune_provider::InvitationResult {
    let member =
        ui.server
            .and_then(|id| store.server(id))
            .map(|server| rune_provider::InvitationPreview {
                destination: rune_provider::InvitationDestination::Server(server.id),
                name: server.name.clone(),
                members: Some(8),
                online: Some(3),
                ..joinable()
            });
    let other = |destination, name: &str, members, online| rune_provider::InvitationPreview {
        destination,
        name: name.into(),
        description: None,
        members,
        online,
        ..joinable()
    };
    let result = match target.key.as_str() {
        "local-fixture" => Ok(joinable()),
        "member-fixture" => member.ok_or(rune_provider::InvitationError::Invalid),
        "guest-fixture" => Ok(other(
            rune_provider::InvitationDestination::Guest {
                server: rune_model::ServerId(999_998),
                channel: rune_model::ChannelId(999_997),
            },
            "Radio RUNE · simulado",
            Some(40),
            Some(6),
        )),
        "group-fixture" => Ok(other(
            rune_provider::InvitationDestination::Group(rune_model::ChannelId(999_996)),
            "Los jueves · simulado",
            Some(4),
            None,
        )),
        "friend-fixture" => Ok(other(
            rune_provider::InvitationDestination::Friend(rune_model::UserId(999_995)),
            "Lucía · simulada",
            None,
            None,
        )),
        _ => Err(rune_provider::InvitationError::Invalid),
    };
    rune_provider::InvitationResult {
        request,
        target,
        result,
    }
}

pub fn attachment(store: &Store, ui: &Ui) -> Option<(Event, RemoteImage, limn::RasterImage)> {
    let channel = ui.channel()?;
    let timeline = store.timeline(channel)?;
    if !timeline.latest {
        return None;
    }
    let mut message = timeline.messages().last()?.as_ref().clone();
    message.id = MessageId(9_000_000_000_000_082);
    message.kind = rune_model::MessageKind::Default;
    message.delivery = rune_model::Delivery::Sent;
    message.timestamp = rune_provider::mock::wall_clock();
    message.content = "Imagen y archivo de prueba local".into();
    message.reply_to = None;
    message.reactions.clear();
    message.mentions.clear();
    message.stickers.clear();
    message.text_references.clear();
    let source = RemoteImage {
        url: "https://fixture.invalid/rune-attachment.png".into(),
        kind: ImageKind::AttachmentPreview,
    };
    message.attachments = vec![
        Attachment {
            id: AttachmentId(82),
            filename: "Una imagen con nombre muy largo para comprobar el recorte y el ancho.png"
                .into(),
            size_bytes: 5868,
            content_type: Some("image/png".into()),
            dimensions: Some((960, 540)),
            preview: Some(source.clone()),
            download: Some(RemoteFile {
                url: source.url.clone(),
            }),
            spoiler: false,
            voice: None,
            explicit: false,
            placeholder: None,
        },
        Attachment {
            id: AttachmentId(83),
            filename: "Notas del proyecto y recursos de ejemplo.pdf".into(),
            size_bytes: 20480,
            content_type: Some("application/pdf".into()),
            dimensions: None,
            preview: None,
            download: Some(RemoteFile {
                url: "https://fixture.invalid/notes.pdf".into(),
            }),
            spoiler: false,
            voice: None,
            explicit: false,
            placeholder: None,
        },
        // A spoiler keeps its preview reference but shows nothing until revealed.
        Attachment {
            id: AttachmentId(84),
            filename: "SPOILER_final.png".into(),
            size_bytes: 5868,
            content_type: Some("image/png".into()),
            dimensions: Some((960, 540)),
            preview: Some(source.clone()),
            download: Some(RemoteFile {
                url: source.url.clone(),
            }),
            spoiler: true,
            voice: None,
            explicit: false,
            placeholder: None,
        },
    ];
    let image = limn::RasterImage::new(
        "c70e30d34f864733efb6c4afbf5270c1be63833a3e2e7648d3c9da1a94e829ec",
        limn::RasterImageFormat::Png,
        5868,
        std::sync::Arc::from(include_str!("../fixtures/attachment.png.b64")),
        "Imagen geométrica de prueba local",
    )?;
    Some((
        Event::MessageCreated {
            message,
            nonce: None,
        },
        source,
        image,
    ))
}

/// The message `--capture-viewer` adds: three pictures to open in the viewer,
/// a small drawing, a large photo and a logo with transparency.
pub const VIEWER_MESSAGE: MessageId = MessageId(9_000_000_000_000_086);

/// [`VIEWER_MESSAGE`], after the latest message of the open conversation;
/// [`simulated`] draws its pictures. Nothing reaches a provider.
pub fn viewer(store: &Store, ui: &Ui) -> Option<Event> {
    let channel = ui.channel()?;
    let timeline = store.timeline(channel)?;
    if !timeline.latest {
        return None;
    }
    let mut message = timeline.messages().last()?.as_ref().clone();
    message.id = VIEWER_MESSAGE;
    message.kind = rune_model::MessageKind::Default;
    message.delivery = rune_model::Delivery::Sent;
    message.timestamp = rune_provider::mock::wall_clock();
    message.content = "Tres imágenes para el visor (simuladas)".into();
    message.reply_to = None;
    message.reactions.clear();
    message.mentions.clear();
    message.stickers.clear();
    message.text_references.clear();
    message.embeds.clear();
    let picture = |id: u64, filename: &str, size_bytes: u64, dimensions: (u32, u32)| {
        let url = format!("https://simulacion.invalid/visor/{filename}");
        Attachment {
            id: AttachmentId(id),
            filename: filename.into(),
            size_bytes,
            content_type: Some("image/png".into()),
            dimensions: Some(dimensions),
            preview: Some(RemoteImage {
                url: url.clone(),
                kind: ImageKind::AttachmentPreview,
            }),
            download: Some(RemoteFile { url }),
            spoiler: false,
            voice: None,
            explicit: false,
            placeholder: None,
        }
    };
    message.attachments = vec![
        picture(86, "dibujo.png", 48_213, VIEWER_SMALL),
        picture(87, "foto.png", 3_912_448, VIEWER_LARGE),
        picture(88, "logo.png", 21_904, VIEWER_LOGO),
    ];
    Some(Event::MessageCreated {
        message,
        nonce: None,
    })
}

const VIEWER_SMALL: (u32, u32) = (300, 340);
const VIEWER_LARGE: (u32, u32) = (2400, 1600);
const VIEWER_LOGO: (u32, u32) = (512, 512);

/// The `index`th picture of [`media`]'s album, which [`simulated`] draws.
fn album_picture(index: usize) -> rune_model::EmbedImage {
    rune_model::EmbedImage {
        source: RemoteImage {
            url: format!("https://simulacion.invalid/visor/album-{index}.png"),
            kind: ImageKind::AttachmentPreview,
        },
        dimensions: ALBUM,
    }
}

const ALBUM: (u32, u32) = (960, 540);

/// The pixels of a picture of [`viewer`] or of [`media`]'s album: the large
/// one is served at the preview's 768 by 512 unless its own size is asked
/// for.
fn viewer_picture(name: &str, original: bool) -> Option<image::RgbaImage> {
    // The album's: the same hills at dawn, at noon and at dusk.
    if let Some(index) = name
        .strip_prefix("album-")
        .and_then(|rest| rest.strip_suffix(".png"))
        .and_then(|index| index.parse::<u32>().ok())
        .filter(|index| *index < 3)
    {
        let (width, height) = ALBUM;
        let skies = [[250, 170, 120], [110, 180, 240], [120, 70, 150]];
        let sky = skies[index as usize];
        return Some(image::RgbaImage::from_fn(width, height, |x, y| {
            let (fx, fy) = (x as f32, y as f32);
            let hill = fy > 330.0 + 60.0 * (fx / 120.0 + index as f32).sin();
            let sun = (fx - 240.0 - 240.0 * index as f32).hypot(fy - 140.0) < 50.0;
            let t = fy / height as f32;
            let [r, g, b] = if hill {
                [60, (110 + 30 * index) as u8, 70]
            } else if sun {
                [250, 230, 140]
            } else {
                sky.map(|channel: u32| (channel as f32 * (1.0 - 0.3 * t)) as u8)
            };
            image::Rgba([r, g, b, 255])
        }));
    }
    match name {
        // A flat drawing: sky, sun, two hills and a path, every pixel opaque.
        "dibujo.png" => {
            let (width, height) = VIEWER_SMALL;
            Some(image::RgbaImage::from_fn(width, height, |x, y| {
                let (fx, fy) = (x as f32, y as f32);
                let sun = (fx - 210.0).hypot(fy - 90.0) < 38.0;
                let back = fy > 200.0 + 30.0 * (fx / 60.0).sin();
                let front = fy > 250.0 + 18.0 * (fx / 45.0 + 1.0).cos();
                let path =
                    front && (fx - 150.0 - (fy - 250.0) * 0.6).abs() < 6.0 + (fy - 250.0) * 0.2;
                let [r, g, b] = if path {
                    [214, 190, 140]
                } else if front {
                    [72, 140, 84]
                } else if back {
                    [104, 160, 120]
                } else if sun {
                    [250, 210, 90]
                } else {
                    let t = fy / 200.0;
                    [(120.0 + 90.0 * t) as u8, (170.0 + 50.0 * t) as u8, 235]
                };
                image::Rgba([r, g, b, 255])
            }))
        }
        // A photo: a dusk gradient, a ridge, and a fine grid to judge sharpness.
        "foto.png" => {
            let (width, height) = if original { VIEWER_LARGE } else { (768, 512) };
            let scale = width as f32 / VIEWER_LARGE.0 as f32;
            Some(image::RgbaImage::from_fn(width, height, |x, y| {
                let (fx, fy) = (x as f32 / scale, y as f32 / scale);
                let ridge = 980.0 + 140.0 * (fx / 310.0).sin() + 60.0 * (fx / 97.0).cos();
                let grid = (fx as u32 % 80 < 2 || fy as u32 % 80 < 2) && fy > ridge;
                let t = fy / 1600.0;
                let [r, g, b] = if grid {
                    [235, 228, 210]
                } else if fy > ridge {
                    [(40.0 + 30.0 * t) as u8, (52.0 + 20.0 * t) as u8, 70]
                } else {
                    [
                        (250.0 - 120.0 * t) as u8,
                        (150.0 - 60.0 * t) as u8,
                        (110.0 + 90.0 * t) as u8,
                    ]
                };
                image::Rgba([r, g, b, 255])
            }))
        }
        // A dark star on nothing: transparent around it, half-transparent at its glow.
        "logo.png" => {
            let (width, height) = VIEWER_LOGO;
            Some(image::RgbaImage::from_fn(width, height, |x, y| {
                let (dx, dy) = (x as f32 - 256.0, y as f32 - 256.0);
                let radius = dx.hypot(dy);
                let edge = 150.0 + 60.0 * (dy.atan2(dx) * 5.0).cos();
                if radius < edge {
                    image::Rgba([36, 30, 64, 255])
                } else if radius < edge + 24.0 {
                    image::Rgba([120, 90, 220, (255.0 * (1.0 - (radius - edge) / 24.0)) as u8])
                } else {
                    image::Rgba([0, 0, 0, 0])
                }
            }))
        }
        _ => None,
    }
}

/// Simulated records, a forward, an app's card with a picture, a voted poll
/// and one still to vote after the latest message, to see rich content
/// drawn; nothing reaches a provider. Returns the picture to serve locally.
pub fn rich(store: &mut Store, ui: &mut Ui) -> Option<(RemoteImage, limn::RasterImage)> {
    use rune_model::{Embed, EmbedField, EmbedImage, Forwarded, MessageKind, Poll, PollAnswer};
    let base = ui.channel().and_then(|channel| {
        let timeline = store.timeline(channel)?;
        timeline
            .latest
            .then(|| {
                timeline
                    .messages()
                    .last()
                    .map(|message| message.as_ref().clone())
            })
            .flatten()
    })?;
    let now = rune_provider::mock::wall_clock();
    let blank = |id: u64, kind: MessageKind| {
        let mut message = base.clone();
        message.id = MessageId(9_000_000_000_000_100 + id);
        message.timestamp = now;
        message.kind = kind;
        message.delivery = rune_model::Delivery::Sent;
        message.content.clear();
        message.reply_to = None;
        message.edited = None;
        message.reactions.clear();
        message.attachments.clear();
        message.stickers.clear();
        message.mentions.clear();
        message.text_references.clear();
        message.embeds.clear();
        message.forwarded = None;
        message.poll = None;
        message.components = Default::default();
        message
    };
    let pinned = blank(1, MessageKind::Pinned);
    let mut boost = blank(2, MessageKind::Boost { tier: 1 });
    boost.content = "2".into();
    let mut forward = blank(5, MessageKind::Default);
    forward.forwarded = Some(Forwarded {
        content: "Mañana a las **18 h** probamos la versión nueva. ¿Quién se suma?".into(),
        timestamp: now.plus_minutes(-90),
        attachments: vec![Attachment {
            id: AttachmentId(84),
            filename: "agenda.pdf".into(),
            size_bytes: 20_480,
            content_type: Some("application/pdf".into()),
            dimensions: None,
            preview: None,
            download: None,
            spoiler: false,
            voice: None,
            explicit: false,
            placeholder: None,
        }],
        // An app's spoilers in the copy: the second one already revealed.
        components: rune_model::Components {
            parts: ["Foto del ensayo (simulada)", "Plano de la sala (simulado)"]
                .map(|said| rune_model::Component::Section {
                    parts: vec![rune_model::Component::Text(said.into())],
                    accessory: Box::new(rune_model::Component::Thumbnail(
                        rune_model::components::Media {
                            image: None,
                            description: Some(said.into()),
                            spoiler: true,
                            explicit: false,
                        },
                    )),
                })
                .into(),
            ..Default::default()
        },
        ..Forwarded::default()
    });
    ui.messages.revealed.insert(
        forward.id,
        vec![
            rune_components::message::PARTS_SPOILER
                + rune_components::message::first_spoiler(true, None)
                + 1,
        ],
    );
    let source = RemoteImage {
        url: "https://fixture.invalid/rune-embed.png".into(),
        kind: ImageKind::AttachmentPreview,
    };
    let mut card = blank(3, MessageKind::Default);
    card.content = "Resumen del día (simulado)".into();
    card.embeds = vec![Embed {
        author: Some("Panel de estado".into()),
        title: Some("Compilación de RUNE".into()),
        url: Some("https://example.invalid/build".into()),
        description: Some("Todo **en verde**. Revisa los detalles abajo.".into()),
        fields: vec![
            EmbedField {
                name: "Pruebas".into(),
                value: "Todas aprobadas".into(),
                inline: true,
            },
            EmbedField {
                name: "Rama".into(),
                value: "`master`".into(),
                inline: true,
            },
        ],
        footer: Some("Simulación local".into()),
        timestamp: Some(now),
        color: Some(0x23A55A),
        image: Some(EmbedImage {
            source: source.clone(),
            dimensions: (960, 540),
        }),
        ..Embed::default()
    }];
    // A notice the service sends about the account, as the adapter maps it.
    card.embeds.push(Embed {
        kind: rune_model::EmbedKind::SystemNotice,
        title: Some("Mensaje importante acerca de tu cuenta (simulado)".into()),
        description: Some("Revisa la actividad reciente de tu cuenta.".into()),
        url: Some("https://example.invalid/notice".into()),
        timestamp: Some(now),
        ..Embed::default()
    });
    let mut poll = blank(4, MessageKind::Default);
    let answer = |id, text: &str, emoji: Option<&str>, votes, me| PollAnswer {
        id,
        text: text.into(),
        emoji: emoji.map(str::to_owned),
        votes,
        me,
    };
    poll.poll = Some(Poll {
        question: "¿Qué recorrido sigue después del contenido enriquecido?".into(),
        answers: vec![
            answer(1, "Hilos y foros", Some("🧵"), 7, true),
            answer(2, "Amigos y privados", Some("👋"), 4, false),
            answer(3, "Llamadas", None, 1, false),
        ],
        expiry: Some(now.plus_minutes(26 * 60)),
        results_known: true,
        ..Poll::default()
    });
    // Not voted yet: its answers are the choices, and results wait.
    let mut open = blank(6, MessageKind::Default);
    open.poll = Some(Poll {
        question: "¿A qué hora probamos? (simulado)".into(),
        answers: vec![
            answer(1, "A las 18 h", Some("🕕"), 3, false),
            answer(2, "A las 21 h", Some("🌙"), 2, false),
        ],
        expiry: Some(now.plus_minutes(90)),
        multiselect: true,
        results_known: true,
        ..Poll::default()
    });
    // The record of a closed poll, answering the poll above as the service does.
    let mut result = blank(7, MessageKind::PollResult);
    result.reply_to = Some(poll.id);
    let field = |name: &str, value: &str| EmbedField {
        name: name.into(),
        value: value.into(),
        inline: false,
    };
    result.embeds = vec![Embed {
        kind: rune_model::EmbedKind::PollResult,
        fields: vec![
            field("poll_question_text", "¿Qué recorrido sigue? (simulado)"),
            field("victor_answer_id", "1"),
            field("victor_answer_text", "Hilos y foros"),
            field("victor_answer_votes", "7"),
            field("total_votes", "12"),
            field("victor_answer_emoji_name", "🧵"),
        ],
        ..Embed::default()
    }];
    let mut voice = blank(8, MessageKind::Default);
    voice.content = "Estoy en la sala, ¿se suman? (simulado)".into();
    let room = base.channel;
    let room = store
        .channel(room)
        .and_then(|found| found.server)
        .into_iter()
        .flat_map(|server| store.channel_tree(server))
        .flat_map(|(_, channels)| channels)
        .find(|found| found.kind == rune_model::ChannelKind::Voice)
        .map(|found| (found.id, found.name.clone()));
    voice.embeds = vec![Embed {
        kind: rune_model::EmbedKind::VoiceChannel,
        author: Some("Canal de voz".into()),
        title: room.as_ref().map(|(_, name)| name.clone()),
        description: Some("Hay gente charlando ahora (simulado).".into()),
        channel: room.map(|(id, _)| id),
        ..Embed::default()
    }];
    let mut parts = blank(9, MessageKind::Default);
    parts.components = app_parts(&source);
    let at = std::time::Instant::now();
    for message in [
        pinned, boost, card, poll, forward, open, result, voice, parts,
    ] {
        let applied = store.apply(
            Event::MessageCreated {
                message,
                nonce: None,
            },
            at,
        );
        // The capture harness never dispatches this fixture's intents.
        let _ = rune_ui::on_applied(ui, store, &applied, at);
    }
    let image = limn::RasterImage::new(
        "c70e30d34f864733efb6c4afbf5270c1be63833a3e2e7648d3c9da1a94e829ec",
        limn::RasterImageFormat::Png,
        5868,
        std::sync::Arc::from(include_str!("../fixtures/attachment.png.b64")),
        "Imagen geométrica de prueba local",
    )?;
    Some((source, image))
}

/// The words of the capture's three-page document, for its search: each
/// line's words set left to right from the top, as a text extractor places
/// them, in letters of the same width: where [`document_page`] draws them.
pub fn document_text() -> std::sync::Arc<[Vec<rune_model::PdfWord>]> {
    [
        "Notas del proyecto (simuladas)\nObjetivos del taller y recursos de ejemplo.",
        "Agenda\nEl jueves revisamos el reproductor de video y la búsqueda en PDF.",
        "Recursos\nEjemplos de limn, capturas y la agenda del próximo taller.",
    ]
    .map(|page| {
        let mut words = Vec::new();
        for (row, line) in page.lines().enumerate() {
            let top = 0.1 + row as f32 * 0.08;
            let mut left = 0.1;
            for word in line.split_whitespace() {
                let right = left + word.chars().count() as f32 * 0.012;
                words.push(rune_model::PdfWord {
                    text: word.to_owned(),
                    bounds: [left, top, right, top + 0.05],
                });
                left = right + 0.012;
            }
        }
        words
    })
    .into()
}

/// Page 2 of the capture's document, its `marks` highlighted as the core
/// highlights a rendered page: Poppler's picture of a page RUNE wrote with
/// the words [`document_text`] places, in Courier. Nothing is rendered.
pub fn document_page(marks: &[rune_model::PdfMark]) -> Option<limn::RasterImage> {
    let png = include_bytes!("../fixtures/document-page.png").to_vec();
    let data = rune_core::highlight_page(png, marks)?;
    super::preview(
        &data,
        &rune_i18n::tr!("Página {page} del documento", page = 2),
    )
}

/// The message of the video [`media`] adds, which the capture plays.
pub const MEDIA_VIDEO: MessageId = MessageId(9_000_000_000_000_205);

/// Simulated cards with pictures after the latest message: a gallery of
/// one link, a card whose link is a spoiler and one whose text hides
/// something, a GIF card and a forward with a picture; with `players`, an
/// audio file and a video instead, without files (the host plays RUNE's
/// samples). Nothing reaches a provider; returns the picture to serve
/// locally.
pub fn media(
    store: &mut Store,
    ui: &mut Ui,
    players: bool,
) -> Option<(RemoteImage, limn::RasterImage)> {
    use rune_model::{Embed, EmbedImage, EmbedKind, Forwarded, MessageKind};
    let base = ui.channel().and_then(|channel| {
        let timeline = store.timeline(channel)?;
        timeline
            .latest
            .then(|| timeline.messages().last().map(|m| m.as_ref().clone()))
            .flatten()
    })?;
    let now = rune_provider::mock::wall_clock();
    let blank = |id: u64, content: &str| {
        let mut message = base.clone();
        message.id = MessageId(9_000_000_000_000_200 + id);
        message.timestamp = now;
        message.kind = MessageKind::Default;
        message.delivery = rune_model::Delivery::Sent;
        message.content = content.into();
        message.reply_to = None;
        message.edited = None;
        message.reactions.clear();
        message.attachments.clear();
        message.stickers.clear();
        message.mentions.clear();
        message.text_references.clear();
        message.embeds.clear();
        message.forwarded = None;
        message.poll = None;
        message.components = Default::default();
        message
    };
    let source = RemoteImage {
        url: "https://fixture.invalid/rune-embed.png".into(),
        kind: ImageKind::AttachmentPreview,
    };
    let picture = || {
        Some(EmbedImage {
            source: source.clone(),
            dimensions: (960, 540),
        })
    };
    // Three cards of one link, as a post with three pictures arrives: three
    // pictures of their own, not the GIF card's (which the capture animates).
    let mut album = blank(4, "Fotos del taller (simuladas)");
    album.embeds = (0..3)
        .map(|index| Embed {
            url: Some("https://example.invalid/album".into()),
            title: (index == 0).then(|| "Álbum del taller (simulado)".into()),
            description: (index == 0).then(|| "Tres fotos de la misma publicación.".into()),
            color: Some(0x5865F2),
            image: Some(album_picture(index)),
            ..Embed::default()
        })
        .collect();
    let mut spoiled = blank(3, "Mira el final ||https://example.invalid/final.png||");
    spoiled.embeds = vec![
        Embed {
            kind: EmbedKind::Image,
            url: Some("https://example.invalid/final.png".into()),
            image: picture(),
            ..Embed::default()
        },
        Embed {
            title: Some("Pista del acertijo (simulada)".into()),
            description: Some("La respuesta es ||cuarenta y dos||.".into()),
            url: Some("https://example.invalid/pista".into()),
            ..Embed::default()
        },
    ];
    let mut gif = blank(1, "");
    gif.embeds = vec![Embed {
        kind: EmbedKind::Gifv,
        url: Some("https://example.invalid/gif".into()),
        provider: Some("Sitio de GIF (simulado)".into()),
        thumbnail: picture(),
        video: picture(),
        ..Embed::default()
    }];
    let mut forward = blank(2, "");
    forward.forwarded = Some(Forwarded {
        content: "La foto del cartel (simulada)".into(),
        timestamp: now.plus_minutes(-30),
        attachments: vec![Attachment {
            id: AttachmentId(204),
            filename: "cartel.png".into(),
            size_bytes: 5868,
            content_type: Some("image/png".into()),
            dimensions: Some((960, 540)),
            preview: Some(source.clone()),
            download: Some(RemoteFile {
                url: source.url.clone(),
            }),
            spoiler: false,
            voice: None,
            explicit: false,
            placeholder: None,
        }],
        ..Forwarded::default()
    });
    let mut audio = blank(6, "La grabación del ensayo (simulada): un tono generado.");
    audio.attachments = vec![Attachment {
        id: AttachmentId(206),
        filename: "ensayo.mp3".into(),
        size_bytes: 12_347,
        content_type: Some("audio/mpeg".into()),
        dimensions: None,
        preview: None,
        download: None,
        spoiler: false,
        voice: None,
        explicit: false,
        placeholder: None,
    }];
    let mut video = blank(5, "Así quedó la demo (simulada): un patrón de prueba.");
    video.attachments = vec![Attachment {
        id: AttachmentId(205),
        filename: "demo.mp4".into(),
        size_bytes: 61_134,
        content_type: Some("video/mp4".into()),
        dimensions: Some((320, 180)),
        preview: None,
        download: None,
        spoiler: false,
        voice: None,
        explicit: false,
        placeholder: None,
    }];
    debug_assert_eq!(video.id, MEDIA_VIDEO);
    let at = std::time::Instant::now();
    let messages = if players {
        vec![audio, video]
    } else {
        vec![gif, forward, spoiled, album]
    };
    for message in messages {
        let applied = store.apply(
            Event::MessageCreated {
                message,
                nonce: None,
            },
            at,
        );
        // The capture harness never dispatches this fixture's intents.
        let _ = rune_ui::on_applied(ui, store, &applied, at);
    }
    let image = limn::RasterImage::new(
        "c70e30d34f864733efb6c4afbf5270c1be63833a3e2e7648d3c9da1a94e829ec",
        limn::RasterImageFormat::Png,
        5868,
        std::sync::Arc::from(include_str!("../fixtures/attachment.png.b64")),
        "Imagen geométrica de prueba local",
    )?;
    Some((source, image))
}

/// A message an app laid out, with the local picture as its thumbnail.
fn app_parts(source: &RemoteImage) -> rune_model::Components {
    use rune_model::components::{Button, Media};
    use rune_model::{ButtonStyle, Component, EmbedImage};
    let button = |label: &str, style, url: Option<&str>| {
        Component::Button(Button {
            label: Some(label.into()),
            style,
            url: url.map(str::to_owned),
            ..Button::default()
        })
    };
    rune_model::Components {
        parts: vec![Component::Container {
            accent: Some(0x5865F2),
            spoiler: false,
            parts: vec![
                Component::Section {
                    parts: vec![Component::Text(
                        "### Recordatorio del taller (simulado)\nLa reunión es el **jueves a las 18 h**."
                            .into(),
                    )],
                    accessory: Box::new(Component::Thumbnail(Media {
                        image: Some(EmbedImage {
                            source: source.clone(),
                            dimensions: (960, 540),
                        }),
                        description: None,
                        spoiler: false,
                        explicit: false,
                    })),
                },
                Component::Separator {
                    divider: true,
                    large: false,
                },
                Component::Row(vec![
                    // With a server emoji: the local picture stands for it,
                    // and plays as an animated one with `--capture-animation`
                    // (limn-teclado-y-superficies).
                    Component::Button(Button {
                        label: Some("Confirmar asistencia".into()),
                        style: ButtonStyle::Primary,
                        emoji: Some(":taller:".into()),
                        picture: Some(source.clone()),
                        ..Button::default()
                    }),
                    button(
                        "Abrir agenda",
                        ButtonStyle::Link,
                        Some("https://example.invalid/agenda"),
                    ),
                ]),
                // A picked option with a server emoji: the local picture
                // stands for its artwork.
                Component::Row(vec![Component::Select(rune_model::components::Select {
                    placeholder: Some("Elige un horario".into()),
                    chosen: vec![rune_model::components::Choice {
                        label: "Jueves 18 h (simulado)".into(),
                        emoji: Some(":taller:".into()),
                        picture: Some(source.clone()),
                    }],
                    ..Default::default()
                })]),
            ],
        }],
        whole: true,
        ..Default::default()
    }
}

/// Two local frames, 480 x 270 like the attachment's picture, that alternate
/// every half second: a capture shows the surface drawn in the still's place.
pub fn animation() -> std::sync::Arc<rune_core::Animation> {
    let frame = |shift: u32| rune_core::AnimationFrame {
        delay: std::time::Duration::from_millis(500),
        rgba: (0..270u32)
            .flat_map(|y| {
                (0..480u32).flat_map(move |x| {
                    let band = ((x + shift) / 60).is_multiple_of(2);
                    let shade = (y * 255 / 270) as u8;
                    if band {
                        [shade / 2, 90, 200, 255]
                    } else {
                        [230, 160, shade, 255]
                    }
                })
            })
            .collect(),
    };
    std::sync::Arc::new(rune_core::Animation {
        width: 480,
        height: 270,
        frames: vec![frame(0), frame(60)],
    })
}

/// A picture made here for a capture of the profile editor, prepared as a
/// chosen file is: no selector opens and nothing reaches a provider.
/// A made-up picture for a new emoji or sticker, made as a chosen one is.
pub fn expression_picture(
    kind: rune_provider::ExpressionKind,
) -> Result<Option<(rune_provider::ExpressionImage, limn::RasterImage)>, String> {
    let picture = image::RgbaImage::from_fn(256, 256, |x, y| {
        let inside = (x as i32 - 128).pow(2) + (y as i32 - 128).pow(2) < 110 * 110;
        image::Rgba([217, (90 + y / 4) as u8, 60, if inside { 255 } else { 0 }])
    });
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(picture)
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    let (image, preview) = rune_core::prepare_expression_image(png.get_ref(), kind)?;
    Ok(super::preview(&preview, rune_i18n::t("Imagen elegida")).map(|preview| (image, preview)))
}

/// A picture made here for a capture's profile, framed as `crop` says, with
/// the whole of it to frame it (edicion-de-perfil).
pub fn profile_picture(
    kind: rune_provider::ProfileImageKind,
    crop: rune_provider::Crop,
) -> Result<
    Option<(
        rune_provider::ProfileImage,
        limn::RasterImage,
        rune_ui::PictureSource,
    )>,
    String,
> {
    let picture = image::RgbImage::from_fn(640, 400, |x, y| {
        image::Rgb([(40 + x / 4) as u8, (60 + y / 3) as u8, (180 - x / 8) as u8])
    });
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(picture)
        .write_to(&mut png, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    let (image, preview) = rune_core::prepare_profile_image(png.get_ref(), kind, crop, false)?;
    let source = rune_core::profile_image_source(png.get_ref())?;
    let preview = super::preview(&preview, rune_i18n::t("Imagen elegida"));
    Ok(preview
        .zip(super::picture_source(&source))
        .map(|(preview, source)| (image, preview, source)))
}

/// A picture the simulation names on `simulacion.invalid` (P5), drawn here
/// for a capture: a ring for a decoration, a stripe for a nameplate, a
/// disc for the rest, in a color of its name. Nothing is fetched.
/// Decoded for a screen of `density`, as the image service decodes.
pub fn simulated(
    source: &rune_model::RemoteImage,
    density: f32,
) -> Option<(limn::RasterImage, Option<rune_model::Rgb>)> {
    let path = source.url.split("simulacion.invalid/").nth(1)?;
    // A size asked for draws the same picture (a jumbo emoji's).
    let path = path.split('?').next()?;
    // `cargando/` never arrives: what stands in for it shows (contenido-de-mensajes).
    if path.starts_with("perfiles/") || path.starts_with("grupos/") || path.starts_with("cargando/")
    {
        return None;
    }
    if let Some(name) = path.strip_prefix("visor/") {
        let picture = viewer_picture(name, source.kind == ImageKind::Original)?;
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(picture)
            .write_to(&mut png, image::ImageFormat::Png)
            .ok()?;
        let data = rune_core::normalize_image(png.get_ref(), source, density)?;
        return super::preview(&data, rune_i18n::t("Imagen simulada"))
            .map(|image| (image, data.color));
    }
    // An animated vector sticker: RUNE's own sample, drawn as one from the service.
    if path.ends_with(".json") {
        let lottie = include_bytes!("../../../assets/sticker-sample.json");
        let data = rune_core::normalize_image(lottie, source, density)?;
        return super::preview(&data, rune_i18n::t("Imagen simulada"))
            .map(|image| (image, data.color));
    }
    let hash = path.bytes().fold(0x811c_9dc5u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    let [r, g, b, _] = hash.to_le_bytes();
    let (r, g, b) = (r / 2 + 100, g / 2 + 90, b / 2 + 110);
    let (width, height) = if path.starts_with("placas/") {
        (229, 43)
    } else if path.starts_with("efectos/") {
        (225, 440)
    } else {
        (128, 128)
    };
    let picture = image::RgbaImage::from_fn(width, height, |x, y| {
        let (dx, dy) = (
            x as f32 - width as f32 / 2.0,
            y as f32 - height as f32 / 2.0,
        );
        let distance = (dx * dx + dy * dy).sqrt();
        let alpha = if path.starts_with("placas/") {
            (x * 200 / width) as u8
        } else if path.starts_with("efectos/") {
            // Sparks scattered over the canvas.
            let spark = (x.wrapping_mul(7919) ^ y.wrapping_mul(104_729)) % 211 < 2;
            if spark {
                230
            } else {
                0
            }
        } else if path.starts_with("decoraciones/") {
            // A ring outside the face, with leaves on it.
            let leaf = ((dy.atan2(dx) * 6.0).sin() > 0.6) as u8;
            if (52.0..64.0).contains(&distance) || (leaf == 1 && (46.0..64.0).contains(&distance)) {
                255
            } else {
                0
            }
        } else if distance < 60.0 {
            255
        } else {
            0
        };
        let shade = (x * 60 / width) as u8;
        image::Rgba([r.saturating_add(shade), g, b.saturating_sub(shade), alpha])
    });
    let mut png = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(picture)
        .write_to(&mut png, image::ImageFormat::Png)
        .ok()?;
    let data = rune_core::normalize_image(png.get_ref(), source, density)?;
    super::preview(&data, rune_i18n::t("Imagen simulada")).map(|image| (image, data.color))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_albums_pictures_are_three_of_their_own_each_drawn_for_the_capture() {
        let drawn: Vec<_> = (0..3)
            .map(|index| {
                let picture = album_picture(index);
                assert_eq!(picture.dimensions, ALBUM);
                simulated(&picture.source, 1.0)
                    .expect("the capture draws it")
                    .0
                    .sha256()
                    .to_owned()
            })
            .collect();
        assert!(
            drawn[0] != drawn[1] && drawn[1] != drawn[2] && drawn[0] != drawn[2],
            "{drawn:?}"
        );
        assert!(viewer_picture("album-3.png", false).is_none());
    }
}
