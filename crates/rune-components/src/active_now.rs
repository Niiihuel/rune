//! The «Activo ahora» column of the friends page, from plain props (Discord's
//! column): one card per group of friends, with the voice channel they share,
//! their streams and their activities, and the card's menu. The props are
//! built from the state in `rune-ui::active_now`.

use limn::{
    Color, Component, DropdownMenu, Element, FontFamily, Icon, Point, PopoverPlacement,
    RasterImage, ScrollArea, Tooltip,
};
use rune_i18n::t;
use rune_model::{ChannelId, Rgb, Status, UserId};

use crate::look::menu_metrics_within;
use crate::voice::{self as voice_parts, MenuEntry};
use crate::{avatar_with_image, centered, gap, line, strong_line, text, with_status, Look};

/// The ids the column's controls report. A voice channel's line and a stream
/// report `voice:{id}`, as the voice column's rows do.
pub mod ids {
    /// The scrolling list of the column.
    pub const LIST: &str = "active-now";
    /// A card: its menu opens. Followed by the group's key.
    pub const CARD: &str = "active:card:";
    /// The lead person's avatar: the card's menu on a click, their person menu
    /// on a secondary click. Followed by their user ID.
    pub const AVATAR: &str = "active:avatar:";
    /// A server, from a card's icon or «Ir al servidor». Followed by its ID.
    pub const SERVER: &str = "active:server:";
    /// «Unirse al canal de voz». Followed by the channel's ID.
    pub const JOIN: &str = "active:join:";
    /// «Mirar transmisión de {name}». Followed by the streamer's user ID.
    pub const WATCH: &str = "active:watch:";
    /// The card's menu, dismissed by a click outside it.
    pub const MENU_DISMISS: &str = "active:menu:dismiss";
}

/// The column shows only in windows wider than this, as Discord's does.
pub const BREAKPOINT: f32 = 1200.0;

/// Whether the column shows in a window `window` wide.
pub fn shown_in(window: f32) -> bool {
    window > BREAKPOINT
}

/// 30 % of the page, from 360 to 420 px wide.
pub fn column_width(page: f32) -> f32 {
    (page * 0.3).clamp(360.0, 420.0)
}

/// Between cards, and between the heading and the first one.
const CARD_GAP: f32 = 12.0;
/// The space a card's header and sections keep at each side.
const INSET: f32 = 16.0;
/// Each stacked avatar starts this far past the one before: 2 px overlap.
const OVERLAP: f32 = 22.0;

/// A person in a card: their picture, their name and, for the card's first
/// person only, their status.
#[derive(Clone, Debug, PartialEq)]
pub struct Person {
    pub user: UserId,
    pub name: String,
    /// Their color, when the account is known.
    pub accent: Option<Rgb>,
    pub image: Option<RasterImage>,
    /// Their status dot, for the header's person when presence is shown.
    pub status: Option<Status>,
}

/// A voice channel of a card: its server (its icon, or its initials), the
/// channel's name and the people in it.
#[derive(Clone, Debug, PartialEq)]
pub struct VoiceView {
    pub server: Option<rune_model::ServerId>,
    pub server_name: String,
    pub initials: String,
    pub server_image: Option<RasterImage>,
    pub channel: ChannelId,
    pub channel_name: String,
    pub members: Vec<Person>,
}

/// Someone of the card who streams in its channel: what they play, if
/// anything, and the stream's picture once it arrived.
#[derive(Clone, Debug, PartialEq)]
pub struct StreamView {
    pub channel: ChannelId,
    pub person: Person,
    /// «Transmitiendo {game}» or «Compartiendo su pantalla», already translated.
    pub line: String,
    pub preview: Option<RasterImage>,
}

/// One activity of a card, drawn as its kind asks.
#[derive(Clone, Debug, PartialEq)]
pub enum ActivityView {
    /// Rich presence: a picture, its title, state and timer.
    Rich {
        image: Option<RasterImage>,
        small_image: Option<RasterImage>,
        title: String,
        state: Option<String>,
        /// «Transcurrido» or «Restantes» with the time, already translated.
        timer: Option<String>,
    },
    /// A game with nothing but its name: who plays it.
    Plain {
        name: String,
        /// «1 persona», «3 personas», already translated.
        count: String,
        people: Vec<Person>,
    },
    /// A Spotify track: its art, the track, its artists and who listens.
    Spotify {
        art: Option<RasterImage>,
        track: String,
        artists: Option<String>,
        listeners: Vec<Person>,
    },
}

/// The picture at the header's right: an activity's, a controller for several.
#[derive(Clone, Debug, PartialEq)]
pub enum HeaderMark {
    None,
    Picture(RasterImage),
    Controller,
}

/// One card: a group of friends, its header, its sections and its menu.
#[derive(Clone, Debug, PartialEq)]
pub struct ActiveCard {
    /// The group's key: `channel:{id}`, `party:{id}` or `user:{id}`.
    pub key: String,
    /// The first person is the header's; the rest are in the title's count.
    pub members: Vec<Person>,
    pub title: String,
    pub subtitle: Option<String>,
    pub mark: HeaderMark,
    pub voice: Vec<VoiceView>,
    pub streams: Vec<StreamView>,
    pub activities: Vec<ActivityView>,
    /// The entries of the menu, empty while it is closed.
    pub menu: Vec<MenuEntry>,
    pub menu_open: bool,
}

/// The column of `page` wide, beside the friends page: its heading, its cards
/// or the empty state. `room` is the window's height, which a menu may grow to.
pub fn column(cards: &[ActiveCard], page: f32, room: f32, look: &Look) -> Element {
    let width = column_width(page);
    // A stream's picture fills the card: the column less its own border, the
    // list's sides, the card's border and the section's inset.
    let picture = width - 67.0;
    let mut content = vec![
        gap(0.0, 8.0),
        strong_line(t("Activo ahora"), 20.0, look.text),
        gap(0.0, 9.0),
    ];
    if cards.is_empty() {
        content.push(gap(0.0, CARD_GAP));
        content.push(empty(look));
    }
    for card in cards {
        content.push(gap(0.0, CARD_GAP));
        content.push(card_view(card, picture, room, look));
    }
    content.push(gap(0.0, INSET));
    let list = Element::column(vec![
        gap(0.0, 6.0),
        Element::row(vec![
            gap(INSET, 0.0),
            Element::column(content).flex(1.0),
            gap(INSET, 0.0),
        ]),
    ]);
    let scroll = ScrollArea::new(ids::LIST, list)
        .appearance(look.scrollbar(true))
        .render()
        .flex(1.0);
    Element::panel(Element::row(vec![
        Element::panel(Element::spacer())
            .size(Some(1.0), None)
            .background(look.border),
        scroll,
    ]))
    .size(Some(width), None)
    .background(look.canvas)
}

/// What the column says while nobody is around: a raised panel, centred.
pub fn empty(look: &Look) -> Element {
    Element::panel(Element::column(vec![
        Element::text(t("No hay nadie, por ahora..."))
            .font_family(FontFamily::SansMedium)
            .font_size(16.0)
            .foreground(look.text)
            .center_content(),
        gap(0.0, 4.0),
        text(
            t("¡Cuando un amigo inicie una actividad, como jugar o pasar el rato en el chat de voz, lo podrás ver aquí!"),
            14.0,
            look.muted,
        )
        .center_content(),
    ]))
    .radius(8.0)
    .background(look.raised)
    .padding(16.0)
}

/// A card: its header, a line between each of its sections, and its menu
/// beside it, 8 px away. `picture` is the width a stream's picture takes.
fn card_view(card: &ActiveCard, picture: f32, room: f32, look: &Look) -> Element {
    let mut body =
        vec![Element::panel(inset(header(card, look), INSET, INSET)).background(look.hover)];
    let sections = card
        .voice
        .iter()
        .map(|voice| voice_section(voice, look))
        .chain(
            card.streams
                .iter()
                .map(|stream| stream_section(stream, picture, look)),
        )
        .chain(
            card.activities
                .iter()
                .map(|activity| activity_section(activity, look)),
        );
    for section in sections {
        body.push(divider(look));
        body.push(inset(section, INSET, INSET));
    }
    let trigger = Element::clickable(format!("{}{}", ids::CARD, card.key), Element::column(body))
        .background(look.card)
        .border(look.border, 1.0)
        .radius(16.0)
        .hover_background(look.hover)
        .clip_children();
    DropdownMenu::new(
        trigger,
        voice_parts::menu_entries(card.menu.clone(), look),
        card.menu_open && !card.menu.is_empty(),
        ids::MENU_DISMISS,
        look.menu_palette(),
    )
    .context_menu_style()
    .context_metrics(menu_metrics_within(room))
    .width(voice_parts::menu_width(&card.menu))
    .placement(PopoverPlacement::LeftStart)
    .render()
    .popover_gap(8.0)
}

/// The card's header: its first person's avatar, the title and the subtitle,
/// and the mark at the right.
fn header(card: &ActiveCard, look: &Look) -> Element {
    let avatar = match card.members.first() {
        Some(lead) => Element::clickable(
            format!("{}{}", ids::AVATAR, lead.user.0),
            face(lead, 32.0, look),
        ),
        None => gap(32.0, 32.0),
    };
    let mut titles = vec![strong_line(card.title.clone(), 16.0, look.text)];
    if let Some(subtitle) = &card.subtitle {
        titles.push(line(subtitle.clone(), 14.0, look.muted));
    }
    let mut parts = vec![
        avatar.centered_y(),
        Element::column(titles).flex(1.0).centered_y(),
    ];
    match &card.mark {
        HeaderMark::None => {}
        HeaderMark::Picture(image) => parts.push(
            Element::raster_image(image.clone().cover())
                .size(Some(24.0), Some(24.0))
                .radius(4.0)
                .centered_y(),
        ),
        // A circle of the raised ground: the header's own is the hover tint.
        HeaderMark::Controller => parts.push(
            centered(
                Element::icon(Icon::Gamepad)
                    .foreground(look.text)
                    .size(Some(18.0), Some(18.0)),
            )
            .size(Some(24.0), Some(24.0))
            .radius(12.0)
            .background(look.raised)
            .centered_y(),
        ),
    }
    Element::row(parts).gap(12.0).size(None, Some(36.0))
}

/// A person's avatar, with their status dot when they have one.
fn face(person: &Person, size: f32, look: &Look) -> Element {
    let accent = person.accent.map_or(look.faint, |rgb| look.ink(rgb));
    let avatar = avatar_with_image(&person.name, accent, size, person.image.clone());
    match person.status {
        Some(status) => with_status(avatar, size, look.status(status), look.card),
        None => avatar,
    }
}

/// `child` with `vertical` above and below and `horizontal` at each side.
fn inset(child: Element, vertical: f32, horizontal: f32) -> Element {
    Element::column(vec![
        gap(0.0, vertical),
        Element::row(vec![
            gap(horizontal, 0.0),
            child.flex(1.0),
            gap(horizontal, 0.0),
        ]),
        gap(0.0, vertical),
    ])
}

fn divider(look: &Look) -> Element {
    Element::panel(Element::spacer())
        .size(None, Some(1.0))
        .background(look.border)
}

/// A voice channel: its server's icon (a click opens the server), its names
/// (a click joins the channel) and the stacked avatars of its people.
fn voice_section(voice: &VoiceView, look: &Look) -> Element {
    let names = Element::clickable(
        format!("voice:{}", voice.channel.0),
        Element::column(vec![
            strong_line(voice.server_name.clone(), 14.0, look.text),
            line(voice.channel_name.clone(), 12.0, look.muted),
        ]),
    )
    .flex(1.0)
    .centered_y();
    Element::row(vec![
        server_mark(voice, look),
        names,
        stacked(&voice.members, look).centered_y(),
    ])
    .gap(12.0)
}

/// A server's icon, round, or its initials on the raised ground, with a speaker
/// badge 2 px past its bottom right corner.
fn server_mark(voice: &VoiceView, look: &Look) -> Element {
    let icon = match &voice.server_image {
        Some(image) => Element::raster_image(image.clone().cover()),
        None => centered(
            Element::text(voice.initials.clone())
                .font_family(FontFamily::SansMedium)
                .font_size(14.0)
                .foreground(look.text),
        )
        .background(look.raised),
    }
    .size(Some(32.0), Some(32.0))
    .radius(16.0);
    let badge = centered(
        Element::icon(Icon::Volume)
            .foreground(look.on_accent)
            .size(Some(10.0), Some(10.0)),
    )
    .size(Some(16.0), Some(16.0))
    .radius(8.0)
    .background(look.accent)
    .translate(Point::new(2.0, 2.0));
    let corner = Element::column(vec![
        Element::spacer().flex(1.0),
        Element::row(vec![Element::spacer().flex(1.0), badge]),
    ]);
    let mark = Element::stack(vec![icon, corner]).size(Some(32.0), Some(32.0));
    match voice.server {
        Some(server) => Element::clickable(format!("{}{}", ids::SERVER, server.0), mark),
        None => mark,
    }
}

/// Someone live in the card's channel: their avatar and what they stream, and
/// the stream's picture, 16:9 across `picture` wide. A click joins the channel;
/// «Mirar transmisión» shows over the picture while the pointer is on it.
fn stream_section(stream: &StreamView, picture: f32, look: &Look) -> Element {
    let person = &stream.person;
    let head = Element::row(vec![
        face(person, 32.0, look).centered_y(),
        Element::column(vec![
            strong_line(person.name.clone(), 14.0, look.text),
            line(stream.line.clone(), 12.0, look.muted),
        ])
        .flex(1.0)
        .centered_y(),
    ])
    .gap(12.0);
    let shown = match &stream.preview {
        Some(image) => Element::raster_image(image.clone().cover()),
        None => centered(
            Element::icon(Icon::Screen)
                .foreground(look.muted)
                .size(Some(26.0), Some(26.0)),
        )
        .background(look.raised),
    }
    .size(Some(picture), Some(picture * 9.0 / 16.0))
    .radius(8.0);
    let hint = centered(
        Element::panel(Element::row(vec![
            gap(16.0, 0.0),
            text(t("Mirar transmisión"), 14.0, Color::rgb(255, 255, 255)).centered_y(),
            gap(16.0, 0.0),
        ]))
        .size(None, Some(32.0))
        .radius(16.0)
        .background(Color::rgba(0, 0, 0, 153)),
    )
    .reveal_on_hover(format!("active:stream-hover:{}", person.user.0));
    let preview = Element::clickable(
        format!("voice:{}", stream.channel.0),
        Element::stack(vec![shown, hint]).hover_region(),
    );
    Element::column(vec![head, gap(0.0, 8.0), preview])
}

/// An activity's section, as its kind asks for it.
fn activity_section(view: &ActivityView, look: &Look) -> Element {
    match view {
        ActivityView::Rich {
            image,
            small_image,
            title,
            state,
            timer,
        } => {
            let picture = match image {
                Some(image) => Element::raster_image(image.clone().cover())
                    .size(Some(32.0), Some(32.0))
                    .radius(4.0),
                None => game_box(look),
            };
            let picture = match small_image {
                Some(small) => {
                    Element::stack(vec![picture, small_corner(small)]).size(Some(32.0), Some(32.0))
                }
                None => picture,
            };
            let mut lines = vec![strong_line(title.clone(), 14.0, look.text)];
            if let Some(state) = state {
                lines.push(line(state.clone(), 12.0, look.muted));
            }
            if let Some(timer) = timer {
                lines.push(line(timer.clone(), 12.0, look.muted));
            }
            Element::row(vec![picture, Element::column(lines).flex(1.0).centered_y()]).gap(12.0)
        }
        ActivityView::Plain {
            name,
            count,
            people,
        } => Element::row(vec![
            game_box(look),
            Element::column(vec![
                strong_line(name.clone(), 14.0, look.text),
                line(count.clone(), 12.0, look.muted),
            ])
            .flex(1.0)
            .centered_y(),
            stacked(people, look).centered_y(),
        ])
        .gap(12.0),
        ActivityView::Spotify {
            art,
            track,
            artists,
            listeners,
        } => {
            let cover = match art {
                Some(image) => Element::raster_image(image.clone().cover()),
                None => centered(
                    Element::icon(Icon::MusicNote)
                        .foreground(look.muted)
                        .size(Some(18.0), Some(18.0)),
                )
                .background(look.raised),
            }
            .size(Some(32.0), Some(32.0));
            let mut lines = vec![strong_line(track.clone(), 14.0, look.text)];
            if let Some(artists) = artists {
                lines.push(line(artists.clone(), 12.0, look.muted));
            }
            let mut parts = vec![cover, Element::column(lines).flex(1.0).centered_y()];
            if listeners.len() >= 2 {
                parts.push(stacked(listeners, look).centered_y());
            }
            Element::row(parts).gap(12.0)
        }
    }
}

/// The square a game without a picture stands in: a game controller.
fn game_box(look: &Look) -> Element {
    centered(
        Element::icon(Icon::Gamepad)
            .foreground(look.muted)
            .size(Some(18.0), Some(18.0)),
    )
    .size(Some(32.0), Some(32.0))
    .radius(4.0)
    .background(look.raised)
}

/// A small round picture of 16 px at a 32 px square's bottom right, 4 px past
/// its edge.
fn small_corner(image: &RasterImage) -> Element {
    let small = Element::raster_image(image.clone().cover())
        .size(Some(16.0), Some(16.0))
        .radius(8.0)
        .translate(Point::new(4.0, 4.0));
    Element::column(vec![
        Element::spacer().flex(1.0),
        Element::row(vec![Element::spacer().flex(1.0), small]),
    ])
}

/// The avatars of `people` overlapping: up to two, three when there are three,
/// else two and a «+N» for the rest. A click opens a person's profile.
pub fn stacked(people: &[Person], look: &Look) -> Element {
    let shown = if people.len() <= 3 { people.len() } else { 2 };
    let more = people.len() - shown;
    // A stack gives each child the whole stack, so each one sits in a row of its
    // own: a slot a step past the last, 2 px under the next face.
    let mut items = Vec::new();
    for (index, person) in people[..shown].iter().enumerate() {
        let id = format!("pp:full:{}", person.user.0);
        let clickable = Element::clickable(id.clone(), face(person, 24.0, look));
        let avatar = Tooltip::new(clickable, id, person.name.clone(), look.palette()).render();
        let last = index + 1 == shown && more < 2;
        let slot = if last { 24.0 } else { OVERLAP };
        items.push(Element::row(vec![avatar]).size(Some(slot), Some(24.0)));
    }
    if more >= 2 {
        let label = format!("+{more}");
        let width = (label.chars().count() as f32 * 8.0 + 8.0).max(24.0);
        items.push(
            Element::row(vec![
                Element::spacer().flex(1.0),
                Element::text(label)
                    .font_family(FontFamily::SansMedium)
                    .font_size(14.0)
                    .foreground(look.text)
                    .centered_y(),
                Element::spacer().flex(1.0),
            ])
            .size(Some(width), Some(24.0))
            .radius(12.0)
            .background(look.raised),
        );
    }
    if items.is_empty() {
        return gap(0.0, 24.0);
    }
    Element::row(items).size(None, Some(24.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    fn person(id: u64, name: &str) -> Person {
        Person {
            user: UserId(id),
            name: name.into(),
            accent: Some(Rgb(0x61_AF_EF)),
            image: None,
            status: None,
        }
    }

    fn card(members: Vec<Person>) -> ActiveCard {
        ActiveCard {
            key: "channel:109".into(),
            members,
            title: "Ada, Bruno y 1 otro".into(),
            subtitle: Some("Celeste – 23 min".into()),
            mark: HeaderMark::None,
            voice: vec![VoiceView {
                server: Some(rune_model::ServerId(1)),
                server_name: "Taller de Rust".into(),
                initials: "TR".into(),
                server_image: None,
                channel: ChannelId(109),
                channel_name: "Sala común".into(),
                members: vec![person(10, "Ada"), person(11, "Bruno")],
            }],
            streams: Vec::new(),
            activities: vec![ActivityView::Rich {
                image: None,
                small_image: None,
                title: "Capítulo 7".into(),
                state: Some("En la cumbre".into()),
                timer: Some("Transcurrido: 23:00".into()),
            }],
            menu: Vec::new(),
            menu_open: false,
        }
    }

    fn painted(element: Element, size: Size) -> Vec<String> {
        let (_, laid) = frame(element, size);
        texts(&laid)
    }

    #[test]
    fn the_column_is_shown_only_wider_than_the_breakpoint() {
        assert!(!shown_in(1200.0));
        assert!(shown_in(1201.0));
    }

    #[test]
    fn the_column_takes_thirty_percent_clamped_to_360_and_420() {
        assert_eq!(column_width(900.0), 360.0);
        assert_eq!(column_width(1_200.0), 360.0);
        assert_eq!(column_width(1_400.0), 420.0);
        assert_eq!(column_width(3_000.0), 420.0);
    }

    #[test]
    fn an_empty_column_says_it_is_quiet_for_now() {
        let texts = painted(
            column(&[], 1_000.0, 800.0, &Look::DARK),
            Size::new(420.0, 600.0),
        );
        assert!(texts.contains(&"Activo ahora".to_owned()), "{texts:?}");
        assert!(
            texts.contains(&"No hay nadie, por ahora...".to_owned()),
            "{texts:?}"
        );
        assert!(texts
            .iter()
            .any(|text| text.starts_with("¡Cuando un amigo")));
    }

    #[test]
    fn a_card_shows_its_title_voice_activity_and_its_menu_when_open() {
        let mut open = card(vec![person(10, "Ada"), person(11, "Bruno")]);
        open.menu_open = true;
        open.menu = vec![MenuEntry::Item(
            format!("{}109", ids::JOIN),
            "Unirse al canal de voz".into(),
            None,
        )];
        let (_, laid) = frame(
            column(&[open], 1_000.0, 800.0, &Look::DARK),
            Size::new(420.0, 800.0),
        );
        let shown = texts(&laid);
        for expected in [
            "Ada, Bruno y 1 otro",
            "Celeste – 23 min",
            "Taller de Rust",
            "Sala común",
            "Capítulo 7",
            "En la cumbre",
            "Transcurrido: 23:00",
            "Unirse al canal de voz",
        ] {
            assert!(
                shown.contains(&expected.to_owned()),
                "{expected}: {shown:?}"
            );
        }
        let found = controls(&laid);
        assert!(found.contains(&format!("{}109", ids::JOIN)), "{found:?}");
        assert!(found.contains(&format!("{}10", ids::AVATAR)), "{found:?}");
        assert!(found.contains(&"voice:109".to_owned()), "{found:?}");
    }

    #[test]
    fn stacked_avatars_show_two_three_or_two_and_a_count() {
        let faces = |count: u64| -> Vec<String> {
            let people: Vec<Person> = (0..count).map(|id| person(id, "Persona")).collect();
            painted(stacked(&people, &Look::DARK), Size::new(200.0, 40.0))
        };
        assert!(!faces(2).iter().any(|text| text.starts_with('+')));
        assert!(!faces(3).iter().any(|text| text.starts_with('+')));
        assert!(faces(5).contains(&"+3".to_owned()), "{:?}", faces(5));
        let (_, laid) = frame(
            stacked(
                &(0..5).map(|id| person(id, "Persona")).collect::<Vec<_>>(),
                &Look::DARK,
            ),
            Size::new(200.0, 40.0),
        );
        // Two faces and the count's pill are the people who can be clicked: the two
        // shown ones open their profile.
        let profiles = controls(&laid)
            .into_iter()
            .filter(|id| id.starts_with("pp:full:"))
            .count();
        assert_eq!(profiles, 2);
    }

    #[test]
    fn the_stream_picture_is_sixteen_by_nine() {
        let mut streaming = card(vec![person(10, "Ada")]);
        streaming.streams = vec![StreamView {
            channel: ChannelId(109),
            person: person(10, "Ada"),
            line: "Compartiendo su pantalla".into(),
            preview: None,
        }];
        let (_, laid) = frame(
            column(&[streaming], 1_000.0, 800.0, &Look::DARK),
            Size::new(420.0, 800.0),
        );
        assert!(texts(&laid).contains(&"Compartiendo su pantalla".to_owned()));
        assert!(laid.regions.iter().any(|region| region.id == "voice:109"));
    }
}
