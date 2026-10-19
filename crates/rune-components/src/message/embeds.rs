//! Cards under a message, the copy a forward carries, and polls.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use limn::{
    Access, Button, Color, Component, Element, FontFamily, Icon, Markdown, RasterImage, Role,
    TextSpan,
};
use rune_i18n::{t, tr};
use rune_model::{
    Attachment, ChannelId, Embed, EmbedField, EmbedImage, EmbedKind, Forwarded, Poll, RemoteImage,
    TextReference, Timestamp,
};

use crate::text::{self, Names, Spoilers};
use crate::{gap, icon, line, Look};

/// The widest a card gets, as the timeline of the reference client shows it.
const CARD_WIDTH: f32 = 520.0;
/// The box a card's large picture, or a picture alone, fits in.
pub(super) const PICTURE: (f32, f32) = (400.0, 300.0);
/// The box of the small picture beside a card's text.
pub(super) const THUMBNAIL: f32 = 80.0;
/// The widest a poll gets.
const POLL_WIDTH: f32 = 440.0;
/// Where a card's own spoilers are numbered among its message's, after its
/// parts' ones from [`super::first_spoiler`]: its text's from here, and its
/// parts' text's from [`PARTS_TEXT`].
const CARD_TEXT: usize = 500;
pub(super) const PARTS_TEXT: usize = 700;
/// The card itself, when its link or its text hides it or its pictures.
const CARD_SPOILER: usize = 999;
/// The gap between the pictures of a gallery.
const GALLERY_GAP: f32 = 4.0;

/// The revealed number of the card that starts at `first` of a message's
/// cards, or of the cards of the copy it forwards: what a card written
/// behind a spoiler, or its pictures, wait for.
pub fn card_spoiler(forwarded: bool, first: usize) -> usize {
    super::PARTS_SPOILER + super::first_spoiler(forwarded, Some(first)) + CARD_SPOILER
}

/// The action that opens the `picture`th picture of the cards that start
/// at `first`, of the message `base` names or of its forward, in the viewer.
pub fn card_picture(base: &str, forwarded: bool, first: usize, picture: usize) -> String {
    format!("{base}:card:{}:{first}:{picture}", u8::from(forwarded))
}

/// The cards' place in a message and how a reader reaches their pictures.
struct Place<'a> {
    embeds: &'a [Embed],
    first: usize,
    count: usize,
    forwarded: bool,
    /// Its pictures wait behind its spoiler.
    hidden: bool,
    /// They wait because the account's filter hides them: drawn blurred
    /// behind the cover (contenido-de-mensajes).
    explicit: bool,
    /// The number of its text's next spoiler.
    text: Cell<usize>,
}

impl Place<'_> {
    /// The action that opens its `picture`th picture in the viewer.
    fn view(&self, picture: usize, draw: &Draw<'_>) -> Option<String> {
        (!draw.passive).then(|| card_picture(&draw.base, self.forwarded, self.first, picture))
    }
}

/// What every card of a message is drawn with.
pub(super) struct Draw<'a> {
    pub scale: f32,
    /// A sample, as in the settings' preview: nothing opens.
    pub passive: bool,
    /// Pictures are shown; off, a picture alone reads as its link.
    pub pictures: bool,
    pub now: Timestamp,
    pub images: Option<&'a HashMap<RemoteImage, RasterImage>>,
    pub failures: Option<&'a HashSet<RemoteImage>>,
    pub names: Option<&'a Names>,
    pub look: &'a Look,
    /// The message's own controls start with this: `msg:{id}`.
    pub base: String,
    /// The message's revealed spoilers: an app's parts' ones from
    /// [`super::PARTS_SPOILER`].
    pub revealed: &'a [usize],
    /// Why the account may not join each voice channel its cards name; one
    /// missing is a channel RUNE does not know.
    pub voice: Option<&'a HashMap<ChannelId, Option<String>>>,
    /// First-frame digests the host plays: a card's animated picture or GIF.
    pub animated: Option<&'a HashSet<String>>,
    /// What the account may do with the parts being drawn, when they are
    /// the message's own and its app takes their use (P3).
    pub app: Option<super::components::AppParts<'a>>,
    /// The account's filter hides the explicit pictures of these cards
    /// until each card is revealed (contenido-de-mensajes).
    pub obscure_explicit: bool,
}

/// What the account may do with a poll, beyond reading it.
pub(super) struct Voting<'a> {
    /// The action IDs of its controls start with this.
    pub base: String,
    /// The account may vote on it.
    pub allowed: bool,
    /// Why it may not, when that is known.
    pub denied: Option<&'a str>,
    /// What a multiselect vote has picked so far, before «Votar».
    pub chosen: &'a [u32],
}

/// The action that opens `url`, through the timeline's link handler.
pub(super) fn opens(url: &str) -> String {
    format!("markdown_link:embed:0:{url}")
}

/// A message's cards, or a forward's (`forwarded`), one under another;
/// those that only add a picture to the card before them join its gallery.
/// `content` is what the message says: a card whose link it hides behind a
/// spoiler is hidden too.
pub(super) fn cards(
    embeds: &[Embed],
    forwarded: bool,
    content: &str,
    width: f32,
    draw: &Draw<'_>,
) -> Vec<Element> {
    rune_model::card_groups(embeds)
        .into_iter()
        .map(|(first, count)| {
            let embed = &embeds[first];
            let reveal = card_spoiler(forwarded, first);
            let shown = draw.revealed.contains(&reveal);
            if embed.spoiled(content) && !shown {
                let size = (width.min(CARD_WIDTH), 64.0);
                let action = format!("{}:spoiler:{reveal}", draw.base);
                // One click shows both: it says what the filter hides, and
                // nothing of the picture.
                return if obscures(embeds, first, count, draw) {
                    explicit(size, &action, None, draw)
                } else {
                    spoiler(size, &action, draw)
                };
            }
            let explicit = !shown && obscures(embeds, first, count, draw);
            let place = Place {
                embeds,
                first,
                count,
                forwarded,
                explicit,
                hidden: embed.hides_pictures(content) && !shown || explicit,
                text: Cell::new(
                    super::PARTS_SPOILER + super::first_spoiler(forwarded, Some(first)) + CARD_TEXT,
                ),
            };
            card(&place, width, draw)
        })
        .collect()
}

/// A box hiding what a spoiler covers, `size` large, until a click on it
/// reports `action`; nothing of what it hides is drawn or loaded.
pub(super) fn spoiler(size: (f32, f32), action: &str, draw: &Draw<'_>) -> Element {
    let look = draw.look;
    let label = Element::column(vec![
        crate::strong_line("SPOILER", 13.0, look.text),
        line(t("Hacé clic para mostrar"), 11.0, look.muted),
    ])
    .gap(2.0);
    let card = Element::panel(crate::centered(label))
        .size(Some(size.0), Some(size.1))
        .background(look.card)
        .border(look.border, 1.0)
        .radius(8.0);
    if draw.passive {
        card
    } else {
        Element::clickable(action, card)
            .size(Some(size.0), Some(size.1))
            .accessible(Access::new(Role::Button).name(t("Mostrar spoiler")))
    }
}

/// What covers a card's hidden pictures: its spoiler, or the explicit
/// content filter's cover (contenido-de-mensajes).
fn covered(place: &Place<'_>, size: (f32, f32), action: &str, draw: &Draw<'_>) -> Element {
    if place.explicit {
        explicit(size, action, place.embeds[place.first].picture(), draw)
    } else {
        spoiler(size, action, draw)
    }
}

/// Whether the account's filter hides the pictures of the cards from
/// `first`: one of them is explicit (contenido-de-mensajes).
pub(super) fn obscures(embeds: &[Embed], first: usize, count: usize, draw: &Draw<'_>) -> bool {
    draw.obscure_explicit && card_explicit(embeds, first, count)
}

/// Whether one of the `count` cards from `first` is explicit, so that the
/// account's filter may hide their pictures; the first one's picture is
/// then drawn blurred, as [`rune_model::ImageKind::Obscured`].
pub fn card_explicit(embeds: &[Embed], first: usize, count: usize) -> bool {
    embeds
        .get(first..first + count)
        .is_some_and(|cards| cards.iter().any(|card| card.explicit))
}

/// What an explicit picture shows over itself while the account's filter
/// hides it, revealed with a click.
pub(super) fn explicit_label(look: &Look) -> Element {
    Element::panel(
        Element::column(vec![
            crate::strong_line(t("CONTENIDO EXPLÍCITO"), 13.0, look.text),
            line(t("Hacé clic para mostrar"), 11.0, look.muted),
        ])
        .gap(2.0),
    )
    .padding(8.0)
    .radius(8.0)
    .background(Look::alpha(look.canvas, 200))
}

/// A box `size` large over a card's explicit pictures: `behind` blurred,
/// once here, under [`explicit_label`], until a click reports `action`.
pub(super) fn explicit(
    size: (f32, f32),
    action: &str,
    behind: Option<&EmbedImage>,
    draw: &Draw<'_>,
) -> Element {
    let look = draw.look;
    let blurred = behind.and_then(|image| draw.images?.get(&image.source.obscured()));
    let mut layers: Vec<Element> = blurred
        .map(|blurred| {
            Element::raster_image(blurred.clone().cover()).size(Some(size.0), Some(size.1))
        })
        .into_iter()
        .collect();
    layers.push(crate::centered(explicit_label(look)));
    let card = Element::stack(layers)
        .size(Some(size.0), Some(size.1))
        .background(look.card)
        .border(look.border, 1.0)
        .radius(8.0)
        .clip_children();
    if draw.passive {
        card
    } else {
        Element::clickable(action, card)
            .size(Some(size.0), Some(size.1))
            .accessible(Access::new(Role::Button).name(t("Mostrar contenido explícito")))
    }
}

/// One card: a stripe of its color, then site, author, title, text, fields,
/// pictures and footer, each only when it has one. A picture alone is drawn
/// without the card; several of one link, as a gallery.
fn card(place: &Place<'_>, width: f32, draw: &Draw<'_>) -> Element {
    let look = draw.look;
    let scale = draw.scale;
    let embed = &place.embeds[place.first];
    let first = super::first_spoiler(place.forwarded, Some(place.first));
    if embed.is_media() && draw.pictures {
        let image = embed.picture().expect("media has a picture");
        if place.hidden {
            let reveal = card_spoiler(place.forwarded, place.first);
            let size = fitted(image, width, PICTURE);
            return covered(
                place,
                size,
                &format!("{}:spoiler:{reveal}", draw.base),
                draw,
            );
        }
        return picture(
            image,
            width,
            PICTURE,
            place.view(0, draw),
            embed.video.as_ref(),
            draw,
        );
    }
    match embed.kind {
        EmbedKind::SystemNotice => return notice(embed, width, &place.text, draw),
        EmbedKind::PollResult => return poll_result(embed, width, None, draw),
        EmbedKind::VoiceChannel => return voice(embed, width, &place.text, draw),
        _ => {}
    }
    let outer = width.min(CARD_WIDTH);
    let thumbnail = embed
        .thumbnail
        .as_ref()
        .filter(|_| embed.kind == EmbedKind::Rich && draw.pictures);
    let inner = (outer - 4.0 - 28.0).max(40.0);
    let text_width = if thumbnail.is_some() {
        (inner - THUMBNAIL - 16.0).max(40.0)
    } else {
        inner
    };
    let mut parts = Vec::new();
    if let Some(provider) = &embed.provider {
        parts.push(line(provider, 12.0 * scale, look.muted));
    }
    if let Some(author) = &embed.author {
        parts.push(wrapped(
            author,
            14.0 * scale,
            FontFamily::SansMedium,
            look.text,
            text_width,
        ));
    }
    if let Some(title) = &embed.title {
        let ink = if embed.url.is_some() {
            look.palette().link()
        } else {
            look.text
        };
        let mut span = TextSpan::new(title, ink, 16.0 * scale, FontFamily::SansMedium);
        if let Some(url) = embed.url.as_ref().filter(|_| !draw.passive) {
            span = span.action(opens(url));
        }
        parts.push(Element::formatted_text(vec![span]).size(Some(text_width), None));
    }
    if let Some(description) = &embed.description {
        parts.push(markdown(
            description,
            &embed.text_references,
            text_width,
            scale * 0.93,
            &place.text,
            draw,
        ));
    }
    if !embed.fields.is_empty() {
        parts.push(fields(&embed.fields, embed, text_width, &place.text, draw));
    }
    if !embed.components.is_empty() {
        parts.push(super::components::parts(
            &embed.components,
            &embed.text_references,
            first,
            text_width,
            draw,
            &|_, _| None,
        ));
    }
    if embed.kind == EmbedKind::Other {
        parts.push(wrapped(
            t("RUNE todavía no muestra todo lo que trae este tipo de tarjeta"),
            12.0 * scale,
            FontFamily::Sans,
            look.faint,
            text_width,
        ));
    }
    let mut body = Element::column(parts).gap(8.0);
    // Its pictures as the viewer counts them: the gallery's, then this one.
    let viewable = rune_model::card_pictures(place.embeds, place.first, place.count).len();
    if let Some(thumbnail) = thumbnail.filter(|_| !place.hidden) {
        let view = place.view(viewable.saturating_sub(1), draw);
        body = Element::row(vec![
            body.flex(1.0),
            picture(
                thumbnail,
                THUMBNAIL,
                (THUMBNAIL, THUMBNAIL),
                view,
                None,
                draw,
            ),
        ])
        .gap(16.0);
    }
    let mut parts = vec![body];
    let large: Vec<&EmbedImage> = place.embeds[place.first..place.first + place.count]
        .iter()
        .filter_map(|card| card.image.as_ref())
        .collect();
    let reveal = format!(
        "{}:spoiler:{}",
        draw.base,
        card_spoiler(place.forwarded, place.first)
    );
    // A video shows its poster large, with a mark to play it on its page.
    let poster = embed
        .picture()
        .filter(|_| embed.kind == EmbedKind::Video && draw.pictures);
    if let Some(poster) = poster {
        parts.push(if place.hidden {
            covered(place, fitted(poster, inner, PICTURE), &reveal, draw)
        } else {
            poster_of(poster, inner, embed.url.as_deref(), draw)
        });
    } else if !draw.pictures || embed.kind == EmbedKind::Video {
        // Nothing large to show.
    } else if place.hidden && (!large.is_empty() || thumbnail.is_some()) {
        let size = large.first().map_or((inner.min(PICTURE.0), 96.0), |image| {
            fitted(image, inner, PICTURE)
        });
        parts.push(covered(place, size, &reveal, draw));
    } else if large.len() > 1 {
        parts.push(gallery(&large, inner, place, draw));
    } else if let Some(image) = large.first() {
        parts.push(picture(
            image,
            inner,
            PICTURE,
            place.view(0, draw),
            None,
            draw,
        ));
    }
    let footer: Vec<_> = embed
        .footer
        .iter()
        .cloned()
        .chain(embed.timestamp.map(|at| crate::time::stamp(at, draw.now)))
        .collect();
    if !footer.is_empty() {
        parts.push(wrapped(
            &footer.join(" • "),
            12.0 * scale,
            FontFamily::Sans,
            look.muted,
            inner,
        ));
    }
    let stripe = embed.color.map_or(look.border, |rgb| {
        Color::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
    });
    Element::row(vec![
        Element::panel(Element::spacer())
            .size(Some(4.0), None)
            .background(stripe),
        Element::panel(Element::column(parts).gap(8.0))
            .padding(12.0)
            .flex(1.0),
    ])
    .background(look.card)
    .border(look.border, 1.0)
    .radius(4.0)
    .size(Some(outer), None)
    .accessible(
        Access::new(Role::Group).name(embed.title.clone().unwrap_or_else(|| t("Tarjeta").into())),
    )
}

/// A notice from the service about the account, as its own client shows it:
/// a mark and the header, the body, the date, and «Más información» when it
/// links to the page that explains it.
fn notice(embed: &Embed, width: f32, spoilers: &Cell<usize>, draw: &Draw<'_>) -> Element {
    let look = draw.look;
    let scale = draw.scale;
    let outer = width.min(CARD_WIDTH);
    let inner = (outer - 32.0).max(40.0);
    let header = embed.title.as_deref().unwrap_or(t("Aviso sobre tu cuenta"));
    let mut parts = vec![Element::row(vec![
        icon(Icon::InfoCircle, look.accent).centered_y(),
        wrapped(
            header,
            16.0 * scale,
            FontFamily::SansMedium,
            look.text,
            inner - 28.0,
        )
        .centered_y(),
    ])
    .gap(8.0)];
    if let Some(body) = &embed.description {
        parts.push(markdown(
            body,
            &embed.text_references,
            inner,
            scale * 0.93,
            spoilers,
            draw,
        ));
    }
    if let Some(at) = embed.timestamp {
        parts.push(line(
            crate::time::stamp(at, draw.now),
            12.0 * scale,
            look.muted,
        ));
    }
    if let Some(url) = embed.url.as_ref().filter(|_| !draw.passive) {
        parts.push(Element::row(vec![
            Button::new(opens(url), t("Más información"), look.palette()).render(),
            Element::spacer().flex(1.0),
        ]));
    }
    Element::panel(Element::column(parts).gap(8.0))
        .padding(16.0)
        .background(look.card)
        .border(look.border, 1.0)
        .radius(8.0)
        .size(Some(outer), None)
        .accessible(Access::new(Role::Group).name(tr!("Aviso: {header}", header)))
}

/// The result of a closed poll, as the reference client draws it under the
/// line that says so: the winner's emoji, the winning answer and its share,
/// or a draw, or no votes; and «Ver encuesta» when `open` leads to the poll.
pub(super) fn poll_result(
    embed: &Embed,
    width: f32,
    open: Option<String>,
    draw: &Draw<'_>,
) -> Element {
    let look = draw.look;
    let scale = draw.scale;
    let outer = width.min(POLL_WIDTH);
    let count = |name| {
        embed
            .field(name)
            .and_then(|value| value.trim().parse::<u64>().ok())
    };
    let total = count("total_votes");
    let share = total
        .filter(|total| *total > 0)
        .map(|total| (count("victor_answer_votes").unwrap_or(0) * 100 + total / 2) / total)
        .unwrap_or(0);
    let winner = embed
        .field("victor_answer_id")
        .map(|_| embed.field("victor_answer_text").unwrap_or(t("Sin texto")));
    let (title, detail) = match (total, winner) {
        (Some(0), _) => (t("No hubo un ganador").to_owned(), None),
        (_, Some(answer)) => (
            answer.to_owned(),
            Some(tr!("Respuesta ganadora • {share} %", share)),
        ),
        (Some(_), None) => (
            t("El resultado fue un empate").to_owned(),
            Some(tr!("{share} %", share)),
        ),
        (None, None) => (t("La encuesta terminó").to_owned(), None),
    };
    // A server's own emoji shows its picture; its name stands in until then.
    let emoji = winner
        .and(embed.field("victor_answer_emoji_name"))
        .map(|name| match embed.field("victor_answer_emoji_id") {
            Some(_) => format!(":{name}:"),
            None => name.to_owned(),
        });
    let picture = winner
        .and(embed.thumbnail.as_ref())
        .and_then(|image| draw.images?.get(&image.source));
    let mark = match (picture, emoji) {
        (Some(picture), name) => Element::raster_image(picture.clone())
            .size(Some(24.0 * scale), Some(24.0 * scale))
            .accessible(Access::new(Role::Image).name(name.unwrap_or_default())),
        (None, Some(emoji)) => Element::text(emoji)
            .font_family(FontFamily::Sans)
            .font_size(if winner.is_some() { 22.0 } else { 14.0 } * scale)
            .foreground(look.text),
        (None, None) => icon(
            if winner.is_some() {
                Icon::CheckCircle
            } else {
                Icon::ChartBar
            },
            look.accent,
        ),
    };
    let narrow = outer < 320.0;
    let button_width = if open.is_some() && !narrow {
        132.0
    } else {
        0.0
    };
    let text_width = (outer - 24.0 - 40.0 - 12.0 - button_width).max(48.0);
    let mut about = vec![wrapped(
        &title,
        15.0 * scale,
        FontFamily::SansMedium,
        look.text,
        text_width,
    )];
    if let Some(detail) = detail {
        about.push(line(detail, 12.0 * scale, look.muted));
    }
    let mut row = vec![
        Element::panel(crate::centered(mark))
            .size(Some(40.0), Some(40.0))
            .radius(8.0)
            .background(look.canvas)
            .centered_y(),
        Element::column(about).gap(2.0).flex(1.0).centered_y(),
    ];
    let button = open
        .filter(|_| !draw.passive)
        .map(|action| Button::new(action, t("Ver encuesta"), look.palette()).render());
    let mut parts = Vec::new();
    match button {
        Some(button) if narrow => {
            parts.push(Element::row(row).gap(12.0));
            parts.push(Element::row(vec![button, Element::spacer().flex(1.0)]));
        }
        Some(button) => {
            row.push(button.centered_y());
            parts.push(Element::row(row).gap(12.0));
        }
        None => parts.push(Element::row(row).gap(12.0)),
    }
    Element::panel(Element::column(parts).gap(8.0))
        .padding(12.0)
        .background(look.card)
        .border(look.border, 1.0)
        .radius(8.0)
        .size(Some(outer), None)
        .accessible(Access::new(Role::Group).name(tr!("Resultado de la encuesta: {title}", title)))
}

/// Why «Unirse» of a voice card cannot be used, when it cannot.
fn voice_denied<'a>(embed: &Embed, draw: &'a Draw<'_>) -> Option<&'a str> {
    let Some(channel) = embed.channel else {
        return Some(t("RUNE no sabe a qué canal de voz lleva esta tarjeta"));
    };
    match draw.voice.and_then(|known| known.get(&channel)) {
        Some(reason) => reason.as_deref(),
        None => Some(t("RUNE no conoce ese canal de voz")),
    }
}

/// A voice channel to join: a speaker, its title and text, and «Unirse»,
/// disabled with the reason when the account may not join it from here.
fn voice(embed: &Embed, width: f32, spoilers: &Cell<usize>, draw: &Draw<'_>) -> Element {
    let look = draw.look;
    let scale = draw.scale;
    let outer = width.min(POLL_WIDTH);
    let inner = (outer - 24.0 - 40.0 - 12.0).max(48.0);
    let title = embed.title.as_deref().unwrap_or(t("Canal de voz"));
    let mut about = vec![
        line(
            embed.author.as_deref().unwrap_or(t("Canal de voz")),
            12.0 * scale,
            look.muted,
        ),
        wrapped(
            title,
            15.0 * scale,
            FontFamily::SansMedium,
            look.text,
            inner,
        ),
    ];
    if let Some(description) = &embed.description {
        about.push(markdown(
            description,
            &embed.text_references,
            inner,
            scale * 0.93,
            spoilers,
            draw,
        ));
    }
    let mut parts = vec![Element::row(vec![
        Element::panel(crate::centered(icon(Icon::Volume, look.online)))
            .size(Some(40.0), Some(40.0))
            .radius(20.0)
            .background(look.canvas),
        Element::column(about).gap(2.0).flex(1.0),
    ])
    .gap(12.0)];
    if !draw.passive {
        let denied = voice_denied(embed, draw);
        let action = embed.channel.map_or_else(
            || format!("{}:voice", draw.base),
            |channel| format!("{}:voice:{channel}", draw.base),
        );
        let mut access = Access::new(Role::Button)
            .name(t("Unirse"))
            .disabled(denied.is_some());
        if let Some(reason) = denied {
            access = access.description(reason);
        }
        parts.push(Element::row(vec![
            Button::new(action, t("Unirse"), look.accent_palette())
                .enabled(denied.is_none())
                .render()
                .accessible(access),
            Element::spacer().flex(1.0),
        ]));
        if let Some(reason) = denied {
            parts.push(wrapped(
                reason,
                12.0 * scale,
                FontFamily::Sans,
                look.faint,
                outer - 24.0,
            ));
        }
    }
    Element::panel(Element::column(parts).gap(10.0))
        .padding(12.0)
        .background(look.card)
        .border(look.border, 1.0)
        .radius(8.0)
        .size(Some(outer), None)
        .accessible(Access::new(Role::Group).name(tr!("Canal de voz: {title}", title)))
}

/// The size `image` is shown at in the box `bounds`, at most `width` wide,
/// keeping its proportions.
fn fitted(image: &EmbedImage, width: f32, bounds: (f32, f32)) -> (f32, f32) {
    let (w, h) = (image.dimensions.0 as f32, image.dimensions.1 as f32);
    let fit = (width.min(bounds.0) / w).min(bounds.1 / h).min(1.0);
    ((w * fit).max(1.0), (h * fit).max(1.0))
}

/// `image` once loaded, played when the host plays it, or what stands in
/// for it while it loads or after it failed. A GIF's `clip`, once loaded,
/// takes the place of its still.
fn shown(image: &EmbedImage, clip: Option<&EmbedImage>, draw: &Draw<'_>) -> (Element, bool) {
    let loaded = |image: &EmbedImage| draw.images.and_then(|images| images.get(&image.source));
    let failed = draw
        .failures
        .is_some_and(|failed| failed.contains(&image.source));
    match clip.and_then(loaded).or_else(|| loaded(image)) {
        Some(raster) => (super::attachments::picture(raster, draw.animated), false),
        None => (
            Element::panel(crate::centered(
                Element::icon(if failed {
                    Icon::WarningTriangle
                } else {
                    Icon::Image
                })
                .foreground(draw.look.muted),
            )),
            failed,
        ),
    }
}

/// A card's picture in the box `bounds` and at most `width` wide, keeping
/// its proportions; its room is reserved before it loads. A click reports
/// `view`, which opens it in the viewer.
pub(super) fn picture(
    image: &EmbedImage,
    width: f32,
    bounds: (f32, f32),
    view: Option<String>,
    clip: Option<&EmbedImage>,
    draw: &Draw<'_>,
) -> Element {
    let (w, h) = fitted(image, width, bounds);
    let (element, failed) = shown(image, clip, draw);
    let shown = element
        .size(Some(w), Some(h))
        .background(draw.look.card)
        .radius(4.0)
        .clip_children()
        .accessible(Access::new(Role::Image).name(if failed {
            t("No se pudo cargar la imagen")
        } else {
            t("Imagen de la tarjeta")
        }));
    match view {
        Some(action) => Element::clickable(action, shown)
            .size(Some(w), Some(h))
            .accessible(Access::new(Role::Button).name(t("Ver imagen"))),
        None => shown,
    }
}

/// A video's poster with a mark to play it on its page, which a click opens.
fn poster_of(poster: &EmbedImage, width: f32, link: Option<&str>, draw: &Draw<'_>) -> Element {
    let look = draw.look;
    let (w, h) = fitted(poster, width, PICTURE);
    let mark = Element::panel(icon(Icon::PlayFill, look.text).icon_size(24.0))
        .padding(12.0)
        .radius(24.0)
        .background(Look::alpha(look.canvas, 200));
    let shown = Element::stack(vec![
        picture(poster, width, PICTURE, None, None, draw),
        crate::centered(mark),
    ])
    .size(Some(w), Some(h));
    match link.filter(|_| !draw.passive) {
        Some(url) => Element::clickable(opens(url), shown)
            .size(Some(w), Some(h))
            .accessible(Access::new(Role::Link).name(t("Reproducir en su página"))),
        None => shown,
    }
}

/// Two to four pictures of one link, as the reference client lays them:
/// two side by side, three with a tall one on the left, four in a square;
/// each fills its cell, cropped around its middle, and opens the viewer.
fn gallery(pictures: &[&EmbedImage], width: f32, place: &Place<'_>, draw: &Draw<'_>) -> Element {
    let side = ((width.min(PICTURE.0) - GALLERY_GAP) / 2.0)
        .floor()
        .max(1.0);
    let cell = |index: usize, size: (f32, f32)| {
        let image = pictures[index];
        let (element, _) = shown(image, None, draw);
        // A loaded picture fills its cell; what stands in for it is centered.
        let element = match draw.images.and_then(|images| images.get(&image.source)) {
            Some(raster) if !draw.animated.is_some_and(|on| on.contains(raster.sha256())) => {
                Element::raster_image(raster.clone().cover())
            }
            Some(raster) => {
                Element::surface(super::animation_surface(raster.sha256()), raster.alt())
                    .surface_fit(limn::SurfaceFit::Cover)
            }
            None => element,
        }
        .size(Some(size.0), Some(size.1))
        .background(draw.look.card)
        .radius(4.0)
        .clip_children()
        .accessible(Access::new(Role::Image).name(t("Imagen de la tarjeta")));
        match place.view(index, draw) {
            Some(action) => Element::clickable(action, element)
                .size(Some(size.0), Some(size.1))
                .accessible(Access::new(Role::Button).name(tr!(
                    "Ver imagen {n} de {total}",
                    n = index + 1,
                    total = pictures.len()
                ))),
            None => element,
        }
    };
    let square = (side, side);
    let column = |cells: Vec<Element>| Element::column(cells).gap(GALLERY_GAP);
    let row = |cells: Vec<Element>| Element::row(cells).gap(GALLERY_GAP);
    match pictures.len() {
        2 => row(vec![cell(0, square), cell(1, square)]),
        3 => row(vec![
            cell(0, (side, side * 2.0 + GALLERY_GAP)),
            column(vec![cell(1, square), cell(2, square)]),
        ]),
        _ => column(vec![
            row(vec![cell(0, square), cell(1, square)]),
            row(vec![cell(2, square), cell(3, square)]),
        ]),
    }
    .accessible(
        Access::new(Role::Group).name(tr!("Galería de {count} imágenes", count = pictures.len())),
    )
}

/// Inline fields sit up to three to a row, fewer where columns would be
/// narrower than a word; any other takes a row of its own.
fn fields(
    fields: &[EmbedField],
    embed: &Embed,
    width: f32,
    spoilers: &Cell<usize>,
    draw: &Draw<'_>,
) -> Element {
    let scale = draw.scale;
    let per_row = ((width + 8.0) / (128.0 * scale)).floor().clamp(1.0, 3.0) as usize;
    let mut rows: Vec<Vec<&EmbedField>> = Vec::new();
    for field in fields {
        match rows.last_mut() {
            Some(row) if field.inline && row.len() < per_row && row.iter().all(|f| f.inline) => {
                row.push(field)
            }
            _ => rows.push(vec![field]),
        }
    }
    Element::column(
        rows.into_iter()
            .map(|row| {
                let each = (width - 8.0 * (row.len() - 1) as f32) / row.len() as f32;
                Element::row(
                    row.into_iter()
                        .map(|field| {
                            Element::column(vec![
                                wrapped(
                                    &field.name,
                                    13.0 * scale,
                                    FontFamily::SansMedium,
                                    draw.look.text,
                                    each,
                                ),
                                markdown(
                                    &field.value,
                                    &embed.text_references,
                                    each,
                                    scale * 0.93,
                                    spoilers,
                                    draw,
                                ),
                            ])
                            .gap(2.0)
                            .size(Some(each), None)
                        })
                        .collect(),
                )
                .gap(8.0)
            })
            .collect(),
    )
    .gap(8.0)
}

/// A forward: a rule down its left, «Reenviado», and the copy as it was.
/// Its files are drawn by `file`, as the message's own are.
pub(super) fn forwarded(
    copy: &Forwarded,
    width: f32,
    draw: &Draw<'_>,
    file: &dyn Fn(&Attachment, f32) -> Element,
) -> Element {
    let look = draw.look;
    let scale = draw.scale;
    let inner = (width - 14.0).max(40.0);
    let mut parts = vec![Element::row(vec![
        icon(Icon::Reply, look.muted).icon_size(14.0).centered_y(),
        Element::text(t("Reenviado"))
            .font_family(FontFamily::Sans)
            .font_size(13.0 * scale)
            .foreground(look.muted)
            .centered_y(),
    ])
    .gap(4.0)];
    if !copy.content.is_empty() {
        // Its spoilers are numbered after the copy's parts' ones.
        let spoilers =
            Cell::new(super::PARTS_SPOILER + super::first_spoiler(true, None) + CARD_TEXT);
        parts.push(markdown(
            &copy.content,
            &copy.text_references,
            inner,
            scale,
            &spoilers,
            draw,
        ));
    }
    // A copy made of parts shows its files where they place them.
    for attachment in copy.attachments.iter().filter(|_| !copy.components.whole) {
        parts.push(file(attachment, inner));
    }
    parts.extend(cards(&copy.embeds, true, &copy.content, inner, draw));
    if !copy.components.is_empty() {
        let named = |name: &str, width: f32| {
            let found = copy
                .attachments
                .iter()
                .find(|found| found.filename == name)?;
            Some(file(found, width))
        };
        parts.push(super::components::parts(
            &copy.components.parts,
            &copy.components.text_references,
            super::components::first_spoiler(true, None),
            inner,
            draw,
            &named,
        ));
    }
    let mut when = crate::time::stamp(copy.timestamp, draw.now);
    if copy.edited.is_some() {
        when.push(' ');
        when.push_str(t("(editado)"));
    }
    parts.push(line(when, 12.0 * scale, look.faint));
    Element::row(vec![
        Element::panel(Element::spacer())
            .size(Some(4.0), None)
            .background(look.border)
            .radius(2.0),
        gap(10.0, 0.0),
        Element::column(parts).gap(6.0).flex(1.0),
    ])
    .size(Some(width), None)
    .accessible(Access::new(Role::Group).name(t("Mensaje reenviado")))
}

/// A poll: its question, each answer with its share of the votes once they
/// are known, and how long it stays open. While the account may still vote,
/// the answers are choices and the results wait for its vote, as in the
/// reference client; a single choice votes on its click.
pub(super) fn poll(poll: &Poll, width: f32, draw: &Draw<'_>, voting: &Voting<'_>) -> Element {
    let look = draw.look;
    let scale = draw.scale;
    let outer = width.min(POLL_WIDTH);
    let inner = outer - 32.0;
    let ended = poll.ended(draw.now);
    let total = poll.votes();
    let voted = poll.voted();
    let open = !ended && voting.allowed && !draw.passive;
    let results = !open || voted;
    let mut parts = vec![wrapped(
        &poll.question,
        17.0 * scale,
        FontFamily::SansMedium,
        look.text,
        inner,
    )];
    if !ended {
        parts.push(line(
            if poll.multiselect {
                t("Elegí una o más respuestas")
            } else {
                t("Elegí una respuesta")
            },
            12.0 * scale,
            look.muted,
        ));
    }
    let leader = poll
        .answers
        .iter()
        .map(|answer| answer.votes)
        .max()
        .unwrap_or(0);
    for answer in &poll.answers {
        let mut label = answer.text.clone();
        if let Some(emoji) = &answer.emoji {
            label = format!("{emoji} {label}");
        }
        if !results {
            let picked = voting.chosen.contains(&answer.id);
            let (mark, edge) = if picked {
                (icon(Icon::CheckCircle, look.accent), look.accent)
            } else {
                (icon(Icon::Circle, look.muted), look.border)
            };
            let row = Element::row(vec![
                mark.centered_y(),
                gap(10.0, 0.0),
                wrapped(
                    label.trim(),
                    15.0 * scale,
                    FontFamily::Sans,
                    look.text,
                    (inner - 64.0).max(48.0),
                )
                .centered_y(),
                Element::spacer().flex(1.0),
            ])
            .padding(10.0);
            let role = if poll.multiselect {
                Role::CheckBox
            } else {
                Role::RadioButton
            };
            parts.push(
                Element::clickable(format!("{}:answer:{}", voting.base, answer.id), row)
                    .background(look.canvas)
                    .hover_background(look.selected)
                    .border(edge, 1.0)
                    .radius(8.0)
                    .size(Some(inner), None)
                    .accessible(Access::new(role).name(label.trim()).toggled(picked)),
            );
            continue;
        }
        // The answer keeps what the count and the check leave; it wraps there.
        // In a narrow poll the count goes under the answer instead of beside it.
        let stacked = inner < 320.0 * scale;
        let count_width = if poll.results_known && !stacked {
            112.0 * scale
        } else {
            0.0
        };
        let check_width = if answer.me { 36.0 } else { 0.0 };
        let text_width = (inner - 20.0 - count_width - check_width).max(48.0);
        let count = poll.results_known.then(|| {
            let share = (u64::from(answer.votes) * 100 + total / 2)
                .checked_div(total)
                .unwrap_or(0);
            line(
                tr!(
                    "{votes} · {share} %",
                    votes = votes(u64::from(answer.votes)),
                    share
                ),
                13.0 * scale,
                look.muted,
            )
        });
        let text = wrapped(
            label.trim(),
            15.0 * scale,
            FontFamily::Sans,
            look.text,
            text_width,
        );
        let (text, count) = match count {
            Some(count) if stacked => (
                Element::column(vec![text, count.size(Some(text_width), None)]).gap(2.0),
                None,
            ),
            count => (text, count),
        };
        let mut row = vec![text.centered_y(), Element::spacer().flex(1.0)];
        if answer.me {
            row.push(icon(Icon::CheckCircle, look.accent).centered_y());
            row.push(gap(8.0, 0.0));
        }
        if let Some(count) = count {
            row.push(count.size(Some(count_width - 8.0), None).centered_y());
        }
        let row = Element::row(row).padding(10.0);
        let fill = if poll.results_known && total > 0 {
            let winner = ended && answer.votes == leader && leader > 0;
            let ink = if answer.me || winner {
                Look::alpha(look.accent, 70)
            } else {
                Look::alpha(look.text, 24)
            };
            let share = answer.votes as f32 / total as f32;
            Element::row(vec![
                Element::panel(Element::spacer())
                    .size(Some(inner * share), None)
                    .background(ink),
                Element::spacer().flex(1.0),
            ])
        } else {
            Element::spacer()
        };
        parts.push(
            Element::stack(vec![fill, row])
                .background(look.canvas)
                .border(if answer.me { look.accent } else { look.border }, 1.0)
                .radius(8.0)
                .clip_children()
                .size(Some(inner), None),
        );
    }
    let mut footer = Vec::new();
    if poll.results_known {
        footer.push(votes(total));
    } else {
        footer.push(t("Resultados todavía no recibidos").into());
    }
    footer.push(match poll.expiry {
        _ if ended => t("La encuesta terminó").into(),
        Some(expiry) => remaining(expiry.0 - draw.now.0),
        None => t("Sin cierre").into(),
    });
    parts.push(wrapped(
        &footer.join(" · "),
        12.0 * scale,
        FontFamily::Sans,
        look.muted,
        inner,
    ));
    let button = if open && voted {
        Some(Button::new(
            format!("{}:remove", voting.base),
            t("Quitar voto"),
            look.palette(),
        ))
    } else if open && poll.multiselect {
        Some(
            Button::new(
                format!("{}:vote", voting.base),
                t("Votar"),
                look.accent_palette(),
            )
            .enabled(!voting.chosen.is_empty()),
        )
    } else {
        None
    };
    if let Some(button) = button {
        parts.push(Element::row(vec![
            button.render(),
            Element::spacer().flex(1.0),
        ]));
    }
    if let Some(reason) = voting
        .denied
        .filter(|_| !ended && !voting.allowed && !draw.passive)
    {
        parts.push(wrapped(
            reason,
            12.0 * scale,
            FontFamily::Sans,
            look.faint,
            inner,
        ));
    }
    Element::panel(Element::column(parts).gap(8.0))
        .padding(16.0)
        .background(look.card)
        .border(look.border, 1.0)
        .radius(8.0)
        .size(Some(outer), None)
        .accessible(
            Access::new(Role::Group).name(tr!("Encuesta: {question}", question = poll.question)),
        )
}

fn votes(count: u64) -> String {
    if count == 1 {
        t("1 voto").into()
    } else {
        tr!("{count} votos", count)
    }
}

/// "Quedan 3 d", "Quedan 5 h", "Quedan 12 min".
fn remaining(millis: i64) -> String {
    let minutes = (millis / 60_000).max(1);
    if minutes >= 48 * 60 {
        tr!("Quedan {days} d", days = minutes / (24 * 60))
    } else if minutes >= 60 {
        tr!("Quedan {hours} h", hours = minutes / 60)
    } else {
        tr!("Quedan {minutes} min", minutes)
    }
}

/// A card's or a part's text. Its spoilers stay hidden until each is
/// revealed, numbered from `spoilers`, which moves past them.
// shortcut: a text past a few hundred spoilers numbers the rest as the next
// block's; only which of them reveal together changes.
pub(super) fn markdown(
    source: &str,
    references: &[TextReference],
    width: f32,
    scale: f32,
    spoilers: &Cell<usize>,
    draw: &Draw<'_>,
) -> Element {
    let mut element = Markdown::new(source, draw.look.palette())
        .width(width)
        .font_size(super::MARKDOWN_BODY * scale)
        .render()
        .element;
    if let Some(names) = draw.names {
        // Mentions in a card are names to read; they never notify anyone.
        let next = text::decorate(
            &mut element,
            names,
            &[],
            references,
            false,
            &Spoilers {
                revealed: draw.revealed,
                action: format!("{}:spoiler:", draw.base),
                first: spoilers.get(),
            },
            text::Pictures {
                loaded: draw.images,
                playing: draw.animated,
                jumbo: false,
                emoji_action: None,
            },
            draw.look,
        );
        spoilers.set(next);
    }
    super::written_as(element, draw.passive)
}

pub(super) fn wrapped(text: &str, size: f32, face: FontFamily, ink: Color, width: f32) -> Element {
    Element::text(text)
        .font_family(face)
        .font_size(size)
        .foreground(ink)
        .size(Some(width), None)
}

#[cfg(test)]
mod tests {
    use super::remaining;

    #[test]
    fn time_left_reads_in_its_largest_unit() {
        assert_eq!(remaining(30_000), "Quedan 1 min");
        assert_eq!(remaining(59 * 60_000), "Quedan 59 min");
        assert_eq!(remaining(5 * 3_600_000), "Quedan 5 h");
        assert_eq!(remaining(72 * 3_600_000), "Quedan 3 d");
    }
}
