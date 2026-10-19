//! Presence and visual identity (P5), from plain props: the cards of what
//! someone is doing, where they are connected from, their badges and
//! connected accounts, and the marks a person carries beside their name
//! and over their face (a role's picture or emoji, a server tag, a
//! decoration, a nameplate behind their row). Pictures arrive decoded;
//! service icons are RUNE's own glyphs, never copied artwork.

use limn::{
    Button, ButtonVariant, Color, Component, Element, FontFamily, Icon, Point, RasterImage,
};

use rune_i18n::{t, tr};

use crate::{gap, icon, line, strong_line, text, Look};

/// The actions these pieces report.
pub mod ids {
    /// A badge with a link: the prefix and its place.
    pub const BADGE: &str = "pp:badge=";
    /// A connected account with a page: the prefix and its place.
    pub const CONNECTION: &str = "pp:connection=";
}

/// A role's mark beside a name: its picture, or its emoji.
#[derive(Clone, Debug, PartialEq)]
pub enum RoleMark {
    /// `None` while its picture loads.
    Image {
        role: String,
        image: Option<RasterImage>,
    },
    Emoji {
        role: String,
        emoji: String,
    },
}

/// A server's tag beside a name: its badge and a few letters.
#[derive(Clone, Debug, PartialEq)]
pub struct TagChip {
    pub text: String,
    pub badge: Option<RasterImage>,
    /// Whose tag it is, for its hint: the server's name when known.
    pub server: Option<String>,
}

/// The artwork behind someone's row, on its colors.
#[derive(Clone, Debug, PartialEq)]
pub struct Plate {
    pub image: Option<RasterImage>,
    /// From the left to the right; `None` draws the artwork alone.
    pub colors: Option<(Color, Color)>,
    /// The moving artwork, once loaded: in place of the still while the
    /// pointer is over the row, or for good while it is `chosen`.
    pub animated: Option<RasterImage>,
    pub chosen: bool,
}

/// What a person carries beside their name and over their face.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    /// A frame over their picture, a fifth larger than it.
    pub decoration: Option<RasterImage>,
    /// More colors after the name's ink, for its gradient: one or two.
    pub gradient: Vec<Color>,
    pub role: Option<RoleMark>,
    pub tag: Option<TagChip>,
    /// In lists: the artwork behind their row.
    pub plate: Option<Plate>,
    /// In lists: they are only on a phone, drawn instead of their dot.
    pub mobile: bool,
}

/// Where someone is connected from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Platform {
    Desktop,
    Mobile,
    Web,
}

impl Platform {
    fn label(self) -> &'static str {
        match self {
            Self::Desktop => t("Escritorio"),
            Self::Mobile => t("Móvil"),
            Self::Web => "Web",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Desktop => Icon::Laptop,
            Self::Mobile => Icon::Phone,
            Self::Web => Icon::Globe,
        }
    }
}

/// What kind of thing someone is doing, for its header and glyph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActivityKind {
    Playing,
    Listening,
    Watching,
    Streaming,
    Competing,
}

impl ActivityKind {
    fn header(self) -> &'static str {
        match self {
            Self::Playing => t("Jugando"),
            Self::Listening => t("Escuchando"),
            Self::Watching => t("Viendo"),
            Self::Streaming => t("Transmitiendo"),
            Self::Competing => t("Compitiendo"),
        }
    }

    fn icon(self) -> Icon {
        match self {
            Self::Playing => Icon::Gamepad,
            Self::Listening => Icon::MusicNote,
            Self::Watching => Icon::Screen,
            Self::Streaming => Icon::Video,
            Self::Competing => Icon::Target,
        }
    }
}

/// How long it has been going, or how far a track is.
#[derive(Clone, Debug, PartialEq)]
pub enum ActivityTime {
    /// "47:12 transcurridos".
    Elapsed(String),
    /// "2:15 restantes".
    Remaining(String),
    /// A track: how far, its length, and the share played from 0 to 1.
    Track {
        at: String,
        length: String,
        progress: f32,
    },
}

/// One activity as its card shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct ActivityCard {
    pub kind: ActivityKind,
    pub name: String,
    pub detail: Option<String>,
    pub state: Option<String>,
    pub image: Option<RasterImage>,
    pub small_image: Option<RasterImage>,
    pub time: Option<ActivityTime>,
}

/// A badge on a profile.
#[derive(Clone, Debug, PartialEq)]
pub struct BadgeChip {
    pub description: String,
    pub image: Option<RasterImage>,
    /// It leads somewhere: clicking it reports [`ids::BADGE`] and its place.
    pub link: bool,
}

/// A connected account in the full profile.
#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionRow {
    /// "github": picks the glyph.
    pub service: String,
    /// "GitHub".
    pub label: String,
    pub name: String,
    pub verified: bool,
    /// It has a page: clicking reports [`ids::CONNECTION`] and its place.
    pub link: bool,
}

/// What a profile's card shows beyond who someone is (P5).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CardIdentity {
    pub marks: Marks,
    pub activities: Vec<ActivityCard>,
    pub platforms: Vec<Platform>,
    pub badges: Vec<BadgeChip>,
    /// One per role of the card's `roles`, in order: its mark, if any.
    pub role_marks: Vec<Option<RoleMark>>,
    /// The effect drawn over the card.
    pub effect: Option<CardEffect>,
    /// The pictures its texts name, drawn in place of their tokens.
    pub pictures: Vec<TextPicture>,
}

/// A picture a text names by `token` (a server's emoji in «Sobre mí»):
/// drawn in its place once loaded, `:name:` until then; `playing` while
/// the host plays its animation.
#[derive(Clone, Debug, PartialEq)]
pub struct TextPicture {
    pub token: String,
    pub name: String,
    pub image: Option<RasterImage>,
    pub playing: bool,
}

/// `picture` alone, `side` px: on the surface the host plays it on while it
/// moves, `:name:` until it loads.
pub fn picture(picture: &TextPicture, side: f32, look: &Look) -> Element {
    let name = format!(":{}:", picture.name);
    match &picture.image {
        Some(image) if picture.playing => {
            Element::surface(crate::message::animation_surface(image.sha256()), name)
                .surface_fit(limn::SurfaceFit::Contain)
                .size(Some(side), Some(side))
        }
        Some(image) => Element::raster_image(image.clone())
            .size(Some(side), Some(side))
            .accessible(limn::Access::default().description(name)),
        None => text(name, 13.0, look.text),
    }
}

/// What a list's row says under a name: `words` in one line cut with «…»,
/// `size` px in `ink`, after a server's `emoji` 14 px and 4 px apart, as
/// the official client's lists draw a custom status
/// (`--custom-activity-status-emoji-size` 14 px beside small text);
/// `None` with neither (2026-10-18: the emoji was left out).
pub fn status_line(
    words: Option<&str>,
    emoji: Option<&TextPicture>,
    size: f32,
    ink: Color,
    look: &Look,
) -> Option<Element> {
    let words = words
        .filter(|words| !words.is_empty())
        .map(|words| line(words, size, ink));
    let Some(emoji) = emoji else {
        return words;
    };
    let mut parts = vec![picture(emoji, 14.0, look).centered_y()];
    parts.extend(words.map(|words| words.flex(1.0).centered_y()));
    Some(Element::row(parts).gap(4.0))
}

/// `value` as Markdown, `width` across, with the pictures it names drawn in
/// its lines in place of their tokens, as the official client draws a
/// bio's emoji (1.375 times its letters) (2026-10-17: a line that named one
/// flowed as words and pictures, without Markdown).
pub fn with_pictures(value: &str, pictures: &[TextPicture], width: f32, look: &Look) -> Element {
    // Its lines keep their breaks, as written.
    let source = value.replace('\n', "  \n");
    let mut element = limn::Markdown::new(&source, look.palette())
        .width(width)
        .render()
        .element;
    crate::text::with_pictures(&mut element, pictures);
    element
}

/// An effect over a card (P5): the pictures showing now, each with where
/// its top left goes on the card, in the card's pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct CardEffect {
    pub layers: Vec<(RasterImage, Point)>,
}

/// How opaque a card's effect stays under the pointer: the official
/// popout's 35 % (REA, 2026-10-18).
pub const EFFECT_UNDER_POINTER: f32 = 0.35;

/// `card`, `width` across, with `effect` over it: clipped to the card, it
/// takes no room and no clicks. Each picture is as wide as the card and as
/// tall as its shape makes it, as the official client draws them
/// (2026-10-17: fitted inside a 450×880 canvas they came out narrower than
/// the card and cut short). With `gives_way`, the effect dims to
/// [`EFFECT_UNDER_POINTER`] while the pointer is over the card, so that
/// what it covers reads, as the official popout dims it.
pub fn with_effect(
    card: Element,
    width: f32,
    effect: Option<&CardEffect>,
    gives_way: Option<&str>,
) -> Element {
    let Some(effect) = effect.filter(|effect| !effect.layers.is_empty()) else {
        return card;
    };
    let layers = effect
        .layers
        .iter()
        .map(|(image, at)| {
            let (across, down) = png_size(image).unwrap_or((450, 880));
            let height = width * down as f32 / across as f32;
            Element::column(vec![Element::row(vec![Element::raster_image(
                image.clone(),
            )
            .size(Some(width), Some(height))])])
            .translate(*at)
        })
        .collect();
    let layers = Element::stack(layers);
    match gives_way {
        Some(id) => Element::overlay(card, layers.dim_on_hover(id, EFFECT_UNDER_POINTER))
            .clip_children()
            .hover_region(),
        None => Element::overlay(card, layers).clip_children(),
    }
}

/// The size a PNG says in its header, read from the start of its base64:
/// pictures arrive normalized to PNG, but a picture does not carry its size.
fn png_size(image: &RasterImage) -> Option<(u32, u32)> {
    let digit = |c: u8| match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    let mut bytes = Vec::with_capacity(24);
    for group in image.data_base64().as_bytes().get(..32)?.chunks(4) {
        let n = group
            .iter()
            .try_fold(0u32, |n, c| Some(n << 6 | u32::from(digit(*c)?)))?;
        bytes.extend_from_slice(&n.to_be_bytes()[1..]);
    }
    let number =
        |at: usize| u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]]);
    (bytes.get(12..16)? == b"IHDR")
        .then(|| (number(16), number(20)))
        .filter(|(across, down)| *across > 0 && *down > 0)
}

/// A face made by `face` for its side, `size` across with `decoration`
/// over it. A decoration is drawn a fifth larger than the face, so the
/// face shrinks inside the same room and nothing around it moves.
pub fn decorated(
    face: impl FnOnce(f32) -> Element,
    size: f32,
    decoration: Option<RasterImage>,
) -> Element {
    let Some(decoration) = decoration else {
        return face(size);
    };
    let inner = size / 1.2;
    Element::stack(vec![
        crate::centered(face(inner)),
        Element::raster_image(decoration).size(Some(size), Some(size)),
    ])
    .size(Some(size), Some(size))
}

/// A role's mark, `size` across, with the role's name as its hint.
pub fn role_mark(mark: &RoleMark, size: f32, look: &Look) -> Element {
    let (role, glyph) = match mark {
        RoleMark::Image { role, image } => (
            role,
            match image {
                Some(image) => Element::raster_image(image.clone()).size(Some(size), Some(size)),
                None => Element::spacer().size(Some(size), Some(size)),
            },
        ),
        RoleMark::Emoji { role, emoji } => (
            role,
            Element::text(emoji.clone())
                .font_family(FontFamily::Sans)
                .font_size(size * 0.85)
                .foreground(look.text),
        ),
    };
    glyph.accessible(limn::Access::default().description(tr!("Rol: {role}", role)))
}

/// A server's tag: its badge and letters on a small raised chip.
pub fn tag_chip(tag: &TagChip, size: f32, look: &Look) -> Element {
    let mut parts = Vec::new();
    if let Some(badge) = &tag.badge {
        parts.push(
            Element::raster_image(badge.clone())
                .size(Some(size), Some(size))
                .centered_y(),
        );
    }
    parts.push(
        Element::text(tag.text.clone())
            .font_family(FontFamily::SansBold)
            .font_size(size * 0.85)
            .foreground(look.text)
            .centered_y(),
    );
    let hint = match &tag.server {
        Some(server) => tr!("Etiqueta de servidor de {server}", server),
        None => t("Etiqueta de servidor").to_owned(),
    };
    Element::panel(Element::row(parts).gap(3.0))
        .padding(2.0)
        .radius(4.0)
        .background(look.raised)
        .border(look.border, 1.0)
        .accessible(limn::Access::default().description(format!("{hint}: {}", tag.text)))
}

/// `name`, written in `ink`, in the gradient of its role when it has one:
/// from `ink` through each color of `gradient`.
pub fn inked(name: Element, ink: Color, gradient: &[Color]) -> Element {
    if gradient.is_empty() {
        return name;
    }
    let stops: Vec<Color> = std::iter::once(ink)
        .chain(gradient.iter().copied())
        .collect();
    name.foreground_gradient(stops)
}

/// What follows a name: the role's mark and the server tag, in that order.
pub fn after_name(marks: &Marks, size: f32, look: &Look) -> Vec<Element> {
    let mut parts = Vec::new();
    if let Some(mark) = &marks.role {
        parts.push(role_mark(mark, size, look).centered_y());
    }
    if let Some(tag) = &marks.tag {
        parts.push(tag_chip(tag, size * 0.8, look).centered_y());
    }
    parts
}

/// `row`, `height` high, on the artwork of `plate`: its colors fading in
/// from the left and the picture against the right edge.
pub fn on_plate(row: Element, height: f32, plate: Option<&Plate>, id: &str) -> Element {
    let Some(plate) = plate else {
        return row;
    };
    let artwork = |image: &RasterImage| {
        Element::raster_image(image.clone().cover()).size(Some(height * 16.0 / 3.0), Some(height))
    };
    // The moving one takes the still's place under the pointer (2026-10-18).
    let art = match (&plate.image, &plate.animated) {
        (_, Some(moving)) if plate.chosen => artwork(moving),
        (Some(still), Some(moving)) => Element::stack(vec![
            artwork(still).conceal_on_hover(format!("{id}:plate")),
            artwork(moving).reveal_on_hover(format!("{id}:plate-moving")),
        ]),
        (still, moving) => still
            .as_ref()
            .or(moving.as_ref())
            .map_or_else(Element::spacer, artwork),
    };
    let mut ground = Element::row(vec![Element::spacer().flex(1.0), art])
        .size(None, Some(height))
        .radius(crate::members::RADIUS)
        .clip_children();
    if let Some((from, to)) = plate.colors {
        ground = ground.horizontal_gradient(vec![Look::alpha(from, 0), Look::alpha(to, 0x4D)]);
    }
    Element::stack(vec![ground, row])
        .size(None, Some(height))
        .hover_region()
}

/// The card of one activity: its kind, picture, lines and time.
pub fn activity_card(card: &ActivityCard, look: &Look) -> Element {
    let header = Element::row(vec![
        icon(card.kind.icon(), look.muted)
            .size(Some(14.0), Some(14.0))
            .centered_y(),
        line(card.kind.header(), 12.0, look.muted)
            .font_family(FontFamily::SansBold)
            .centered_y(),
    ])
    .gap(6.0);
    // The official client's sizes: a 60 px picture and, over its corner
    // and 4 px past it, a 24 px round one cut out of the card's ground.
    const ART: f32 = 60.0;
    const SMALL: f32 = 24.0;
    let art = match &card.image {
        Some(image) => Element::raster_image(image.clone().cover())
            .size(Some(ART), Some(ART))
            .radius(6.0),
        // Seen against the card: its own ground is darker than the card's.
        None => Element::panel(crate::centered(
            icon(card.kind.icon(), look.muted).size(Some(28.0), Some(28.0)),
        ))
        .size(Some(ART), Some(ART))
        .radius(6.0)
        .background(look.menu),
    };
    let art = match &card.small_image {
        Some(small) => Element::stack(vec![
            art,
            Element::column(vec![
                Element::spacer().flex(1.0),
                Element::row(vec![
                    Element::spacer().flex(1.0),
                    Element::panel(
                        Element::raster_image(small.clone().cover())
                            .size(Some(SMALL - 4.0), Some(SMALL - 4.0))
                            .radius(SMALL / 2.0 - 2.0),
                    )
                    .padding(2.0)
                    .radius(SMALL / 2.0)
                    .background(look.raised)
                    .translate(Point::new(4.0, 4.0)),
                ]),
            ]),
        ])
        .size(Some(ART), Some(ART)),
        None => art,
    };
    // Semibold, as the official card's name.
    let mut lines =
        vec![strong_line(card.name.clone(), 14.0, look.text).font_family(FontFamily::SansSemibold)];
    for (value, ink) in [(&card.detail, look.text), (&card.state, look.muted)] {
        if let Some(value) = value {
            lines.push(line(value.clone(), 13.0, ink));
        }
    }
    match &card.time {
        Some(ActivityTime::Elapsed(label) | ActivityTime::Remaining(label)) => {
            lines.push(line(label.clone(), 12.0, look.online));
        }
        Some(ActivityTime::Track { .. }) | None => {}
    }
    let mut body = vec![
        header,
        Element::row(vec![
            art.centered_y(),
            Element::column(lines).gap(2.0).flex(1.0).centered_y(),
        ])
        .gap(12.0),
    ];
    if let Some(ActivityTime::Track {
        at,
        length,
        progress,
    }) = &card.time
    {
        body.push(track_bar(at, length, *progress, look));
    }
    Element::panel(Element::column(body).gap(8.0))
        .padding(10.0)
        .radius(8.0)
        .background(look.raised)
        .accessible(limn::Access::new(limn::Role::Group).name(format!(
            "{} {}",
            card.kind.header(),
            card.name
        )))
}

/// How far a track is: the time, a bar and its length.
fn track_bar(at: &str, length: &str, progress: f32, look: &Look) -> Element {
    let progress = progress.clamp(0.0, 1.0);
    let bar = Element::row(vec![
        Element::panel(Element::spacer())
            .size(None, Some(4.0))
            .radius(2.0)
            .background(look.text)
            .flex(progress.max(0.001)),
        Element::panel(Element::spacer())
            .size(None, Some(4.0))
            .radius(2.0)
            .background(look.border)
            .flex((1.0 - progress).max(0.001)),
    ]);
    Element::row(vec![
        line(at.to_owned(), 11.0, look.muted).centered_y(),
        bar.flex(1.0).centered_y(),
        line(length.to_owned(), 11.0, look.muted).centered_y(),
    ])
    .gap(8.0)
    .accessible(
        limn::Access::new(limn::Role::ProgressIndicator)
            .name(tr!("{at} de {length}", at, length))
            .value(progress),
    )
}

/// Where someone is connected from, as glyphs with their names.
pub fn platforms(places: &[Platform], look: &Look) -> Element {
    let mut parts = Vec::new();
    for place in places {
        parts.push(
            icon(place.icon(), look.muted)
                .size(Some(14.0), Some(14.0))
                .centered_y(),
        );
        parts.push(line(place.label(), 12.0, look.muted).centered_y());
        parts.push(gap(4.0, 0.0));
    }
    Element::row(parts)
        .gap(4.0)
        .accessible(limn::Access::default().description(tr!(
            "Conectado desde: {places}",
            places = places
                .iter()
                .map(|place| place.label())
                .collect::<Vec<_>>()
                .join(", ")
        )))
}

/// Badges as small pictures, or their names while they have none, each
/// with what it means as its hint.
pub fn badges(badges: &[BadgeChip], look: &Look) -> Element {
    let items = badges
        .iter()
        .enumerate()
        .map(|(index, badge)| {
            let face = match &badge.image {
                Some(image) => Element::raster_image(image.clone()).size(Some(22.0), Some(22.0)),
                None => Element::panel(line(badge.description.clone(), 11.0, look.text))
                    .padding(3.0)
                    .radius(4.0)
                    .background(look.raised),
            };
            let id = format!("{}{index}", ids::BADGE);
            let face = if badge.link {
                Element::clickable(id.clone(), face)
                    .radius(4.0)
                    .hover_background(look.hover)
                    .accessible(limn::Access::new(limn::Role::Link).name(badge.description.clone()))
            } else {
                face.accessible(limn::Access::default().description(badge.description.clone()))
            };
            limn::Tooltip::new(face, id, badge.description.clone(), look.menu_palette()).render()
        })
        .collect();
    Element::row(items).gap(4.0).wrap()
}

/// The glyph of a connected service: RUNE's own or a neutral one.
fn service_glyph(service: &str, look: &Look) -> Element {
    match service {
        "github" => icon(Icon::Github, look.text),
        "domain" => icon(Icon::Globe, look.text),
        "spotify" => icon(Icon::MusicNote, look.text),
        "steam" | "xbox" | "playstation" | "battlenet" | "epicgames" | "leagueoflegends"
        | "riotgames" | "roblox" => icon(Icon::Gamepad, look.text),
        _ => icon(Icon::ExternalLink, look.muted),
    }
    .size(Some(20.0), Some(20.0))
}

/// The accounts elsewhere someone connected, each opening its page.
pub fn connections(rows: &[ConnectionRow], look: &Look) -> Element {
    if rows.is_empty() {
        return text(t("No conectó cuentas."), 14.0, look.muted);
    }
    Element::column(
        rows.iter()
            .enumerate()
            .map(|(index, row)| {
                let mut parts = vec![
                    service_glyph(&row.service, look).centered_y(),
                    Element::column(vec![
                        line(row.label.clone(), 12.0, look.muted),
                        line(row.name.clone(), 14.0, look.text),
                    ])
                    .flex(1.0)
                    .centered_y(),
                ];
                if row.verified {
                    parts.push(
                        icon(Icon::CheckCircle, look.online)
                            .size(Some(16.0), Some(16.0))
                            .accessible(
                                limn::Access::default()
                                    .description(t("Cuenta verificada").to_owned()),
                            )
                            .centered_y(),
                    );
                }
                if row.link {
                    parts.push(
                        Button::new(
                            format!("{}{index}", ids::CONNECTION),
                            "",
                            look.menu_palette(),
                        )
                        .icon(Icon::ExternalLink)
                        .variant(ButtonVariant::Ghost)
                        .accessibility_label(tr!(
                            "Abrir {service} de {name}",
                            service = row.label,
                            name = row.name
                        ))
                        .tooltip(tr!("Abrir en {service}", service = row.label))
                        .render()
                        .centered_y(),
                    );
                }
                Element::panel(Element::row(parts).gap(10.0).padding(8.0))
                    .radius(8.0)
                    .background(look.raised)
            })
            .collect(),
    )
    .gap(6.0)
}

/// Seconds as a clock: "4:05", or "1:02:03" past an hour.
pub fn clock(seconds: i64) -> String {
    let seconds = seconds.max(0);
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{frame, texts};
    use limn::Size;

    fn image() -> RasterImage {
        RasterImage::new(
            "1".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "x",
        )
        .unwrap()
    }

    #[test]
    fn a_decoration_overflows_the_face_without_moving_it() {
        let face = |side: f32| {
            Element::panel(Element::spacer())
                .size(Some(side), Some(side))
                .background(Color::rgb(1, 2, 3))
        };
        let row = Element::row(vec![
            gap(10.0, 0.0),
            decorated(face, 48.0, Some(image())),
            line("después", 14.0, Color::WHITE),
        ]);
        let (_, frame) = frame(row, Size::new(300.0, 80.0));
        let drawn = frame
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::RasterImage { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("the decoration is drawn");
        assert_eq!((drawn.x, drawn.y), (10.0, 0.0));
        assert_eq!((drawn.width, drawn.height), (48.0, 48.0));
        let face = frame
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::FillRect { rect, color, .. }
                    if *color == Color::rgb(1, 2, 3) =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .expect("the face is drawn");
        assert_eq!((face.width, face.x), (40.0, 14.0), "smaller, centered");
        // What follows keeps its place after the 48 px room.
        let after = frame
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::Text { text, position, .. } if text == "después" => {
                    Some(*position)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(after.x, 58.0);
    }

    /// A PNG of `width` by `height` as a picture: only its header matters.
    fn png(width: u32, height: u32) -> RasterImage {
        let mut bytes = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        bytes.extend(width.to_be_bytes());
        bytes.extend(height.to_be_bytes());
        const DIGITS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let base64: String = bytes
            .chunks(3)
            .flat_map(|chunk| {
                let n = u32::from(chunk[0]) << 16
                    | u32::from(*chunk.get(1).unwrap_or(&0)) << 8
                    | u32::from(*chunk.get(2).unwrap_or(&0));
                (0..4).map(move |k| DIGITS[(n >> (18 - 6 * k) & 63) as usize] as char)
            })
            .collect();
        RasterImage::new(
            "3".repeat(64),
            limn::RasterImageFormat::Png,
            24,
            std::sync::Arc::from(base64.as_str()),
            "x",
        )
        .unwrap()
    }

    #[test]
    fn an_effect_covers_the_whole_width_at_its_own_shape_over_everything() {
        // A tall leaf picture, as a real effect's: 450 by 1086 px.
        let card = Element::panel(Element::spacer()).size(Some(300.0), Some(800.0));
        let effect = CardEffect {
            layers: vec![(png(450, 1086), Point::new(0.0, 0.0))],
        };
        let (_, laid) = frame(
            with_effect(card, 300.0, Some(&effect), None),
            Size::new(300.0, 800.0),
        );
        let drawn = laid
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::RasterImage { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("the effect is drawn");
        assert_eq!(
            (drawn.x, drawn.y, drawn.width),
            (0.0, 0.0, 300.0),
            "the card's width"
        );
        assert!(
            (drawn.height - 724.0).abs() < 0.5,
            "its own shape: {drawn:?}"
        );
        assert_eq!(png_size(&image()), None, "not a PNG: the canvas's shape");
    }

    /// The pictures painted and seen (not faded away), by their names.
    fn painted(frame: &limn::Frame) -> Vec<String> {
        frame
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::RasterImage { image, opacity, .. } if *opacity > 0.0 => {
                    Some(image.alt().to_owned())
                }
                _ => None,
            })
            .collect()
    }

    fn named(name: &str, digit: &str) -> RasterImage {
        RasterImage::new(
            digit.repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            name,
        )
        .unwrap()
    }

    #[test]
    fn a_nameplate_moves_under_the_pointer_and_while_chosen() {
        let plate = |chosen| Plate {
            image: Some(named("quieta", "1")),
            colors: None,
            animated: Some(named("animada", "2")),
            chosen,
        };
        let row = |plate: &Plate| {
            on_plate(
                line("Ada", 14.0, Color::WHITE),
                41.0,
                Some(plate),
                "member:1",
            )
        };
        let size = Size::new(280.0, 41.0);
        let (mut tree, still) = frame(row(&plate(false)), size);
        assert_eq!(painted(&still), ["quieta"]);
        tree.handle_event(limn::UiEvent::PointerMove(Point::new(100.0, 20.0)));
        assert_eq!(
            painted(&tree.layout(size)),
            ["animada"],
            "under the pointer"
        );
        tree.handle_event(limn::UiEvent::PointerLeave);
        assert_eq!(painted(&tree.layout(size)), ["quieta"]);
        let (_, chosen) = frame(row(&plate(true)), size);
        assert_eq!(painted(&chosen), ["animada"], "its card is open");
        let still_only = Plate {
            animated: None,
            ..plate(false)
        };
        let (mut tree, _) = frame(row(&still_only), size);
        tree.handle_event(limn::UiEvent::PointerMove(Point::new(100.0, 20.0)));
        assert_eq!(painted(&tree.layout(size)), ["quieta"], "not loaded yet");
    }

    #[test]
    fn a_cards_effect_gives_way_under_the_pointer() {
        let card = Element::panel(Element::spacer()).size(Some(300.0), Some(200.0));
        let effect = CardEffect {
            layers: vec![(named("hojas", "4"), Point::new(0.0, 0.0))],
        };
        let size = Size::new(300.0, 200.0);
        let (mut tree, away) = frame(
            with_effect(card, 300.0, Some(&effect), Some("efecto")),
            size,
        );
        let opacity = |frame: &limn::Frame| {
            frame.commands.iter().find_map(|command| match command {
                limn::DrawCommand::RasterImage { opacity, .. } => Some(*opacity),
                _ => None,
            })
        };
        assert_eq!(painted(&away), ["hojas"]);
        assert_eq!(opacity(&away), Some(1.0));
        tree.handle_event(limn::UiEvent::PointerMove(Point::new(150.0, 100.0)));
        let under = opacity(&tree.layout(size)).unwrap();
        assert!(
            (under - EFFECT_UNDER_POINTER).abs() < 0.01,
            "dimmed so that what it covers reads: {under}"
        );
    }

    #[test]
    fn an_effect_lies_over_the_card_without_taking_room() {
        let card = Element::panel(Element::spacer()).size(Some(300.0), Some(200.0));
        let effect = CardEffect {
            layers: vec![(image(), Point::new(30.0, 60.0))],
        };
        let (_, frame) = frame(
            Element::column(vec![
                with_effect(card, 300.0, Some(&effect), None),
                line("debajo", 14.0, Color::WHITE),
            ]),
            Size::new(300.0, 600.0),
        );
        let drawn = frame
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::RasterImage { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("the effect is drawn");
        assert_eq!((drawn.x, drawn.y, drawn.width), (30.0, 60.0, 300.0));
        let below = frame
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::Text { text, position, .. } if text == "debajo" => {
                    Some(*position)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(below.y, 200.0, "the card keeps its height");
    }

    #[test]
    fn a_name_takes_its_roles_gradient_only_when_it_has_one() {
        let ink = Color::rgb(200, 0, 0);
        let painted = |gradient: &[Color]| {
            let (_, frame) = frame(
                inked(strong_line("Ada", 16.0, ink), ink, gradient),
                Size::new(200.0, 40.0),
            );
            texts(&frame).iter().filter(|text| *text == "Ada").count()
        };
        assert_eq!(painted(&[]), 1, "plain ink");
        assert!(painted(&[Color::rgb(0, 0, 200)]) > 1, "painted in bands");
    }

    #[test]
    fn a_bio_draws_the_pictures_it_names_in_place_and_their_names_until_they_load() {
        let pictures = vec![
            TextPicture {
                token: "<a:rana:15>".into(),
                name: "rana".into(),
                image: Some(image()),
                playing: false,
            },
            TextPicture {
                token: "<:rust:22>".into(),
                name: "rust".into(),
                image: None,
                playing: false,
            },
        ];
        let bio = "HOLA **MUNDO** LINDO\n<a:rana:15><a:rana:15>\n\ny <:rust:22> fin";
        assert!(
            frame(with_pictures("**negrita**", &[], 280.0, &Look::DARK), Size::new(300.0, 40.0))
                .1
                .commands
                .iter()
                .any(|command| matches!(command, limn::DrawCommand::Text { text, .. } if text == "negrita")),
            "Markdown"
        );
        let (tree, laid) = frame(
            with_pictures(bio, &pictures, 280.0, &Look::DARK),
            Size::new(300.0, 200.0),
        );
        let drawn: Vec<limn::Rect> = laid
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::RasterImage { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(drawn.len(), 2, "each token in place: {drawn:?}");
        // 1.375 times Markdown's 14 px, in one line of text.
        assert_eq!((drawn[0].width, drawn[0].height), (19.25, 19.25));
        assert!(
            drawn[1].x > drawn[0].x && drawn[1].y == drawn[0].y,
            "side by side"
        );
        let shown = texts(&laid);
        assert!(shown.iter().all(|text| !text.contains('<')), "{shown:?}");
        assert!(
            shown.iter().any(|text| text.contains(":rust:")),
            "its name while it loads: {shown:?}"
        );
        assert!(
            shown
                .windows(3)
                .any(|three| three == ["HOLA ", "MUNDO", " LINDO"]),
            "Markdown, its line kept and «MUNDO» in its bold face: {shown:?}"
        );
        // One text: what is copied names each picture.
        let source: String = tree
            .frame()
            .text_layouts()
            .iter()
            .map(|block| block.source.as_str())
            .collect();
        assert!(
            source.contains(":rana::rana:") && source.contains(":rust:"),
            "{source}"
        );
        // Playing, the picture is the host's surface.
        let moving = vec![TextPicture {
            playing: true,
            ..pictures[0].clone()
        }];
        let (_, laid) = frame(
            with_pictures("<a:rana:15>", &moving, 280.0, &Look::DARK),
            Size::new(300.0, 40.0),
        );
        assert!(laid.commands.iter().any(|command| matches!(
            command,
            limn::DrawCommand::Surface { id, .. } if *id == crate::message::animation_surface(image().sha256())
        )));
        // Without a token, the text as it was.
        let (_, plain) = frame(
            with_pictures("hola", &pictures, 280.0, &Look::DARK),
            Size::new(300.0, 40.0),
        );
        assert_eq!(texts(&plain), ["hola"]);
    }

    #[test]
    fn an_activity_card_keeps_its_pictures_clear_of_its_lines_and_its_neighbours() {
        // As a real one came: no large picture, a small one, four lines.
        let card = ActivityCard {
            kind: ActivityKind::Playing,
            name: "Valorant Tracker App".into(),
            detail: Some("Competitive: 9 - 12".into()),
            state: Some("Chamber | Party 2/5".into()),
            image: None,
            small_image: Some(image()),
            time: Some(ActivityTime::Elapsed("7:23 elapsed".into())),
        };
        let (_, laid) = frame(
            Element::column(vec![
                line("arriba", 14.0, Color::WHITE),
                activity_card(&card, &Look::DARK),
                line("abajo", 14.0, Color::WHITE),
            ])
            .gap(12.0),
            Size::new(293.0, 400.0),
        );
        let text_at = |wanted: &str| {
            laid.commands.iter().find_map(|command| match command {
                limn::DrawCommand::Text { text, position, .. } if text == wanted => Some(*position),
                _ => None,
            })
        };
        let small = laid
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::RasterImage { rect, .. } => Some(*rect),
                _ => None,
            })
            .expect("the small picture");
        for wanted in [
            "Valorant Tracker App",
            "Competitive: 9 - 12",
            "Chamber | Party 2/5",
        ] {
            let at = text_at(wanted).unwrap();
            assert!(
                at.x >= small.x + small.width + 4.0,
                "{wanted} clear of the pictures"
            );
        }
        // The card holds all of itself: what is above and below keeps clear.
        let ground = laid
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::FillRect { rect, color, .. } if *color == Look::DARK.raised => {
                    Some(*rect)
                }
                _ => None,
            })
            .unwrap();
        assert!(ground.y >= text_at("arriba").unwrap().y + 14.0);
        assert!(small.y + small.height <= ground.y + ground.height);
        assert!(text_at("abajo").unwrap().y >= ground.y + ground.height);
    }

    #[test]
    fn an_activity_card_shows_its_lines_and_a_tracks_bar() {
        let card = ActivityCard {
            kind: ActivityKind::Listening,
            name: "Radio".into(),
            detail: Some("Tema".into()),
            state: Some("Artista".into()),
            image: None,
            small_image: Some(image()),
            time: Some(ActivityTime::Track {
                at: "1:15".into(),
                length: "3:30".into(),
                progress: 0.36,
            }),
        };
        let (_, frame) = frame(activity_card(&card, &Look::DARK), Size::new(300.0, 200.0));
        let shown = texts(&frame);
        for expected in ["Escuchando", "Radio", "Tema", "Artista", "1:15", "3:30"] {
            assert!(shown.iter().any(|text| text == expected), "{expected}");
        }
        assert_eq!(clock(75), "1:15");
        assert_eq!(clock(3723), "1:02:03");
    }

    #[test]
    fn marks_badges_and_connections_name_what_they_are() {
        let marks = Marks {
            role: Some(RoleMark::Emoji {
                role: "Moderación".into(),
                emoji: "🛡️".into(),
            }),
            tag: Some(TagChip {
                text: "RUST".into(),
                badge: None,
                server: Some("Taller".into()),
            }),
            ..Default::default()
        };
        let parts = after_name(&marks, 16.0, &Look::DARK);
        assert_eq!(parts.len(), 2);
        let badges = badges(
            &[
                BadgeChip {
                    description: "HypeSquad".into(),
                    image: Some(image()),
                    link: true,
                },
                BadgeChip {
                    description: "Sin dibujo".into(),
                    image: None,
                    link: false,
                },
            ],
            &Look::DARK,
        );
        let (_, frame) = frame(
            Element::column(vec![
                Element::row(parts),
                badges,
                connections(
                    &[ConnectionRow {
                        service: "github".into(),
                        label: "GitHub".into(),
                        name: "ada".into(),
                        verified: true,
                        link: true,
                    }],
                    &Look::DARK,
                ),
                platforms(&[Platform::Desktop, Platform::Mobile], &Look::DARK),
            ]),
            Size::new(320.0, 400.0),
        );
        let ids = crate::testing::controls(&frame);
        assert!(
            ids.contains(&"pp:badge=0".to_owned()) && ids.contains(&"pp:connection=0".to_owned())
        );
        assert!(
            !ids.contains(&"pp:badge=1".to_owned()),
            "a badge without a link"
        );
        let shown = texts(&frame);
        for expected in ["RUST", "Sin dibujo", "GitHub", "ada", "Escritorio", "Móvil"] {
            assert!(shown.iter().any(|text| text == expected), "{expected}");
        }
    }
}
