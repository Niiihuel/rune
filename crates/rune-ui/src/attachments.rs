//! The received-image viewer resolves current entities instead of retaining URLs.

use crate::{editor_id, Intent, Ui, ViewContext};
use limn::{Component, Element, ImageViewer, Key, UiAction, UiEvent};
use rune_components::{
    message::{attachment_ids, Copied},
    Look,
};
use rune_i18n::t;
use rune_model::{
    Attachment, AttachmentId, ChannelId, ImageKind, Message, MessageId, RemoteFile, RemoteImage,
    UserId,
};
use rune_state::Store;

pub(crate) const CLOSE: &str = "attachment-viewer:close";
const ZOOM_IN: &str = "attachment-viewer:zoom-in";
const ZOOM_OUT: &str = "attachment-viewer:zoom-out";
const FIT: &str = "attachment-viewer:fit";
const PREVIOUS: &str = "attachment-viewer:previous";
const NEXT: &str = "attachment-viewer:next";
pub(crate) const ORIGINAL: &str = "attachment-viewer:original";
/// A preview is decoded to fit this box; a larger picture has more to show.
const PREVIEW_BOX: (u32, u32) = (768, 512);

/// What the viewer shows: a file of a message (or of the copy it forwards),
/// or a picture of its cards.
#[derive(Clone, Debug, PartialEq)]
pub enum Viewed {
    File(AttachmentId),
    /// The `picture`th picture of the cards that start at `first`, of the
    /// message's own cards or of its forward's.
    Card {
        forwarded: bool,
        first: usize,
        picture: usize,
    },
}

/// Account-local presentation state; payloads stay in the bounded image cache.
#[derive(Clone, Debug, PartialEq)]
pub struct AttachmentViewer {
    pub account: Option<UserId>,
    pub channel: ChannelId,
    pub message: MessageId,
    pub viewed: Viewed,
    pub zoom: f32,
    /// The account asked for the picture at its own size, not its preview's.
    pub original: bool,
}

/// What the open viewer shows, resolved from the current message.
pub(crate) struct Shown<'a> {
    pub title: String,
    pub preview: &'a RemoteImage,
    /// The picture at its own size, when it is larger than its preview.
    pub original: Option<RemoteImage>,
    /// The file to save, for a file.
    pub file: Option<(&'a Attachment, &'a RemoteFile)>,
    /// Its place among the pictures of its cards.
    pub index: usize,
    pub total: usize,
}

fn message(store: &Store, channel: ChannelId, message: MessageId) -> Option<&Message> {
    store
        .channel(channel)
        .filter(|channel| Store::visible(channel))?;
    store.timeline(channel)?.get(message).map(|found| &**found)
}

pub(crate) fn attachment(
    store: &Store,
    channel: ChannelId,
    message_id: MessageId,
    id: AttachmentId,
) -> Option<&Attachment> {
    message(store, channel, message_id)?
        .files()
        .find(|attachment| attachment.id == id)
}

/// A spoiler the account has not revealed: nothing of it is requested.
pub(crate) fn hidden(ui: &Ui, attachment: &Attachment) -> bool {
    attachment.spoiler && !ui.messages.revealed_files.contains(&attachment.id)
}

/// An explicit picture the account has not revealed: blurred where the
/// account's filter hides it (contenido-de-mensajes).
pub(crate) fn unrevealed(ui: &Ui, attachment: &Attachment) -> bool {
    attachment.explicit && !ui.messages.revealed_files.contains(&attachment.id)
}

/// Whether the account's filter hides `attachment` of `found` now.
fn obscured(ui: &Ui, store: &Store, found: &Message, attachment: &Attachment) -> bool {
    unrevealed(ui, attachment) && store.obscures_explicit(found.channel, found.author)
}

/// The same picture at its own size, when it is larger than its preview.
fn original(source: &RemoteImage, dimensions: Option<(u32, u32)>) -> Option<RemoteImage> {
    dimensions
        .filter(|(w, h)| *w > PREVIEW_BOX.0 || *h > PREVIEW_BOX.1)
        .map(|_| RemoteImage {
            url: source.url.clone(),
            kind: ImageKind::Original,
        })
}

/// The cards of `found` the viewer may show the pictures of: the message's
/// own, or the copy's it forwards, when their pictures are not hidden.
fn card_pictures<'a>(
    ui: &Ui,
    store: &Store,
    found: &'a Message,
    forwarded: bool,
    first: usize,
) -> Option<Vec<&'a rune_model::EmbedImage>> {
    let (embeds, content) = if forwarded {
        let copy = found.forwarded.as_ref()?;
        (&copy.embeds, &copy.content)
    } else {
        (&found.embeds, &found.content)
    };
    let (_, count) = rune_model::card_groups(embeds)
        .into_iter()
        .find(|(start, _)| *start == first)?;
    let revealed = ui.messages.revealed.get(&found.id).is_some_and(|shown| {
        shown.contains(&rune_components::message::card_spoiler(forwarded, first))
    });
    // Explicit pictures the account's filter hides open once revealed.
    let explicit = rune_components::message::card_explicit(embeds, first, count)
        && store.obscures_explicit(found.channel, found.author);
    (revealed || !embeds[first].hides_pictures(content) && !explicit)
        .then(|| rune_model::card_pictures(embeds, first, count))
}

pub(crate) fn shown<'a>(ui: &Ui, store: &'a Store) -> Option<Shown<'a>> {
    let viewer = ui.attachment_viewer.as_ref()?;
    (ui.channel() == Some(viewer.channel) && viewer.account == store.me()).then_some(())?;
    match viewer.viewed {
        Viewed::File(id) => {
            let message = message(store, viewer.channel, viewer.message)?;
            let found = attachment(store, viewer.channel, viewer.message, id)
                .filter(|found| !hidden(ui, found) && !obscured(ui, store, message, found))?;
            let preview = found.preview.as_ref()?;
            Some(Shown {
                title: found.filename.clone(),
                preview,
                original: original(preview, found.dimensions),
                file: found.download.as_ref().map(|file| (found, file)),
                index: 0,
                total: 1,
            })
        }
        Viewed::Card {
            forwarded,
            first,
            picture,
        } => {
            let found = message(store, viewer.channel, viewer.message)?;
            let pictures = card_pictures(ui, store, found, forwarded, first)?;
            let image = pictures.get(picture)?;
            let embeds = if forwarded {
                &found.forwarded.as_ref()?.embeds
            } else {
                &found.embeds
            };
            Some(Shown {
                title: embeds[first]
                    .title
                    .clone()
                    .unwrap_or_else(|| t("Imagen de la tarjeta").into()),
                preview: &image.source,
                original: original(&image.source, Some(image.dimensions)),
                file: None,
                index: picture,
                total: pictures.len(),
            })
        }
    }
}

/// The source the viewer wants loaded: its preview, and the picture at its
/// own size once the account asked for it.
pub(crate) fn viewer_sources(ui: &Ui, store: &Store) -> Vec<RemoteImage> {
    let Some(shown) = shown(ui, store) else {
        return Vec::new();
    };
    let mut wanted = vec![shown.preview.clone()];
    if ui
        .attachment_viewer
        .as_ref()
        .is_some_and(|viewer| viewer.original)
    {
        wanted.extend(shown.original);
    }
    wanted
}

/// Deletion, a permission update or navigation cannot leave stale account media open.
pub(crate) fn refresh(ui: &mut Ui, store: &Store) {
    if ui.attachment_viewer.is_some() && shown(ui, store).is_none() {
        ui.attachment_viewer = None;
    }
}

/// The control that opened what the viewer shows: focus goes back to it.
fn opener(viewer: &AttachmentViewer) -> String {
    match viewer.viewed {
        Viewed::File(id) => attachment_ids::action(viewer.message, id, "preview"),
        Viewed::Card {
            forwarded,
            first,
            picture,
        } => rune_components::message::card_picture(
            &format!("msg:{}", viewer.message),
            forwarded,
            first,
            picture,
        ),
    }
}

fn close(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let still = shown(ui, store).is_some();
    let Some(viewer) = ui.attachment_viewer.take() else {
        return Vec::new();
    };
    if ui.channel() == Some(viewer.channel) && ui.prefs.image_previews && still {
        vec![Intent::Focus(opener(&viewer))]
    } else {
        ui.channel()
            .map(|channel| vec![Intent::Focus(editor_id(channel))])
            .unwrap_or_default()
    }
}

/// Open the viewer on `viewed`, closing a document's; returns what that asks.
fn open(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    message: MessageId,
    viewed: Viewed,
) -> Vec<Intent> {
    let mut intents = crate::documents::close(ui);
    ui.attachment_viewer = Some(AttachmentViewer {
        account: store.me(),
        channel,
        message,
        viewed,
        zoom: 1.0,
        original: false,
    });
    intents.push(Intent::Focus(CLOSE.into()));
    intents
}

/// `card:{forwarded}:{first}:{picture}` of a message: open that picture of
/// its cards, unless it is hidden or previews are off.
pub(crate) fn open_card(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    id: MessageId,
    what: &str,
) -> Vec<Intent> {
    let mut parts = what.split(':');
    let mut number = || parts.next()?.parse::<usize>().ok();
    let (Some(forwarded), Some(first), Some(picture)) = (number(), number(), number()) else {
        return Vec::new();
    };
    let opened = ui.prefs.image_previews
        && message(store, channel, id)
            .and_then(|found| card_pictures(ui, store, found, forwarded == 1, first))
            .is_some_and(|pictures| picture < pictures.len());
    if !opened {
        return Vec::new();
    }
    open(
        ui,
        store,
        channel,
        id,
        Viewed::Card {
            forwarded: forwarded == 1,
            first,
            picture,
        },
    )
}

/// Show the picture before or after, among its cards' pictures.
fn step(ui: &mut Ui, store: &Store, by: isize) {
    let total = shown(ui, store).map_or(1, |shown| shown.total);
    if let Some(AttachmentViewer {
        viewed: Viewed::Card { picture, .. },
        zoom,
        original,
        ..
    }) = ui.attachment_viewer.as_mut()
    {
        if let Some(next) = picture.checked_add_signed(by).filter(|next| *next < total) {
            *picture = next;
            *zoom = 1.0;
            *original = false;
        }
    }
}

pub(crate) fn update(ui: &mut Ui, store: &Store, action: &UiAction) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    match id.as_str() {
        CLOSE => return Some(close(ui, store)),
        ZOOM_IN | ZOOM_OUT | FIT => {
            if let Some(viewer) = ui.attachment_viewer.as_mut() {
                viewer.zoom = match id.as_str() {
                    ZOOM_IN => (viewer.zoom * 1.5).min(ImageViewer::MAX_ZOOM),
                    ZOOM_OUT => (viewer.zoom / 1.5).max(1.0),
                    _ => 1.0,
                };
            }
            return Some(Vec::new());
        }
        PREVIOUS | NEXT => {
            step(ui, store, if id == PREVIOUS { -1 } else { 1 });
            return Some(Vec::new());
        }
        ORIGINAL => {
            if let Some(viewer) = ui.attachment_viewer.as_mut() {
                viewer.original = !viewer.original;
            }
            return Some(Vec::new());
        }
        _ => {}
    }
    let (message, id, verb) = attachment_ids::parse(id)?;
    let Some(channel) = ui.channel() else {
        return Some(Vec::new());
    };
    let Some(found) = attachment(store, channel, message, id) else {
        return Some(Vec::new());
    };
    if verb == "reveal" {
        // A spoiler, or an explicit picture (contenido-de-mensajes).
        if found.spoiler || found.explicit {
            ui.messages.revealed_files.insert(id);
        }
        return Some(Vec::new());
    }
    if verb == "preview" {
        let shown = self::message(store, channel, message)
            .is_some_and(|row| !hidden(ui, found) && !obscured(ui, store, row, found));
        if found.preview.is_some() && ui.prefs.image_previews && shown {
            return Some(open(ui, store, channel, message, Viewed::File(id)));
        }
        return Some(Vec::new());
    }
    let Some(source) = &found.download else {
        return Some(Vec::new());
    };
    if verb == "save-as" {
        return Some(crate::save_as::start(ui, store, channel, message, found));
    }
    // The picture or the video itself, not while it is hidden
    // (contenido-de-mensajes).
    let copied = match verb {
        "copy-image" => Some(Copied::Image),
        "copy-video" => Some(Copied::Video),
        _ => None,
    };
    if let Some(copied) = copied {
        let allowed = rune_components::message::copyable(found) == Some(copied)
            && !hidden(ui, found)
            && self::message(store, channel, message)
                .is_some_and(|row| !obscured(ui, store, row, found));
        return Some(
            allowed
                .then(|| Intent::CopyFile {
                    source: source.clone(),
                    filename: found.filename.clone(),
                    size_bytes: found.size_bytes,
                    image: copied == Copied::Image,
                })
                .into_iter()
                .collect(),
        );
    }
    Some(match verb {
        "open" => vec![Intent::OpenUrl(source.url.clone())],
        "copy" => vec![Intent::Copy(source.url.clone())],
        "download" => vec![Intent::DownloadFile {
            source: source.clone(),
            filename: found.filename.clone(),
            size_bytes: found.size_bytes,
        }],
        _ => Vec::new(),
    })
}

pub(crate) fn intercept(ui: &mut Ui, store: &Store, event: &UiEvent) -> Option<Vec<Intent>> {
    ui.attachment_viewer.as_ref()?;
    let UiEvent::Key { key, .. } = event else {
        return None;
    };
    match key {
        Key::Escape => return Some(close(ui, store)),
        Key::ArrowLeft | Key::ArrowRight => {
            step(ui, store, if *key == Key::ArrowLeft { -1 } else { 1 });
            return Some(Vec::new());
        }
        _ => {}
    }
    // Let the modal's focus trap and activation work. Conversation shortcuts
    // must not edit or switch the workspace behind the picture.
    if matches!(key, Key::Tab | Key::Enter | Key::Space) {
        None
    } else {
        Some(Vec::new())
    }
}

pub(crate) fn overlay(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let Some(viewer) = &ui.attachment_viewer else {
        return root;
    };
    let Some(found) = shown(ui, store) else {
        return root;
    };
    let mut image = ImageViewer::new(root, true, &found.title, CLOSE, look.menu_palette())
        .viewport(context.window)
        .return_focus(opener(viewer))
        .zoom(viewer.zoom)
        .zoom_actions(ZOOM_OUT, FIT, ZOOM_IN)
        .gallery(found.index, found.total)
        .previous_action(PREVIOUS)
        .next_action(NEXT);
    // The same action as the attachment's menu: it resolves the entity again.
    if let Some((file, _)) = found.file {
        image = image.action(
            attachment_ids::action(viewer.message, file.id, "save-as"),
            limn::Icon::ArrowDown,
            t("Guardar como…"),
        );
    }
    let preview = ui.images.get(found.preview).cloned();
    // An animation plays as the timeline plays it; its own size is its
    // preview's, so it is not offered.
    let playing = preview
        .as_ref()
        .filter(|ready| ui.animated.contains(ready.sha256()));
    if playing.is_none() && found.original.is_some() {
        image = image.action(
            ORIGINAL,
            if viewer.original {
                limn::Icon::Collapse
            } else {
                limn::Icon::Expand
            },
            if viewer.original {
                t("Ver la vista previa")
            } else {
                t("Ver en resolución original")
            },
        );
    }
    // The picture at its own size replaces its preview once it is here.
    let large = found
        .original
        .as_ref()
        .filter(|_| viewer.original)
        .and_then(|source| ui.images.get(source).cloned());
    if let Some(ready) = large.or(preview.clone()) {
        // Played by the host as it plays the preview in the timeline.
        if let Some(ready) = playing {
            image = image.surface(
                rune_components::message::animation_surface(ready.sha256()),
                ready.alt(),
            );
        }
        image = image.image(ready);
    } else if ui.image_failures.contains(found.preview) {
        image = image.error(t("No se pudo cargar la imagen"));
    }
    image.render()
}
