//! The settings of one server, over the whole window: the sidebar of pages,
//! the page column, the close button with «ESC», the bar of unsaved changes,
//! and the rows each page lists (roles, permissions, members, bans, audit
//! log entries). Pure functions of props like the rest of the crate; `rune-ui`
//! keeps the drafts and turns the actions into commands.
//!
//! Measures follow the reference captures at 1361×797 logical pixels: a
//! 254 px sidebar of 38.5 px items, a 705 px page column 58 px to its right,
//! the close button 42 px past the column, all centered as one block.

use limn::{
    Button, ButtonSize, ButtonVariant, Checkbox, Color, ColorPicker, Component, ContextMenu,
    ContextMenuEntry, ContextMenuItem, DropdownMenu, DropdownMenuEntry, DropdownMenuItem, Element,
    FontFamily, Hsv, Icon, Popover, PopoverPlacement, ScrollArea, ScrollbarAppearance, Size,
    Switch, Tab, Tabs, TabsVariant, TextSpan, TooltipSpec,
};
use rune_i18n::{es, t, tr};
use rune_model::{AuditAction, AuditActionKind, Permissions, Rgb, RoleId, UserId};

use crate::{avatar, gap, icon, line, strong_line, text, Look};

/// The sidebar's width, and the height and pitch of its items.
pub const SIDEBAR_WIDTH: f32 = 254.0;
pub const ITEM_HEIGHT: f32 = 38.5;
const ITEM_GAP: f32 = 2.0;
/// Text inside a sidebar item, and of its headings, starts this far in.
const ITEM_INSET: f32 = 10.5;
/// A page's column, and the wider one of the member table.
pub const CONTENT_WIDTH: f32 = 705.0;
pub const WIDE_CONTENT_WIDTH: f32 = 847.0;
/// Between the sidebar and a page's column.
pub const PAGE_INSET: f32 = 58.0;
/// The close button, and the space between the page and it.
pub const CLOSE_SIZE: f32 = 38.0;
const CLOSE_GAP: f32 = 42.0;
/// Space over a page's title, and under its end for the bar to float in.
const PAGE_TOP: f32 = 60.0;
const PAGE_BOTTOM: f32 = 96.0;
/// The strip at the window's top where its buttons are: the region right of
/// the sidebar starts under it, so a page scrolls away there and its
/// scrollbar never runs under the buttons.
const TOP_STRIP: f32 = crate::sidebar::HEADER;
/// Space over a page's title inside the region, which starts under the strip.
pub const REGION_TOP: f32 = PAGE_TOP - TOP_STRIP;
/// The bar of unsaved changes: as wide as the column and 20.5 px past each side.
pub const BAR_HEIGHT: f32 = 55.5;
const BAR_OVERHANG: f32 = 20.5;
const BAR_BOTTOM: f32 = 22.5;
/// The role editor: the list of roles beside it, and its own column.
pub const ROLE_LIST_WIDTH: f32 = 262.0;
pub const EDITOR_WIDTH: f32 = 472.0;
/// Least room kept at each side of the window.
const EDGE: f32 = 16.0;

/// The swatches of the color pickers, in two rows of ten.
pub const COLORS: [Rgb; 20] = [
    Rgb(0x1A_BC_9C),
    Rgb(0x2E_CC_71),
    Rgb(0x34_98_DB),
    Rgb(0x9B_59_B6),
    Rgb(0xE9_1E_63),
    Rgb(0xF1_C4_0F),
    Rgb(0xE6_7E_22),
    Rgb(0xE7_4C_3C),
    Rgb(0x95_A5_A6),
    Rgb(0x60_7D_8B),
    Rgb(0x11_80_6A),
    Rgb(0x1F_8B_4C),
    Rgb(0x20_66_94),
    Rgb(0x71_36_8A),
    Rgb(0xAD_14_57),
    Rgb(0xC2_7C_0E),
    Rgb(0xA8_43_00),
    Rgb(0x99_2D_22),
    Rgb(0x97_9C_9F),
    Rgb(0x54_6E_7A),
];
/// What a role without a color of its own is drawn in.
pub const DEFAULT_ROLE_COLOR: Rgb = Rgb(0x99_AA_B5);

/// Where each part goes for a window this wide.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    /// From the window's left edge to the sidebar.
    pub margin: f32,
    /// The page's column.
    pub content: f32,
}

impl Metrics {
    /// The sidebar, the page and the close button are centered as a block;
    /// a `wide` page grows to the right of where a usual one ends. In a
    /// narrow window the column gives up room first.
    pub fn new(window_width: f32, wide: bool) -> Self {
        let fixed = SIDEBAR_WIDTH + PAGE_INSET + CLOSE_GAP + CLOSE_SIZE;
        let margin = ((window_width - fixed - CONTENT_WIDTH) / 2.0).max(EDGE);
        let room = (window_width - margin - fixed - EDGE).max(240.0);
        let widest = if wide {
            WIDE_CONTENT_WIDTH
        } else {
            CONTENT_WIDTH
        };
        Self {
            margin,
            content: room.min(widest),
        }
    }

    /// The region right of the sidebar: the page's inset and its column.
    pub fn region(&self) -> f32 {
        PAGE_INSET + self.content
    }
}

/// One line of the sidebar.
#[derive(Clone, Debug, PartialEq)]
pub enum SidebarEntry {
    /// A group's title, in capitals.
    Heading(String),
    /// What the settings are of, in capitals: a mark such as a channel's
    /// `#`, a name, and a brighter detail after it, such as its category.
    Title {
        icon: Option<Icon>,
        name: String,
        detail: Option<String>,
    },
    /// A page, or an action such as deleting the server.
    Item {
        id: String,
        label: String,
        selected: bool,
        /// Drawn in the danger color with a trash can: it destroys something.
        danger: bool,
    },
    Separator,
}

/// The sidebar of pages, with a quiet line at its end saying what the
/// provider is.
pub fn sidebar(entries: &[SidebarEntry], footnote: &str, look: &Look) -> Element {
    let mut rows = vec![gap(0.0, PAGE_TOP)];
    for entry in entries {
        rows.push(match entry {
            SidebarEntry::Heading(title) => Element::row(vec![
                gap(ITEM_INSET, 0.0),
                line(title.to_uppercase(), 12.0, look.faint)
                    .font_family(FontFamily::SansBold)
                    .flex(1.0)
                    .centered_y(),
            ])
            .size(None, Some(26.0)),
            SidebarEntry::Title { icon, name, detail } => {
                let mut parts = vec![gap(ITEM_INSET, 0.0)];
                if let Some(glyph) = icon {
                    parts.push(
                        Element::icon(*glyph)
                            .foreground(look.faint)
                            .size(Some(12.0), Some(12.0))
                            .centered_y(),
                    );
                }
                parts.push(
                    line(name.to_uppercase(), 12.0, look.faint)
                        .font_family(FontFamily::SansBold)
                        .centered_y(),
                );
                if let Some(detail) = detail {
                    parts.push(
                        line(detail.to_uppercase(), 12.0, look.muted)
                            .font_family(FontFamily::SansBold)
                            .flex(1.0)
                            .centered_y(),
                    );
                }
                Element::row(parts).gap(5.0).size(None, Some(26.0))
            }
            SidebarEntry::Separator => Element::column(vec![
                gap(0.0, 9.0),
                Element::row(vec![
                    gap(ITEM_INSET, 0.0),
                    Element::panel(Element::spacer())
                        .size(None, Some(1.0))
                        .background(look.border)
                        .flex(1.0),
                    gap(ITEM_INSET, 0.0),
                ]),
                gap(0.0, 9.0),
            ]),
            SidebarEntry::Item {
                id,
                label,
                selected,
                danger,
            } => sidebar_item(id, label, *selected, *danger, look),
        });
    }
    rows.push(gap(0.0, 16.0));
    // Under the list, out of its scroll: what the connection is (a
    // simulation says so) stays in sight however many pages there are.
    Element::column(vec![
        ScrollArea::new("ss:sidebar", Element::column(rows).gap(ITEM_GAP))
            .appearance(ScrollbarAppearance::overlay().buttons(false))
            .render()
            .flex(1.0),
        Element::row(vec![
            gap(ITEM_INSET, 0.0),
            text(footnote, 12.0, look.faint).flex(1.0),
        ])
        .size(Some(SIDEBAR_WIDTH), None),
        gap(0.0, 12.0),
    ])
    .gap(4.0)
    .size(Some(SIDEBAR_WIDTH), None)
}

fn sidebar_item(id: &str, label: &str, selected: bool, danger: bool, look: &Look) -> Element {
    let ink = if danger {
        look.danger
    } else if selected {
        look.text
    } else {
        look.muted
    };
    let mut parts = vec![line(label, 16.0, ink).flex(1.0).centered_y()];
    if danger {
        parts.push(icon(Icon::Trash, look.danger).centered_y());
    }
    let item = Element::clickable(
        id,
        Element::row(vec![
            gap(ITEM_INSET, 0.0),
            Element::row(parts).gap(8.0).flex(1.0),
            gap(ITEM_INSET, 0.0),
        ]),
    )
    .size(Some(SIDEBAR_WIDTH), Some(ITEM_HEIGHT))
    .radius(6.0)
    .hover_background(look.hover)
    .hover_foreground(if danger { look.danger } else { look.text })
    .accessible(
        limn::Access::new(limn::Role::Tab)
            .name(label)
            .selected(selected),
    );
    if selected {
        item.background(look.selected)
    } else {
        item
    }
}

/// The round button that closes the settings, with «ESC» under it.
pub fn close_button(id: &str, look: &Look) -> Element {
    Element::column(vec![
        Element::clickable(
            id,
            crate::centered(icon(Icon::Close, look.muted).size(Some(16.0), Some(16.0))),
        )
        .size(Some(CLOSE_SIZE), Some(CLOSE_SIZE))
        .radius(CLOSE_SIZE / 2.0)
        .border(look.faint, 2.0)
        .hover_background(look.hover)
        .hover_foreground(look.text)
        .accessible(limn::Access::new(limn::Role::Button).name(t("Cerrar (Esc)"))),
        Element::row(vec![
            Element::spacer().flex(1.0),
            line("ESC", 12.0, look.faint).font_family(FontFamily::SansBold),
            Element::spacer().flex(1.0),
        ])
        .size(Some(CLOSE_SIZE), Some(16.0)),
    ])
    .gap(6.0)
}

/// What the bar of unsaved changes shows.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct UnsavedBar {
    /// The changes left and their answer has not come yet.
    pub saving: bool,
    /// Someone tried to leave with changes: the bar asks again, louder.
    pub nudged: bool,
}

/// The bar that floats at the foot of a page while something differs from
/// what is saved: «Restablecer» drops the changes, «Guardar cambios» sends
/// them. The ids are `ss:reset` and `ss:save`.
pub fn unsaved_bar(bar: UnsavedBar, look: &Look) -> Element {
    unsaved_bar_for("ss", bar, look)
}

/// [`unsaved_bar`] for other settings: its ids are `{prefix}:reset` and
/// `{prefix}:save`.
pub fn unsaved_bar_for(prefix: &str, bar: UnsavedBar, look: &Look) -> Element {
    let (fill, ink) = if bar.nudged {
        (look.danger, Color::rgb(255, 255, 255))
    } else {
        (look.menu, look.text)
    };
    let message = if bar.nudged {
        t("Guarda o restablece los cambios antes de salir")
    } else {
        t("¡Cuidado! Tienes cambios sin guardar")
    };
    let (reset, save) = (format!("{prefix}:reset"), format!("{prefix}:save"));
    let reset = Button::new(reset, t("Restablecer"), look.menu_palette())
        .variant(ButtonVariant::Link)
        .button_size(ButtonSize::Small)
        .ink(if bar.nudged {
            ink
        } else {
            look.menu_palette().link()
        })
        .enabled(!bar.saving)
        .render();
    let save = Button::new(save, t("Guardar cambios"), look.menu_palette())
        .variant(ButtonVariant::Success)
        .button_size(ButtonSize::Small)
        .width(129.5)
        .loading(bar.saving)
        .render();
    Element::panel(
        Element::row(vec![
            strong_line(message, 16.0, ink).flex(1.0).centered_y(),
            reset.centered_y(),
            save.centered_y(),
        ])
        .gap(12.0)
        .padding(12.0),
    )
    .size(None, Some(BAR_HEIGHT))
    .background(fill)
    .radius(8.0)
    .border(look.border, 1.0)
    .shadow(look.shadow, limn::Point::new(0.0, 8.0), 20.0, 0.0)
    .accessible(limn::Access::new(limn::Role::Status).name(message))
}

/// The whole window while the settings are open: the sidebar on its own
/// ground, `region` right of it (a page, or the role editor) up to the
/// window's edge, where its scrollbar runs as in the reference, the close
/// button over it, and the bar floating over the page's foot when there is
/// one.
pub fn shell(
    window: Size,
    metrics: Metrics,
    sidebar: Element,
    region: Element,
    bar: Option<Element>,
    look: &Look,
) -> Element {
    shell_for("ss", window, metrics, sidebar, region, bar, look)
}

/// [`shell`] for other settings: the close button reports `{prefix}:close`.
pub fn shell_for(
    prefix: &str,
    window: Size,
    metrics: Metrics,
    sidebar: Element,
    region: Element,
    bar: Option<Element>,
    look: &Look,
) -> Element {
    let base = Element::row(vec![
        Element::panel(Element::row(vec![gap(metrics.margin, 0.0), sidebar]))
            .size(Some(metrics.margin + SIDEBAR_WIDTH), None)
            .background(look.sidebar),
        Element::column(vec![gap(0.0, TOP_STRIP), region.flex(1.0)]).flex(1.0),
    ])
    .size(Some(window.width), Some(window.height))
    .background(look.canvas);
    // Where it stood when the region ended at the page's column: it stays
    // put while the page scrolls under it.
    let close = Element::column(vec![
        gap(0.0, 64.0),
        Element::row(vec![
            gap(
                metrics.margin + SIDEBAR_WIDTH + metrics.region() + CLOSE_GAP,
                0.0,
            ),
            close_button(&format!("{prefix}:close"), look),
        ]),
    ]);
    let mut layers = vec![base, close];
    if let Some(bar) = bar {
        let left = metrics.margin + SIDEBAR_WIDTH + PAGE_INSET - BAR_OVERHANG;
        layers.push(Element::column(vec![
            Element::spacer().flex(1.0),
            Element::row(vec![
                gap(left, 0.0),
                bar.size(Some(metrics.content + BAR_OVERHANG * 2.0), Some(BAR_HEIGHT)),
                Element::spacer().flex(1.0),
            ]),
            gap(0.0, BAR_BOTTOM),
        ]));
    }
    Element::stack(layers)
}

/// A page's column: `children` under one another, scrolling on their own,
/// `width` wide after the inset from the sidebar.
pub fn page(scroll_id: &str, width: f32, children: Vec<Element>) -> Element {
    ScrollArea::new(
        scroll_id,
        Element::row(vec![
            gap(PAGE_INSET, 0.0),
            Element::column(vec![
                gap(0.0, REGION_TOP),
                Element::column(children).gap(16.0),
                gap(0.0, PAGE_BOTTOM),
            ])
            .size(Some(width), None),
        ]),
    )
    .appearance(ScrollbarAppearance::overlay().buttons(false))
    .render()
}

/// A page's title, 20 px, and what the page is for under it.
pub fn page_title(title: &str, description: Option<&str>, look: &Look) -> Element {
    let mut parts = vec![strong_line(title, 20.0, look.text)];
    if let Some(description) = description {
        parts.push(text(description, 14.0, look.muted));
    }
    Element::column(parts).gap(6.0)
}

/// A field's name, 16 px, with a red mark when it cannot stay empty.
pub fn field_label(label: &str, required: bool, look: &Look) -> Element {
    let mut spans = vec![TextSpan::new(label, look.text, 16.0, FontFamily::Sans)];
    if required {
        spans.push(TextSpan::new(" *", look.danger, 16.0, FontFamily::Sans));
    }
    Element::formatted_text(spans)
}

/// A line between two sections of a page.
pub fn divider(look: &Look) -> Element {
    Element::column(vec![
        gap(0.0, 12.0),
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border),
        gap(0.0, 12.0),
    ])
}

/// Why a page, or part of it, cannot be changed.
pub fn denied(reason: &str, look: &Look) -> Element {
    Element::panel(
        Element::row(vec![
            icon(Icon::InfoCircle, look.muted),
            text(reason, 14.0, look.muted).flex(1.0),
        ])
        .gap(10.0)
        .padding(12.0),
    )
    .background(look.raised)
    .radius(8.0)
    .accessible(limn::Access::new(limn::Role::Status).name(reason))
}

/// A setting that is on or off: its title, a note under it, and the switch.
pub fn switch_row(
    id: &str,
    title: &str,
    note: Option<&str>,
    on: bool,
    enabled: bool,
    look: &Look,
) -> Element {
    let mut copy = vec![text(title, 16.0, look.text)];
    if let Some(note) = note {
        copy.push(text(note, 14.0, look.muted));
    }
    Element::row(vec![
        Element::column(copy).gap(4.0).flex(1.0),
        Switch::new(id, on, look.palette())
            .accessibility_label(title)
            .enabled(enabled)
            .render(),
    ])
    .gap(24.0)
}

/// An action of a menu, enabled or not and why.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuAction {
    pub id: String,
    pub label: String,
    pub icon: Option<Icon>,
    /// It destroys or removes something: drawn in the danger color.
    pub danger: bool,
    /// Why it cannot be chosen, when it cannot.
    pub denied: Option<String>,
    /// A line before it.
    pub separated: bool,
}

impl MenuAction {
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            danger: false,
            denied: None,
            separated: false,
        }
    }

    pub fn icon(mut self, icon: Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn denied(mut self, reason: Option<String>) -> Self {
        self.denied = reason;
        self
    }

    pub fn separated(mut self) -> Self {
        self.separated = true;
        self
    }
}

/// `anchor` with a menu of `actions` that shows while `open`.
pub fn menu(
    anchor: Element,
    actions: &[MenuAction],
    open: bool,
    dismiss: &str,
    look: &Look,
) -> Element {
    let mut entries = Vec::new();
    for action in actions {
        if action.separated {
            entries.push(DropdownMenuEntry::Separator);
        }
        let mut item = DropdownMenuItem::new(action.id.clone(), action.label.clone());
        if let Some(glyph) = action.icon {
            item = item.icon(glyph);
        }
        if action.danger {
            item = item.destructive(look.danger);
        }
        if let Some(reason) = &action.denied {
            // The line under the label may be cut; the hint says it whole.
            item = item
                .enabled(false)
                .description(reason.clone())
                .tooltip_with(TooltipSpec::new(reason.clone()));
        }
        entries.push(DropdownMenuEntry::Item(item));
    }
    DropdownMenu::new(anchor, entries, open, dismiss, look.menu_palette())
        .placement(PopoverPlacement::BottomEnd)
        .width(320.0)
        .max_visible_items(12)
        .render()
}

/// `child` with `actions` as its context menu, opened with the secondary button.
pub fn context_menu(id: &str, child: Element, actions: &[MenuAction], look: &Look) -> Element {
    let mut entries = Vec::new();
    for action in actions {
        if action.separated {
            entries.push(ContextMenuEntry::Separator);
        }
        let mut item = ContextMenuItem::new(action.id.clone(), action.label.clone())
            .enabled(action.denied.is_none());
        if let Some(glyph) = action.icon {
            item = item.icon(glyph);
        }
        entries.push(ContextMenuEntry::Item(item));
    }
    ContextMenu::new(id, child, entries, look.menu_palette()).render()
}

/// A square button of an icon alone, 40×40 as the rows of the reference have.
pub fn square_button(id: &str, glyph: Icon, label: &str, enabled: bool, look: &Look) -> Element {
    Button::new(id, "", look.palette())
        .icon(glyph)
        .variant(ButtonVariant::Outline)
        .button_size(ButtonSize::Large)
        .accessibility_label(label)
        .tooltip(label)
        .enabled(enabled)
        .render()
}

/// The account's primary action, filled in the accent.
pub fn primary_button(id: &str, label: &str, enabled: bool, look: &Look) -> Element {
    Button::new(id, label, look.accent_palette())
        .button_size(ButtonSize::Small)
        .enabled(enabled)
        .render()
}

/// A round mark of `color`, for a role.
pub fn color_dot(color: Color, size: f32) -> Element {
    Element::panel(Element::spacer())
        .size(Some(size), Some(size))
        .radius(size / 2.0)
        .background(color)
}

/// `rgb` as six hex digits, as the custom color field shows it.
pub fn hex(rgb: Rgb) -> String {
    format!("{:06x}", rgb.0 & 0xFF_FF_FF)
}

/// Six hex digits, with or without a leading `#`, as a color.
pub fn parse_hex(value: &str) -> Option<Rgb> {
    let digits = value.trim().trim_start_matches('#');
    (digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| u32::from_str_radix(digits, 16).ok().map(Rgb))
        .flatten()
}

/// A color picker: the presets, a swatch of the custom color that opens a
/// picker of any color (a field of saturation and brightness, a hue bar and
/// its hex digits), and, for roles, the default swatch.
#[derive(Clone, Debug, PartialEq)]
pub struct Swatches {
    /// Prefixes the ids: `{prefix}:default`, `{prefix}:custom`,
    /// `{prefix}:{rrggbb}` for a preset, `{prefix}:hex`, `{prefix}:area`
    /// and `{prefix}:hue` for the picker's parts and `{prefix}:dismiss` to
    /// close it.
    pub prefix: String,
    pub chosen: Option<Rgb>,
    /// Offer a default swatch drawn in this color, chosen while `chosen` is `None`.
    pub default: Option<Rgb>,
    /// What the hex field holds while the picker is open.
    pub custom: Option<String>,
    pub enabled: bool,
    /// Too narrow for one row: the presets go under the big swatches.
    pub narrow: bool,
    /// Where the picker's thumbs stand while it is used, which a gray or
    /// black does not say; `None` takes them from the color.
    pub picked: Option<Hsv>,
}

/// The color picker of [`Swatches`].
pub fn swatches(view: &Swatches, look: &Look) -> Element {
    let prefix = &view.prefix;
    let shown = view.chosen.or(view.default).unwrap_or(DEFAULT_ROLE_COLOR);
    let check = |on: bool, size: f32| {
        if on {
            crate::centered(
                Element::icon(Icon::Check)
                    .foreground(Color::rgb(255, 255, 255))
                    .size(Some(size), Some(size)),
            )
        } else {
            Element::spacer()
        }
    };
    let swatch = |id: String,
                  color: Rgb,
                  chosen: bool,
                  content: Element,
                  width: f32,
                  height: f32,
                  name: &str| {
        let body = Element::panel(content)
            .size(Some(width), Some(height))
            .radius(if width > 30.0 { 8.0 } else { 7.0 })
            .background(look.ink(color));
        if view.enabled {
            Element::clickable(id, body)
                .size(Some(width), Some(height))
                .radius(7.0)
                .accessible(
                    limn::Access::new(limn::Role::Button)
                        .name(name.to_owned())
                        .selected(chosen),
                )
        } else {
            body
        }
    };
    let mut big = Vec::new();
    if let Some(default) = view.default {
        big.push(swatch(
            format!("{prefix}:default"),
            default,
            view.chosen.is_none(),
            check(view.chosen.is_none(), 20.0),
            70.5,
            53.5,
            t("Color predeterminado"),
        ));
    }
    let custom = swatch(
        format!("{prefix}:custom"),
        shown,
        false,
        Element::column(vec![
            Element::row(vec![
                Element::spacer().flex(1.0),
                Element::icon(Icon::Edit)
                    .foreground(Color::rgb(255, 255, 255))
                    .size(Some(14.0), Some(14.0)),
            ]),
            Element::spacer().flex(1.0),
        ])
        .padding(6.0),
        70.0,
        53.0,
        t("Color personalizado"),
    );
    let typed = view.custom.as_deref().and_then(parse_hex).unwrap_or(shown);
    let (r, g, b) = typed.channels();
    let field = ColorPicker::new(
        prefix.clone(),
        view.picked
            .unwrap_or_else(|| Hsv::from(Color::rgb(r, g, b))),
        look.menu_palette(),
    )
    .part_labels(
        t("Saturación y brillo"),
        t("Tono"),
        t("Color en hexadecimal"),
    )
    .width(208.0)
    .enabled(view.enabled)
    .render()
    .padding(6.0);
    big.push(
        Popover::new(
            custom,
            field,
            view.custom.is_some(),
            format!("{prefix}:dismiss"),
            look.menu_palette(),
        )
        .placement(PopoverPlacement::BottomStart)
        .width(220.0)
        .render(),
    );
    let presets: Vec<Element> = COLORS
        .chunks(10)
        .map(|row| {
            Element::row(
                row.iter()
                    .map(|color| {
                        swatch(
                            format!("{prefix}:{}", hex(*color)),
                            *color,
                            view.chosen == Some(*color),
                            check(view.chosen == Some(*color), 14.0),
                            21.5,
                            21.5,
                            &format!("#{}", hex(*color)),
                        )
                    })
                    .collect(),
            )
            .gap(10.5)
        })
        .collect();
    if view.narrow {
        return Element::column(vec![
            Element::row(big).gap(10.0),
            Element::column(presets).gap(10.5),
        ])
        .gap(12.0);
    }
    Element::row(vec![
        Element::row(big).gap(10.0),
        Element::column(presets).gap(10.5).centered_y(),
    ])
    .gap(12.0)
}

/// The width [`swatches`] takes in one row, with the default swatch or without.
pub fn swatches_width(with_default: bool) -> f32 {
    let presets = 10.0 * 21.5 + 9.0 * 10.5;
    70.0 + 12.0 + presets + if with_default { 70.5 + 10.0 } else { 0.0 }
}

/// The space between two of [`tabs`].
const TAB_GAP: f32 = 32.0;

/// How wide [`tabs`] with these labels are, so a view can say one shorter.
// shortcut: widths estimated from the characters, as the friends' tabs do.
pub fn tabs_width(labels: &[&str]) -> f32 {
    let tabs: f32 = labels
        .iter()
        .map(|label| label.chars().count() as f32 * 9.5 + 16.0)
        .sum();
    tabs + TAB_GAP * labels.len().saturating_sub(1) as f32
}

/// Underlined tabs: 16 px labels, the active one in the accent and
/// underlined, a hairline under all of them.
pub fn tabs(items: &[(String, String, bool, bool)], look: &Look) -> Element {
    let tabs = items
        .iter()
        .map(|(id, label, active, enabled)| {
            Tab::new(id.clone(), label.clone(), tabs_width(&[label]))
                .active(*active)
                .enabled(*enabled)
        })
        .collect();
    Tabs::new(tabs, look.palette())
        .variant(TabsVariant::Underlined)
        .accent(look.accent)
        .font_size(16.0)
        .height(38.0)
        .gap(TAB_GAP)
        .id("ss:tabs")
        .render()
}

/// Where a dragged role would land relative to a row.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DropEdge {
    Above,
    Below,
}

/// A 2 px line in the accent where a dragged row would go.
fn insertion(edge: Option<DropEdge>, at: DropEdge, look: &Look) -> Element {
    Element::panel(Element::spacer())
        .size(None, Some(2.0))
        .background(if edge == Some(at) {
            look.accent
        } else {
            Color::TRANSPARENT
        })
}

/// The card on the roles page that opens the permissions of `@everyone`.
pub fn everyone_card(id: &str, look: &Look) -> Element {
    Element::clickable(
        id,
        Element::row(vec![
            Element::panel(crate::centered(icon(Icon::Users, look.muted)))
                .size(Some(32.0), Some(32.0))
                .radius(16.0)
                .background(look.canvas)
                .centered_y(),
            Element::column(vec![
                strong_line(t("Permisos predeterminados"), 16.0, look.text),
                line(
                    t("@everyone · se aplica a todos los miembros del servidor"),
                    12.0,
                    look.muted,
                ),
            ])
            .gap(4.0)
            .flex(1.0)
            .centered_y(),
            icon(Icon::ChevronRight, look.muted).centered_y(),
        ])
        .gap(14.0)
        .padding(18.0),
    )
    .size(None, Some(77.0))
    .radius(8.0)
    .background(look.raised)
    .border(look.border, 1.0)
    .hover_background(look.selected)
    .accessible(limn::Access::new(limn::Role::Button).name(t("Permisos predeterminados")))
}

/// A role as the roles page lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleLine {
    pub role: RoleId,
    pub name: String,
    pub color: Color,
    pub members: usize,
    /// It can be dragged to another place.
    pub movable: bool,
    /// Why it cannot be changed, when it cannot.
    pub locked: Option<String>,
    pub menu: Vec<MenuAction>,
    pub menu_open: bool,
    pub drop: Option<DropEdge>,
}

/// The id a role's row reports: a click opens its editor; it is also what
/// is dragged and dropped on.
pub fn role_id(role: RoleId) -> String {
    format!("ss:role:{role}")
}

/// A row of the roles page, 65 px: the handle to drag it, its color, its
/// name, how many have it, and buttons to edit it and for more.
pub fn role_row(row: &RoleLine, look: &Look) -> Element {
    let handle = if row.movable {
        icon(Icon::DragHandle, look.faint)
    } else {
        gap(16.0, 16.0)
    };
    let mut name = vec![
        color_dot(row.color, 14.0).centered_y(),
        line(&row.name, 16.0, look.text).flex(1.0).centered_y(),
    ];
    if row.locked.is_some() {
        name.push(icon(Icon::InfoCircle, look.faint).centered_y());
    }
    let summary = Element::clickable(
        role_id(row.role),
        Element::row(vec![
            handle.centered_y(),
            Element::row(name).gap(12.0).flex(1.0),
            Element::row(vec![
                line(row.members.to_string(), 16.0, look.muted).centered_y(),
                icon(Icon::Users, look.muted).centered_y(),
            ])
            .gap(6.0)
            .size(Some(120.0), None),
        ])
        .gap(12.0)
        .padding(12.0),
    )
    .radius(8.0)
    .hover_background(look.hover)
    .accessible(limn::Access::new(limn::Role::ListItem).name(row.name.clone()));
    let summary = if row.movable {
        summary.draggable().drop_target()
    } else {
        summary.drop_target()
    };
    let more = menu(
        square_button(
            &format!("ss:role:{}:menu", row.role),
            Icon::More,
            t("Más opciones"),
            true,
            look,
        ),
        &row.menu,
        row.menu_open,
        "ss:role-menu:dismiss",
        look,
    );
    let edit = square_button(
        &format!("ss:role:{}:edit", row.role),
        Icon::Edit,
        t("Editar rol"),
        true,
        look,
    );
    Element::column(vec![
        insertion(row.drop, DropEdge::Above, look),
        Element::row(vec![
            summary.flex(1.0),
            edit.centered_y(),
            more.centered_y(),
        ])
        .gap(10.0)
        .size(None, Some(63.0)),
        insertion(row.drop, DropEdge::Below, look),
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border),
    ])
}

/// A role as the list beside the editor shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleItem {
    pub role: RoleId,
    pub name: String,
    pub color: Color,
    pub selected: bool,
    pub movable: bool,
    pub drop: Option<DropEdge>,
}

/// The list beside the role editor: «← ATRÁS», the button that makes a role,
/// and every role, highest first, `@everyone` last.
pub fn role_list(items: &[RoleItem], can_create: bool, look: &Look) -> Element {
    let back = Element::clickable(
        "ss:editor:back",
        Element::row(vec![
            icon(Icon::ArrowLeft, look.muted).centered_y(),
            line(t("ATRÁS"), 16.0, look.muted)
                .font_family(FontFamily::SansBold)
                .centered_y(),
        ])
        .gap(10.0),
    )
    .hover_foreground(look.text)
    .accessible(limn::Access::new(limn::Role::Button).name(t("Volver a los roles")));
    let create = Button::new("ss:editor:create", "", look.palette())
        .icon(Icon::Add)
        .variant(ButtonVariant::Ghost)
        .accessibility_label(t("Crear rol"))
        .tooltip(t("Crear rol"))
        .enabled(can_create)
        .render();
    let mut rows = vec![
        Element::row(vec![back.centered_y(), Element::spacer().flex(1.0), create])
            .size(None, Some(36.0)),
        gap(0.0, 12.0),
    ];
    for item in items {
        let body = Element::clickable(
            role_id(item.role),
            Element::row(vec![
                color_dot(item.color, 12.5).centered_y(),
                line(
                    &item.name,
                    15.0,
                    if item.selected { look.text } else { look.muted },
                )
                .flex(1.0)
                .centered_y(),
            ])
            .gap(10.0)
            .padding(10.0),
        )
        .size(None, Some(36.0))
        .radius(6.0)
        .hover_background(look.hover)
        .accessible(
            limn::Access::new(limn::Role::ListItem)
                .name(item.name.clone())
                .selected(item.selected),
        );
        let body = if item.selected {
            body.background(look.selected)
        } else {
            body
        };
        let body = if item.movable {
            body.draggable().drop_target()
        } else {
            body.drop_target()
        };
        rows.push(Element::column(vec![
            insertion(item.drop, DropEdge::Above, look),
            body,
            insertion(item.drop, DropEdge::Below, look),
        ]));
    }
    ScrollArea::new(
        "ss:role-list",
        Element::column(vec![
            gap(0.0, REGION_TOP),
            Element::row(vec![
                gap(51.0, 0.0),
                Element::column(rows).gap(0.5).flex(1.0),
                gap(26.0, 0.0),
            ]),
            gap(0.0, PAGE_BOTTOM),
        ]),
    )
    .appearance(ScrollbarAppearance::overlay().buttons(false))
    .render()
    .size(Some(ROLE_LIST_WIDTH), None)
}

/// A permission, its label, and what granting it lets a member do.
pub type PermissionInfo = (Permissions, &'static str, &'static str);

/// The permissions RUNE knows, grouped and named as servers with roles name
/// them. Every bit of [`Permissions::ALL`] is here once.
pub const PERMISSION_GROUPS: &[(&str, &[PermissionInfo])] = &[
    (
        es("Permisos generales del servidor"),
        &[
            (
                Permissions::VIEW,
                es("Ver canales"),
                es("Permite ver los canales del servidor, salvo los privados."),
            ),
            (
                Permissions::MANAGE_CHANNELS,
                es("Gestionar canales"),
                es("Permite crear, editar y borrar canales."),
            ),
            (
                Permissions::MANAGE_ROLES,
                es("Gestionar roles"),
                es("Permite crear roles y editar o borrar los que estén por debajo del rol más alto propio. También permite cambiar los permisos de cada canal."),
            ),
            (
                Permissions::MANAGE_EMOJI,
                es("Gestionar expresiones"),
                es("Permite agregar, editar y borrar los emojis personalizados del servidor."),
            ),
            (
                Permissions::MANAGE_WEBHOOKS,
                es("Gestionar webhooks"),
                es("Permite crear, editar y borrar los webhooks, que publican mensajes de otras aplicaciones o sitios."),
            ),
            (
                Permissions::VIEW_AUDIT_LOG,
                es("Ver el registro de auditoría"),
                es("Permite ver quién hizo cada cambio en el servidor."),
            ),
            (
                Permissions::MANAGE_SERVER,
                es("Gestionar el servidor"),
                es("Permite cambiar el nombre, la descripción y el color del servidor."),
            ),
        ],
    ),
    (
        es("Permisos de membresía"),
        &[
            (
                Permissions::CREATE_INVITE,
                es("Crear invitaciones"),
                es("Permite invitar a gente nueva al servidor."),
            ),
            (
                Permissions::CHANGE_NICK,
                es("Cambiar apodo"),
                es("Permite cambiar el apodo propio en este servidor."),
            ),
            (
                Permissions::MANAGE_NICKS,
                es("Gestionar apodos"),
                es("Permite cambiar el apodo de otros miembros."),
            ),
            (
                Permissions::KICK,
                es("Expulsar miembros"),
                es("Permite sacar a otros miembros del servidor. Pueden volver a entrar con otra invitación."),
            ),
            (
                Permissions::BAN,
                es("Vetar miembros"),
                es("Permite sacar a otros miembros del servidor y que no puedan volver."),
            ),
            (
                Permissions::TIMEOUT_MEMBERS,
                es("Suspender miembros"),
                es("Quien esté suspendido no puede escribir, reaccionar ni hablar en la voz por un tiempo."),
            ),
        ],
    ),
    (
        es("Permisos de los canales de texto"),
        &[
            (
                Permissions::SEND,
                es("Enviar mensajes"),
                es("Permite escribir en los canales de texto."),
            ),
            (
                Permissions::SEND_IN_THREADS,
                es("Enviar mensajes en hilos"),
                es("Permite escribir en los hilos."),
            ),
            (
                Permissions::CREATE_THREADS,
                es("Crear hilos públicos"),
                es("Permite abrir hilos que todos en el canal pueden ver."),
            ),
            (
                Permissions::CREATE_PRIVATE_THREADS,
                es("Crear hilos privados"),
                es("Permite abrir hilos que solo ven quienes son invitados y la moderación."),
            ),
            (
                Permissions::EMBED_LINKS,
                es("Insertar enlaces"),
                es("Los enlaces que se compartan muestran una vista previa."),
            ),
            (
                Permissions::ATTACH,
                es("Adjuntar archivos"),
                es("Permite subir archivos e imágenes."),
            ),
            (
                Permissions::SEND_POLLS,
                es("Crear encuestas"),
                es("Permite empezar encuestas en los canales de texto."),
            ),
            (
                Permissions::REACT,
                es("Añadir reacciones"),
                es("Permite agregar reacciones nuevas a un mensaje. Sin este permiso se puede sumar a las que ya están."),
            ),
            (
                Permissions::EXTERNAL_EMOJI,
                es("Usar emojis externos"),
                es("Permite usar emojis de otros servidores."),
            ),
            (
                Permissions::EXTERNAL_STICKERS,
                es("Usar stickers externos"),
                es("Permite enviar stickers de otros servidores."),
            ),
            (
                Permissions::MENTION_EVERYONE,
                es("Mencionar @everyone, @here y todos los roles"),
                es("Permite avisar a todo el servidor o a quienes estén en línea, y mencionar cualquier rol aunque no sea mencionable."),
            ),
            (
                Permissions::MANAGE_MESSAGES,
                es("Gestionar mensajes"),
                es("Permite borrar mensajes de otros miembros."),
            ),
            (
                Permissions::PIN_MESSAGES,
                es("Fijar mensajes"),
                es("Permite fijar y desfijar mensajes en los canales de texto."),
            ),
            (
                Permissions::MANAGE_THREADS,
                es("Gestionar hilos"),
                es("Permite renombrar, cerrar y borrar hilos, y ver los privados."),
            ),
            (
                Permissions::READ_HISTORY,
                es("Leer el historial de mensajes"),
                es("Permite leer los mensajes enviados antes de entrar al canal. Sin este permiso solo se ven los que llegan en línea."),
            ),
            (
                Permissions::USE_APP_COMMANDS,
                es("Usar comandos de aplicaciones"),
                es("Permite usar los comandos de las aplicaciones del servidor con / en el cuadro de texto."),
            ),
        ],
    ),
    (
        es("Permisos de los canales de voz"),
        &[
            (
                Permissions::CONNECT,
                es("Conectar"),
                es("Permite entrar a los canales de voz y escuchar."),
            ),
            (
                Permissions::SPEAK,
                es("Hablar"),
                es("Permite hablar en los canales de voz."),
            ),
            (
                Permissions::STREAM,
                es("Video"),
                es("Permite compartir la cámara o la pantalla."),
            ),
            (
                Permissions::USE_VOICE_ACTIVITY,
                es("Usar actividad de voz"),
                es("Permite hablar sin pulsar una tecla. Sin este permiso hay que usar pulsar para hablar."),
            ),
            (
                Permissions::PRIORITY_SPEAKER,
                es("Prioridad de palabra"),
                es("Mientras habla, baja el volumen de quienes no tienen este permiso."),
            ),
            (
                Permissions::REQUEST_TO_SPEAK,
                es("Pedir la palabra"),
                es("Permite levantar la mano en un escenario para pedir hablar."),
            ),
            (
                Permissions::MUTE_MEMBERS,
                es("Silenciar miembros"),
                es("Permite silenciar a otros en los canales de voz, para todos. En un escenario, invitar a hablar y pasar al público."),
            ),
            (
                Permissions::DEAFEN_MEMBERS,
                es("Ensordecer miembros"),
                es("Permite que otros no puedan hablar ni escuchar en los canales de voz."),
            ),
            (
                Permissions::MOVE_MEMBERS,
                es("Mover miembros"),
                es("Permite desconectar a otros de la voz o moverlos entre canales a los que se tiene acceso."),
            ),
        ],
    ),
    (
        es("Permisos avanzados"),
        &[(
            Permissions::ADMINISTRATOR,
            es("Administrador"),
            es("Da todos los permisos y pasa por encima de los permisos de cada canal. Es peligroso darlo."),
        )],
    ),
    // Scheduled events (eventos-programados).
    (
        es("Permisos de eventos"),
        &[
            (
                Permissions::CREATE_EVENTS,
                es("Crear eventos"),
                es("Permite planear eventos y editar o borrar los que haya creado uno."),
            ),
            (
                Permissions::MANAGE_EVENTS,
                es("Gestionar eventos"),
                es("Permite editar, iniciar, terminar y borrar los eventos de cualquiera."),
            ),
        ],
    ),
];

/// The id of a permission's switch: `ss:perm:{bit}`.
pub fn permission_id(permission: Permissions) -> String {
    format!("ss:perm:{}", permission.0.trailing_zeros())
}

/// The permission a switch's id names.
pub fn permission_of(bit: &str) -> Option<Permissions> {
    let bit: u32 = bit.parse().ok()?;
    (bit < Permissions::ALL.0.count_ones()).then(|| Permissions(1 << bit))
}

/// The groups of [`PERMISSION_GROUPS`], in the current language, with only
/// the permissions whose label or description holds `query`, ignoring case;
/// empty groups go.
pub fn permissions_matching(query: &str) -> Vec<(&'static str, Vec<PermissionInfo>)> {
    let query = query.trim().to_lowercase();
    PERMISSION_GROUPS
        .iter()
        .map(|(group, permissions)| {
            (
                t(group),
                permissions
                    .iter()
                    .map(|&(permission, label, description)| (permission, t(label), t(description)))
                    .filter(|(_, label, description)| {
                        query.is_empty()
                            || label.to_lowercase().contains(&query)
                            || description.to_lowercase().contains(&query)
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .filter(|(_, permissions)| !permissions.is_empty())
        .collect()
}

/// A group's heading in the permissions tab, with «Borrar permisos» beside
/// the first one when they can be changed.
pub fn permission_heading(title: &str, clear: bool, look: &Look) -> Element {
    let mut parts = vec![line(title, 20.0, look.text).flex(1.0).centered_y()];
    if clear {
        parts.push(
            Button::new("ss:perms:clear", t("Borrar permisos"), look.palette())
                .variant(ButtonVariant::Link)
                .button_size(ButtonSize::Small)
                .render()
                .centered_y(),
        );
    }
    Element::row(parts).gap(12.0).size(None, Some(40.0))
}

/// One permission: its name, what it allows, its switch, and a note under
/// it, such as why it cannot be turned on.
pub fn permission_row(
    permission: Permissions,
    label: &str,
    description: &str,
    on: bool,
    enabled: bool,
    note: Option<&str>,
    look: &Look,
) -> Element {
    let switch = Switch::new(permission_id(permission), on, look.palette())
        .accessibility_label(label)
        .enabled(enabled)
        .render();
    permission_entry(label, description, note, switch, look)
}

/// A permission's name, what it allows and a note under it, with `control`
/// beside them, and the line that parts it from the next.
pub fn permission_entry(
    label: &str,
    description: &str,
    note: Option<&str>,
    control: Element,
    look: &Look,
) -> Element {
    let mut copy = vec![
        text(label, 16.0, look.text),
        text(description, 14.0, look.muted).max_width(390.0),
    ];
    if let Some(note) = note {
        copy.push(text(note, 12.0, look.faint));
    }
    Element::column(vec![
        gap(0.0, 14.0),
        Element::row(vec![Element::column(copy).gap(4.0).flex(1.0), control]).gap(24.0),
        gap(0.0, 14.0),
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border),
    ])
}

/// A role as a small pill: its color as a dot and as a tint behind its name.
pub fn role_pill(name: &str, color: Color, look: &Look) -> Element {
    Element::panel(
        Element::row(vec![
            color_dot(color, 10.0).centered_y(),
            line(name, 12.0, look.text).centered_y(),
        ])
        .gap(5.0)
        .padding(5.0),
    )
    .size(None, Some(24.0))
    .radius(4.0)
    .background(Look::alpha(color, 40))
}

/// A role offered to a member, checked when they have it.
#[derive(Clone, Debug, PartialEq)]
pub struct RoleChoice {
    pub id: String,
    pub name: String,
    pub color: Color,
    pub checked: bool,
    /// Why it cannot be given or taken, when it cannot.
    pub denied: Option<String>,
}

/// The rows of a role picker: a dot, the name and a checkbox each.
pub fn role_choices(choices: &[RoleChoice], look: &Look) -> Element {
    if choices.is_empty() {
        return text(t("No hay roles para dar."), 13.0, look.muted);
    }
    let rows = choices
        .iter()
        .map(|choice| {
            let mut copy = vec![line(&choice.name, 15.0, look.text)];
            if let Some(reason) = &choice.denied {
                copy.push(text(reason, 11.0, look.faint));
            }
            Element::row(vec![
                color_dot(choice.color, 12.0).centered_y(),
                Element::column(copy).gap(2.0).flex(1.0).centered_y(),
                Checkbox::new(choice.id.clone(), choice.checked, look.menu_palette())
                    .accessibility_label(choice.name.clone())
                    .enabled(choice.denied.is_none())
                    .render()
                    .centered_y(),
            ])
            .gap(10.0)
            .padding(6.0)
        })
        .collect();
    Element::column(rows).gap(2.0)
}

/// A member as the members page lists them.
#[derive(Clone, Debug, PartialEq)]
pub struct MemberLine {
    pub user: UserId,
    pub name: String,
    pub handle: String,
    /// The color of their highest colored role.
    pub ink: Color,
    pub accent: Color,
    /// Their picture, once loaded.
    pub image: Option<limn::RasterImage>,
    pub owner: bool,
    /// Their roles, highest first.
    pub roles: Vec<(String, Color)>,
    /// The roles they can be given or have taken, while the picker is open:
    /// `None` when roles cannot be managed here.
    pub choices: Option<Vec<RoleChoice>>,
    pub picker_open: bool,
    pub menu: Vec<MenuAction>,
    pub menu_open: bool,
}

/// The columns of the members table after the name.
const ROLES_COLUMN: f32 = 300.0;
const ACTIONS_COLUMN: f32 = 44.0;

/// The roles column of a members table `width` wide (2026-10-19): at most
/// [`ROLES_COLUMN`], and never more than about half of what the name and
/// the roles share, so a narrow window keeps room for the name.
fn roles_column(width: f32) -> f32 {
    ((width - ACTIONS_COLUMN - 40.0) * 0.45).clamp(48.0, ROLES_COLUMN)
}

/// The heading of the members table, `width` wide.
pub fn members_heading(width: f32, look: &Look) -> Element {
    let label = |value: &str| {
        line(value, 12.0, look.muted)
            .font_family(FontFamily::SansBold)
            .centered_y()
    };
    Element::row(vec![
        label(t("NOMBRE")).flex(1.0),
        label(t("ROLES")).size(Some(roles_column(width)), None),
        gap(ACTIONS_COLUMN, 0.0),
    ])
    .gap(12.0)
    .padding(8.0)
    .size(None, Some(40.0))
}

/// A row of the members table, 47 px and `width` wide: the person, their
/// roles (as many as fit) with a button to give more, and their menu, also
/// on the secondary button.
pub fn member_row(row: &MemberLine, width: f32, look: &Look) -> Element {
    let roles = roles_column(width);
    let fit = match roles {
        r if r >= 220.0 => 2,
        r if r >= 120.0 => 1,
        _ => 0,
    };
    let mut name = vec![strong_line(&row.name, 15.0, row.ink)];
    let handle = if row.owner {
        tr!("{handle} · dueño", handle = row.handle)
    } else {
        row.handle.clone()
    };
    name.push(line(handle, 12.0, look.muted));
    let mut pills: Vec<Element> = row
        .roles
        .iter()
        .take(fit)
        .map(|(name, color)| role_pill(name, *color, look).centered_y())
        .collect();
    if row.roles.len() > fit {
        pills.push(line(format!("+{}", row.roles.len() - fit), 12.0, look.muted).centered_y());
    }
    if let Some(choices) = &row.choices {
        let add = Button::new(format!("ss:member:{}:roles", row.user), "", look.palette())
            .icon(Icon::Add)
            .variant(ButtonVariant::Ghost)
            .button_size(ButtonSize::Small)
            .accessibility_label(t("Agregar o quitar roles"))
            .tooltip(t("Roles"))
            .render();
        pills.push(
            Popover::new(
                add,
                Element::column(vec![
                    line(t("Roles"), 12.0, look.muted).font_family(FontFamily::SansBold),
                    role_choices(choices, look),
                ])
                .gap(6.0)
                .padding(4.0),
                row.picker_open,
                "ss:role-picker:dismiss",
                look.menu_palette(),
            )
            .placement(PopoverPlacement::BottomStart)
            .width(240.0)
            .render()
            .centered_y(),
        );
    }
    let kebab = menu(
        Button::new(format!("ss:member:{}:menu", row.user), "", look.palette())
            .icon(Icon::More)
            .variant(ButtonVariant::Ghost)
            .button_size(ButtonSize::Small)
            .accessibility_label(tr!("Más opciones para {name}", name = row.name))
            .render(),
        &row.menu,
        row.menu_open,
        "ss:member-menu:dismiss",
        look,
    );
    let body = Element::panel(
        Element::row(vec![
            Element::row(vec![
                crate::avatar_with_image(&row.name, row.accent, 32.0, row.image.clone())
                    .centered_y(),
                Element::column(name).gap(1.0).flex(1.0).centered_y(),
            ])
            .gap(10.0)
            .flex(1.0),
            Element::row(pills).gap(6.0).size(Some(roles), None),
            Element::row(vec![kebab.centered_y()]).size(Some(ACTIONS_COLUMN), None),
        ])
        .gap(12.0)
        .padding(8.0),
    )
    .size(None, Some(47.0))
    .radius(8.0)
    .hover_background(look.hover)
    .accessible(limn::Access::new(limn::Role::ListItem).name(row.name.clone()));
    Element::column(vec![
        context_menu(
            &format!("ss:member:{}:context", row.user),
            body,
            &row.menu,
            look,
        ),
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border),
    ])
}

/// A person in a short list: a role's members, or the people to add to it.
#[derive(Clone, Debug, PartialEq)]
pub struct PersonLine {
    pub name: String,
    pub handle: String,
    pub accent: Color,
    /// Their picture, once loaded.
    pub image: Option<limn::RasterImage>,
}

impl PersonLine {
    /// Their picture, or their initial on their accent, `size` across.
    fn avatar(&self, size: f32) -> Element {
        crate::avatar_with_image(&self.name, self.accent, size, self.image.clone())
    }
}

/// A member of a role, with the button that takes the role away from them.
pub fn role_member_row(person: &PersonLine, remove: &str, enabled: bool, look: &Look) -> Element {
    Element::row(vec![
        person.avatar(24.0).centered_y(),
        strong_line(&person.name, 15.0, look.text).centered_y(),
        line(&person.handle, 14.0, look.muted)
            .flex(1.0)
            .centered_y(),
        Button::new(remove, "", look.palette())
            .icon(Icon::CloseCircle)
            .variant(ButtonVariant::Ghost)
            .button_size(ButtonSize::Small)
            .accessibility_label(tr!("Quitar el rol a {name}", name = person.name))
            .tooltip(t("Quitar"))
            .enabled(enabled)
            .render(),
    ])
    .gap(8.0)
    .padding(6.0)
    .size(None, Some(44.0))
}

/// A person to choose, with a checkbox: the dialog that adds members to a role.
pub fn pick_row(id: &str, person: &PersonLine, checked: bool, look: &Look) -> Element {
    Element::row(vec![
        Checkbox::new(id, checked, look.menu_palette())
            .accessibility_label(person.name.clone())
            .render()
            .centered_y(),
        person.avatar(24.0).centered_y(),
        strong_line(&person.name, 15.0, look.text).centered_y(),
        line(&person.handle, 14.0, look.muted)
            .flex(1.0)
            .centered_y(),
    ])
    .gap(10.0)
    .padding(4.0)
    .size(None, Some(40.0))
}

/// Someone banned from the server.
#[derive(Clone, Debug, PartialEq)]
pub struct BanLine {
    pub user: UserId,
    pub person: PersonLine,
    pub reason: Option<String>,
    /// Why the ban cannot be revoked, when it cannot.
    pub denied: Option<String>,
}

/// A row of the bans page, with the button that revokes the ban.
pub fn ban_row(row: &BanLine, look: &Look) -> Element {
    let mut copy = vec![Element::row(vec![
        strong_line(&row.person.name, 15.0, look.text),
        line(&row.person.handle, 13.0, look.muted),
    ])
    .gap(8.0)];
    copy.push(line(
        row.reason.as_deref().map_or_else(
            || t("Sin motivo").to_owned(),
            |reason| tr!("Motivo: {reason}", reason),
        ),
        13.0,
        look.muted,
    ));
    Element::column(vec![
        Element::row(vec![
            row.person.avatar(32.0).centered_y(),
            Element::column(copy).gap(2.0).flex(1.0).centered_y(),
            Button::new(
                format!("ss:ban:{}:revoke", row.user),
                t("Revocar veto"),
                look.palette(),
            )
            .variant(ButtonVariant::Outline)
            .button_size(ButtonSize::Small)
            .enabled(row.denied.is_none())
            .render()
            .centered_y(),
        ])
        .gap(12.0)
        .padding(10.0)
        .size(None, Some(60.0)),
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border),
    ])
}

/// An invitation of the server, as its page lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct InviteLine {
    pub code: String,
    /// Who made it, when known.
    pub inviter: Option<PersonLine>,
    /// Where it leads: "#general".
    pub channel: Option<String>,
    /// "3 de 10 usos", or "Usos desconocidos".
    pub uses: String,
    /// "Vence: Hoy a las 14:05", or "No vence".
    pub expires: String,
    /// Why it cannot be revoked, when it cannot.
    pub denied: Option<String>,
    /// It has a link to copy.
    pub copy: bool,
}

/// A row of the invitations page, with the buttons that copy its link
/// and revoke it.
pub fn invite_row(row: &InviteLine, look: &Look) -> Element {
    let mut title = vec![strong_line(&row.code, 15.0, look.text)];
    if let Some(channel) = &row.channel {
        title.push(line(channel, 13.0, look.muted).flex(1.0).centered_y());
    }
    let who = row.inviter.as_ref().map_or_else(
        || avatar("?", look.faint, 32.0),
        |person| person.avatar(32.0),
    );
    let by = row.inviter.as_ref().map_or_else(
        || t("Creador desconocido").to_owned(),
        |person| tr!("Creada por {name}", name = person.name),
    );
    list_row(
        who,
        vec![
            Element::row(title).gap(8.0),
            line(by, 13.0, look.muted),
            line(format!("{} · {}", row.uses, row.expires), 13.0, look.muted),
        ],
        row.copy
            .then(|| {
                Button::new(format!("ss:invite:{}:copy", row.code), "", look.palette())
                    .icon(Icon::Copy)
                    .variant(ButtonVariant::Outline)
                    .button_size(ButtonSize::Small)
                    .accessibility_label(tr!("Copiar el enlace de {code}", code = row.code))
                    .tooltip(t("Copiar enlace"))
                    .render()
            })
            .into_iter()
            .chain([Button::new(
                format!("ss:invite:{}:revoke", row.code),
                t("Revocar"),
                look.palette(),
            )
            .variant(ButtonVariant::Outline)
            .button_size(ButtonSize::Small)
            .enabled(row.denied.is_none())
            .accessibility_label(tr!("Revocar la invitación {code}", code = row.code))
            .render()])
            .collect(),
        78.0,
        look,
    )
}

/// A webhook or an integration of the server, as its page lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemLine {
    /// What its controls are named by: `webhook:{id}`, `integration:{id}`.
    pub key: String,
    pub name: String,
    /// Where it posts, what it is, who made it.
    pub notes: Vec<String>,
    pub glyph: Icon,
    /// Its buttons: the action its id ends with, its icon, and its label.
    pub actions: Vec<(&'static str, Icon, &'static str)>,
    /// Why its buttons are off, when they are.
    pub denied: Option<String>,
}

/// A row of the integrations page: its mark, its name and notes, and icon
/// buttons named `ss:{key}:{action}`.
pub fn item_row(row: &ItemLine, look: &Look) -> Element {
    let lead = Element::panel(crate::centered(icon(row.glyph, look.muted)))
        .size(Some(32.0), Some(32.0))
        .radius(16.0)
        .background(look.hover);
    let mut copy = vec![strong_line(&row.name, 15.0, look.text)];
    if !row.notes.is_empty() {
        copy.push(line(row.notes.join(" · "), 13.0, look.muted));
    }
    let buttons = row
        .actions
        .iter()
        .map(|(action, glyph, label)| {
            Button::new(format!("ss:{}:{action}", row.key), "", look.palette())
                .icon(*glyph)
                .variant(ButtonVariant::Outline)
                .button_size(ButtonSize::Small)
                .enabled(row.denied.is_none())
                .accessibility_label(format!("{label} {}", row.name))
                .tooltip(*label)
                .render()
        })
        .collect();
    list_row(lead, copy, buttons, 60.0, look)
}

/// A custom emoji or sticker of the server, as its page lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct ExpressionLine {
    /// What its controls are named by: `emoji:{id}` or `sticker:{id}`.
    pub key: String,
    /// ":ferris:" for an emoji, the name for a sticker.
    pub name: String,
    /// Animated, restricted to roles, unavailable.
    pub notes: Vec<String>,
    /// Its picture, when loaded; else a placeholder.
    pub preview: Option<limn::RasterImage>,
    pub denied: Option<String>,
}

/// A row of the emoji or sticker page: its picture, its name, and the
/// buttons that rename and delete it.
pub fn expression_row(row: &ExpressionLine, look: &Look) -> Element {
    let preview = row.preview.clone().map_or_else(
        || {
            Element::panel(crate::centered(icon(Icon::Image, look.faint)))
                .radius(6.0)
                .background(look.hover)
        },
        Element::raster_image,
    );
    let mut copy = vec![strong_line(&row.name, 15.0, look.text)];
    if !row.notes.is_empty() {
        copy.push(line(row.notes.join(" · "), 13.0, look.muted));
    }
    // Icons, so a narrow page still has room for the name.
    let button = |action: &str, glyph: Icon, label: &str| {
        Button::new(format!("ss:{}:{action}", row.key), "", look.palette())
            .icon(glyph)
            .variant(ButtonVariant::Outline)
            .button_size(ButtonSize::Small)
            .enabled(row.denied.is_none())
            .accessibility_label(format!("{label} {}", row.name))
            .tooltip(label)
            .render()
    };
    list_row(
        preview.size(Some(32.0), Some(32.0)),
        copy,
        vec![
            button("rename", Icon::Edit, t("Renombrar")),
            button("delete", Icon::Trash, t("Eliminar")),
        ],
        60.0,
        look,
    )
}

/// A row of a list page, `height` tall: a leading mark, lines of text,
/// buttons, and the rule under it.
pub(crate) fn list_row(
    lead: Element,
    copy: Vec<Element>,
    buttons: Vec<Element>,
    height: f32,
    look: &Look,
) -> Element {
    let mut row = vec![
        lead.centered_y(),
        Element::column(copy).gap(2.0).flex(1.0).centered_y(),
    ];
    row.extend(buttons.into_iter().map(Element::centered_y));
    Element::column(vec![
        Element::row(row)
            .gap(12.0)
            .padding(10.0)
            .size(None, Some(height)),
        Element::panel(Element::spacer())
            .size(None, Some(1.0))
            .background(look.border),
    ])
}

/// Whether an audit entry made, changed or removed something: its icon's
/// badge is green, amber or red.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditKind {
    Create,
    Update,
    Delete,
}

impl AuditKind {
    pub fn of(action: &AuditAction) -> Self {
        match action {
            AuditAction::ChannelCreated { .. }
            | AuditAction::RoleCreated { .. }
            | AuditAction::InviteCreated { .. }
            | AuditAction::EmojiCreated { .. }
            | AuditAction::StickerCreated { .. } => Self::Create,
            AuditAction::ChannelDeleted { .. }
            | AuditAction::RoleDeleted { .. }
            | AuditAction::MemberKicked { .. }
            | AuditAction::MemberBanned { .. }
            | AuditAction::InviteDeleted { .. }
            | AuditAction::EmojiDeleted { .. }
            | AuditAction::StickerDeleted { .. } => Self::Delete,
            _ => Self::Update,
        }
    }
}

/// What the filter of actions calls each kind of entry, in the order it
/// lists them.
pub const AUDIT_ACTIONS: [(AuditActionKind, &str); 22] = [
    (AuditActionKind::ServerUpdated, es("Actualizar el servidor")),
    (AuditActionKind::ChannelCreated, es("Crear un canal")),
    (AuditActionKind::ChannelUpdated, es("Actualizar un canal")),
    (AuditActionKind::ChannelDeleted, es("Eliminar un canal")),
    (
        AuditActionKind::PermissionsChanged,
        es("Actualizar los permisos de un canal"),
    ),
    (AuditActionKind::RoleCreated, es("Crear un rol")),
    (AuditActionKind::RoleUpdated, es("Actualizar un rol")),
    (AuditActionKind::RoleDeleted, es("Eliminar un rol")),
    (
        AuditActionKind::MemberRolesChanged,
        es("Actualizar los roles de un miembro"),
    ),
    (AuditActionKind::NickChanged, es("Cambiar un apodo")),
    (AuditActionKind::MemberKicked, es("Expulsar a un miembro")),
    (AuditActionKind::MemberBanned, es("Vetar a un miembro")),
    (AuditActionKind::MemberUnbanned, es("Revocar un veto")),
    (AuditActionKind::MemberTimedOut, es("Aislar a un miembro")),
    (AuditActionKind::InviteCreated, es("Crear una invitación")),
    (AuditActionKind::InviteDeleted, es("Revocar una invitación")),
    (AuditActionKind::EmojiCreated, es("Agregar un emoji")),
    (AuditActionKind::EmojiUpdated, es("Actualizar un emoji")),
    (AuditActionKind::EmojiDeleted, es("Eliminar un emoji")),
    (AuditActionKind::StickerCreated, es("Agregar un sticker")),
    (AuditActionKind::StickerUpdated, es("Actualizar un sticker")),
    (AuditActionKind::StickerDeleted, es("Eliminar un sticker")),
];

/// What the filter of actions calls a kind of entry: one of [`AUDIT_ACTIONS`].
pub fn audit_label(kind: AuditActionKind) -> &'static str {
    AUDIT_ACTIONS
        .iter()
        .find(|(of, _)| *of == kind)
        .map_or("", |(_, label)| t(label))
}

/// A run of an audit sentence, in bold when it names what was changed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Span {
    pub text: String,
    pub strong: bool,
}

fn plain(text: impl Into<String>) -> Span {
    Span {
        text: text.into(),
        strong: false,
    }
}

fn strong(text: impl Into<String>) -> Span {
    Span {
        text: text.into(),
        strong: true,
    }
}

/// What an entry says happened: "Ana creó el rol **Moderación**". `person`
/// names someone by id; names of roles and channels are the ones they had.
pub fn audit_sentence(
    actor: &str,
    action: &AuditAction,
    person: &dyn Fn(UserId) -> String,
) -> Vec<Span> {
    let (verb, object) = match action {
        AuditAction::ServerUpdated => (t("actualizó el servidor"), None),
        AuditAction::ChannelCreated { name, .. } => (t("creó el canal "), Some(name.clone())),
        AuditAction::ChannelUpdated { name, .. } => (t("actualizó el canal "), Some(name.clone())),
        AuditAction::ChannelDeleted { name } => (t("eliminó el canal "), Some(name.clone())),
        AuditAction::PermissionsChanged { name, .. } => {
            (t("actualizó los permisos del canal "), Some(name.clone()))
        }
        AuditAction::RoleCreated { name, .. } => (t("creó el rol "), Some(name.clone())),
        AuditAction::RoleUpdated { name, .. } => (t("actualizó el rol "), Some(name.clone())),
        AuditAction::RoleDeleted { name } => (t("eliminó el rol "), Some(name.clone())),
        AuditAction::MemberRolesChanged { user, .. } => {
            (t("actualizó los roles de "), Some(person(*user)))
        }
        AuditAction::NickChanged { user, .. } => (t("cambió el apodo de "), Some(person(*user))),
        AuditAction::MemberKicked { user } => (t("expulsó a "), Some(person(*user))),
        AuditAction::MemberBanned { user } => (t("vetó a "), Some(person(*user))),
        AuditAction::MemberUnbanned { user } => (t("revocó el veto a "), Some(person(*user))),
        AuditAction::InviteDeleted { code } => (t("revocó la invitación "), Some(code.clone())),
        AuditAction::EmojiUpdated { name } => (t("actualizó el emoji "), Some(format!(":{name}:"))),
        AuditAction::EmojiDeleted { name } => (t("eliminó el emoji "), Some(format!(":{name}:"))),
        AuditAction::StickerUpdated { name } => (t("actualizó el sticker "), Some(name.clone())),
        AuditAction::StickerDeleted { name } => (t("eliminó el sticker "), Some(name.clone())),
        AuditAction::MemberTimedOut {
            user,
            until: Some(_),
        } => (t("aisló a "), Some(person(*user))),
        AuditAction::MemberTimedOut { user, until: None } => {
            (t("quitó el aislamiento a "), Some(person(*user)))
        }
        AuditAction::InviteCreated { code } => (t("creó la invitación "), Some(code.clone())),
        AuditAction::EmojiCreated { name } => (t("agregó el emoji "), Some(format!(":{name}:"))),
        AuditAction::StickerCreated { name } => (t("agregó el sticker "), Some(name.clone())),
        AuditAction::Other { what } => return vec![strong(actor), plain(format!(" {what}"))],
    };
    let mut spans = vec![strong(actor), plain(format!(" {verb}"))];
    if let Some(object) = object {
        spans.push(strong(object));
    }
    spans
}

/// The numbered lines an entry opens to: the roles given and taken, the new
/// nickname, the reason. Empty when there is nothing more to say.
pub fn audit_details(
    action: &AuditAction,
    reason: Option<&str>,
    role: &dyn Fn(RoleId) -> String,
) -> Vec<Vec<Span>> {
    let mut lines = Vec::new();
    match action {
        AuditAction::MemberRolesChanged { added, removed, .. } => {
            for id in added {
                lines.push(vec![plain(t("Agregó el rol ")), strong(role(*id))]);
            }
            for id in removed {
                lines.push(vec![plain(t("Quitó el rol ")), strong(role(*id))]);
            }
        }
        AuditAction::NickChanged {
            nick: Some(nick), ..
        } => {
            lines.push(vec![plain(t("Cambió el apodo a ")), strong(nick.clone())]);
        }
        AuditAction::NickChanged { nick: None, .. } => lines.push(vec![plain(t("Quitó el apodo"))]),
        _ => {}
    }
    if let Some(reason) = reason.filter(|reason| !reason.trim().is_empty()) {
        lines.push(vec![plain(t("Motivo: ")), strong(reason)]);
    }
    lines
}

/// An entry of the audit log as a card.
#[derive(Clone, Debug, PartialEq)]
pub struct AuditLine {
    /// The id its header reports, to open or close its details.
    pub id: String,
    pub actor: String,
    pub accent: Color,
    /// The actor's picture, once loaded.
    pub image: Option<limn::RasterImage>,
    pub kind: AuditKind,
    pub sentence: Vec<Span>,
    /// When, as the timeline writes it: "Hoy a las 14:05".
    pub when: String,
    pub details: Vec<Vec<Span>>,
    pub expanded: bool,
}

fn spans(runs: &[Span], size: f32, look: &Look) -> Element {
    Element::formatted_text(
        runs.iter()
            .map(|run| {
                if run.strong {
                    TextSpan::new(run.text.clone(), look.text, size, FontFamily::SansMedium)
                } else {
                    TextSpan::new(run.text.clone(), look.muted, size, FontFamily::Sans)
                }
            })
            .collect(),
    )
}

/// An audit log entry, 72.5 px closed: the kind's icon with its colored
/// badge, who did it, what, when, and a chevron when it has details.
pub fn audit_row(row: &AuditLine, look: &Look) -> Element {
    let (glyph, badge) = match row.kind {
        AuditKind::Create => (Icon::Add, look.online),
        AuditKind::Update => (Icon::Refresh, look.idle),
        AuditKind::Delete => (Icon::Trash, look.danger),
    };
    let mut header = vec![
        Element::panel(crate::centered(icon(glyph, badge)))
            .size(Some(28.0), Some(28.0))
            .radius(14.0)
            .background(Look::alpha(badge, 36))
            .centered_y(),
        crate::avatar_with_image(&row.actor, row.accent, 40.0, row.image.clone()).centered_y(),
        Element::column(vec![
            spans(&row.sentence, 16.0, look),
            line(&row.when, 14.0, look.faint),
        ])
        .gap(3.0)
        .flex(1.0)
        .centered_y(),
    ];
    let open = !row.details.is_empty();
    if open {
        header.push(
            icon(
                if row.expanded {
                    Icon::ChevronDown
                } else {
                    Icon::ChevronRight
                },
                look.muted,
            )
            .centered_y(),
        );
    }
    let header = Element::row(header).gap(12.0).padding(14.0);
    let header = if open {
        Element::clickable(row.id.clone(), header)
            .radius(8.0)
            .hover_background(look.hover)
            .accessible(
                limn::Access::new(limn::Role::Button)
                    .name(
                        row.sentence
                            .iter()
                            .map(|run| run.text.as_str())
                            .collect::<String>(),
                    )
                    .expanded(row.expanded),
            )
    } else {
        header
    };
    // 72.5 px as measured, taller when a long sentence wraps.
    let mut parts = vec![header.min_size(0.0, 72.5)];
    if row.expanded {
        let lines = row
            .details
            .iter()
            .enumerate()
            .map(|(index, detail)| {
                let mut runs = vec![TextSpan::new(
                    format!("{:02} — ", index + 1),
                    look.idle,
                    14.0,
                    FontFamily::SansBold,
                )];
                runs.extend(detail.iter().map(|run| {
                    TextSpan::new(
                        run.text.clone(),
                        if run.strong { look.text } else { look.muted },
                        15.0,
                        if run.strong {
                            FontFamily::SansMedium
                        } else {
                            FontFamily::Sans
                        },
                    )
                }));
                Element::formatted_text(runs)
            })
            .collect();
        parts.push(
            Element::row(vec![
                gap(54.0, 0.0),
                Element::column(lines).gap(8.0).flex(1.0),
            ])
            .padding(14.0),
        );
    }
    Element::panel(Element::column(parts))
        .radius(8.0)
        .background(look.raised)
        .border(look.border, 1.0)
}

/// How the server shows itself, beside its profile page: its mark, its name,
/// its description and how many are in it.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerCard {
    pub name: String,
    pub initials: String,
    pub accent: Color,
    pub description: Option<String>,
    pub online: usize,
    pub members: usize,
    // Server administration, rest (P7).
    /// A picture chosen and not saved yet, shown in place of the initials.
    pub picture: Option<limn::RasterImage>,
    /// What the profile says it is about: each trait's emoji and label.
    pub traits: Vec<String>,
    // Icon, banner and community (2026-10-19).
    /// Its banner, in place of the accent over the card.
    pub banner: Option<limn::RasterImage>,
}

/// The preview card of the profile page, 318.5 px wide.
pub fn server_card(card: &ServerCard, look: &Look) -> Element {
    let initials = || crate::centered(strong_line(&card.initials, 18.0, Color::rgb(255, 255, 255)));
    let mark = Element::panel(
        card.picture
            .clone()
            .map_or_else(initials, Element::raster_image),
    )
    .clip_children()
    .size(Some(64.0), Some(64.0))
    .radius(18.0)
    .background(card.accent)
    .border(look.raised, 4.0);
    let mut copy = vec![
        strong_line(&card.name, 18.0, look.text),
        Element::row(vec![
            color_dot(look.online, 8.0).centered_y(),
            line(
                tr!("{count} en línea", count = card.online),
                13.0,
                look.muted,
            )
            .centered_y(),
            color_dot(look.offline, 8.0).centered_y(),
            line(
                if card.members == 1 {
                    t("1 miembro").to_owned()
                } else {
                    tr!("{count} miembros", count = card.members)
                },
                13.0,
                look.muted,
            )
            .centered_y(),
        ])
        .gap(6.0),
    ];
    if let Some(description) = &card.description {
        copy.push(text(description.clone(), 13.0, look.muted));
    }
    if !card.traits.is_empty() {
        copy.push(
            Element::row(
                card.traits
                    .iter()
                    .map(|found| {
                        Element::panel(line(found.clone(), 12.0, look.text).padding(6.0))
                            .radius(6.0)
                            .background(look.canvas)
                    })
                    .collect(),
            )
            .gap(6.0),
        );
    }
    Element::panel(Element::column(vec![
        Element::stack(vec![
            Element::column(vec![
                Element::panel(card.banner.clone().map_or_else(Element::spacer, |banner| {
                    // The whole width, cropped around the middle.
                    Element::raster_image(banner.cover()).size(Some(318.5), Some(96.0))
                }))
                .clip_children()
                .size(None, Some(96.0))
                .background(Look::alpha(card.accent, 160)),
                gap(0.0, 36.0),
            ]),
            Element::column(vec![
                gap(0.0, 60.0),
                Element::row(vec![gap(16.0, 0.0), mark, Element::spacer().flex(1.0)]),
            ]),
        ]),
        Element::column(copy).gap(6.0).padding(16.0),
    ]))
    .size(Some(318.5), None)
    .radius(12.0)
    .background(look.raised)
    .border(look.border, 1.0)
    .clip_children()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame as lay, texts};
    use limn::Size;

    const LOOK: Look = Look::DARK;

    #[test]
    fn the_catalog_names_every_permission_once() {
        let mut seen = Permissions::NONE;
        for (_, permissions) in PERMISSION_GROUPS {
            for (permission, label, description) in *permissions {
                assert!(!seen.intersects(*permission), "{label} twice");
                assert!(!label.is_empty() && !description.is_empty());
                seen = seen | *permission;
            }
        }
        assert_eq!(seen, Permissions::ALL);
        assert_eq!(permission_of("31"), Some(Permissions::ADMINISTRATOR));
        assert_eq!(permission_id(Permissions::KICK), "ss:perm:10");
        assert_eq!(permission_of("34"), Some(Permissions::MANAGE_WEBHOOKS));
        assert_eq!(permission_of("35"), Some(Permissions::USE_APP_COMMANDS));
        assert_eq!(permission_of("36"), Some(Permissions::SEND_POLLS));
        assert_eq!(permission_of("37"), Some(Permissions::PIN_MESSAGES));
        assert_eq!(permission_of("38"), Some(Permissions::EXTERNAL_STICKERS));
        // Scheduled events (eventos-programados).
        assert_eq!(permission_of("39"), Some(Permissions::CREATE_EVENTS));
        assert_eq!(permission_of("40"), Some(Permissions::MANAGE_EVENTS));
        assert_eq!(permission_of("41"), None);
    }

    #[test]
    fn searching_permissions_keeps_their_groups() {
        let found = permissions_matching("MIEMBROS");
        let groups: Vec<&str> = found.iter().map(|(group, _)| *group).collect();
        assert!(groups.contains(&"Permisos de membresía"), "{groups:?}");
        assert!(found.iter().flat_map(|(_, permissions)| permissions).all(
            |(_, label, description)| {
                format!("{label} {description}")
                    .to_lowercase()
                    .contains("miembros")
            }
        ));
        assert!(permissions_matching("zzz").is_empty());
        assert_eq!(permissions_matching("").len(), PERMISSION_GROUPS.len());
    }

    #[test]
    fn hex_reads_six_digits_with_or_without_a_hash() {
        assert_eq!(parse_hex("#3498db"), Some(Rgb(0x34_98_DB)));
        assert_eq!(parse_hex(" E74C3C "), Some(Rgb(0xE7_4C_3C)));
        assert_eq!(parse_hex("3498d"), None);
        assert_eq!(parse_hex("zz98db"), None);
        assert_eq!(hex(Rgb(0x0A_0B_0C)), "0a0b0c");
    }

    #[test]
    fn audit_sentences_name_who_and_what() {
        let person = |user: UserId| format!("persona{user}");
        let said = |action: AuditAction| -> String {
            audit_sentence("Ana", &action, &person)
                .into_iter()
                .map(|span| span.text)
                .collect()
        };
        assert_eq!(
            said(AuditAction::RoleCreated {
                role: RoleId(5),
                name: "Temporal".into()
            }),
            "Ana creó el rol Temporal"
        );
        assert_eq!(
            said(AuditAction::MemberBanned { user: UserId(7) }),
            "Ana vetó a persona7"
        );
        assert_eq!(
            said(AuditAction::ServerUpdated),
            "Ana actualizó el servidor"
        );
        let spans = audit_sentence(
            "Ana",
            &AuditAction::RoleDeleted {
                name: "Temporal".into(),
            },
            &person,
        );
        assert!(spans.last().unwrap().strong, "the object is in bold");
        let role = |role: RoleId| format!("rol{role}");
        let details = audit_details(
            &AuditAction::MemberRolesChanged {
                user: UserId(7),
                added: vec![RoleId(1)],
                removed: vec![RoleId(2)],
            },
            Some("spam"),
            &role,
        );
        assert_eq!(details.len(), 3);
        assert_eq!(details[1][1].text, "rol2");
        assert!(audit_details(&AuditAction::ServerUpdated, None, &role).is_empty());
        assert_eq!(
            AuditKind::of(&AuditAction::MemberKicked { user: UserId(1) }),
            AuditKind::Delete
        );
    }

    #[test]
    fn the_shell_centers_the_block_and_narrows_the_page_first() {
        let wide = Metrics::new(1361.0, false);
        assert!((wide.margin - 132.5).abs() < 1.0, "{wide:?}");
        assert_eq!(wide.content, CONTENT_WIDTH);
        let members = Metrics::new(1361.0, true);
        assert!(members.content > CONTENT_WIDTH && members.content <= WIDE_CONTENT_WIDTH);
        let narrow = Metrics::new(900.0, false);
        assert_eq!(narrow.margin, EDGE);
        assert!(narrow.content < CONTENT_WIDTH);
        let sidebar = sidebar(
            &[
                SidebarEntry::Heading("Laboratorio".into()),
                SidebarEntry::Item {
                    id: "ss:page:profile".into(),
                    label: "Perfil del servidor".into(),
                    selected: true,
                    danger: false,
                },
                SidebarEntry::Separator,
                SidebarEntry::Item {
                    id: "ss:delete-server".into(),
                    label: "Eliminar servidor".into(),
                    selected: false,
                    danger: true,
                },
            ],
            "Simulación local",
            &LOOK,
        );
        let size = Size::new(1361.0, 797.0);
        let shell = shell(
            size,
            wide,
            sidebar,
            page(
                "ss:scroll",
                wide.content,
                vec![page_title("Perfil", None, &LOOK)],
            ),
            Some(unsaved_bar(UnsavedBar::default(), &LOOK)),
            &LOOK,
        );
        let (_, frame) = lay(shell, size);
        let ids = controls(&frame);
        for id in [
            "ss:page:profile",
            "ss:delete-server",
            "ss:close",
            "ss:reset",
            "ss:save",
        ] {
            assert!(ids.contains(&id.to_owned()), "{id} in {ids:?}");
        }
        let item = frame
            .regions
            .iter()
            .find(|region| region.id == "ss:page:profile")
            .unwrap();
        assert!((item.rect.x - 132.5).abs() < 1.0 && item.rect.width == SIDEBAR_WIDTH);
        assert_eq!(item.rect.height, ITEM_HEIGHT);
        let save = frame
            .regions
            .iter()
            .find(|region| region.id == "ss:save")
            .unwrap();
        assert!(
            save.rect.y > 797.0 - BAR_BOTTOM - BAR_HEIGHT,
            "the bar floats at the foot"
        );
        let painted = texts(&frame);
        assert!(painted.contains(&"LABORATORIO".to_owned()));
        assert!(painted.contains(&"ESC".to_owned()));
    }

    #[test]
    fn a_locked_role_still_opens_but_does_not_drag() {
        let row = RoleLine {
            role: RoleId(4),
            name: "Moderación".into(),
            color: Color::rgb(1, 2, 3),
            members: 3,
            movable: false,
            locked: Some("Está por encima de tu rol".into()),
            menu: vec![MenuAction::new("ss:role:4:delete", "Eliminar")
                .danger()
                .denied(Some("Está por encima de tu rol".into()))],
            menu_open: false,
            drop: None,
        };
        let (_, frame) = lay(role_row(&row, &LOOK), Size::new(700.0, 80.0));
        let ids = controls(&frame);
        assert!(ids.contains(&"ss:role:4".to_owned()));
        assert!(ids.contains(&"ss:role:4:edit".to_owned()));
        assert!(texts(&frame).contains(&"3".to_owned()));
    }

    #[test]
    fn the_tabs_take_the_width_they_are_said_to() {
        let labels = ["Mostrar", "Permisos", "Gestionar miembros (10)"];
        let items: Vec<_> = labels
            .iter()
            .enumerate()
            .map(|(index, label)| (format!("tab{index}"), label.to_string(), index == 0, true))
            .collect();
        let (_, frame) = lay(tabs(&items, &LOOK), Size::new(800.0, 60.0));
        let tab = |id: &str| {
            frame
                .regions
                .iter()
                .find(|region| region.id == id)
                .unwrap()
                .rect
        };
        let (first, last) = (tab("tab0"), tab("tab2"));
        assert!(
            (last.x + last.width - first.x - tabs_width(&labels)).abs() <= 1.0,
            "{first:?} to {last:?}"
        );
        // Two digits do not fit the editor: the view says it shorter.
        assert!(tabs_width(&labels) > EDITOR_WIDTH);
    }

    #[test]
    fn swatches_offer_presets_a_custom_color_and_the_default() {
        let view = Swatches {
            prefix: "ss:color".into(),
            chosen: Some(COLORS[2]),
            default: Some(DEFAULT_ROLE_COLOR),
            custom: None,
            enabled: true,
            narrow: false,
            picked: None,
        };
        assert_eq!(swatches_width(true), EDITOR_WIDTH);
        let (_, frame) = lay(swatches(&view, &LOOK), Size::new(472.0, 80.0));
        let ids = controls(&frame);
        assert!(ids.contains(&"ss:color:default".to_owned()));
        assert!(ids.contains(&"ss:color:custom".to_owned()));
        assert!(ids.contains(&"ss:color:3498db".to_owned()));
        assert_eq!(ids.len(), 22, "{ids:?}");
        // The platform hears which one is chosen.
        let chosen = |id: &str| {
            frame
                .access
                .iter()
                .find(|node| node.id == id)
                .is_some_and(|node| node.selected == Some(true))
        };
        assert!(chosen("ss:color:3498db"));
        assert!(!chosen("ss:color:1abc9c") && !chosen("ss:color:default"));
        let disabled = Swatches {
            enabled: false,
            ..view
        };
        let (_, laid) = lay(swatches(&disabled, &LOOK), Size::new(472.0, 80.0));
        assert!(controls(&laid).is_empty());
        // The custom swatch opens a picker of any color, which does nothing
        // without the permission.
        let open = Swatches {
            custom: Some("3498db".into()),
            ..disabled
        };
        let (_, laid) = lay(swatches(&open, &LOOK), Size::new(472.0, 400.0));
        for part in ["ss:color:area", "ss:color:hue", "ss:color:hex"] {
            let usable = laid.regions.iter().any(|region| region.id == part)
                && !laid
                    .access
                    .iter()
                    .any(|node| node.id == part && node.disabled);
            assert!(!usable, "{part}");
        }
        let (_, laid) = lay(
            swatches(
                &Swatches {
                    enabled: true,
                    ..open
                },
                &LOOK,
            ),
            Size::new(472.0, 400.0),
        );
        for part in ["ss:color:area", "ss:color:hue", "ss:color:hex"] {
            assert!(
                laid.regions.iter().any(|region| region.id == part),
                "{part}"
            );
        }
    }

    #[test]
    fn a_member_row_offers_roles_and_its_menu() {
        let row = MemberLine {
            user: UserId(9),
            name: "Ana".into(),
            handle: "ana".into(),
            ink: LOOK.text,
            accent: LOOK.accent,
            image: None,
            owner: false,
            roles: vec![
                ("Moderación".into(), LOOK.danger),
                ("Bots".into(), LOOK.online),
                ("Colaboración".into(), LOOK.idle),
            ],
            choices: Some(vec![RoleChoice {
                id: "ss:member:9:role:21".into(),
                name: "Moderación".into(),
                color: LOOK.danger,
                checked: true,
                denied: None,
            }]),
            picker_open: true,
            menu: vec![MenuAction::new("ss:member:9:kick", "Expulsar a Ana").danger()],
            menu_open: false,
        };
        let (_, frame) = lay(member_row(&row, 820.0, &LOOK), Size::new(820.0, 400.0));
        let ids = controls(&frame);
        assert!(ids.contains(&"ss:member:9:roles".to_owned()));
        assert!(ids.contains(&"ss:member:9:menu".to_owned()));
        assert!(ids.contains(&"ss:member:9:role:21".to_owned()), "{ids:?}");
        let painted = texts(&frame);
        assert!(painted.contains(&"+1".to_owned()), "{painted:?}");
        // A narrow window keeps the name and shows fewer roles (2026-10-19).
        assert!(roles_column(400.0) < 160.0 && roles_column(1000.0) == ROLES_COLUMN);
        let (_, frame) = lay(member_row(&row, 400.0, &LOOK), Size::new(400.0, 400.0));
        let painted = texts(&frame);
        assert!(painted.contains(&"Ana".to_owned()) && painted.contains(&"+2".to_owned()));
    }

    #[test]
    fn an_audit_entry_opens_only_when_it_has_details() {
        let entry = AuditLine {
            id: "ss:audit:1".into(),
            actor: "Ana".into(),
            accent: LOOK.accent,
            image: None,
            kind: AuditKind::Update,
            sentence: vec![strong("Ana"), plain(" actualizó el servidor")],
            when: "Hoy a las 14:05".into(),
            details: Vec::new(),
            expanded: false,
        };
        let (_, frame) = lay(audit_row(&entry, &LOOK), Size::new(705.0, 100.0));
        assert!(controls(&frame).is_empty());
        let open = AuditLine {
            details: vec![vec![plain("Motivo: "), strong("spam")]],
            expanded: true,
            ..entry
        };
        let (_, laid) = lay(audit_row(&open, &LOOK), Size::new(705.0, 200.0));
        assert_eq!(controls(&laid), vec!["ss:audit:1".to_owned()]);
        let painted = texts(&laid).join("");
        assert!(
            painted.contains("01 — ") && painted.contains("spam"),
            "{painted}"
        );
    }

    #[test]
    fn invitation_and_expression_rows_name_their_controls_and_say_what_is_unknown() {
        let size = Size::new(705.0, 120.0);
        let invite = InviteLine {
            code: "Ab3dEf7h".into(),
            inviter: None,
            channel: Some("#general".into()),
            uses: "Usos desconocidos".into(),
            expires: "No vence".into(),
            denied: None,
            copy: true,
        };
        let (_, frame) = lay(invite_row(&invite, &LOOK), size);
        assert_eq!(
            controls(&frame),
            ["ss:invite:Ab3dEf7h:copy", "ss:invite:Ab3dEf7h:revoke"]
        );
        let painted = texts(&frame).join(" ");
        assert!(
            painted.contains("Creador desconocido") && painted.contains("Usos desconocidos"),
            "{painted}"
        );
        let mut emoji = ExpressionLine {
            key: "emoji:9000".into(),
            name: ":ferris:".into(),
            notes: vec!["Animado".into()],
            preview: None,
            denied: None,
        };
        let (_, frame) = lay(expression_row(&emoji, &LOOK), size);
        assert_eq!(
            controls(&frame),
            ["ss:emoji:9000:rename", "ss:emoji:9000:delete"]
        );
        assert!(texts(&frame).contains(&"Animado".to_owned()));
        // Denied, the buttons stay but do nothing.
        emoji.denied = Some("Este proveedor no permite gestionar emojis y stickers".into());
        let (_, frame) = lay(expression_row(&emoji, &LOOK), size);
        assert!(controls(&frame).is_empty());
        let webhook = ItemLine {
            key: "webhook:7".into(),
            name: "Avisos de CI".into(),
            notes: vec!["#general".into(), "Creado por Ana".into()],
            glyph: Icon::Bolt,
            actions: vec![
                ("edit", Icon::Edit, "Editar"),
                ("delete", Icon::Trash, "Eliminar"),
            ],
            denied: None,
        };
        let (_, frame) = lay(item_row(&webhook, &LOOK), size);
        assert_eq!(
            controls(&frame),
            ["ss:webhook:7:edit", "ss:webhook:7:delete"]
        );
        assert!(texts(&frame).contains(&"#general · Creado por Ana".to_owned()));
        assert_eq!(
            audit_label(AuditActionKind::EmojiDeleted),
            "Eliminar un emoji"
        );
        // Every kind but `Other` can be a filter, each once.
        let kinds: std::collections::HashSet<_> = AUDIT_ACTIONS.iter().map(|(k, _)| *k).collect();
        assert_eq!(kinds.len(), AUDIT_ACTIONS.len());
        assert!(!kinds.contains(&AuditActionKind::Other));
        let timed: String = audit_sentence(
            "Ana",
            &AuditAction::MemberTimedOut {
                user: UserId(3),
                until: None,
            },
            &|_| "Beto".into(),
        )
        .into_iter()
        .map(|span| span.text)
        .collect();
        assert_eq!(timed, "Ana quitó el aislamiento a Beto");
        let said: String = audit_sentence(
            "Ana",
            &AuditAction::InviteDeleted {
                code: "Ab3dEf7h".into(),
            },
            &|_| String::new(),
        )
        .into_iter()
        .map(|span| span.text)
        .collect();
        assert_eq!(said, "Ana revocó la invitación Ab3dEf7h");
    }
}
