//! The frame around the conversation: its header, the account's panel at
//! the foot of the window's left side, the voice connection above it, the line that
//! says how the connection is, and the panel of measurements.

use limn::{
    Button, ButtonSizing, ButtonVariant, Color, Component, ControlSize, Element, FontFamily, Icon,
    IconButton, PopoverPlacement, Tooltip, TooltipPlacement,
};

use rune_i18n::{t, tr};

use crate::{icon, line, strong_line, with_status, Look};

use crate::gap;

/// The box at the header's right that opens message search, wide and high.
pub const SEARCH: (f32, f32) = (258.5, 32.5);

/// The arrows of the history at the header's start, and the keys the
/// hints of the arrows and the search name (atajos-y-navegacion).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HeaderNav {
    /// There is somewhere to go back to, and forward.
    pub back: bool,
    pub forward: bool,
    pub back_keys: Option<String>,
    pub forward_keys: Option<String>,
    pub search_keys: Option<String>,
    /// A narrow window: smaller arrows, in place of the conversation's glyph.
    pub compact: bool,
    /// Room kept free at the header's end for the window's own buttons
    /// (barra-e-icono).
    pub end: f32,
}

/// IDs of the header's arrows.
pub const BACK: &str = "nav:back";
pub const FORWARD: &str = "nav:forward";

/// The conversation's header: the arrows of the history, its kind, its
/// name, its topic (a click shows it whole), and its tools: the
/// conversation's own (`tools`, such as the bell and the pinned messages),
/// the member list's toggle, and the search, a box while the header is
/// `wide` and an icon otherwise.
pub fn channel_header(
    glyph: Icon,
    title: &str,
    // With the pictures it names, drawn in its line.
    topic: Option<(&str, &[crate::identity::TextPicture])>,
    tools: Vec<Element>,
    members_open: Option<bool>,
    wide: bool,
    // What a thread hangs from, before its name: the parent's glyph and name.
    parent: Option<(Icon, &str)>,
    // The conversation's own face, in place of its glyph: a group's picture.
    face: Option<Element>,
    nav: &HeaderNav,
    look: &Look,
) -> Element {
    let arrow = |id: &str, glyph: Icon, hint: &str, keys: &Option<String>, enabled: bool| {
        // Without somewhere to go, well under the header's other icons: a
        // disabled button paints its palette's muted ink.
        let palette = limn::ComponentPalette {
            muted: Look::alpha(look.faint, 96),
            ..look.palette()
        };
        let mut button = IconButton::new(id, glyph, palette)
            .variant(ButtonVariant::Ghost)
            .size(if nav.compact {
                ControlSize::Dense
            } else {
                ControlSize::Regular
            })
            .tooltip(hint)
            .tooltip_placement(TooltipPlacement::Below)
            .ink(look.dim)
            .hover_ink(look.text);
        if let Some(keys) = keys {
            button = button.tooltip_shortcut(keys.clone());
        }
        if !enabled {
            button = button.disabled();
        }
        button.render().centered_y()
    };
    let mut parts = vec![
        gap(if nav.compact { 6.0 } else { 8.0 }, 0.0),
        arrow(BACK, Icon::ArrowLeft, t("Atrás"), &nav.back_keys, nav.back),
        arrow(
            FORWARD,
            Icon::ArrowRight,
            t("Adelante"),
            &nav.forward_keys,
            nav.forward,
        ),
        gap(if nav.compact { 8.0 } else { 10.0 }, 0.0),
    ];
    if let Some((parent_glyph, parent)) = parent {
        parts.extend([
            icon(parent_glyph, look.faint).large_icon(18.0).centered_y(),
            gap(8.0, 0.0),
            // The parent gives way first: the thread's own name matters more.
            line(parent, 16.0, look.muted)
                .max_width(160.0)
                .shrink_to_fit()
                .centered_y(),
            gap(8.0, 0.0),
            icon(Icon::ChevronRight, look.faint)
                .size(Some(12.0), Some(12.0))
                .centered_y(),
            gap(8.0, 0.0),
        ]);
    }
    // A narrow header gives the glyph's room to the arrows; a face stays.
    if let Some(face) = face {
        parts.extend([face.centered_y(), gap(14.0, 0.0)]);
    } else if !nav.compact {
        parts.extend([
            icon(glyph, look.dim).large_icon(18.0).centered_y(),
            gap(14.0, 0.0),
        ]);
    }
    parts.extend([strong_line(title, 16.0, look.text)
        .shrink_to_fit()
        .centered_y()]);
    if let Some((topic, pictures)) = topic {
        parts.push(gap(12.0, 0.0));
        parts.push(
            Element::panel(Element::spacer())
                .size(Some(1.0), Some(22.0))
                .background(look.border)
                .centered_y(),
        );
        parts.push(gap(12.0, 0.0));
        parts.push(
            Element::clickable(
                crate::channel_menu::ids::TOPIC,
                crate::text::line_with_pictures(
                    topic,
                    pictures,
                    14.0,
                    look.muted,
                    limn::INLINE_IMAGE_SCALE,
                )
                .unwrap_or_else(|| line(topic, 14.0, look.muted)),
            )
            .hover_foreground(look.text)
            .accessible(limn::Access::new(limn::Role::Button).name(t("Tema del canal")))
            .centered_y()
            .shrink_to_fit(),
        );
        // Past its text the header is free: it moves a window RUNE draws
        // the frame of (barra-e-icono).
        parts.push(Element::spacer().flex(1.0));
    } else {
        parts.push(Element::spacer().flex(1.0));
    }
    // The header's icons are 45 apart: 32 each and 13 between.
    for tool in tools {
        parts.push(gap(13.0, 0.0));
        parts.push(tool.centered_y());
    }
    if let Some(open) = members_open {
        parts.push(gap(13.0, 0.0));
        parts.push(
            IconButton::new("members:toggle", Icon::Users, look.palette())
                .variant(ButtonVariant::Ghost)
                .tooltip(if open {
                    t("Ocultar miembros")
                } else {
                    t("Mostrar miembros")
                })
                .tooltip_placement(TooltipPlacement::Below)
                .ink(if open { look.text } else { look.dim })
                .hover_ink(look.text)
                .render()
                .centered_y(),
        );
    }
    if wide {
        parts.push(gap(17.0, 0.0));
        parts.push(search_box(nav.search_keys.as_deref(), look).centered_y());
        parts.push(gap(8.5, 0.0));
    } else {
        parts.push(gap(13.0, 0.0));
        parts.push(
            IconButton::new("message-search:open", Icon::Search, look.palette())
                .variant(ButtonVariant::Ghost)
                .tooltip(t("Buscar mensajes cargados"))
                .tooltip_shortcut(nav.search_keys.clone().unwrap_or_default())
                .tooltip_placement(TooltipPlacement::Below)
                .ink(look.dim)
                .hover_ink(look.text)
                .render()
                .centered_y(),
        );
        parts.push(gap(12.0, 0.0));
    }
    parts.push(gap(nav.end, 0.0));
    Element::panel(Element::row(parts))
        .size(None, Some(crate::sidebar::HEADER))
        .background(look.canvas)
}

/// The search of the header: a box that looks like a field and opens the
/// local search, which finds messages already loaded in this conversation.
fn search_box(keys: Option<&str>, look: &Look) -> Element {
    let field = Element::clickable(
        "message-search:open",
        Element::row(vec![
            gap(10.0, 0.0),
            line(t("Buscar mensajes"), 14.0, look.dim)
                .flex(1.0)
                .centered_y(),
            icon(Icon::Search, look.muted).centered_y(),
            gap(9.0, 0.0),
        ]),
    )
    .size(Some(SEARCH.0), Some(SEARCH.1))
    .radius(8.0)
    .background(look.canvas)
    .border(look.border, 1.0)
    .hover_background(look.hover)
    .accessible(limn::Access::new(limn::Role::Button).name(t("Buscar mensajes cargados")));
    Tooltip::new(
        field,
        "message-search:open",
        t("Buscar mensajes cargados"),
        look.menu_palette(),
    )
    .shortcut(keys.unwrap_or_default())
    .placement(TooltipPlacement::Below)
    .render()
}

/// The account at the foot of the window's left side.
pub struct UserPanel {
    pub name: String,
    pub image: Option<limn::RasterImage>,
    pub handle: String,
    pub accent: Color,
    /// No indicator when the provider does not publish presence.
    pub status: Option<Color>,
    /// Under the name: the custom status, or the status. The handle takes
    /// its place while the pointer is over the account.
    pub status_label: String,
    pub muted: bool,
    pub deafened: bool,
    /// Whether this provider has voice controls.
    pub voice: bool,
    /// The account's profile popout, floating over the panel while open.
    pub popout: Option<Element>,
    /// The quick voice panel of the headphones' arrow (`true`) or of the
    /// microphone's, while one is open: [`crate::quick_voice::panel`].
    pub quick: Option<(bool, Element)>,
}

/// The box of the account's panel, its hairline included: 61.5 px high, its
/// corners rounded. It floats over the foot of the rail and the column,
/// 11.5 from the window's left edge, 11 from its bottom and from the
/// column's divider.
pub const USER_PANEL_HEIGHT: f32 = 61.5;
/// Around the account's panel: left, right (the column's divider included)
/// and bottom.
pub const USER_PANEL_MARGIN: (f32, f32, f32) = (11.5, 12.0, 11.0);
/// The buttons of the account's panel are this high; the microphone's and the
/// headphones' are this wide, before the arrow of their options.
const PANEL_BUTTON: (f32, f32) = (34.5, 34.0);

/// The geometry of a button of the account's panel.
fn panel_button(button: IconButton) -> IconButton {
    button
        .sizing(ButtonSizing {
            height: PANEL_BUTTON.1,
            font_size: 13.0,
            icon_size: 16.0,
            horizontal_padding: 9.0,
            inner_spacing: 8.0,
        })
        .width(PANEL_BUTTON.0)
}

/// A button split in two: the action, and an arrow that opens its options;
/// all of it red while the action is `on` (muted, deafened).
fn split_button(action: Element, arrow_id: &str, hint: &str, on: bool, look: &Look) -> Element {
    let arrow = Element::clickable(
        arrow_id,
        crate::centered(
            Element::icon(Icon::ChevronDown)
                .foreground(if on { look.danger } else { look.muted })
                .size(Some(12.0), Some(12.0)),
        ),
    )
    .size(Some(17.0), Some(PANEL_BUTTON.1))
    .radius(4.0)
    .hover_background(look.hover)
    .hover_foreground(look.text)
    .accessible(limn::Access::new(limn::Role::Button).name(hint));
    let split = Element::row(vec![
        action.centered_y(),
        gap(1.0, 0.0),
        Tooltip::new(arrow, arrow_id, hint, look.menu_palette())
            .render()
            .centered_y(),
    ]);
    if on {
        // On Ónix a faint red, #1a0607.
        Element::panel(split)
            .radius(4.0)
            .background(Look::alpha(look.danger, 32))
    } else {
        split
    }
}

/// The account's panel: its mark and name (which open its profile popout),
/// the microphone and the headphones with the arrows of their options, and
/// the settings.
pub fn user_panel(panel: UserPanel, look: &Look) -> Element {
    let palette = look.sidebar_palette();
    let face = crate::avatar_with_image(&panel.name, panel.accent, 32.0, panel.image);
    let face = match panel.status {
        Some(status) => with_status(face, 32.0, status, look.rail),
        None => face,
    };
    let second = Element::stack(vec![
        line(&panel.status_label, 13.0, look.muted).conceal_on_hover("me:line:status"),
        line(&panel.handle, 13.0, look.muted).reveal_on_hover("me:line:handle"),
    ]);
    let account = Element::clickable(
        "me:status",
        Element::row(vec![
            gap(4.0, 0.0),
            face.centered_y(),
            gap(10.5, 0.0),
            Element::column(vec![strong_line(&panel.name, 16.0, look.text), second])
                .gap(1.0)
                .flex(1.0)
                .centered_y(),
            gap(4.0, 0.0),
        ]),
    )
    .size(None, Some(42.0))
    .radius(6.0)
    .hover_background(look.hover)
    .hover_region()
    .accessibility_label(match panel.status {
        Some(_) => tr!(
            "{name}, {status}. Perfil y estado",
            name = panel.name,
            status = panel.status_label
        ),
        None => tr!(
            "{name}, {handle}. Perfil",
            name = panel.name,
            handle = panel.handle
        ),
    });
    let microphone = panel_button(IconButton::new(
        "me:mute",
        if panel.muted {
            Icon::MicrophoneMuted
        } else {
            Icon::Microphone
        },
        palette,
    ))
    .variant(ButtonVariant::Ghost)
    .tooltip(if panel.muted {
        t("Activar micrófono")
    } else {
        t("Silenciar")
    })
    .ink(if panel.muted { look.danger } else { look.muted })
    .render();
    let headphones = panel_button(IconButton::new(
        "me:deafen",
        if panel.deafened {
            Icon::Custom(crate::HEADPHONES_OFF)
        } else {
            Icon::Headphones
        },
        palette,
    ))
    .variant(ButtonVariant::Ghost)
    .tooltip(if panel.deafened {
        t("Activar sonido")
    } else {
        t("Ensordecer")
    })
    .ink(if panel.deafened {
        look.danger
    } else {
        look.muted
    })
    .render();
    let mut microphone = split_button(
        microphone,
        crate::profile::ids::INPUT,
        t("Opciones de entrada"),
        panel.muted,
        look,
    );
    let mut headphones = split_button(
        headphones,
        crate::profile::ids::OUTPUT,
        t("Opciones de salida"),
        panel.deafened,
        look,
    );
    if let Some((output, quick)) = panel.quick {
        let anchor = if output {
            &mut headphones
        } else {
            &mut microphone
        };
        *anchor = Element::popover(
            std::mem::replace(anchor, Element::spacer()),
            quick,
            crate::quick_voice::ids::DISMISS,
            PopoverPlacement::TopStart,
        )
        .popover_gap(10.0);
    }
    let settings = panel_button(IconButton::new("me:settings", Icon::Settings, palette))
        .width(PANEL_BUTTON.1)
        .variant(ButtonVariant::Ghost)
        .tooltip(t("Ajustes de usuario"))
        .render();
    let mut controls = vec![
        gap(10.5, 0.0),
        account.flex(1.0).centered_y(),
        gap(8.5, 0.0),
    ];
    if panel.voice {
        controls.extend([
            microphone.centered_y(),
            gap(8.5, 0.0),
            headphones.centered_y(),
            gap(8.5, 0.0),
        ]);
    }
    controls.extend([settings.centered_y(), gap(16.0, 0.0)]);
    let body = Element::panel(Element::row(controls))
        .size(None, Some(USER_PANEL_HEIGHT))
        .radius(8.0)
        .background(look.rail)
        .border(look.border, 1.0);
    // The popout opens above the whole box, its left edge on the box's.
    crate::profile::popout(
        body,
        panel.popout,
        crate::profile::ids::DISMISS,
        PopoverPlacement::TopStart,
        look,
    )
}

/// What the provider is, at the foot of the column of channels, such as
/// "Simulación local · sin conexión real": what is simulated says so.
pub fn provider_note(provider: &str, look: &Look) -> Element {
    Element::row(vec![
        gap(crate::sidebar::INSET + 11.0, 0.0),
        icon(Icon::InfoCircle, look.faint).centered_y(),
        gap(6.0, 0.0),
        line(provider, 11.0, look.faint).flex(1.0).centered_y(),
        gap(crate::sidebar::INSET, 0.0),
    ])
    .size(None, Some(24.0))
}

/// A line across the conversation while the connection is not there, with
/// what can be done about it at its end.
pub fn connection_banner(
    sentence: &str,
    warning: bool,
    action: Option<Element>,
    look: &Look,
) -> Element {
    let ink = if warning { look.idle } else { look.muted };
    let mut row = vec![
        icon(
            if warning {
                Icon::WarningTriangle
            } else {
                Icon::Refresh
            },
            ink,
        )
        .centered_y(),
        line(sentence, 13.0, look.text).flex(1.0).centered_y(),
    ];
    row.extend(action.map(Element::centered_y));
    Element::panel(Element::row(row).gap(8.0).padding(8.0)).background(Look::alpha(ink, 36))
}

/// The button that takes the conversation back to its newest message.
pub fn jump_to_latest(count: usize, look: &Look) -> Element {
    let label = if count > 0 {
        tr!("{count} mensajes nuevos", count)
    } else {
        t("Ir a lo más reciente").to_owned()
    };
    Element::row(vec![
        Element::spacer().flex(1.0),
        Button::new("jump:latest", label, look.accent_palette())
            .variant(ButtonVariant::Default)
            .icon(Icon::ArrowDown)
            .render(),
        Element::spacer().size(Some(16.0), None),
    ])
}

/// Measurements over the window, one per line.
pub fn metrics_panel(lines: &[String], look: &Look) -> Element {
    Element::panel(
        Element::column(
            lines
                .iter()
                .map(|value| {
                    Element::text(value.clone())
                        .font_family(FontFamily::Monospace)
                        .font_size(11.0)
                        .foreground(look.text)
                })
                .collect(),
        )
        .gap(2.0)
        .padding(8.0),
    )
    .radius(6.0)
    .background(Look::alpha(look.rail, 230))
    .border(look.border, 1.0)
}
