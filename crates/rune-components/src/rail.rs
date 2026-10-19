//! The rail of servers down the left edge: the home of direct conversations,
//! then one mark per server, with what is unread and mentioned, and the
//! folders the account groups them in.

use limn::{
    Badge, BadgePlacement, Color, Component, CornerRadii, Element, FontFamily, Icon, ScrollArea,
    Tooltip, TooltipFace, TooltipItem, TooltipPlacement,
};

use rune_i18n::{t, tr};

use crate::{centered, Look};

/// The rail's width, its divider included: 85.5 of marks and a hairline.
pub const WIDTH: f32 = 86.5;
/// A mark's side.
pub const MARK: f32 = 42.0;
/// A mark's corners: a rounded square whether chosen or not.
const MARK_RADIUS: f32 = 10.0;
/// From the rail's left edge to a mark.
const INSET: f32 = 21.5;
/// Between marks.
const GAP: f32 = 10.5;
/// Under the last mark.
const BOTTOM: f32 = 11.0;
/// Above home. Discord's sits under its own title bar; RUNE has the
/// system's, so home is centered in the 52 px band of the headers instead
/// of touching the window's edge.
const TOP: f32 = 5.0;
/// The pill at the rail's edge: as wide as this, and as high as one of the
/// three below while the mark is chosen, under the pointer, or waiting.
pub const PILL_WIDTH: f32 = 4.5;
pub const PILL_SELECTED: f32 = 41.0;
pub const PILL_HOVER: f32 = 19.0;
pub const PILL_UNREAD: f32 = 6.5;
/// The faces a row of the hint shows at most, as the official client's
/// (`UserSummaryItem` with `max: 6`): six fit; with more, five and «+N».
pub const VOICE_SHOWN: usize = 6;
/// The speakers of a stage the hint shows at most (`max: 3`).
pub const STAGE_SHOWN: usize = 3;
/// The hint's widest, as the official client's server hint (196 px of
/// content and its padding).
const HINT_WIDTH: f32 = 220.0;
/// The face of every rail hint's title: semibold, as the official client's.
const HINT_TITLE: FontFamily = FontFamily::SansSemibold;

/// One mark of the rail.
#[derive(Clone, Debug, PartialEq)]
pub struct RailItem {
    /// The action a click reports.
    pub id: String,
    /// The name its hint shows.
    pub name: String,
    /// What the mark says without a picture.
    pub initials: String,
    pub accent: Color,
    /// A glyph in place of the initials.
    pub glyph: Option<Icon>,
    pub image: Option<limn::RasterImage>,
    pub selected: bool,
    pub unread: bool,
    pub mentions: u32,
    /// Its notifications are silenced: its hint says so.
    pub muted: bool,
    pub voice: RailVoice,
}

/// Who is in voice in a server, for its hint and the speaker on its mark.
/// As the official client: none of it from the idle (AFK) channel nor from
/// channels the account muted.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RailVoice {
    /// Who is in its voice channels and does not share a screen nor use an
    /// app there, by channel and then as they are listed in it.
    pub people: Vec<RailFace>,
    /// Who shares a screen there: their own row, as the official client.
    pub streaming: Vec<RailFace>,
    /// The account's own call is there.
    pub own: bool,
    /// Who speaks on its stages: a row of their own, first.
    pub stage: Vec<RailFace>,
    /// How many listen on its stages.
    pub listeners: usize,
    /// Who uses an app launched in one of its voice channels: the last row.
    pub activities: Vec<RailFace>,
}

impl RailVoice {
    /// Everyone in its voice channels, its stages aside: what lights the
    /// speaker on its mark.
    pub fn count(&self) -> usize {
        self.people.len() + self.streaming.len() + self.activities.len()
    }
}

/// Where a mark dragged over another would land.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Placement {
    Before,
    /// In its folder, or a new folder with both (a server only).
    Inside,
    After,
}

impl Placement {
    pub fn key(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::Inside => "inside",
            Self::After => "after",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        [Self::Before, Self::Inside, Self::After]
            .into_iter()
            .find(|placement| placement.key() == key)
    }
}

/// The prefix of the zones a dragged mark lands on: the action continues
/// with the placement and the mark's id (`rail:drop:inside:server:1`).
pub const DROP: &str = "rail:drop:";

/// The rail's marks being dragged.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RailDrag {
    /// Servers and folders can be dragged: the provider keeps their order.
    pub enabled: bool,
    /// The mark being dragged, while one is.
    pub source: Option<String>,
    /// Whether it may land inside another: a server may, a folder not.
    pub inside: bool,
    /// The mark it would land on now, and how.
    pub over: Option<(String, Placement)>,
}

/// Someone in voice, as the hint shows them.
#[derive(Clone, Debug, PartialEq)]
pub struct RailFace {
    pub name: String,
    /// Their picture, once loaded; their initial on `color` until then.
    pub image: Option<limn::RasterImage>,
    pub color: Color,
}

/// A folder of servers on the rail.
#[derive(Clone, Debug, PartialEq)]
pub struct RailFolder {
    /// The action a click reports: it opens or closes it.
    pub id: String,
    /// Its name, or the names of its servers when it has none.
    pub name: String,
    pub color: Color,
    pub open: bool,
    pub servers: Vec<RailItem>,
}

/// One entry of the rail: a server, or a folder of them.
#[derive(Clone, Debug, PartialEq)]
// Servers are the common case and a rail holds a few dozen entries, built
// once per frame: boxing each one would cost more than the folders' slack.
#[allow(clippy::large_enum_variant)]
pub enum RailEntry {
    Server(RailItem),
    Folder(RailFolder),
}

/// The rail: `home` first, a rule, then `entries` and the button that makes
/// a new server, in a column that scrolls when there are more than fit, with
/// the hairline that parts it from the column of channels. Servers and
/// folders are dragged to reorder them, as `drag` says.
pub fn server_rail(
    home: RailItem,
    entries: Vec<RailEntry>,
    drag: &RailDrag,
    look: &Look,
) -> Element {
    let last = entries.last().map(|entry| match entry {
        RailEntry::Server(item) => item.id.clone(),
        RailEntry::Folder(folder) => folder.id.clone(),
    });
    let mut marks = vec![(None, mark(home, None, look)), (None, rule(look))];
    marks.extend(entries.into_iter().map(|entry| match entry {
        RailEntry::Server(item) => (
            Some((item.id.clone(), Placement::Before)),
            mark(item, Some(drag), look),
        ),
        RailEntry::Folder(folder) => (
            Some((folder.id.clone(), Placement::Before)),
            folder_entry(folder, drag, look),
        ),
    }));
    // Above «+», after the last of them.
    marks.push((last.map(|last| (last, Placement::After)), add_server(look)));
    let column = Element::column(vec![
        Element::spacer().size(None, Some(TOP)),
        spaced(marks, drag),
        Element::spacer().size(None, Some(BOTTOM)),
    ]);
    Element::row(vec![
        Element::panel(
            ScrollArea::new("rail", column)
                .appearance(look.scrollbar(false))
                .render()
                .flex(1.0),
        )
        .background(look.rail)
        .flex(1.0),
        Element::panel(Element::spacer())
            .size(Some(1.0), None)
            .background(look.border),
    ])
    .size(Some(WIDTH), None)
}

/// The hairline under home: 10.5 below it, 12 above the first server.
fn rule(look: &Look) -> Element {
    Element::column(vec![
        Element::row(vec![
            Element::spacer().size(Some(26.0), None),
            Element::panel(Element::spacer())
                .size(Some(33.5), Some(1.0))
                .background(look.border),
        ]),
        Element::spacer().size(None, Some(1.5)),
    ])
}

/// `marks` one under another, [`GAP`] apart. While a mark is dragged, the
/// gap over each takes a drop as its landing says: before the mark under it.
fn spaced(marks: Vec<(Option<(String, Placement)>, Element)>, drag: &RailDrag) -> Element {
    let dragging = drag.source.is_some();
    let mut column = Vec::with_capacity(marks.len() * 2);
    for (index, (landing, mark)) in marks.into_iter().enumerate() {
        if index > 0 {
            column.push(match landing.filter(|_| dragging) {
                Some((key, placement)) => Element::clickable(
                    format!("{DROP}{}:{key}:gap", placement.key()),
                    Element::spacer(),
                )
                .size(None, Some(GAP))
                .drop_target(),
                None => Element::spacer().size(None, Some(GAP)),
            });
        }
        column.push(mark);
    }
    Element::column(column)
}

/// The mark and the placement a drop zone of the rail stands for; `None`
/// for any other action.
pub fn drop_zone(id: &str) -> Option<(&str, Placement)> {
    let (placement, key) = id.strip_prefix(DROP)?.split_once(':')?;
    Some((
        key.strip_suffix(":gap").unwrap_or(key),
        Placement::from_key(placement)?,
    ))
}

/// `row`, the band of the mark `key`, while a mark is dragged: the zones to
/// land on it, `placements` from top to bottom, and the ring or the line of
/// where the dragged one would land now. Nothing while none is.
fn landing(
    row: Element,
    key: &str,
    placements: &[Placement],
    drag: &RailDrag,
    look: &Look,
) -> Element {
    if drag.source.is_none() {
        return row;
    }
    let placements: Vec<Placement> = placements
        .iter()
        .copied()
        .filter(|placement| drag.inside || *placement != Placement::Inside)
        .collect();
    let height = MARK / placements.len().max(1) as f32;
    let zones = Element::column(
        placements
            .iter()
            .map(|placement| {
                Element::clickable(
                    format!("{DROP}{}:{key}", placement.key()),
                    Element::spacer(),
                )
                .size(None, Some(height))
                .drop_target()
            })
            .collect(),
    );
    let shown = match drag.over.as_ref().filter(|(over, _)| over == key) {
        Some((_, Placement::Inside)) => Element::row(vec![
            Element::spacer().size(Some(INSET), None),
            Element::panel(Element::spacer())
                .size(Some(MARK), Some(MARK))
                .radius(MARK_RADIUS)
                .border(look.online, 2.0),
            Element::spacer().flex(1.0),
        ]),
        Some((_, placement)) => {
            let bar = Element::row(vec![
                Element::spacer().size(Some(INSET), None),
                Element::panel(Element::spacer())
                    .size(Some(MARK), Some(3.0))
                    .radius(1.5)
                    .background(look.online),
                Element::spacer().flex(1.0),
            ]);
            Element::column(if *placement == Placement::Before {
                vec![bar, Element::spacer().flex(1.0)]
            } else {
                vec![Element::spacer().flex(1.0), bar]
            })
        }
        None => Element::spacer(),
    };
    Element::stack(vec![row, shown, zones]).size(None, Some(MARK))
}

/// The pill against the rail's edge, rounded on the side that shows.
fn pill(height: f32, look: &Look) -> Element {
    let round = (height / 2.0).min(4.0);
    Element::column(vec![
        Element::spacer().flex(1.0),
        Element::row(vec![
            Element::panel(Element::spacer())
                .size(Some(PILL_WIDTH), Some(height))
                .corner_radii(CornerRadii {
                    top_left: 0.0,
                    top_right: round,
                    bottom_right: round,
                    bottom_left: 0.0,
                })
                .background(look.text),
            Element::spacer().flex(1.0),
        ]),
        Element::spacer().flex(1.0),
    ])
}

/// One mark: a rounded square that lights in the accent when chosen or
/// under the pointer, the pill at the rail's edge that says it is chosen
/// (tall), under the pointer (half) or waiting (a dot), and the red count
/// of its mentions over its corner. With `drag`, a server that is dragged
/// and dropped on.
fn mark(item: RailItem, drag: Option<&RailDrag>, look: &Look) -> Element {
    let white = Color::rgb(255, 255, 255);
    let (fill, ink) = if item.selected {
        (item.accent, white)
    } else {
        (look.raised, look.text)
    };
    let face = match item.glyph {
        Some(glyph) => Element::icon(glyph).large_icon(22.0),
        None => Element::text(item.initials.clone())
            .font_family(FontFamily::SansMedium)
            .font_size(16.0),
    }
    .foreground(ink);
    let face = match item.image {
        Some(image) => Element::stack(vec![
            centered(face),
            Element::raster_image(image.cover())
                .size(Some(MARK), Some(MARK))
                .radius(MARK_RADIUS),
        ])
        .size(Some(MARK), Some(MARK)),
        None => face,
    };
    let button = Element::clickable(item.id.clone(), centered(face))
        .size(Some(MARK), Some(MARK))
        .radius(MARK_RADIUS)
        .background(fill)
        .hover_background(item.accent)
        .hover_foreground(white)
        .accessible(
            limn::Access::new(limn::Role::Button)
                .name(match item.voice.count() {
                    0 => item.name.clone(),
                    1 => tr!("{name}, 1 persona en voz", name = item.name),
                    count => tr!("{name}, {count} personas en voz", name = item.name, count),
                })
                .selected(item.selected),
        );
    let button = if drag.is_some_and(|drag| drag.enabled) {
        button.draggable()
    } else {
        button
    };
    // As the official client's: its name, who speaks on a stage and how
    // many listen, who is in voice, who streams, who uses an app, then
    // whether it is silenced.
    let hint = hint(button, item.id.clone(), item.name, look).max_width(HINT_WIDTH);
    let hint = faces(hint, Icon::Users, &item.voice.stage, STAGE_SHOWN);
    let hint = if item.voice.listeners > 0 {
        hint.icon_row(
            t("Escuchando"),
            [TooltipItem::new(item.voice.listeners.to_string()).icon(Icon::Headphones)],
        )
    } else {
        hint
    };
    let hint = faces(hint, Icon::Volume, &item.voice.people, VOICE_SHOWN);
    let hint = faces(hint, Icon::Screen, &item.voice.streaming, VOICE_SHOWN);
    let hint = faces(hint, Icon::Gamepad, &item.voice.activities, VOICE_SHOWN);
    let hinted = if item.muted {
        hint.footer(t("Silenciado"))
    } else {
        hint
    }
    .render();
    let edge = if item.selected {
        pill(PILL_SELECTED, look)
    } else {
        let mut states = vec![pill(PILL_HOVER, look).reveal_on_hover(format!("{}:pill", item.id))];
        if item.unread {
            states.push(pill(PILL_UNREAD, look).conceal_on_hover(format!("{}:dot", item.id)));
        }
        Element::stack(states)
    };
    let marked = Element::row(vec![
        edge.size(Some(INSET), Some(MARK)),
        hinted,
        // Room for the ring of the count, which overhangs the mark.
        Element::spacer().size(Some(3.0), None),
    ])
    .size(None, Some(MARK))
    .hover_region();
    // A muted server lights it only for the account's own call.
    let marked = if item.voice.own || (!item.muted && item.voice.count() > 0) {
        speaker(marked, item.voice.own, look)
    } else {
        marked
    };
    let marked = if item.mentions > 0 {
        mentions(marked, item.mentions, look)
    } else {
        marked
    };
    let row = Element::row(vec![marked, Element::spacer().flex(1.0)]).size(None, Some(MARK));
    match drag {
        Some(drag) => landing(
            row,
            &item.id,
            &[Placement::Before, Placement::Inside, Placement::After],
            drag,
            look,
        ),
        None => row,
    }
}

/// The side of one of the four small faces of a closed folder.
const TILE: f32 = 16.0;

/// A closed folder's face: up to four of its servers, two by two.
fn mosaic(servers: &[RailItem]) -> Element {
    let white = Color::rgb(255, 255, 255);
    let tile = |item: &RailItem| match &item.image {
        Some(image) => Element::raster_image(image.clone().cover())
            .size(Some(TILE), Some(TILE))
            .radius(5.0),
        None => Element::panel(centered(
            Element::text(
                item.initials
                    .chars()
                    .next()
                    .map(String::from)
                    .unwrap_or_default(),
            )
            .font_family(FontFamily::SansMedium)
            .font_size(8.0)
            .foreground(white),
        ))
        .size(Some(TILE), Some(TILE))
        .radius(5.0)
        .background(item.accent),
    };
    let rows: Vec<Element> = servers
        .chunks(2)
        .take(2)
        .map(|pair| Element::row(pair.iter().map(tile).collect()).gap(3.0))
        .collect();
    Element::column(rows).gap(3.0)
}

/// A folder: closed, one mark in its color with its servers in small, that
/// sums what they have waiting; open, its mark and its servers' under it on
/// a band of its color. The pill says what its servers say. It and its
/// servers are dragged and dropped on as `drag` says.
fn folder_entry(folder: RailFolder, drag: &RailDrag, look: &Look) -> Element {
    let selected = folder.servers.iter().any(|item| item.selected);
    let unread = folder.servers.iter().any(|item| item.unread);
    let mentions = folder
        .servers
        .iter()
        .fold(0u32, |sum, item| sum.saturating_add(item.mentions));
    let tint = |alpha: u8| Color::rgba(folder.color.r, folder.color.g, folder.color.b, alpha);
    let face = if folder.open {
        Element::icon(Icon::Folder)
            .large_icon(22.0)
            .foreground(folder.color)
    } else {
        mosaic(&folder.servers)
    };
    let button = Element::clickable(folder.id.clone(), centered(face))
        .size(Some(MARK), Some(MARK))
        .radius(MARK_RADIUS)
        .background(if folder.open { look.raised } else { tint(70) })
        .hover_background(tint(120))
        .accessible(
            limn::Access::new(limn::Role::Button)
                .name(if folder.open {
                    tr!("Carpeta {name}, abierta", name = folder.name)
                } else {
                    tr!("Carpeta {name}, cerrada", name = folder.name)
                })
                .selected(selected && !folder.open),
        );
    let button = if drag.enabled {
        button.draggable()
    } else {
        button
    };
    let hinted = hint(button, folder.id.clone(), folder.name, look).render();
    let edge = if selected && !folder.open {
        pill(PILL_SELECTED, look)
    } else {
        let mut states =
            vec![pill(PILL_HOVER, look).reveal_on_hover(format!("{}:pill", folder.id))];
        if unread && !folder.open {
            states.push(pill(PILL_UNREAD, look).conceal_on_hover(format!("{}:dot", folder.id)));
        }
        Element::stack(states)
    };
    let marked = Element::row(vec![
        edge.size(Some(INSET), Some(MARK)),
        hinted,
        Element::spacer().size(Some(3.0), None),
    ])
    .size(None, Some(MARK))
    .hover_region();
    let marked = if mentions > 0 && !folder.open {
        self::mentions(marked, mentions, look)
    } else {
        marked
    };
    let head = Element::row(vec![marked, Element::spacer().flex(1.0)]).size(None, Some(MARK));
    // Open, what lands after it goes after its last server instead.
    let placements: &[Placement] = if folder.open {
        &[Placement::Before, Placement::Inside]
    } else {
        &[Placement::Before, Placement::Inside, Placement::After]
    };
    let head = landing(head, &folder.id, placements, drag, look);
    if !folder.open {
        return head;
    }
    // A folder lands beside folders, never among the servers of one.
    let among = drag.source.is_none() || drag.inside;
    let mut column = vec![(None, head)];
    column.extend(folder.servers.into_iter().map(|item| {
        (
            among.then(|| (item.id.clone(), Placement::Before)),
            mark(item, among.then_some(drag), look),
        )
    }));
    // The band behind the folder's marks, as wide as a mark and a little more.
    let band = Element::row(vec![
        Element::spacer().size(Some(INSET - 3.0), None),
        Element::panel(Element::spacer())
            .size(Some(MARK + 6.0), None)
            .radius(MARK_RADIUS + 3.0)
            .background(tint(40)),
        Element::spacer().flex(1.0),
    ]);
    Element::stack(vec![band, spaced(column, drag)])
}

/// A hint of the rail, as the official client's: at once, its title at 16
/// px in semibold on the floating black.
fn hint(anchor: Element, id: impl Into<String>, title: impl Into<String>, look: &Look) -> Tooltip {
    Tooltip::new(anchor, id, title, look.menu_palette())
        .placement(TooltipPlacement::Right)
        .delay_ms(0)
        .title_font_size(16.0)
        .title_font_family(HINT_TITLE)
        .background(look.hint())
}

/// `row` with the red count of `count` mentions over the bottom right corner
/// of its mark: 17 px of white on red, ringed 3 px in the rail's color.
fn mentions(row: Element, count: u32, look: &Look) -> Element {
    Badge::new(row, look.palette())
        .count(count)
        .placement(BadgePlacement::BottomRight)
        .height(17.0 + 2.0 * 3.0)
        .outline(look.rail, 3.0)
        .color(look.danger_fill)
        .ink(Color::rgb(255, 255, 255))
        .font_size(12.0)
        .font_family(FontFamily::SansBold)
        .description(tr!("{count} menciones", count))
        .render()
}

/// `hint` with a row of `people`'s faces after `icon`: all of them up to
/// `most`, else one fewer and «+N» (the official client's rule); nothing
/// without people.
fn faces(hint: Tooltip, icon: Icon, people: &[RailFace], most: usize) -> Tooltip {
    let shown = if people.len() <= most {
        people.len()
    } else {
        most - 1
    };
    hint.faces(
        Some(icon),
        people[..shown].iter().map(|person| {
            let face = TooltipFace::new(person.name.clone(), person.color);
            match &person.image {
                Some(image) => face.image(image.clone().cover()),
                None => face,
            }
        }),
        people.len() - shown,
    )
}

/// `row` with a speaker over the top right corner of its mark, ringed in
/// the rail's color: someone is in voice there, green when it is the
/// account's own call.
fn speaker(row: Element, own: bool, look: &Look) -> Element {
    let (fill, ink) = if own {
        (look.online, Color::rgb(255, 255, 255))
    } else {
        (look.raised, look.text)
    };
    let disc = Element::panel(centered(
        Element::icon(Icon::Volume).small_icon(12.0).foreground(ink),
    ))
    .size(Some(18.0), Some(18.0))
    .radius(9.0)
    .background(fill);
    let ringed = Element::panel(disc)
        .padding(2.0)
        .radius(11.0)
        .background(look.rail);
    let corner = Element::column(vec![
        Element::row(vec![Element::spacer().flex(1.0), ringed]),
        Element::spacer().flex(1.0),
    ])
    // Measured as nothing, as the count of mentions.
    .size(Some(0.0), Some(0.0));
    Element::stack(vec![row, corner]).size(None, Some(MARK))
}

/// The "+" after the servers, which makes a new one.
fn add_server(look: &Look) -> Element {
    let id = crate::server_menu::ids::SERVER_OPEN;
    let button = Element::clickable(
        id,
        centered(
            Element::icon(Icon::Add)
                .large_icon(20.0)
                .foreground(look.text),
        ),
    )
    .size(Some(MARK), Some(MARK))
    .radius(MARK_RADIUS)
    .background(look.raised)
    .hover_background(look.accent)
    .hover_foreground(Color::rgb(255, 255, 255))
    .accessibility_label(t("Añadir un servidor"));
    let hinted = hint(button, id, t("Añadir un servidor"), look).render();
    Element::row(vec![
        Element::spacer().size(Some(INSET), Some(MARK)),
        hinted,
        Element::spacer().flex(1.0),
    ])
    .size(None, Some(MARK))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::{Frame, Size, UiEvent};

    fn item(id: &str, selected: bool, mentions: u32) -> RailItem {
        RailItem {
            image: None,
            id: id.into(),
            name: format!("Servidor {id}"),
            initials: "TS".into(),
            accent: Color::rgb(200, 100, 50),
            glyph: None,
            selected,
            unread: mentions > 0,
            mentions,
            muted: false,
            voice: RailVoice::default(),
        }
    }

    fn names(count: usize) -> Vec<RailFace> {
        (1..=count)
            .map(|n| RailFace {
                name: format!("Persona{n}"),
                image: None,
                color: Color::rgb(90, 120, 200),
            })
            .collect()
    }

    /// The faces the hint draws: round and as wide as limn's.
    fn faces_drawn(frame: &Frame) -> Vec<f32> {
        frame
            .commands
            .iter()
            .filter_map(|command| match command {
                limn::DrawCommand::FillRect {
                    rect,
                    radius,
                    color,
                } if rect.width == limn::TOOLTIP_FACE
                    && *radius == limn::TOOLTIP_FACE / 2.0
                    && *color == Color::rgb(90, 120, 200) =>
                {
                    Some(rect.x)
                }
                _ => None,
            })
            .collect()
    }

    /// The rail with `voice` on server:1, hovered there once its hint shows.
    fn hovered(voice: RailVoice) -> Frame {
        let mut server = item("server:1", false, 0);
        server.voice = voice;
        let rail = server_rail(
            item("home", false, 0),
            vec![RailEntry::Server(server)],
            &RailDrag::default(),
            &Look::DARK,
        );
        let mut tree = limn::UiTree::new(Element::row(vec![rail, Element::spacer().flex(1.0)]));
        tree.set_reduced_motion(true);
        tree.set_text_measurer(|text, _, _| text.chars().count() as f32 * 7.0);
        let size = Size::new(600.0, 400.0);
        let frame = tree.layout(size);
        let mark = frame
            .regions
            .iter()
            .find(|region| region.id == "server:1")
            .unwrap()
            .rect;
        tree.handle_event(limn::UiEvent::PointerMove(limn::Point::new(
            mark.x + 10.0,
            mark.y + 10.0,
        )));
        tree.layout_at(
            size,
            std::time::Instant::now() + std::time::Duration::from_secs(2),
        )
    }

    fn icons(frame: &Frame, icon: Icon) -> usize {
        frame
            .commands
            .iter()
            .filter(|command| matches!(command, limn::DrawCommand::Icon { icon: drawn, .. } if *drawn == icon))
            .count()
    }

    fn fills(frame: &Frame, color: Color) -> usize {
        frame
            .commands
            .iter()
            .filter(|command| {
                matches!(command, limn::DrawCommand::FillRect { color: fill, rect, .. }
                    if *fill == color && rect.width == 18.0)
            })
            .count()
    }

    #[test]
    fn the_hint_shows_the_faces_of_who_is_in_voice_and_who_streams() {
        let frame = hovered(RailVoice {
            people: names(7),
            streaming: names(1),
            ..RailVoice::default()
        });
        let painted = texts(&frame);
        assert!(painted.contains(&"Servidor server:1".to_owned()));
        // More than six: five faces and «+2», as the official client; the
        // one who streams on the screen's row.
        assert_eq!(faces_drawn(&frame).len(), 5 + 1);
        assert!(painted.contains(&"+2".to_owned()), "{painted:?}");
        assert!(
            painted.contains(&"P".to_owned()),
            "initials while no picture"
        );
        // The speaker on the mark and the one of the row; the screen's row.
        assert_eq!(icons(&frame, Icon::Volume), 2);
        assert_eq!(icons(&frame, Icon::Screen), 1);
        let name = frame
            .access
            .iter()
            .find(|node| node.id == "server:1")
            .unwrap()
            .name
            .clone();
        assert_eq!(name, "Servidor server:1, 8 personas en voz");
    }

    #[test]
    fn the_hint_names_the_server_in_semibold_as_the_official_client() {
        let frame = hovered(RailVoice::default());
        let face = frame.commands.iter().find_map(|command| match command {
            limn::DrawCommand::Text {
                text, font_family, ..
            } if text == "Servidor server:1" => Some(*font_family),
            _ => None,
        });
        assert_eq!(face, Some(FontFamily::SansSemibold));
    }

    #[test]
    fn only_rows_with_people_and_exactly_six_need_no_count() {
        let frame = hovered(RailVoice {
            people: names(6),
            ..RailVoice::default()
        });
        assert_eq!(faces_drawn(&frame).len(), 6);
        let painted = texts(&frame);
        assert!(
            !painted.iter().any(|text| text.starts_with('+')),
            "{painted:?}"
        );
        assert_eq!(icons(&frame, Icon::Screen), 0, "nobody streams: no row");
        let quiet = hovered(RailVoice::default());
        assert_eq!(
            icons(&quiet, Icon::Volume),
            0,
            "nobody in voice: no row, no speaker"
        );
        assert_eq!(
            quiet
                .access
                .iter()
                .find(|node| node.id == "server:1")
                .unwrap()
                .name,
            "Servidor server:1"
        );
        let one = hovered(RailVoice {
            people: names(1),
            ..RailVoice::default()
        });
        assert!(one
            .access
            .iter()
            .any(|node| node.name == "Servidor server:1, 1 persona en voz"));
    }

    #[test]
    fn the_speaker_is_green_on_the_own_call_and_keeps_the_mentions() {
        let rail = |voice: RailVoice, mentions| {
            let mut server = item("server:1", false, mentions);
            server.voice = voice;
            frame(
                Element::row(vec![
                    server_rail(
                        item("home", false, 0),
                        vec![RailEntry::Server(server)],
                        &RailDrag::default(),
                        &Look::DARK,
                    ),
                    Element::spacer().flex(1.0),
                ]),
                Size::new(400.0, 300.0),
            )
            .1
        };
        let others = rail(
            RailVoice {
                people: names(2),
                ..RailVoice::default()
            },
            3,
        );
        assert_eq!(fills(&others, Look::DARK.raised), 1);
        assert_eq!(fills(&others, Look::DARK.online), 0);
        assert!(texts(&others).contains(&"3".to_owned()), "the count stays");
        // Joining: the account's call marks it before anyone is listed.
        let own = rail(
            RailVoice {
                own: true,
                ..RailVoice::default()
            },
            0,
        );
        assert_eq!(fills(&own, Look::DARK.online), 1);
        let mark = own
            .regions
            .iter()
            .find(|region| region.id == "server:1")
            .unwrap()
            .rect;
        let badge = own
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::FillRect { color, rect, .. } if *color == Look::DARK.online => {
                    Some(*rect)
                }
                _ => None,
            })
            .unwrap();
        assert!(
            badge.y < mark.y + mark.height / 2.0 && badge.x + badge.width > mark.x + mark.width,
            "over the top right corner: {badge:?} {mark:?}"
        );
        assert_eq!(fills(&rail(RailVoice::default(), 0), Look::DARK.raised), 0);
    }

    #[test]
    fn every_server_is_a_control_in_order() {
        let rail = server_rail(
            item("home", false, 0),
            vec![
                RailEntry::Server(item("server:1", true, 0)),
                RailEntry::Server(item("server:2", false, 3)),
            ],
            &RailDrag::default(),
            &Look::DARK,
        );
        let (_, frame) = frame(
            Element::row(vec![rail, Element::spacer().flex(1.0)]),
            Size::new(400.0, 500.0),
        );
        let ids: Vec<String> = controls(&frame)
            .into_iter()
            .filter(|id| id == "home" || id.starts_with("server:"))
            .collect();
        assert_eq!(ids, vec!["home", "server:1", "server:2"]);
        assert!(
            controls(&frame).contains(&crate::server_menu::ids::SERVER_OPEN.to_owned()),
            "a new server can be made from the rail"
        );
    }

    #[test]
    fn a_closed_folder_sums_its_servers_and_an_open_one_lists_them() {
        let folder = |open| RailFolder {
            id: "folder:7".into(),
            name: "Juegos".into(),
            color: Color::rgb(88, 101, 242),
            open,
            servers: vec![item("server:1", false, 2), item("server:2", false, 3)],
        };
        let shown = |open| {
            let rail = server_rail(
                item("home", false, 0),
                vec![RailEntry::Folder(folder(open))],
                &RailDrag::default(),
                &Look::DARK,
            );
            frame(
                Element::row(vec![rail, Element::spacer().flex(1.0)]),
                Size::new(400.0, 500.0),
            )
            .1
        };
        let closed = shown(false);
        let ids = controls(&closed);
        assert!(ids.contains(&"folder:7".to_owned()));
        assert!(
            !ids.contains(&"server:1".to_owned()),
            "closed, its servers hide"
        );
        assert!(
            texts(&closed).contains(&"5".to_owned()),
            "it sums the mentions"
        );
        assert!(closed
            .access
            .iter()
            .any(|node| node.name == "Carpeta Juegos, cerrada"));
        let open = shown(true);
        let ids: Vec<String> = controls(&open)
            .into_iter()
            .filter(|id| id.starts_with("folder:") || id.starts_with("server:"))
            .collect();
        assert_eq!(ids, ["folder:7", "server:1", "server:2"]);
        assert!(
            !texts(&open).contains(&"5".to_owned()),
            "each server shows its own"
        );
    }

    #[test]
    fn the_hint_shows_stages_and_apps_and_a_muted_server_keeps_its_speaker_off() {
        let hint = hovered(RailVoice {
            stage: names(4),
            listeners: 12,
            activities: names(2),
            ..RailVoice::default()
        });
        let painted = texts(&hint);
        // Three at most on a stage: two and «+2»; the two in an app.
        assert_eq!(faces_drawn(&hint).len(), 2 + 2, "{painted:?}");
        assert!(painted.contains(&"+2".to_owned()), "{painted:?}");
        assert!(painted.contains(&"Escuchando".to_owned()), "{painted:?}");
        assert!(painted.contains(&"12".to_owned()), "{painted:?}");
        assert_eq!(icons(&hint, Icon::Users), 1);
        assert_eq!(icons(&hint, Icon::Headphones), 1);
        assert_eq!(icons(&hint, Icon::Gamepad), 1, "the apps' row");
        // A stage alone does not light the speaker; an app does.
        assert_eq!(icons(&hint, Icon::Volume), 1, "only the speaker");
        let rail = |voice: RailVoice, muted: bool| {
            let mut server = item("server:1", false, 0);
            server.voice = voice;
            server.muted = muted;
            frame(
                Element::row(vec![
                    server_rail(
                        item("home", false, 0),
                        vec![RailEntry::Server(server)],
                        &RailDrag::default(),
                        &Look::DARK,
                    ),
                    Element::spacer().flex(1.0),
                ]),
                Size::new(400.0, 300.0),
            )
            .1
        };
        let talking = RailVoice {
            people: names(2),
            ..RailVoice::default()
        };
        assert_eq!(fills(&rail(talking.clone(), false), Look::DARK.raised), 1);
        assert_eq!(fills(&rail(talking, true), Look::DARK.raised), 0, "muted");
        let own = RailVoice {
            own: true,
            ..RailVoice::default()
        };
        assert_eq!(
            fills(&rail(own, true), Look::DARK.online),
            1,
            "its own call"
        );
    }

    /// The rail with two servers and a closed folder while `source` is
    /// dragged (`None`: before any is).
    fn dragged_rail(source: Option<&str>, over: Option<(&str, Placement)>) -> Element {
        let folder = RailFolder {
            id: "folder:7".into(),
            name: "Juegos".into(),
            color: Color::rgb(88, 101, 242),
            open: false,
            servers: vec![item("server:3", false, 0)],
        };
        let drag = RailDrag {
            enabled: true,
            source: source.map(str::to_owned),
            inside: !source.is_some_and(|source| source.starts_with("folder:")),
            over: over.map(|(key, placement)| (key.to_owned(), placement)),
        };
        let rail = server_rail(
            item("home", false, 0),
            vec![
                RailEntry::Server(item("server:1", false, 0)),
                RailEntry::Server(item("server:2", false, 0)),
                RailEntry::Folder(folder),
            ],
            &drag,
            &Look::DARK,
        );
        Element::row(vec![rail, Element::spacer().flex(1.0)])
    }

    fn dragging(source: &str, over: Option<(&str, Placement)>) -> (limn::UiTree, Frame) {
        let mut tree = limn::UiTree::new(dragged_rail(Some(source), over));
        tree.set_reduced_motion(true);
        tree.set_text_measurer(|text, _, _| text.chars().count() as f32 * 7.0);
        let frame = tree.layout(Size::new(400.0, 500.0));
        (tree, frame)
    }

    fn rect_of(frame: &Frame, id: &str) -> limn::Rect {
        frame
            .regions
            .iter()
            .find(|region| region.id == id)
            .unwrap_or_else(|| panic!("{id}"))
            .rect
    }

    #[test]
    fn a_dragged_mark_lands_before_inside_or_after_another_and_in_the_gaps() {
        // The press is on the mark; once the drag starts the window draws
        // the zones, and the drag goes on over them.
        let mut tree = limn::UiTree::new(dragged_rail(None, None));
        tree.set_reduced_motion(true);
        tree.set_text_measurer(|text, _, _| text.chars().count() as f32 * 7.0);
        let size = Size::new(400.0, 500.0);
        let quiet = tree.layout(size);
        assert!(!quiet
            .regions
            .iter()
            .any(|region| region.id.starts_with(DROP)));
        let one = rect_of(&quiet, "server:1");
        let at = |rect: limn::Rect, y: f32| limn::Point::new(rect.x + rect.width / 2.0, y);
        tree.handle_event(UiEvent::PointerDown(at(one, one.y + 20.0)));
        let started = tree.handle_event(UiEvent::PointerMove(at(one, one.y + 30.0)));
        assert!(
            matches!(&started[0], limn::UiAction::DragStarted { id, .. } if id == "server:1"),
            "{started:?}"
        );
        tree.set_root(dragged_rail(Some("server:1"), None));
        let frame = tree.layout(size);
        let two = rect_of(&frame, "server:2");
        let over = |tree: &mut limn::UiTree, point| {
            tree.handle_event(UiEvent::PointerMove(point))
                .into_iter()
                .find_map(|action| match action {
                    limn::UiAction::DragMoved { over, .. } => Some(over),
                    _ => None,
                })
                .flatten()
        };
        let zone = |tree: &mut limn::UiTree, point| {
            over(tree, point).map(|id| {
                let (key, placement) = drop_zone(&id).unwrap();
                (key.to_owned(), placement)
            })
        };
        let inside = zone(&mut tree, at(two, two.y + MARK / 2.0));
        assert_eq!(inside, Some(("server:2".into(), Placement::Inside)));
        let before = zone(&mut tree, at(two, two.y + 3.0));
        assert_eq!(before, Some(("server:2".into(), Placement::Before)));
        let after = zone(&mut tree, at(two, two.y + MARK - 3.0));
        assert_eq!(after, Some(("server:2".into(), Placement::After)));
        // Between two marks: before the one under it.
        let gap = zone(&mut tree, at(two, two.y - GAP / 2.0));
        assert_eq!(gap, Some(("server:2".into(), Placement::Before)));
        // Above «+», after the last entry.
        let add = rect_of(&frame, crate::server_menu::ids::SERVER_OPEN);
        let end = zone(&mut tree, at(add, add.y - GAP / 2.0));
        assert_eq!(end, Some(("folder:7".into(), Placement::After)));
        assert_eq!(drop_zone("server:2"), None);
        // A folder lands only before or after: halves, nothing inside.
        let (_, frame) = dragging("folder:7", None);
        let zones: Vec<String> = frame
            .regions
            .iter()
            .filter(|region| region.id.starts_with(DROP) && region.id.ends_with(":server:2"))
            .map(|region| region.id.clone())
            .collect();
        assert_eq!(
            zones,
            ["rail:drop:before:server:2", "rail:drop:after:server:2"]
        );
    }

    #[test]
    fn a_dragged_folder_never_lands_among_the_servers_of_an_open_one() {
        let zones = |source: &str| {
            let open = RailFolder {
                id: "folder:7".into(),
                name: "Juegos".into(),
                color: Color::rgb(88, 101, 242),
                open: true,
                servers: vec![item("server:3", false, 0), item("server:4", false, 0)],
            };
            let drag = RailDrag {
                enabled: true,
                source: Some(source.into()),
                inside: !source.starts_with("folder:"),
                over: None,
            };
            let rail = server_rail(
                item("home", false, 0),
                vec![
                    RailEntry::Folder(RailFolder {
                        id: "folder:8".into(),
                        open: false,
                        ..open.clone()
                    }),
                    RailEntry::Folder(open),
                ],
                &drag,
                &Look::DARK,
            );
            let mut tree = limn::UiTree::new(rail);
            tree.set_text_measurer(|text, _, _| text.chars().count() as f32 * 7.0);
            tree.layout(Size::new(400.0, 500.0))
                .regions
                .iter()
                .filter(|region| region.id.starts_with(DROP) && region.id.contains(":server:"))
                .count()
        };
        assert_eq!(zones("folder:8"), 0);
        assert!(zones("server:1") > 0);
    }

    #[test]
    fn where_it_would_land_shows_as_a_ring_or_a_line() {
        let ring = |frame: &Frame| {
            frame
                .commands
                .iter()
                .filter(|command| matches!(command, limn::DrawCommand::StrokeRect { color, .. } if *color == Look::DARK.online))
                .count()
        };
        let line = |frame: &Frame| {
            frame
                .commands
                .iter()
                .filter_map(|command| match command {
                    limn::DrawCommand::FillRect { color, rect, .. }
                        if *color == Look::DARK.online && rect.height == 3.0 =>
                    {
                        Some(*rect)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        let (_, none) = dragging("server:1", None);
        assert_eq!((ring(&none), line(&none).len()), (0, 0));
        let (_, inside) = dragging("server:1", Some(("server:2", Placement::Inside)));
        assert_eq!((ring(&inside), line(&inside).len()), (1, 0));
        let (_, after) = dragging("server:1", Some(("server:2", Placement::After)));
        let two = rect_of(&after, "server:2");
        let drawn = line(&after);
        assert_eq!(drawn.len(), 1);
        assert!(drawn[0].y > two.y + MARK / 2.0, "{drawn:?} {two:?}");
    }
}
