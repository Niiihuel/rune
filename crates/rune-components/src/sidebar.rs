//! The column of channels: a server's categories and channels, the people
//! in its voice channels, or the list of direct conversations.

use limn::{
    Badge, ButtonVariant, Color, Component, ControlSize, CornerRadii, Element, FontFamily, Icon,
    IconButton,
};
use rune_i18n::{t, tr};

use crate::{centered, gap, icon, line, strong_line, with_status, Look};

/// The column's width, its divider included: 293.5 of list and a hairline.
pub const WIDTH: f32 = 294.5;
/// The height of the bar at the top of the column and of the conversation,
/// a hairline under each.
pub const HEADER: f32 = 52.0;

/// A channel's row is this high.
pub const ROW: f32 = 38.0;
/// Between two rows of the column.
pub const ROW_GAP: f32 = 2.5;
/// From the column's edges to its rows.
pub const INSET: f32 = 9.5;
/// A category's heading is this high, with this much room above it.
const HEADING: f32 = 24.0;
const HEADING_ROOM: f32 = 16.5;

/// A channel as the column lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChannelRow {
    /// The action a click reports; also what is dragged and dropped on.
    pub id: String,
    pub name: String,
    pub voice: bool,
    /// The kind's glyph when it is not `#` or a loudspeaker: a forum's, an
    /// announcement channel's.
    pub glyph: Option<Icon>,
    pub selected: bool,
    pub unread: bool,
    pub mentions: u32,
    /// The account cannot write here.
    pub read_only: bool,
    /// The account muted it: drawn dim.
    pub muted: bool,
    /// The action of the gear that edits it, while the account may.
    pub settings: Option<String>,
    /// It can be dragged to another place, and others dropped on it.
    pub movable: bool,
    /// How a drag in course draws it.
    pub drag: DragMark,
    /// `@everyone` may not see it: a padlock follows its name.
    pub private: bool,
    // Voice protocol (voz-protocolo).
    /// A voice channel: how long someone has been in it, as read ("1 h 05
    /// min"), dim at the row's end.
    pub voice_since: Option<String>,
}

/// How a drag in course draws a row of the column.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum DragMark {
    #[default]
    None,
    /// What is dragged, or a row it cannot land on: dimmed.
    Dim,
    /// It would land just above this row.
    Above,
    /// It would land just below this row.
    Below,
    /// It would go inside this row: a voice channel taking someone carried.
    Inside,
}

/// A category as the column lists it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CategoryRow {
    /// The action a click reports (it folds the category); also what is
    /// dragged and dropped on.
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    /// The action of the «+» that makes a channel in it, while the account may.
    pub create: Option<String>,
    pub movable: bool,
    pub drag: DragMark,
}

/// A direct conversation as the column lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct DirectRow {
    pub id: String,
    pub image: Option<limn::RasterImage>,
    pub title: String,
    pub subtitle: Option<String>,
    /// A server's emoji before `subtitle`, when that is their custom
    /// status (2026-10-18).
    pub subtitle_emoji: Option<crate::identity::TextPicture>,
    pub accent: Color,
    pub status: Option<Color>,
    pub group: bool,
    pub selected: bool,
    pub unread: bool,
}

/// The bar at the top of the column with a title, such as the direct
/// conversations'.
pub fn header(title: &str, look: &Look) -> Element {
    Element::panel(Element::row(vec![
        gap(INSET + 12.0, 0.0),
        strong_line(title, 16.0, look.text).flex(1.0).centered_y(),
        icon(Icon::ChevronDown, look.text)
            .size(Some(12.0), Some(12.0))
            .centered_y(),
        gap(INSET + 12.0, 0.0),
    ]))
    .size(None, Some(HEADER))
    .background(look.sidebar)
}

/// The bar at the top of the column with `content` at its left, such as
/// [`server_name`] and its menu.
pub fn header_bar(content: Element, look: &Look) -> Element {
    Element::panel(Element::row(vec![
        gap(INSET, 0.0),
        content.shrink_to_fit().centered_y(),
        Element::spacer().flex(1.0),
        gap(INSET, 0.0),
    ]))
    .size(None, Some(HEADER))
    .background(look.sidebar)
}

/// The server's name as the button that opens its menu, its chevron up
/// while the menu is `open`.
pub fn server_name(title: &str, open: bool, look: &Look) -> Element {
    let chevron = if open {
        Icon::ChevronUp
    } else {
        Icon::ChevronDown
    };
    Element::clickable(
        crate::server_menu::ids::TOGGLE,
        Element::row(vec![
            gap(12.0, 0.0),
            strong_line(title, 16.0, look.text)
                .shrink_to_fit()
                .centered_y(),
            gap(8.0, 0.0),
            icon(chevron, look.text)
                .size(Some(12.0), Some(12.0))
                .centered_y(),
            gap(12.0, 0.0),
        ]),
    )
    .size(None, Some(31.5))
    .radius(8.0)
    .background(if open {
        look.hover
    } else {
        Color::rgba(0, 0, 0, 0)
    })
    .hover_background(look.hover)
    .accessibility_label(title)
}

/// A category: its name as written, with a chevron that says whether it is
/// folded (a click folds it) and, for whoever may, a «+» that makes a
/// channel in it.
pub fn category(row: CategoryRow, look: &Look) -> Element {
    let chevron = if row.collapsed {
        Icon::ChevronRight
    } else {
        Icon::ChevronDown
    };
    let ink = if row.drag == DragMark::Dim {
        dimmed(look)
    } else {
        look.dim
    };
    let mut parts = vec![
        gap(11.0, 0.0),
        line(&row.name, 14.0, ink)
            .font_family(FontFamily::SansMedium)
            .shrink_to_fit()
            .centered_y(),
        gap(4.0, 0.0),
        Element::icon(chevron)
            .foreground(ink)
            .size(Some(12.0), Some(12.0))
            .centered_y(),
        Element::spacer().flex(1.0),
    ];
    if let Some(create) = &row.create {
        parts.push(
            IconButton::new(create.clone(), Icon::Add, look.sidebar_palette())
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Dense)
                .tooltip(t("Crear un canal"))
                .ink(look.dim)
                .hover_ink(look.text)
                .render()
                .centered_y(),
        );
    }
    parts.push(gap(7.5, 0.0));
    let heading = Element::clickable(row.id.clone(), Element::row(parts))
        .size(None, Some(HEADING))
        .hover_foreground(look.text)
        .accessible(
            limn::Access::new(limn::Role::Button)
                .name(row.name.clone())
                .expanded(!row.collapsed),
        );
    let heading = if row.movable {
        heading.draggable().drop_target()
    } else {
        heading
    };
    // Room above a category sets its channels apart from the ones before.
    Element::column(vec![
        gap(0.0, HEADING_ROOM),
        Element::row(vec![
            gap(INSET, 0.0),
            insertion(heading, row.drag, look).flex(1.0),
            gap(INSET, 0.0),
        ]),
    ])
}

/// The ink of what a drag dims.
fn dimmed(look: &Look) -> Color {
    Look::alpha(look.faint, 70)
}

/// `row` with the line where a dragged one would land, over its top or
/// bottom edge so nothing moves.
fn insertion(row: Element, mark: DragMark, look: &Look) -> Element {
    let bar = || {
        Element::panel(Element::spacer())
            .size(None, Some(4.0))
            .radius(2.0)
            .background(look.online)
    };
    let overlay = match mark {
        DragMark::Above => vec![bar(), Element::spacer().flex(1.0)],
        DragMark::Below => vec![Element::spacer().flex(1.0), bar()],
        DragMark::Inside => {
            let ring = Element::panel(Element::spacer())
                .radius(6.0)
                .border(look.online, 2.0);
            return Element::stack(vec![row, ring]);
        }
        _ => return row,
    };
    Element::stack(vec![row, Element::column(overlay)])
}

/// A channel: its kind, its name, and what waits in it. Unread channels are
/// brighter and marked at the column's edge; muted ones are dim. While the
/// pointer is on it, or while it is open, a gear edits it for whoever may.
pub fn channel(row: ChannelRow, look: &Look) -> Element {
    let (ink, glyph_ink) = if row.drag == DragMark::Dim {
        (dimmed(look), dimmed(look))
    } else if row.selected {
        (look.text, look.text)
    } else if row.muted {
        (Look::alpha(look.faint, 150), Look::alpha(look.faint, 150))
    } else if row.unread || row.mentions > 0 {
        (look.text, look.muted)
    } else {
        (look.dim, look.faint)
    };
    let glyph = row
        .glyph
        .unwrap_or(if row.voice { Icon::Volume } else { Icon::Hash });
    let glyph = icon(glyph, glyph_ink).large_icon(20.0);
    let glyph = if row.private {
        // A small padlock over the glyph's top right corner, as the
        // official client's.
        let lock = icon(Icon::Lock, glyph_ink).small_icon(10.0);
        Element::stack(vec![
            glyph,
            Element::column(vec![
                Element::row(vec![Element::spacer().flex(1.0), lock]),
                Element::spacer().flex(1.0),
            ]),
        ])
        .size(Some(20.0), Some(20.0))
    } else {
        glyph
    };
    let mut parts = vec![
        gap(12.0, 0.0),
        glyph.centered_y(),
        gap(11.0, 0.0),
        strong_line(&row.name, 16.0, ink).flex(1.0).centered_y(),
    ];
    if row.read_only {
        parts.push(gap(4.0, 0.0));
        parts.push(icon(Icon::Pin, look.faint).centered_y());
    }
    if let Some(since) = &row.voice_since {
        parts.push(gap(4.0, 0.0));
        parts.push(line(since, 12.0, look.faint).centered_y());
    }
    if row.mentions > 0 {
        parts.push(gap(4.0, 0.0));
        parts.push(count(row.mentions, look).centered_y());
        parts.push(gap(3.0, 0.0));
    }
    if let Some(settings) = &row.settings {
        let gear = IconButton::new(settings.clone(), Icon::Settings, look.sidebar_palette())
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Dense)
            .tooltip(t("Editar canal"))
            .ink(look.dim)
            .hover_ink(look.text)
            .render();
        // Shown on hover; always while it is open.
        parts.push(gap(4.0, 0.0));
        parts.push(if row.selected {
            gear.centered_y()
        } else {
            gear.reveal_on_hover(format!("{}:tools", row.id))
                .centered_y()
        });
    }
    parts.push(gap(7.5, 0.0));
    let mut access = limn::Access::new(limn::Role::ListItem)
        .name(row.name.clone())
        .selected(row.selected);
    let description: Vec<String> = [
        row.private.then(|| t("Canal privado").to_owned()),
        row.voice_since
            .as_ref()
            .map(|since| tr!("En voz desde hace {since}", since)),
    ]
    .into_iter()
    .flatten()
    .collect();
    if !description.is_empty() {
        access = access.description(description.join(" · "));
    }
    let body = Element::clickable(row.id.clone(), Element::row(parts))
        .size(None, Some(ROW))
        .radius(6.0)
        .background(if row.selected {
            look.selected
        } else {
            Color::rgba(0, 0, 0, 0)
        })
        .hover_background(if row.selected {
            look.selected
        } else {
            look.hover
        })
        .hover_foreground(look.text)
        .hover_region()
        .accessible(access);
    let body = if row.movable {
        body.draggable().drop_target()
    } else {
        body
    };
    unread_edge(
        insertion(body, row.drag, look),
        row.unread && !row.selected,
        look,
    )
}

/// A thread's row is this high.
pub const THREAD_ROW: f32 = 32.0;

/// A thread under its channel: indented, hung from a line that comes down
/// from the channel's glyph and goes on to the next thread unless it is the
/// `last`. Brighter when unread, like a channel; it has no gear.
pub fn thread(row: ChannelRow, last: bool, look: &Look) -> Element {
    let ink = if row.selected || row.unread || row.mentions > 0 {
        look.text
    } else if row.muted {
        Look::alpha(look.faint, 150)
    } else {
        look.dim
    };
    let wire = Look::alpha(look.faint, 120);
    let stem = || {
        Element::panel(Element::spacer())
            .size(Some(1.5), None)
            .background(wire)
            .flex(1.0)
    };
    let branch = Element::row(vec![
        // Under the middle of the channel's glyph.
        gap(21.0, 0.0),
        Element::column(vec![
            stem(),
            if last {
                Element::spacer().flex(1.0)
            } else {
                stem()
            },
        ])
        .size(Some(1.5), Some(THREAD_ROW)),
        Element::column(vec![
            Element::spacer().flex(1.0),
            Element::panel(Element::spacer())
                .size(Some(10.0), Some(1.5))
                .background(wire),
            Element::spacer().flex(1.0),
        ])
        .size(Some(10.0), Some(THREAD_ROW)),
        gap(4.0, 0.0),
    ]);
    let mut parts = vec![
        gap(8.0, 0.0),
        strong_line(&row.name, 15.0, ink).flex(1.0).centered_y(),
    ];
    if row.mentions > 0 {
        parts.push(gap(4.0, 0.0));
        parts.push(count(row.mentions, look).centered_y());
        parts.push(gap(3.0, 0.0));
    }
    parts.push(gap(7.5, 0.0));
    let body = Element::clickable(row.id.clone(), Element::row(parts))
        .size(None, Some(THREAD_ROW))
        .radius(6.0)
        .background(if row.selected {
            look.selected
        } else {
            Color::rgba(0, 0, 0, 0)
        })
        .hover_background(if row.selected {
            look.selected
        } else {
            look.hover
        })
        .hover_foreground(look.text)
        .accessible(
            limn::Access::new(limn::Role::ListItem)
                .name(row.name.clone())
                .selected(row.selected),
        );
    unread_edge(
        Element::row(vec![branch, body.flex(1.0)]),
        row.unread && !row.selected,
        look,
    )
}

/// `body` inset from the column's edges, with a pill at the left edge for
/// something unread.
fn unread_edge(body: Element, unread: bool, look: &Look) -> Element {
    let pill = if unread {
        Element::column(vec![
            Element::spacer().flex(1.0),
            Element::panel(Element::spacer())
                .size(Some(4.0), Some(8.0))
                .corner_radii(CornerRadii {
                    top_left: 0.0,
                    top_right: 4.0,
                    bottom_right: 4.0,
                    bottom_left: 0.0,
                })
                .background(look.text),
            Element::spacer().flex(1.0),
        ])
    } else {
        Element::spacer()
    };
    Element::row(vec![
        Element::row(vec![pill, Element::spacer().flex(1.0)]).size(Some(INSET), None),
        body.flex(1.0),
        gap(INSET, 0.0),
    ])
}

/// The red count of mentions.
pub fn count(mentions: u32, look: &Look) -> Element {
    let label = if mentions > 99 {
        "99+".to_owned()
    } else {
        mentions.to_string()
    };
    // Keep horizontal breathing room without subtracting vertical space from
    // the fixed-height capsule: its text line is taller than a 9 px interior.
    Element::panel(centered(Element::row(vec![
        gap(4.0, 0.0),
        Element::text(label)
            .font_family(FontFamily::SansBold)
            .font_size(12.0)
            .foreground(Color::rgb(255, 255, 255)),
        gap(4.0, 0.0),
    ])))
    .size(None, Some(17.0))
    .min_size(17.0, 17.0)
    .radius(8.5)
    .background(look.danger_fill)
}

/// A direct conversation: who it is with, how they are, what waits.
pub fn direct(row: DirectRow, look: &Look) -> Element {
    // A group shows its own picture when it has one, its mark otherwise.
    let face = match row.image {
        None if row.group => group_mark(row.accent, 32.0),
        image => crate::avatar_with_image(&row.title, row.accent, 32.0, image),
    };
    let face = match row.status {
        Some(status) => with_status(face, 32.0, status, look.sidebar),
        None => face,
    };
    let ink = if row.selected || row.unread {
        look.text
    } else {
        look.dim
    };
    let mut copy = vec![strong_line(&row.title, 16.0, ink)];
    copy.extend(crate::identity::status_line(
        row.subtitle.as_deref(),
        row.subtitle_emoji.as_ref(),
        13.0,
        look.faint,
        look,
    ));
    // As a person of the member list.
    let body = Element::clickable(
        row.id.clone(),
        Element::row(vec![
            gap(8.0, 0.0),
            face.centered_y(),
            gap(12.0, 0.0),
            Element::column(copy).gap(1.0).flex(1.0).centered_y(),
            gap(8.0, 0.0),
        ]),
    )
    .size(None, Some(43.0))
    .radius(6.0)
    .background(if row.selected {
        look.selected
    } else {
        Color::rgba(0, 0, 0, 0)
    })
    .hover_background(if row.selected {
        look.selected
    } else {
        look.hover
    })
    .accessible(
        limn::Access::new(limn::Role::ListItem)
            .name(row.title.clone())
            .selected(row.selected),
    );
    unread_edge(body, row.unread && !row.selected, look)
}

/// A muted heading over a group of rows.
/// A group conversation's mark, `side` wide, for one without a picture.
pub fn group_mark(accent: Color, side: f32) -> Element {
    Element::panel(centered(
        icon(Icon::Users, Color::rgb(255, 255, 255)).size(Some(side / 2.0), Some(side / 2.0)),
    ))
    .size(Some(side), Some(side))
    .radius(side / 2.0)
    .background(accent)
}

/// A heading with a small button at its end: its ID and what it does.
pub fn heading_with(title: &str, action: (&str, &str), look: &Look) -> Element {
    let (id, label) = action;
    Element::row(vec![
        heading(title, look).flex(1.0),
        IconButton::new(id, Icon::Add, look.sidebar_palette())
            .variant(ButtonVariant::Ghost)
            .size(ControlSize::Dense)
            .tooltip(label)
            .accessibility_label(label)
            .ink(look.dim)
            .hover_ink(look.text)
            .render()
            .centered_y(),
        gap(INSET + 7.5, 0.0),
    ])
    .size(None, Some(HEADING + HEADING_ROOM))
}

pub fn heading(title: &str, look: &Look) -> Element {
    Element::row(vec![
        gap(INSET + 11.0, 0.0),
        line(title, 14.0, look.dim)
            .font_family(FontFamily::SansMedium)
            .centered_y(),
    ])
    .size(None, Some(HEADING + HEADING_ROOM))
}

/// An icon with a red count over it, for a control with something waiting.
pub fn badged(child: Element, mentions: u32, look: &Look) -> Element {
    if mentions == 0 {
        return child;
    }
    Badge::new(child, look.palette())
        .count(mentions)
        .color(look.danger_fill)
        .render()
}

#[cfg(test)]
mod badge_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    fn row(name: &str, unread: bool, mentions: u32) -> ChannelRow {
        ChannelRow {
            id: format!("channel:{name}"),
            name: name.into(),
            unread,
            mentions,
            ..ChannelRow::default()
        }
    }

    fn heading(name: &str, create: Option<&str>) -> CategoryRow {
        CategoryRow {
            id: "category:1".into(),
            name: name.into(),
            create: create.map(str::to_owned),
            ..CategoryRow::default()
        }
    }

    #[test]
    fn a_private_rows_status_shows_its_servers_emoji_before_its_words() {
        let emoji = crate::identity::TextPicture {
            token: "<:ok:1>".into(),
            name: "ok".into(),
            image: limn::RasterImage::new(
                "a".repeat(64),
                limn::RasterImageFormat::Png,
                3,
                std::sync::Arc::from("AAAA"),
                "ok",
            ),
            playing: false,
        };
        let shown = DirectRow {
            id: "channel:5".into(),
            image: None,
            title: "Ada".into(),
            subtitle: Some("de vacaciones".into()),
            subtitle_emoji: Some(emoji),
            accent: Color::rgb(1, 2, 3),
            status: None,
            group: false,
            selected: false,
            unread: false,
        };
        let (_, frame) = frame(
            direct(shown, &Look::DARK).size(Some(WIDTH), None),
            Size::new(WIDTH, 60.0),
        );
        assert!(texts(&frame).contains(&"de vacaciones".to_owned()));
        assert!(frame.commands.iter().any(|command| matches!(
            command,
            limn::DrawCommand::RasterImage { rect, .. } if rect.width == 14.0
        )));
    }

    #[test]
    fn a_channel_is_one_control_that_shows_its_mentions() {
        let column = Element::column(vec![
            category(heading("General", None), &Look::DARK),
            channel(row("general", true, 3), &Look::DARK),
            channel(row("ayuda", false, 0), &Look::DARK),
        ]);
        let (_, frame) = frame(
            Element::row(vec![
                column.size(Some(WIDTH), None),
                Element::spacer().flex(1.0),
            ]),
            Size::new(600.0, 300.0),
        );
        let ids = controls(&frame);
        assert!(
            ids.contains(&"channel:general".to_owned()) && ids.contains(&"category:1".to_owned())
        );
        let painted = texts(&frame);
        assert!(
            painted.contains(&"3".to_owned()),
            "the mention count is painted: {painted:?}"
        );
        assert!(
            painted.contains(&"General".to_owned()),
            "a category's name is written as it was given"
        );
    }

    #[test]
    fn a_private_channel_shows_a_padlock_and_says_so() {
        let locks = |row: ChannelRow| {
            let (_, frame) = frame(
                channel(row, &Look::DARK).size(Some(WIDTH), None),
                Size::new(WIDTH, 60.0),
            );
            let drawn = frame
                .commands
                .iter()
                .filter(|command| {
                    matches!(
                        command,
                        limn::DrawCommand::Icon {
                            icon: Icon::Lock,
                            ..
                        }
                    )
                })
                .count();
            let said = frame
                .access
                .iter()
                .find(|node| node.name == "secreto")
                .and_then(|node| node.description.clone());
            (drawn, said)
        };
        let private = ChannelRow {
            private: true,
            ..row("secreto", false, 0)
        };
        assert_eq!(
            locks(private.clone()),
            (1, Some("Canal privado".to_owned()))
        );
        assert_eq!(locks(row("secreto", false, 0)).0, 0);
        // A small padlock over the top right corner of the glyph.
        let (_, frame) = frame(
            channel(private, &Look::DARK).size(Some(WIDTH), None),
            Size::new(WIDTH, 60.0),
        );
        let drawn = |wanted: Icon| {
            frame
                .commands
                .iter()
                .find_map(|command| match command {
                    limn::DrawCommand::Icon { icon, rect, .. } if *icon == wanted => Some(*rect),
                    _ => None,
                })
                .unwrap()
        };
        let (hash, lock) = (drawn(Icon::Hash), drawn(Icon::Lock));
        assert_eq!((lock.width, lock.height), (10.0, 10.0));
        assert_eq!((lock.x + lock.width, lock.y), (hash.x + hash.width, hash.y));
    }

    #[test]
    fn a_voice_channel_on_shows_how_long_and_says_so() {
        let room = ChannelRow {
            voice: true,
            private: true,
            voice_since: Some("1 h 05 min".into()),
            ..row("sala", false, 0)
        };
        let (_, frame) = frame(
            channel(room, &Look::DARK).size(Some(WIDTH), None),
            Size::new(WIDTH, 60.0),
        );
        assert!(texts(&frame).contains(&"1 h 05 min".to_owned()));
        let said = frame
            .access
            .iter()
            .find(|node| node.name == "sala")
            .and_then(|node| node.description.clone());
        assert_eq!(
            said.as_deref(),
            Some("Canal privado · En voz desde hace 1 h 05 min")
        );
    }

    #[test]
    fn the_gear_and_the_plus_are_there_only_for_whoever_may() {
        let managed = ChannelRow {
            settings: Some("cs:open:5".into()),
            movable: true,
            selected: true,
            ..row("general", false, 0)
        };
        let column = Element::column(vec![
            category(heading("Texto", Some("newchan:1:9")), &Look::DARK),
            channel(managed, &Look::DARK),
            channel(row("ayuda", false, 0), &Look::DARK),
        ]);
        let (_, frame) = frame(
            Element::row(vec![
                column.size(Some(WIDTH), None),
                Element::spacer().flex(1.0),
            ]),
            Size::new(600.0, 300.0),
        );
        let ids = controls(&frame);
        for id in [
            "cs:open:5",
            "newchan:1:9",
            "channel:general",
            "channel:ayuda",
        ] {
            assert!(ids.contains(&id.to_owned()), "{id}: {ids:?}");
        }
        assert_eq!(
            ids.iter().filter(|id| id.starts_with("cs:open:")).count(),
            1,
            "only the channel that may be edited has a gear"
        );
        let gear = frame
            .regions
            .iter()
            .find(|region| region.id == "cs:open:5")
            .unwrap()
            .rect;
        assert!(gear.x + gear.width <= WIDTH, "{gear:?}");
    }

    #[test]
    fn threads_hang_under_their_channel_as_controls_of_their_own() {
        let column = Element::column(vec![
            channel(row("galería", false, 0), &Look::DARK),
            thread(
                ChannelRow {
                    id: "thread:7".into(),
                    ..row("Salida del sábado", true, 2)
                },
                false,
                &Look::DARK,
            ),
            thread(
                ChannelRow {
                    id: "thread:8".into(),
                    ..row(
                        "un-hilo-con-un-nombre-larguísimo-que-no-entra-en-la-columna",
                        false,
                        0,
                    )
                },
                true,
                &Look::DARK,
            ),
        ]);
        let (_, frame) = frame(
            Element::row(vec![
                column.size(Some(WIDTH), None),
                Element::spacer().flex(1.0),
            ]),
            Size::new(600.0, 200.0),
        );
        let ids = controls(&frame);
        assert!(ids.contains(&"thread:7".to_owned()) && ids.contains(&"thread:8".to_owned()));
        let rect = |id: &str| frame.regions.iter().find(|r| r.id == id).unwrap().rect;
        let (parent, child) = (rect("channel:galería"), rect("thread:7"));
        assert!(child.x > parent.x + 20.0, "indented: {parent:?} {child:?}");
        assert!(rect("thread:8").x + rect("thread:8").width <= WIDTH + 0.5);
        assert!(
            texts(&frame).contains(&"2".to_owned()),
            "its mentions are painted"
        );
    }

    #[test]
    fn a_long_name_stays_inside_the_column() {
        let long = row("un-canal-con-un-nombre-larguísimo-que-no-entra", false, 120);
        let (_, frame) = frame(
            Element::row(vec![
                channel(long, &Look::DARK).size(Some(WIDTH), None),
                Element::spacer().flex(1.0),
            ]),
            Size::new(600.0, 60.0),
        );
        for region in &frame.regions {
            assert!(
                region.rect.x + region.rect.width <= WIDTH + 0.5,
                "{} escapes: {:?}",
                region.id,
                region.rect
            );
        }
        assert!(texts(&frame).contains(&"99+".to_owned()));
    }
}
