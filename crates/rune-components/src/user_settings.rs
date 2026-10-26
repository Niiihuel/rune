//! The account's settings: a large panel floating over the dimmed window,
//! with a sidebar of pages (and, under the open one, links to its sections
//! that follow the scroll), a search box, and the page's column of settings.
//! Pure functions of props like the rest of the crate; `rune-ui` keeps the
//! preferences and turns the actions into intents.
//!
//! Measures follow the reference captures at 1361×797 logical pixels: a
//! panel 43.5 px in from every side, a 269 px sidebar of 235×42 items 47 px
//! apart, a 51.5 px header over the page, and a 743 px column of settings
//! centered in what is left.

use limn::{
    Button, ButtonVariant, Color, Component, ControlSize, Element, FontFamily, Icon, LevelBars,
    RadioGroup, RadioOption, RadioVariant, ScrollArea, ScrollbarAppearance, SearchField, Separator,
    Size, Slider, Switch, SwitchVariant, Tooltip,
};
use rune_model::{
    ChannelId, Delivery, Message, MessageId, MessageKind, Reaction, Timestamp, UserId,
};

use rune_i18n::t;

use crate::message::{message_row, Abilities, Author, MessageStyle, MessageView};
use crate::{avatar, gap, icon, line, strong_line, text, with_status, Look};

/// A person, for the account's page.
pub const PERSON: &str = crate::glyphs::glyph!("person");
/// A shield with a check, for the page of data and privacy.
pub const SHIELD: &str = crate::glyphs::glyph!("shield");

/// Room between the panel and the window, on every side.
pub const MARGIN: f32 = 43.5;
/// The sidebar, and its items.
pub const SIDEBAR_WIDTH: f32 = 269.0;
const ITEM_INSET: f32 = 17.0;
pub const ITEM_WIDTH: f32 = 235.0;
pub const ITEM_HEIGHT: f32 = 36.0;
const ITEM_GAP: f32 = 5.0;
const ANCHOR_HEIGHT: f32 = 32.0;
/// The bar over a page, with its title and the close button.
pub const HEADER_HEIGHT: f32 = 51.5;
/// The column of settings, centered in the page.
pub const COLUMN_WIDTH: f32 = 743.0;
const COLUMN_TOP: f32 = 72.0;
const COLUMN_BOTTOM: f32 = 96.0;
/// Room over and under the line between two sections.
const SECTION_SPACE: f32 = 24.0;
/// Least room kept at each side of a narrow window, and of a narrow column.
const EDGE: f32 = 16.0;
const SIDE_ROOM: f32 = 32.0;
/// How tall the preview over the accessibility settings is.
pub const PREVIEW_HEIGHT: f32 = 199.0;

/// Where the panel and its column go in a window this big.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    pub panel: Size,
    /// The column of settings.
    pub column: f32,
}

impl Geometry {
    /// The panel keeps [`MARGIN`] from the window's edges, less in a small
    /// window; the column takes what the sidebar leaves, up to [`COLUMN_WIDTH`].
    pub fn new(window: Size) -> Self {
        let margin = |side: f32, roomy: f32| if side >= roomy { MARGIN } else { EDGE };
        let panel = Size::new(
            (window.width - 2.0 * margin(window.width, 960.0)).max(320.0),
            (window.height - 2.0 * margin(window.height, 640.0)).max(320.0),
        );
        let room = panel.width - SIDEBAR_WIDTH - 1.0 - 2.0 * SIDE_ROOM;
        Self {
            panel,
            column: room.clamp(200.0, COLUMN_WIDTH),
        }
    }
}

/// Who the settings are of, over the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub struct Account {
    pub name: String,
    pub image: Option<limn::RasterImage>,
    pub accent: Color,
    /// The link that opens the profile editor; `None` when there is none.
    pub edit: Option<String>,
}

/// One line of the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub enum Nav {
    /// A group's title, as written.
    Heading(String),
    Separator,
    Page(NavPage),
}

/// A page of the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub struct NavPage {
    pub id: String,
    pub label: String,
    pub icon: Icon,
    pub selected: bool,
    /// The links to its sections, shown while it is open.
    pub anchors: Vec<NavAnchor>,
}

/// A link to a section of the open page.
#[derive(Clone, Debug, PartialEq)]
pub struct NavAnchor {
    pub id: String,
    pub label: String,
    /// The section being read.
    pub active: bool,
}

/// The search box over the sidebar: a field and, while it holds something,
/// a button that empties it (`{id}:clear`).
pub fn search_box(id: &str, value: &str, look: &Look) -> Element {
    SearchField::new(id, value, t("Buscar"), look.palette())
        .accessibility_label(t("Buscar en los ajustes"))
        .size(ControlSize::Spacious)
        .width(ITEM_WIDTH)
        .clearable(true)
        .render()
}

/// The sidebar: who the settings are of, the search box, and the pages,
/// scrolling under the first two, with a quiet line at its end.
pub fn sidebar(
    account: &Account,
    search: Element,
    nav: &[Nav],
    footnote: &str,
    look: &Look,
) -> Element {
    let head = Element::row(vec![gap(ITEM_INSET, 0.0), account_row(account, look)]);
    let mut rows = vec![gap(0.0, 13.0)];
    for entry in nav {
        match entry {
            // Capitals in small bold: a group's name, the first one at the top.
            Nav::Heading(title) => rows.push(Element::column(vec![
                gap(0.0, if rows.len() > 1 { 12.0 } else { 0.0 }),
                Element::row(vec![
                    gap(16.0, 0.0),
                    line(title.to_uppercase(), 12.0, look.muted)
                        .font_family(FontFamily::SansBold)
                        .flex(1.0)
                        .centered_y(),
                ])
                .size(None, Some(18.0)),
                gap(0.0, 6.0),
            ])),
            Nav::Separator => rows.push(Element::column(vec![
                gap(0.0, 6.0),
                Element::row(vec![
                    gap(ITEM_INSET, 0.0),
                    Element::panel(Element::spacer())
                        .size(Some(ITEM_WIDTH), Some(1.0))
                        .background(look.border),
                ]),
                gap(0.0, 6.0),
            ])),
            Nav::Page(page) => {
                rows.push(nav_item(page, look));
                if page.selected {
                    for anchor in &page.anchors {
                        rows.push(nav_anchor(anchor, look));
                    }
                }
            }
        }
    }
    rows.push(gap(0.0, 20.0));
    rows.push(Element::row(vec![
        gap(ITEM_INSET, 0.0),
        text(footnote, 11.0, look.muted).size(Some(ITEM_WIDTH), None),
    ]));
    rows.push(gap(0.0, 24.0));
    Element::column(vec![
        gap(0.0, 28.5),
        head,
        gap(0.0, 14.5),
        Element::row(vec![gap(ITEM_INSET, 0.0), search]),
        ScrollArea::new("us:nav", Element::column(rows).gap(ITEM_GAP))
            .appearance(ScrollbarAppearance::overlay().buttons(false))
            .render()
            .flex(1.0),
    ])
    .size(Some(SIDEBAR_WIDTH), None)
}

/// Who the settings are of, as a row of the menu: as wide as «Cuenta», with
/// its radius and its hover, the avatar and both lines centered on each
/// other; the whole row opens the profile editor when there is one.
fn account_row(account: &Account, look: &Look) -> Element {
    const AVATAR: f32 = 40.0;
    let mut who = vec![strong_line(&account.name, 16.0, look.text)];
    if account.edit.is_some() {
        who.push(Element::row(vec![
            line(t("Editar perfil"), 14.0, look.muted)
                .shrink_to_fit()
                .centered_y(),
            gap(6.0, 0.0),
            icon(Icon::Edit, look.muted)
                .size(Some(14.0), Some(14.0))
                .centered_y(),
        ]));
    }
    let content = Element::row(vec![
        gap(8.0, 0.0),
        crate::avatar_with_image(&account.name, account.accent, AVATAR, account.image.clone())
            .centered_y(),
        gap(12.0, 0.0),
        Element::column(who).gap(2.0).flex(1.0).centered_y(),
        gap(8.0, 0.0),
    ]);
    let size = (Some(ITEM_WIDTH), Some(AVATAR + 16.0));
    match &account.edit {
        Some(edit) => Element::clickable(edit, content)
            .size(size.0, size.1)
            .radius(8.0)
            .hover_background(look.hover)
            .hover_foreground(look.text)
            .accessible(limn::Access::new(limn::Role::Button).name(t("Editar perfil"))),
        None => content.size(size.0, size.1),
    }
}

fn nav_item(page: &NavPage, look: &Look) -> Element {
    let ink = if page.selected { look.text } else { look.muted };
    let item = Element::clickable(
        &page.id,
        Element::row(vec![
            gap(11.5, 0.0),
            icon(page.icon, ink)
                .size(Some(20.0), Some(20.0))
                .centered_y(),
            gap(11.0, 0.0),
            line(page.label.clone(), 16.0, ink)
                .font_family(FontFamily::SansMedium)
                .flex(1.0)
                .centered_y(),
            gap(8.0, 0.0),
        ]),
    )
    .size(Some(ITEM_WIDTH), Some(ITEM_HEIGHT))
    .radius(6.0)
    .hover_background(look.hover)
    .hover_foreground(look.text)
    .accessible(
        limn::Access::new(limn::Role::Tab)
            .name(page.label.clone())
            .selected(page.selected),
    );
    let item = if page.selected {
        item.background(look.selected)
    } else {
        item
    };
    Element::row(vec![gap(ITEM_INSET, 0.0), item])
}

/// A link to a section, on the guide that runs down the open page's links.
fn nav_anchor(anchor: &NavAnchor, look: &Look) -> Element {
    let (ink, mark) = if anchor.active {
        (look.text, look.accent)
    } else {
        (look.muted, look.border)
    };
    // Two lines at most, never cut: a long title wraps under the guide.
    Element::row(vec![
        gap(ITEM_INSET + 21.5, 0.0),
        Element::panel(Element::spacer())
            .size(Some(1.0), Some(ANCHOR_HEIGHT + ITEM_GAP))
            .background(mark),
        gap(19.0, 0.0),
        Element::clickable(
            &anchor.id,
            text(anchor.label.clone(), 14.0, ink).centered_y(),
        )
        .size(Some(ITEM_WIDTH - 42.0), None)
        .hover_foreground(look.text)
        .accessible(
            limn::Access::new(limn::Role::Link)
                .name(anchor.label.clone())
                .selected(anchor.active),
        ),
    ])
    .min_size(0.0, ANCHOR_HEIGHT)
}

/// The panel: the sidebar, a hairline, and the page under its header with
/// the title and the close button.
pub fn panel(
    geometry: Geometry,
    sidebar: Element,
    title: &str,
    close: &str,
    page: Element,
    look: &Look,
) -> Element {
    let close = Element::clickable(
        close,
        crate::centered(icon(Icon::Close, look.muted).size(Some(16.0), Some(16.0))),
    )
    .size(Some(34.0), Some(34.0))
    .radius(6.0)
    .hover_background(look.selected)
    .hover_foreground(look.text)
    .accessible(limn::Access::new(limn::Role::Button).name(t("Cerrar (Esc)")));
    let header = Element::row(vec![
        gap(18.0, 0.0),
        line(title, 16.0, look.text)
            .font_family(FontFamily::SansBold)
            .flex(1.0)
            .centered_y(),
        close.centered_y(),
        gap(10.0, 0.0),
    ])
    .size(None, Some(HEADER_HEIGHT));
    let hairline = |width: Option<f32>, height: Option<f32>| {
        Element::panel(Element::spacer())
            .size(width, height)
            .background(look.border)
    };
    Element::row(vec![
        sidebar,
        hairline(Some(1.0), None),
        Element::column(vec![header, hairline(None, Some(1.0)), page.flex(1.0)]).flex(1.0),
    ])
    .size(Some(geometry.panel.width), Some(geometry.panel.height))
    .background(look.canvas)
    .border(look.border, 1.0)
    .radius(8.0)
    .clip_children()
    .accessible(limn::Access::new(limn::Role::Dialog).name(t("Ajustes de usuario")))
}

/// A page's column: `sticky` pinned over it while it scrolls, then its
/// sections with a line between two, centered in the page. `top` is an
/// anchor at its very top.
pub fn page(
    scroll_id: &str,
    top: &str,
    geometry: Geometry,
    sticky: Option<Element>,
    sections: Vec<Element>,
    look: &Look,
) -> Element {
    let mut column = vec![gap(0.0, COLUMN_TOP).anchor(top)];
    if let Some(sticky) = sticky {
        column.push(sticky.sticky_top().background(look.canvas));
    }
    for (index, section) in sections.into_iter().enumerate() {
        if index > 0 {
            column.push(divider(look));
        }
        column.push(section);
    }
    column.push(gap(0.0, COLUMN_BOTTOM));
    ScrollArea::new(
        scroll_id,
        Element::row(vec![
            Element::spacer().flex(1.0),
            Element::column(column).size(Some(geometry.column), None),
            Element::spacer().flex(1.0),
        ]),
    )
    .appearance(ScrollbarAppearance::overlay().buttons(false))
    .render()
}

/// The line between two sections.
fn divider(look: &Look) -> Element {
    Element::column(vec![
        gap(0.0, SECTION_SPACE),
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border),
        gap(0.0, SECTION_SPACE),
    ])
}

/// A section: its title, 20 px bold on one line, with an action beside it
/// when there is one, and its settings with a hairline between each two.
/// `anchor` is where its link in the sidebar jumps.
pub fn section(
    anchor: &str,
    title: &str,
    trailing: Option<Element>,
    rows: Vec<Element>,
    look: &Look,
) -> Element {
    let mut head = vec![strong_line(title, 20.0, look.text)
        .font_family(FontFamily::SansBold)
        .flex(1.0)
        .centered_y()];
    if let Some(trailing) = trailing {
        head.push(trailing.centered_y());
    }
    let mut column = vec![
        Element::row(head).gap(16.0).min_size(0.0, 32.0),
        gap(0.0, 12.0),
    ];
    column.extend(with_rules(rows, look));
    Element::column(column).anchor(anchor)
}

/// The rows of a section, with a hairline between each two and none before
/// the first or after the last.
fn with_rules(rows: Vec<Element>, look: &Look) -> Vec<Element> {
    let mut out = Vec::with_capacity(rows.len() * 2);
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            out.push(Separator::horizontal(look.palette()).render());
        }
        out.push(row);
    }
    out
}

/// Rows that stand one under the other, as a group of their own, with the
/// same hairlines as a section's rows.
pub fn rows(rows: Vec<Element>, look: &Look) -> Element {
    Element::column(with_rules(rows, look))
}

/// A title inside a section, over the settings it groups.
pub fn subtitle(title: &str, look: &Look) -> Element {
    Element::column(vec![
        gap(0.0, 24.0),
        strong_line(title, 16.0, look.text).font_family(FontFamily::SansBold),
        gap(0.0, 8.0),
    ])
}

/// A setting: its name, a note under it, and its control at the right,
/// centered with the name. The text takes every width the control leaves.
/// A setting that cannot be changed is dimmed.
pub fn setting(
    label: &str,
    note: Option<&str>,
    control: Option<Element>,
    enabled: bool,
    look: &Look,
) -> Element {
    let (ink, quiet) = if enabled {
        (look.text, look.muted)
    } else {
        (look.muted, look.faint)
    };
    let mut copy = vec![text(label, 16.0, ink).font_family(FontFamily::SansMedium)];
    if let Some(note) = note {
        copy.push(text(note, 14.0, quiet));
    }
    let mut row = vec![Element::column(copy).gap(4.0).flex(1.0).centered_y()];
    if let Some(control) = control {
        row.push(control.centered_y());
    }
    Element::column(vec![
        gap(0.0, 16.0),
        Element::row(row).gap(32.0),
        gap(0.0, 16.0),
    ])
}

/// A setting that is on or off, a light thumb on the accent. One that
/// cannot be changed says why. The switch is the reference's 50×26.
pub fn toggle(
    id: &str,
    label: &str,
    note: Option<&str>,
    on: bool,
    denied: Option<&str>,
    look: &Look,
) -> Element {
    let switch = Switch::new(id, on, look.palette())
        .track_size(50.0, 26.0)
        .variant(SwitchVariant::Accent)
        .ink(look.accent)
        .accessibility_label(label)
        .enabled(denied.is_none())
        .render();
    let note = denied.or(note);
    setting(label, note, Some(switch), denied.is_none(), look)
}

/// One choice among a few, one under the other, each with its note.
pub fn choices(
    id: &str,
    label: &str,
    options: &[(String, &str, Option<&str>)],
    selected: &str,
    look: &Look,
) -> Element {
    let options = options
        .iter()
        .map(|(value, name, note)| {
            let option = RadioOption::new(value.clone(), *name);
            match note {
                // A note that only repeats the name is left out.
                Some(note) if note != name => option.description(*note),
                _ => option,
            }
        })
        .collect();
    RadioGroup::new(id, options, Some(selected), look.palette())
        .variant(RadioVariant::Card)
        .size(ControlSize::Spacious)
        .accessibility_label(label)
        .render()
}

/// A slider that stops at marks, each named over it, the usual one in the
/// accent: `labels[index]` is chosen and `labels[usual]` is the default.
pub fn ticked_slider(
    id: &str,
    label: &str,
    labels: &[&str],
    index: usize,
    usual: usize,
    width: f32,
    look: &Look,
) -> Element {
    let last = labels.len().saturating_sub(1).max(1) as f32;
    Slider::new(id, index as f32 / last, look.palette())
        .steps(labels.len())
        .width(width)
        .ink(look.accent)
        .mark_labels(labels.iter().copied())
        .default_stop(usual)
        .accessibility_label(label)
        .render()
}

/// Which mark of a [`ticked_slider`] of `marks` a fraction falls on.
pub fn mark_at(fraction: f32, marks: usize) -> usize {
    let last = marks.saturating_sub(1);
    ((fraction.clamp(0.0, 1.0) * last as f32).round() as usize).min(last)
}

/// A look the account can choose, as a square of its ground.
#[derive(Clone, Debug, PartialEq)]
pub struct Swatch {
    pub id: String,
    pub name: String,
    pub fill: Color,
    pub selected: bool,
}

/// The looks to choose from: 48 px squares 60 px apart, the chosen one
/// ringed in the accent with a check in its corner, each named on hover.
pub fn theme_swatches(swatches: &[Swatch], look: &Look) -> Element {
    let squares = swatches
        .iter()
        .map(|swatch| {
            let (edge, width) = if swatch.selected {
                (look.accent, 3.0)
            } else {
                (look.border, 1.0)
            };
            let square = Element::clickable(&swatch.id, Element::spacer())
                .size(Some(48.0), Some(48.0))
                .radius(8.0)
                .background(swatch.fill)
                .border(edge, width)
                .accessible(
                    limn::Access::new(limn::Role::RadioButton)
                        .name(swatch.name.clone())
                        .toggled(swatch.selected),
                );
            let square = if swatch.selected {
                let check = Element::panel(crate::centered(
                    icon(Icon::Check, look.on_accent).size(Some(12.0), Some(12.0)),
                ))
                .size(Some(18.0), Some(18.0))
                .radius(9.0)
                .background(look.accent);
                Element::stack(vec![
                    square,
                    Element::column(vec![
                        Element::row(vec![Element::spacer().flex(1.0), check, gap(3.0, 0.0)]),
                        Element::spacer().flex(1.0),
                    ])
                    .padding(3.0),
                ])
                .size(Some(48.0), Some(48.0))
            } else {
                square
            };
            Tooltip::new(
                square,
                swatch.id.clone(),
                swatch.name.clone(),
                look.menu_palette(),
            )
            .render()
        })
        .collect();
    Element::row(squares).gap(12.0)
}

/// Keys of a shortcut, each alternative a combination of keys pressed together.
#[derive(Clone, Debug, PartialEq)]
pub struct Shortcut {
    pub action: String,
    pub keys: Vec<Vec<String>>,
}

/// A list of shortcuts: one row each, what it does at the left and its keys
/// at the right, alternatives one under the other.
pub fn shortcuts(rows: &[Shortcut], look: &Look) -> Element {
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            lines.push(
                Element::panel(Element::spacer())
                    .size(None, Some(1.0))
                    .background(look.border),
            );
        }
        let keys = row
            .keys
            .iter()
            .map(|combination| {
                Element::row(
                    std::iter::once(Element::spacer().flex(1.0))
                        .chain(combination.iter().map(|key| keycap(key, look)))
                        .collect(),
                )
                .gap(4.0)
            })
            .collect();
        lines.push(Element::column(vec![
            gap(0.0, 9.0),
            Element::row(vec![
                gap(18.0, 0.0),
                text(row.action.clone(), 16.0, look.text)
                    .flex(1.0)
                    .centered_y(),
                gap(16.0, 0.0),
                Element::column(keys).gap(4.0).centered_y(),
                gap(18.0, 0.0),
            ])
            .min_size(0.0, 41.5),
            gap(0.0, 9.0),
        ]));
    }
    Element::panel(Element::column(lines))
        .background(look.raised)
        .border(look.border, 1.0)
        .radius(8.0)
}

/// One key, as printed on it.
pub(crate) fn keycap(key: &str, look: &Look) -> Element {
    Element::panel(Element::row(vec![
        gap(6.0, 0.0),
        Element::text(key.to_uppercase())
            .font_family(FontFamily::SansBold)
            .font_size(12.0)
            .foreground(look.text)
            .centered_y(),
        gap(6.0, 0.0),
    ]))
    .size(None, Some(22.5))
    .min_size(22.5, 22.5)
    .background(look.selected)
    .border(look.border, 1.0)
    .radius(4.0)
}

/// A card that leads to settings on another page.
pub fn related(id: &str, glyph: Icon, title: &str, note: &str, look: &Look) -> Element {
    Element::column(vec![
        gap(0.0, 16.0),
        line(t("Ajustes relacionados"), 16.0, look.muted),
        gap(0.0, 12.0),
        Element::clickable(
            id,
            Element::row(vec![
                Element::panel(crate::centered(
                    icon(glyph, look.text).size(Some(20.0), Some(20.0)),
                ))
                .size(Some(50.0), Some(50.0))
                .radius(8.0)
                .background(look.raised)
                .centered_y(),
                Element::column(vec![
                    line(title, 16.0, look.text),
                    text(note, 14.0, look.muted),
                ])
                .gap(3.0)
                .flex(1.0)
                .centered_y(),
                icon(Icon::ChevronRight, look.muted).centered_y(),
                gap(4.0, 0.0),
            ])
            .gap(14.0)
            .padding(4.0),
        )
        .radius(8.0)
        .hover_background(look.hover)
        .accessible(limn::Access::new(limn::Role::Link).name(title.to_owned())),
    ])
}

/// A note that informs, framed in the accent.
pub fn notice(message: &str, look: &Look) -> Element {
    Element::panel(
        Element::row(vec![
            icon(Icon::InfoCircle, look.accent).centered_y(),
            text(message, 14.0, look.text).flex(1.0).centered_y(),
        ])
        .gap(10.0)
        .padding(12.0),
    )
    .background(Look::alpha(look.accent, 14))
    .border(Look::alpha(look.accent, 150), 1.0)
    .radius(8.0)
    .accessible(limn::Access::new(limn::Role::Status).name(message.to_owned()))
}

/// The microphone's level as recent bars, the newest at the right.
pub fn level_bars(id: &str, levels: &[f32], width: f32, look: &Look) -> Element {
    Element::level_bars(LevelBars {
        id: id.to_owned(),
        levels: levels.to_vec(),
        bar: 4.0,
        gap: 4.0,
        color: look.online,
        rest: look.selected,
    })
    .size(Some(width), Some(32.0))
}

/// The level that counts as voice, over the microphone's level: a bar as
/// wide as the slider under it, filled to the level heard (green once it
/// passes the threshold), with a mark at the threshold.
pub fn sensitivity(level: Option<f32>, threshold: f32, width: f32, look: &Look) -> Element {
    const INSET: f32 = 6.0;
    let share = |value: f32| value.clamp(0.0005, 0.9995);
    let level = level.unwrap_or(0.0).clamp(0.0, 1.0);
    let fill = if level >= threshold {
        look.online
    } else {
        look.idle
    };
    let bar = |fraction: f32, color: Color| {
        Element::row(vec![
            Element::panel(Element::spacer())
                .background(color)
                .radius(4.0)
                .flex(share(fraction)),
            Element::spacer().flex(share(1.0 - fraction)),
        ])
    };
    let mark = Element::row(vec![
        Element::spacer().flex(share(threshold)),
        Element::panel(Element::spacer())
            .size(Some(2.0), Some(16.0))
            .background(look.text),
        Element::spacer().flex(share(1.0 - threshold)),
    ]);
    Element::row(vec![
        gap(INSET, 0.0),
        Element::stack(vec![
            Element::column(vec![
                gap(0.0, 4.0),
                Element::stack(vec![
                    Element::panel(Element::spacer())
                        .background(look.selected)
                        .radius(4.0),
                    bar(level, fill),
                ])
                .size(None, Some(8.0)),
            ]),
            mark,
        ])
        .size(None, Some(16.0))
        .flex(1.0),
        gap(INSET, 0.0),
    ])
    .size(Some(width), Some(16.0))
    .accessible(
        limn::Access::new(limn::Role::ProgressIndicator)
            .name(t("Nivel del micrófono"))
            .value(level),
    )
}

/// What the preview of the accessibility settings shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    /// The account, who writes the sample messages.
    pub name: String,
    pub ink: Color,
    pub accent: Color,
    pub style: MessageStyle,
    pub now: Timestamp,
}

/// Two sample messages as the timeline would show them with the chosen
/// settings, three people with their status, and a sample button.
pub fn preview(preview: &Preview, width: f32, look: &Look) -> Element {
    let sample = |id: u64, content: &str, reactions: Vec<Reaction>| Message {
        id: MessageId(u64::MAX - id),
        channel: ChannelId(0),
        author: UserId(0),
        content: content.to_owned(),
        timestamp: preview.now,
        edited: None,
        kind: MessageKind::Default,
        reply_to: None,
        mentions: Vec::new(),
        reactions,
        attachments: Vec::new(),
        stickers: Vec::new(),
        text_references: Vec::new(),
        delivery: Delivery::Sent,
        pinned: false,
        embeds: Vec::new(),
        forwarded: None,
        poll: None,
        components: Default::default(),
        call: None,
        mention_roles: Vec::new(),
        mention_everyone: false,
    };
    let first = sample(
        1,
        t("¿qué pasó con todos los porotos?"),
        vec![
            Reaction {
                emoji: "🦀".into(),
                count: 3,
                me: true,
                ..Default::default()
            },
            Reaction {
                emoji: "👀".into(),
                count: 1,
                me: false,
                ..Default::default()
            },
        ],
    );
    let second = sample(
        2,
        t("aquí tienes un enlace: https://example.org/rune"),
        Vec::new(),
    );
    let messages_width = (width - 120.0).max(160.0);
    let row = |message: &Message| {
        message_row(
            MessageView {
                message,
                author: Author {
                    marks: Default::default(),
                    image: None,
                    name: preview.name.clone(),
                    ink: preview.ink,
                    accent: preview.accent,
                    bot: false,
                },
                continued: false,
                day: None,
                first_new: false,
                start: None,
                reply: None,
                mentions_me: false,
                abilities: Abilities::default(),
                reacting: false,
                author_card: None,
                width: messages_width,
                now: preview.now,
                style: MessageStyle {
                    sample: true,
                    ..preview.style
                },
            },
            look,
        )
    };
    let people = [
        ("Ada", look.online, look.online),
        ("Brisa", look.idle, look.idle),
        ("Cata", look.busy, look.busy),
    ]
    .into_iter()
    .map(|(name, accent, status)| {
        with_status(avatar(name, accent, 32.0), 32.0, status, look.canvas)
    })
    .collect();
    let button = Element::panel(
        Element::text(t("Botón de ejemplo"))
            .font_family(FontFamily::SansMedium)
            .font_size(14.0)
            .foreground(look.on_accent)
            .center_content(),
    )
    .padding(8.0)
    .radius(6.0)
    .background(look.accent);
    let side = Element::column(vec![
        Element::row(vec![
            Element::spacer().flex(1.0),
            Element::column(people).gap(8.0),
        ]),
        Element::spacer().flex(1.0),
        button,
    ]);
    Element::column(vec![
        line(t("Previsualizar"), 16.0, look.muted),
        gap(0.0, 10.0),
        Element::panel(
            Element::row(vec![
                Element::column(vec![row(&first), row(&second)]).size(Some(messages_width), None),
                side.flex(1.0),
            ])
            .padding(12.0),
        )
        .size(Some(width), Some(PREVIEW_HEIGHT))
        .border(look.border, 1.0)
        .radius(8.0)
        .clip_children(),
        gap(0.0, 16.0),
    ])
    .accessible(limn::Access::new(limn::Role::Group).name(t("Previsualizar")))
}

/// A button of the settings, quiet on the page's ground: as tall as the
/// reference's 41 px buttons allow limn's sizes, and never narrower than 96.
pub fn secondary_button(id: &str, label: &str, enabled: bool, look: &Look) -> Element {
    Button::new(id, label, look.palette())
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Spacious)
        .enabled(enabled)
        .render()
        .min_size(96.0, 0.0)
}

/// The button that does what a section is for, in the accent.
pub fn primary_button(id: &str, label: &str, enabled: bool, look: &Look) -> Element {
    Button::new(id, label, look.accent_palette())
        .size(ControlSize::Spacious)
        .enabled(enabled)
        .render()
        .min_size(96.0, 0.0)
}

/// A button that destroys something, in the danger color.
pub fn danger_button(id: &str, label: &str, look: &Look) -> Element {
    Button::new(id, label, look.palette())
        .variant(ButtonVariant::Destructive)
        .size(ControlSize::Spacious)
        .render()
        .min_size(96.0, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};

    #[test]
    fn the_column_takes_what_the_sidebar_leaves_up_to_its_width() {
        let wide = Geometry::new(Size::new(1361.0, 797.0));
        assert_eq!(wide.panel, Size::new(1274.0, 710.0));
        assert_eq!(wide.column, COLUMN_WIDTH);
        let narrow = Geometry::new(Size::new(720.0, 600.0));
        assert_eq!(narrow.panel.width, 688.0);
        assert!(narrow.column < COLUMN_WIDTH && narrow.column >= 200.0);
    }

    #[test]
    fn a_ticked_slider_names_every_mark_and_snaps_to_them() {
        let look = Look::DARK;
        let labels = ["12px", "14px", "16px"];
        let (_, laid) = frame(
            ticked_slider("s", "Tamaño", &labels, 2, 1, 400.0, &look),
            Size::new(400.0, 60.0),
        );
        let painted = texts(&laid);
        for label in labels {
            assert!(painted.contains(&label.to_owned()), "{painted:?}");
        }
        assert!(controls(&laid).contains(&"s".to_owned()));
        assert_eq!(mark_at(0.0, 3), 0);
        assert_eq!(mark_at(0.74, 3), 1);
        assert_eq!(mark_at(1.0, 3), 2);
        assert_eq!(mark_at(2.0, 3), 2);
    }

    #[test]
    fn a_section_draws_a_hairline_between_its_rows_and_not_around_them() {
        let look = Look::DARK;
        let row = || Element::spacer().size(Some(200.0), Some(40.0));
        let (_, laid) = frame(
            section("s", "Título", None, vec![row(), row(), row()], &look),
            Size::new(400.0, 300.0),
        );
        let rules = laid
            .commands
            .iter()
            .filter(|command| {
                matches!(command, limn::DrawCommand::FillRect { rect, color, .. }
                    if *color == look.border && rect.height == 1.0)
            })
            .count();
        assert_eq!(rules, 2, "one between each two of the three rows");
    }

    #[test]
    fn choices_are_cards_and_a_note_repeating_its_name_is_left_out() {
        let look = Look::DARK;
        let options = [
            ("a".to_owned(), "Uno", None),
            ("b".to_owned(), "Dos", Some("Dos")),
            ("c".to_owned(), "Tres", Some("Una nota")),
        ];
        let (_, laid) = frame(
            choices("c", "Elegir", &options, "a", &look),
            Size::new(400.0, 300.0),
        );
        let painted = texts(&laid);
        assert!(painted.iter().any(|text| text == "Una nota"));
        assert_eq!(
            painted.iter().filter(|text| *text == "Dos").count(),
            1,
            "the name alone: {painted:?}"
        );
        // The chosen card takes the selected fill; the others stay raised.
        let fills = |fill: Color| {
            laid.commands
                .iter()
                .filter(|command| {
                    matches!(command, limn::DrawCommand::FillRect { color, .. } if *color == fill)
                })
                .count()
        };
        assert!(fills(look.raised) >= 2, "the other cards are raised");
        assert!(fills(look.selected) >= 1, "the chosen card is marked");
    }

    #[test]
    fn the_preview_shows_messages_that_do_nothing_when_clicked() {
        let look = Look::DARK;
        let (_, laid) = frame(
            preview(
                &Preview {
                    name: "Invitado".into(),
                    ink: look.accent,
                    accent: look.accent,
                    style: MessageStyle::default(),
                    now: Timestamp(1_790_000_000_000),
                },
                743.0,
                &look,
            ),
            Size::new(743.0, 260.0),
        );
        let painted = texts(&laid);
        assert!(painted.iter().any(|text| text.contains("porotos")));
        assert!(
            controls(&laid).is_empty(),
            "no reactions, links or menus: {:?}",
            controls(&laid)
        );
    }

    #[test]
    fn the_sidebar_shows_the_open_page_s_sections_only() {
        let look = Look::DARK;
        let page = |id: &str, selected: bool| {
            Nav::Page(NavPage {
                id: id.into(),
                label: id.into(),
                icon: Icon::Palette,
                selected,
                anchors: vec![NavAnchor {
                    id: format!("{id}:a"),
                    label: format!("{id} a"),
                    active: true,
                }],
            })
        };
        let element = sidebar(
            &Account {
                image: None,
                name: "Invitado".into(),
                accent: look.accent,
                edit: Some("profile:edit".into()),
            },
            search_box("q", "", &look),
            &[page("uno", true), Nav::Separator, page("dos", false)],
            "RUNE",
            &look,
        );
        let (_, laid) = frame(element, Size::new(269.0, 700.0));
        let ids = controls(&laid);
        assert!(ids.contains(&"uno:a".to_owned()) && !ids.contains(&"dos:a".to_owned()));
        assert!(ids.contains(&"profile:edit".to_owned()));
        assert!(!ids.contains(&"q:clear".to_owned()), "nothing to clear");
        // Who the settings are of is a row of the menu: the same inset,
        // width and radius as «uno».
        let rect = |id: &str| {
            laid.regions
                .iter()
                .find(|region| region.id == id)
                .unwrap()
                .rect
        };
        let (edit, uno) = (rect("profile:edit"), rect("uno"));
        assert_eq!((edit.x, edit.width), (uno.x, uno.width));
        let rounded = |at: limn::Rect, corner: f32| {
            laid.commands.iter().any(|command| {
                matches!(command, limn::DrawCommand::FillRect { rect, radius, .. }
                    if *rect == at && *radius == corner)
            })
        };
        assert!(rounded(uno, 6.0), "the selected page's fill");
        let mut tree = limn::UiTree::new(sidebar(
            &Account {
                image: None,
                name: "Invitado".into(),
                accent: look.accent,
                edit: Some("profile:edit".into()),
            },
            search_box("q", "", &look),
            &[page("uno", true)],
            "RUNE",
            &look,
        ));
        tree.set_reduced_motion(true);
        tree.layout(Size::new(269.0, 700.0));
        tree.handle_event(limn::UiEvent::PointerMove(limn::Point::new(
            edit.x + edit.width - 2.0,
            edit.y + edit.height / 2.0,
        )));
        let hovered = tree.layout_at(
            Size::new(269.0, 700.0),
            std::time::Instant::now() + std::time::Duration::from_secs(2),
        );
        assert!(
            hovered.commands.iter().any(|command| matches!(command,
                limn::DrawCommand::FillRect { rect, radius, .. } if *rect == edit && *radius == 8.0)),
            "its hover fills the whole row, rounded"
        );
        // The avatar and the two lines are centered on each other.
        let avatar = hovered
            .commands
            .iter()
            .find_map(|command| match command {
                limn::DrawCommand::FillRect { rect, radius, .. } if *radius == 20.0 => Some(*rect),
                _ => None,
            })
            .unwrap();
        let middle = |rect: limn::Rect| rect.y + rect.height / 2.0;
        assert_eq!(middle(avatar), middle(edit));
    }
}
