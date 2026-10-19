//! Received files use only validated artwork supplied by the host.

use super::{Live, MessageStyle};
use crate::{file_size, line, Look};
use limn::{
    Access, Attachment, Component, ContextMenu, ContextMenuEntry, ContextMenuItem, Element, Icon,
    Role, Size, Tooltip,
};
use rune_i18n::{t, tr};
use rune_model::{AttachmentId, MessageId};

/// Attachment actions carry entity IDs, never a signed URL.
pub mod ids {
    use super::{AttachmentId, MessageId};

    pub fn action(message: MessageId, attachment: AttachmentId, action: &str) -> String {
        format!("attachment:{message}:{attachment}:{action}")
    }

    pub fn parse(id: &str) -> Option<(MessageId, AttachmentId, &str)> {
        let mut parts = id.strip_prefix("attachment:")?.split(':');
        let message = MessageId(parts.next()?.parse().ok()?);
        let attachment = AttachmentId(parts.next()?.parse().ok()?);
        let action = parts.next()?;
        parts
            .next()
            .is_none()
            .then_some((message, attachment, action))
    }
}

/// The surface a host presents an animation's frames under, named by the
/// digest of its still first frame.
pub fn animation_surface(first_frame: &str) -> String {
    format!("animation:{first_frame}")
}

/// A preview's still first frame, or the surface the host plays it on,
/// fitted to the same box and rounded by the same radius as the still.
pub fn picture(
    image: &limn::RasterImage,
    animated: Option<&std::collections::HashSet<String>>,
) -> Element {
    if animated.is_some_and(|playing| playing.contains(image.sha256())) {
        Element::surface(animation_surface(image.sha256()), image.alt())
            .surface_fit(limn::SurfaceFit::Contain)
    } else {
        Element::raster_image(image.clone())
    }
}

/// Identification only: the core validates the downloaded header before parsing.
pub fn is_pdf(attachment: &rune_model::Attachment) -> bool {
    attachment.content_type.as_deref().is_some_and(|mime| {
        mime.split(';')
            .next()
            .unwrap_or_default()
            .trim()
            .eq_ignore_ascii_case("application/pdf")
    }) || attachment
        .filename
        .rsplit_once('.')
        .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("pdf"))
}

/// What «Copiar» puts on the clipboard for a file (contenido-de-mensajes).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Copied {
    /// Its picture's pixels.
    Image,
    /// The file itself, to paste where files go.
    Video,
}

/// What of `attachment` can be copied: a picture RUNE previews, or a video.
pub fn copyable(attachment: &rune_model::Attachment) -> Option<Copied> {
    if attachment.preview.is_some() {
        Some(Copied::Image)
    } else if super::media::is_video(attachment) {
        Some(Copied::Video)
    } else {
        None
    }
}

/// Whether the account's filter hides `attachment` now: it is explicit, the
/// filter applies to its row and it was not revealed yet.
fn obscured(attachment: &rune_model::Attachment, live: Live<'_>) -> bool {
    attachment.explicit
        && live.obscure_explicit
        && !live
            .revealed_files
            .is_some_and(|shown| shown.contains(&attachment.id))
}

/// The picture drawn for `preview`: blurred while `obscured`.
fn shown_source(preview: &rune_model::RemoteImage, obscured: bool) -> rune_model::RemoteImage {
    if obscured {
        preview.obscured()
    } else {
        preview.clone()
    }
}

/// Reserve the same bounded aspect box before and after the decode completes.
fn preview_size(dimensions: Option<(u32, u32)>, width: f32) -> Size {
    let available = if width.is_finite() {
        width.clamp(0.0, 360.0)
    } else {
        0.0
    };
    let (w, h) = dimensions
        .filter(|(w, h)| *w > 0 && *h > 0)
        .unwrap_or((360, 220));
    let scale = (available / w as f32).min(260.0 / h as f32).min(1.0);
    Size::new(w as f32 * scale, h as f32 * scale)
}

pub(super) fn file(
    attachment: &rune_model::Attachment,
    message: MessageId,
    width: f32,
    style: MessageStyle,
    live: Live<'_>,
    look: &Look,
) -> Element {
    let action = |verb| ids::action(message, attachment.id, verb);
    let width = if width.is_finite() {
        width.clamp(0.0, 360.0)
    } else {
        0.0
    };
    let downloadable = attachment.download.is_some() && !style.sample;
    let revealed = live
        .revealed_files
        .is_some_and(|shown| shown.contains(&attachment.id));
    // A spoiler shows nothing of itself, and asks for no pixels, until revealed.
    let hidden = attachment.spoiler && !revealed;
    // An explicit picture the account's filter hides shows blurred until
    // revealed; a video, nothing (contenido-de-mensajes).
    let obscured = obscured(attachment, live);
    let document = downloadable && is_pdf(attachment) && !hidden;
    let preview = attachment.preview.as_ref().filter(|_| style.image_previews);
    let body = if let Some(clip) = &attachment.voice {
        super::voice::player(
            attachment,
            clip,
            message,
            width,
            style.sample,
            live.voice,
            live.voice_left
                .iter()
                .find(|(id, _)| *id == attachment.id)
                .map(|&(_, left)| left),
            super::VOICE_SPEEDS[live.voice_speed % super::VOICE_SPEEDS.len()],
            look,
        )
    } else if hidden || obscured && preview.is_none() {
        let size = match preview {
            Some(_) => preview_size(attachment.dimensions, width),
            None => Size::new(width, 56.0),
        };
        // A spoiler the filter hides too says so: one click shows both,
        // and the person has read what it is.
        let label = Element::column(vec![
            crate::strong_line(
                if obscured {
                    t("CONTENIDO EXPLÍCITO")
                } else {
                    "SPOILER"
                },
                13.0,
                look.text,
            ),
            line(t("Hacé clic para mostrar"), 11.0, look.muted),
        ])
        .gap(2.0);
        let card = Element::panel(crate::centered(label))
            .size(Some(size.width), Some(size.height))
            .background(look.card)
            .border(look.border, 1.0)
            .radius(8.0);
        let card = Element::column(vec![
            card,
            line(&attachment.filename, 12.0, look.muted).size(Some(width), None),
        ])
        .gap(4.0)
        .size(Some(width), None);
        if style.sample {
            card
        } else {
            let name = &attachment.filename;
            Element::clickable(action("reveal"), card)
                .accessible(Access::new(Role::Button).name(if obscured {
                    tr!("Mostrar contenido explícito: {name}", name)
                } else {
                    tr!("Mostrar spoiler: {name}", name)
                }))
                .size(Some(width), None)
        }
    } else if let Some(kind) = super::media::playable(attachment) {
        super::media::player(
            attachment,
            kind,
            message,
            width,
            style.sample,
            live.voice,
            look,
        )
    } else if let Some(source) = preview {
        let size = preview_size(attachment.dimensions, width);
        let source = &shown_source(source, obscured);
        let image = live.attachment_images.and_then(|images| images.get(source));
        let failed = live
            .attachment_failures
            .is_some_and(|failed| failed.contains(source));
        let status = if failed {
            t("No se pudo cargar la imagen")
        } else {
            t("Cargando imagen…")
        };
        let fill = |image: &limn::RasterImage| {
            Element::raster_image(image.clone().cover()).size(Some(size.width), Some(size.height))
        };
        let picture = match image {
            // A blurred picture stands still and fills its box.
            Some(image) if obscured => fill(image),
            Some(image) => picture(image, live.animated),
            None => {
                let mark = Element::panel(crate::centered(
                    Element::icon(if failed {
                        Icon::WarningTriangle
                    } else {
                        Icon::Image
                    })
                    .foreground(look.muted),
                ));
                // Its ThumbHash stands in for it until it is here.
                match attachment
                    .placeholder
                    .as_ref()
                    .and_then(|hash| live.attachment_images?.get(hash))
                {
                    Some(placeholder) => Element::stack(vec![fill(placeholder), mark]),
                    None => mark,
                }
            }
        }
        .size(Some(size.width), Some(size.height))
        .background(look.card)
        .radius(8.0)
        .clip_children();
        let picture = if obscured {
            let label = super::embeds::explicit_label(look);
            Element::stack(vec![picture, crate::centered(label)])
                .size(Some(size.width), Some(size.height))
        } else {
            picture
        };
        let picture = picture.accessible(Access::new(Role::Image).name(&attachment.filename));
        let caption = vec![
            line(&attachment.filename, 12.0, look.text).size(Some(width), None),
            line(file_size(attachment.size_bytes), 11.0, look.muted).size(Some(width), None),
            // A status arriving or clearing must not shift the measured row.
            line(if image.is_none() { status } else { "" }, 11.0, look.muted)
                .size(Some(width), Some(16.0)),
        ];
        let caption = Tooltip::new(
            Element::column(caption).gap(2.0),
            action("name"),
            &attachment.filename,
            look.palette(),
        )
        .render();
        let preview = Element::column(vec![picture, caption])
            .gap(4.0)
            .size(Some(width), None);
        let name = &attachment.filename;
        if style.sample {
            preview
        } else if obscured {
            Element::clickable(action("reveal"), preview)
                .accessible(
                    Access::new(Role::Button)
                        .name(tr!("Mostrar contenido explícito: {name}", name)),
                )
                .size(Some(width), None)
        } else {
            Element::clickable(action("preview"), preview)
                .accessible(Access::new(Role::Button).name(tr!("Ver imagen: {name}", name)))
                .size(Some(width), None)
        }
    } else {
        let description = if document {
            tr!("{size} · Ver PDF", size = file_size(attachment.size_bytes))
        } else if downloadable {
            tr!(
                "{size} · Abrir archivo",
                size = file_size(attachment.size_bytes)
            )
        } else {
            tr!(
                "{size} · Archivo sin vista previa",
                size = file_size(attachment.size_bytes)
            )
        };
        if width >= 80.0 {
            let card = Attachment::new(action("file"), &attachment.filename, look.palette())
                .description(description)
                .icon(Icon::File)
                .width(width);
            if downloadable {
                card.open_action(action(if document { "pdf" } else { "open" }))
            } else {
                card
            }
            .render()
        } else {
            // Attachment cards have an 80 px minimum. A very narrow timeline
            // still keeps the filename and action inside its available width.
            let label = line(&attachment.filename, 12.0, look.text).size(Some(width), Some(36.0));
            if downloadable {
                Element::clickable(action(if document { "pdf" } else { "open" }), label).accessible(
                    Access::new(Role::Button).name(format!(
                        "{}: {}",
                        if document {
                            t("Ver PDF")
                        } else {
                            t("Abrir archivo")
                        },
                        attachment.filename
                    )),
                )
            } else {
                label
            }
        }
    };
    let body = if downloadable {
        let mut actions = Vec::new();
        if document {
            actions.push(ContextMenuEntry::Item(
                ContextMenuItem::new(action("pdf"), t("Ver PDF")).icon(Icon::File),
            ));
        }
        // The picture or the video itself, as Serein copies them
        // (contenido-de-mensajes); not while it is hidden.
        let copied = copyable(attachment).filter(|_| !hidden && !obscured);
        actions.extend(copied.map(|copied| {
            let (verb, label) = match copied {
                Copied::Image => ("copy-image", t("Copiar imagen")),
                Copied::Video => ("copy-video", t("Copiar video")),
            };
            ContextMenuEntry::Item(ContextMenuItem::new(action(verb), label).icon(Icon::Copy))
        }));
        actions.extend([
            ContextMenuEntry::Item(
                ContextMenuItem::new(action("open"), t("Abrir en navegador")).icon(Icon::Globe),
            ),
            ContextMenuEntry::Item(
                ContextMenuItem::new(action("copy"), t("Copiar enlace")).icon(Icon::Copy),
            ),
            ContextMenuEntry::Item(
                ContextMenuItem::new(action("download"), t("Guardar en Descargas"))
                    .icon(Icon::ArrowDown),
            ),
            ContextMenuEntry::Item(
                ContextMenuItem::new(action("save-as"), t("Guardar como…")).icon(Icon::File),
            ),
        ]);
        ContextMenu::new(action("menu"), body, actions, look.menu_palette()).render()
    } else {
        body
    };
    Element::row(vec![body, Element::spacer().flex(1.0)])
}

#[cfg(test)]
#[path = "attachment_tests.rs"]
mod tests;
