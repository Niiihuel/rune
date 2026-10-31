//! RUNE's look: its own colors, in a black (the default), a dark and a light variant, and the
//! palettes limn's components take. The accent is an ember amber; nothing is
//! borrowed from another product's brand.

use limn::{Color, ComponentPalette, ContextMenuMetrics, ScrollbarAppearance};
use rune_i18n::{es, t, tr};
use rune_model::{Rgb, Status};

/// The density of every context menu, as Discord's menus (REA 2026-10-31):
/// rows of at least 32 with 14 px labels, 8 around the rows and 8 on each
/// side of a separator.
pub const MENU_METRICS: ContextMenuMetrics = ContextMenuMetrics {
    row_height: 32.0,
    label_size: 14.0,
    detail_size: 12.0,
    inset: 8.0,
    horizontal_padding: 8.0,
    gap: 8.0,
    separator_height: 17.0,
    scrollbar_gutter: 14.0,
    header_height: 24.0,
    max_height: 480.0,
};

/// [`MENU_METRICS`] for a menu that may grow as tall as `room` less 32, as
/// Discord's menus do with the window; a person's menu is long.
pub fn menu_metrics_within(room: f32) -> ContextMenuMetrics {
    ContextMenuMetrics {
        max_height: (room - 32.0).max(240.0),
        ..MENU_METRICS
    }
}

/// Every color RUNE paints with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// The rail of servers.
    pub rail: Color,
    /// The column of channels and the member list.
    pub sidebar: Color,
    /// The conversation.
    pub canvas: Color,
    /// Fields, buttons and the composer; a message's cards take `card`.
    pub raised: Color,
    /// Menus and popovers.
    pub menu: Color,
    pub hover: Color,
    pub selected: Color,
    pub border: Color,
    pub shadow: Color,
    pub text: Color,
    pub muted: Color,
    /// Between muted and faint: channels already read, the headings of
    /// categories and of member groups, the header's icons.
    pub dim: Color,
    /// Quieter than muted: timestamps of a continued message, hints.
    pub faint: Color,
    pub accent: Color,
    /// Text on the accent.
    pub on_accent: Color,
    /// Behind a message that mentions the account.
    pub mention: Color,
    /// Text and icons that warn or destroy: legible on every ground.
    pub danger: Color,
    /// Fills that warn, under white text: the counts of mentions.
    pub danger_fill: Color,
    pub online: Color,
    pub idle: Color,
    pub busy: Color,
    pub offline: Color,
    /// The cards inside a message (embeds, invitations, attachments, polls,
    /// code): apart from both the conversation and a row under the pointer,
    /// which `raised` is not, and outlined in `border`.
    pub card: Color,
}

impl Look {
    pub const DARK: Self = Self {
        rail: Color::rgb(14, 16, 21),
        sidebar: Color::rgb(20, 23, 30),
        canvas: Color::rgb(26, 29, 37),
        raised: Color::rgb(35, 39, 50),
        menu: Color::rgb(30, 34, 44),
        hover: Color::rgba(255, 255, 255, 12),
        selected: Color::rgba(255, 255, 255, 22),
        border: Color::rgb(44, 49, 61),
        shadow: Color::rgba(0, 0, 0, 120),
        text: Color::rgb(230, 232, 238),
        muted: Color::rgb(154, 161, 178),
        dim: Color::rgb(154, 161, 178),
        faint: Color::rgb(102, 109, 125),
        accent: Color::rgb(224, 164, 60),
        on_accent: Color::rgb(26, 19, 0),
        mention: Color::rgba(224, 164, 60, 24),
        danger: Color::rgb(224, 85, 97),
        danger_fill: Color::rgb(224, 85, 97),
        online: Color::rgb(63, 185, 122),
        idle: Color::rgb(217, 182, 76),
        busy: Color::rgb(224, 85, 97),
        offline: Color::rgb(107, 114, 128),
        // Below the conversation, so a row under the pointer lifts away from
        // it; not the columns' color, which a see-through window leaves out.
        card: Color::rgb(18, 21, 28),
    };

    pub const LIGHT: Self = Self {
        rail: Color::rgb(222, 225, 232),
        sidebar: Color::rgb(236, 238, 243),
        canvas: Color::rgb(250, 250, 252),
        raised: Color::rgb(240, 242, 246),
        menu: Color::rgb(255, 255, 255),
        hover: Color::rgba(20, 24, 33, 12),
        selected: Color::rgba(20, 24, 33, 22),
        border: Color::rgb(214, 218, 227),
        shadow: Color::rgba(15, 20, 30, 40),
        text: Color::rgb(24, 27, 34),
        muted: Color::rgb(90, 97, 112),
        dim: Color::rgb(90, 97, 112),
        faint: Color::rgb(140, 146, 160),
        accent: Color::rgb(176, 112, 10),
        on_accent: Color::rgb(255, 250, 240),
        mention: Color::rgba(224, 164, 60, 40),
        danger: Color::rgb(196, 54, 70),
        danger_fill: Color::rgb(196, 54, 70),
        online: Color::rgb(36, 150, 92),
        idle: Color::rgb(186, 140, 20),
        busy: Color::rgb(196, 54, 70),
        offline: Color::rgb(128, 134, 148),
        // Below a row under the pointer: white would meet the conversation.
        card: Color::rgb(229, 231, 237),
    };

    /// Pure black, for dark rooms and OLED screens, and RUNE's default: every
    /// column on black, set apart by hairlines; text in four levels; the
    /// same amber accent. The greys are the ones measured on the reference
    /// captures (docs/ARCHITECTURE.md).
    pub const ONYX: Self = Self {
        rail: Color::rgb(0, 0, 0),
        sidebar: Color::rgb(0, 0, 0),
        canvas: Color::rgb(0, 0, 0),
        raised: Color::rgb(18, 18, 19),
        menu: Color::rgb(18, 18, 20),
        // Translucent, so a row lifts the same on black (#121212) and in a
        // menu (#232325).
        hover: Color::rgba(255, 255, 255, 18),
        selected: Color::rgb(36, 36, 38),
        border: Color::rgb(30, 30, 32),
        shadow: Color::rgba(0, 0, 0, 160),
        text: Color::rgb(220, 220, 223),
        muted: Color::rgb(150, 151, 158),
        dim: Color::rgb(122, 123, 131),
        faint: Color::rgb(115, 116, 125),
        accent: Color::rgb(224, 164, 60),
        on_accent: Color::rgb(26, 19, 0),
        mention: Color::rgba(224, 164, 60, 22),
        // Measured fills are #d22d39, but as text on black or a menu that
        // falls below 4.5:1; this one reads at about 6:1.
        danger: Color::rgb(235, 95, 94),
        danger_fill: Color::rgb(210, 45, 57),
        online: Color::rgb(63, 185, 122),
        idle: Color::rgb(217, 182, 76),
        busy: Color::rgb(235, 95, 94),
        offline: Color::rgb(107, 114, 128),
        // Between black and a row under the pointer (#121212), as the
        // reference client's embeds (#0a0a0c, outlined).
        card: Color::rgb(10, 10, 12),
    };

    fn palette_on(&self, surface: Color) -> ComponentPalette {
        ComponentPalette {
            // Opaque even through a see-through window ([`Self::see_through`]):
            // limn draws marks and knobs in it.
            surface: Self::alpha(surface, 255),
            raised: self.raised,
            hover: self.hover,
            selected: self.selected,
            border: self.border,
            text: self.text,
            muted: self.muted,
            shadow: self.shadow,
        }
    }

    /// For components on the conversation.
    pub fn palette(&self) -> ComponentPalette {
        self.palette_on(self.canvas)
    }

    /// For components on the sidebars.
    pub fn sidebar_palette(&self) -> ComponentPalette {
        self.palette_on(self.sidebar)
    }

    /// For menus, popovers and dialogs.
    pub fn menu_palette(&self) -> ComponentPalette {
        ComponentPalette {
            raised: self.raised,
            ..self.palette_on(self.menu)
        }
    }

    /// The fill of the rail's hints: the official client's floating black
    /// (#0a0a0c) on a dark look, the menus' on a light one.
    pub fn hint(&self) -> Color {
        if self.canvas.r > 128 {
            self.menu
        } else {
            Color::rgb(10, 10, 12)
        }
    }

    /// A filled control in the accent, such as the send button: limn fills a
    /// default button with the palette's text and draws on it in its surface.
    pub fn accent_palette(&self) -> ComponentPalette {
        ComponentPalette {
            surface: self.on_accent,
            raised: self.accent,
            text: self.accent,
            ..self.palette()
        }
    }

    /// The scrollbar of the columns: painted over the content, a thin thumb
    /// with no track, which a hidden one (`shown` false) shows only while
    /// the pointer is on it, as the rail's.
    pub fn scrollbar(&self, shown: bool) -> ScrollbarAppearance {
        ScrollbarAppearance {
            idle_thumb_opacity: if shown { 150 } else { 0 },
            hover_thumb_opacity: 200,
            active_thumb_opacity: 230,
            hover_track_opacity: 0,
            ..ScrollbarAppearance::overlay()
                .buttons(false)
                .rail_width(8.0)
                .thumb_width(4.5)
                .tint(self.muted)
        }
    }

    /// limn's native reserved rail and controls, tinted for the conversation.
    pub fn chat_scrollbar(&self) -> ScrollbarAppearance {
        ScrollbarAppearance::default().tint(self.muted)
    }

    /// The dot of someone's status.
    pub fn status(&self, status: Status) -> Color {
        match status {
            Status::Online => self.online,
            Status::Idle => self.idle,
            Status::DoNotDisturb => self.busy,
            Status::Invisible | Status::Offline => self.offline,
        }
    }

    /// A color of the model, readable on this look's surfaces: a light look
    /// darkens it a little.
    pub fn ink(&self, color: Rgb) -> Color {
        let (r, g, b) = color.channels();
        if self.canvas.r > 128 {
            let darker = |channel: u8| (channel as f32 * 0.72) as u8;
            Color::rgb(darker(r), darker(g), darker(b))
        } else {
            Color::rgb(r, g, b)
        }
    }

    /// `color` at `alpha` of 255.
    pub fn alpha(color: Color, alpha: u8) -> Color {
        Color::rgba(color.r, color.g, color.b, alpha)
    }

    // Own themes, main color and transparency (temas-propios).
    /// The built-in look kept as `key` (`onyx`, `dark`, `light`).
    pub fn named(key: &str) -> Option<Self> {
        match key {
            "onyx" => Some(Self::ONYX),
            "dark" => Some(Self::DARK),
            "light" => Some(Self::LIGHT),
            _ => None,
        }
    }

    /// The color a theme names `key` ([`TOKENS`]), to change it.
    pub fn token_mut(&mut self, key: &str) -> Option<&mut Color> {
        Some(match key {
            "rail" => &mut self.rail,
            "sidebar" => &mut self.sidebar,
            "canvas" => &mut self.canvas,
            "raised" => &mut self.raised,
            "menu" => &mut self.menu,
            "hover" => &mut self.hover,
            "selected" => &mut self.selected,
            "border" => &mut self.border,
            "shadow" => &mut self.shadow,
            "text" => &mut self.text,
            "muted" => &mut self.muted,
            "dim" => &mut self.dim,
            "faint" => &mut self.faint,
            "accent" => &mut self.accent,
            "on_accent" => &mut self.on_accent,
            "mention" => &mut self.mention,
            "danger" => &mut self.danger,
            "danger_fill" => &mut self.danger_fill,
            "online" => &mut self.online,
            "idle" => &mut self.idle,
            "busy" => &mut self.busy,
            "offline" => &mut self.offline,
            "card" => &mut self.card,
            _ => return None,
        })
    }

    /// The color a theme names `key`.
    pub fn token(&self, key: &str) -> Option<Color> {
        let mut look = *self;
        look.token_mut(key).map(|color| *color)
    }

    /// This look with `accent` as its main color, as Serein tints its
    /// palette: the text on it is white or black, whichever reads better,
    /// and a mention's ground is the accent at the look's own alpha.
    pub fn with_accent(mut self, accent: Color) -> Self {
        let accent = Self::alpha(accent, 255);
        let (white, black) = (Color::rgb(255, 255, 255), Color::rgb(16, 16, 18));
        self.on_accent = if contrast(white, accent) >= contrast(black, accent) {
            white
        } else {
            black
        };
        self.mention = Self::alpha(accent, self.mention.a);
        self.accent = accent;
        self
    }

    /// This look for a window the desktop shows through: the rail, the
    /// columns and the conversation paint nothing, so panels inside one
    /// another never add up, and [`Self::backdrop`] tints the whole window
    /// once, under them. Popovers, menus and components keep opaque grounds.
    pub fn see_through(mut self) -> Self {
        for surface in [&mut self.rail, &mut self.sidebar, &mut self.canvas] {
            *surface = Self::alpha(*surface, 0);
        }
        self
    }

    /// The window's ground when `transparency` percent of the desktop shows
    /// through it: the conversation's color.
    pub fn backdrop(&self, transparency: u8) -> Color {
        let opacity = 100 - u16::from(transparency.min(100));
        Self::alpha(self.canvas, (opacity * 255 / 100) as u8)
    }
}

/// The colors a theme can set, in the editor's groups: the key a theme file
/// names each by, and what the editor calls it.
pub const TOKENS: [(&str, &[(&str, &str)]); 4] = [
    (
        es("Superficies"),
        &[
            ("rail", es("Riel de servidores")),
            ("sidebar", es("Columnas")),
            ("canvas", es("Conversación")),
            ("raised", es("Campos y botones")),
            ("menu", es("Menús")),
            ("hover", es("Bajo el puntero")),
            ("selected", es("Elegido")),
            ("border", es("Bordes")),
            ("shadow", es("Sombras")),
            ("card", es("Tarjetas de un mensaje")),
        ],
    ),
    (
        es("Texto"),
        &[
            ("text", es("Texto")),
            ("muted", es("Texto secundario")),
            ("dim", es("Texto atenuado")),
            ("faint", es("Pistas y horas")),
        ],
    ),
    (
        es("Acciones y estados"),
        &[
            ("accent", es("Color principal")),
            ("on_accent", es("Texto sobre el color principal")),
            ("mention", es("Fondo de una mención")),
            ("danger", es("Peligro")),
            ("danger_fill", es("Relleno de peligro")),
        ],
    ),
    (
        es("Presencia"),
        &[
            ("online", es("En línea")),
            ("idle", es("Ausente")),
            ("busy", es("No molestar")),
            ("offline", es("Desconectado")),
        ],
    ),
];

/// The WCAG contrast ratio of two opaque colors, from 1 to 21.
pub fn contrast(a: Color, b: Color) -> f32 {
    let luminance = |color: Color| {
        let channel = |value: u8| {
            let value = f32::from(value) / 255.0;
            if value <= 0.039_28 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(color.r) + 0.7152 * channel(color.g) + 0.0722 * channel(color.b)
    };
    let (a, b) = (luminance(a), luminance(b));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// What a status is called.
pub fn status_label(status: Status) -> &'static str {
    match status {
        Status::Online => t("En línea"),
        Status::Idle => t("Ausente"),
        Status::DoNotDisturb => t("No molestar"),
        Status::Invisible => t("Invisible"),
        Status::Offline => t("Desconectado"),
    }
}

/// What someone is doing, as a sentence: "Jugando a Factorio".
pub fn activity_line(activity: &rune_model::Activity) -> String {
    use rune_model::ActivityKind;
    let name = &activity.name;
    match activity.kind {
        ActivityKind::Playing => tr!("Jugando a {name}", name),
        ActivityKind::Listening => tr!("Escuchando {name}", name),
        ActivityKind::Watching => tr!("Viendo {name}", name),
        ActivityKind::Streaming => tr!("Transmitiendo {name}", name),
        ActivityKind::Competing => tr!("Compitiendo en {name}", name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CIE L*, 0 (black) to 100 (white), of an opaque color.
    fn lightness(color: Color) -> f32 {
        let linear = |channel: u8| {
            let c = channel as f32 / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let y = 0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b);
        if y > 0.008856 {
            116.0 * y.cbrt() - 16.0
        } else {
            903.3 * y
        }
    }

    /// `color` laid over the opaque `ground`.
    fn over(color: Color, ground: Color) -> Color {
        let mix = |top: u8, under: u8| {
            let a = color.a as f32 / 255.0;
            (under as f32 + (top as f32 - under as f32) * a).round() as u8
        };
        Color::rgb(
            mix(color.r, ground.r),
            mix(color.g, ground.g),
            mix(color.b, ground.b),
        )
    }

    #[test]
    fn a_card_stands_apart_from_the_conversation_and_a_hovered_row() {
        for (name, look) in [
            ("ONYX", Look::ONYX),
            ("DARK", Look::DARK),
            ("LIGHT", Look::LIGHT),
        ] {
            let row = over(look.hover, look.canvas);
            let card = lightness(look.card);
            for (ground, color) in [("canvas", look.canvas), ("hover", row)] {
                // About the smallest step two touching fills show (ΔE ≈ 2.3).
                let fill = (card - lightness(color)).abs();
                assert!(fill >= 2.5, "{name}: card vs {ground}: ΔL* {fill:.1}");
                // The outline reads against what the card sits on.
                let ring = (lightness(look.border) - lightness(color)).abs();
                assert!(ring >= 4.0, "{name}: border vs {ground}: ΔL* {ring:.1}");
            }
            let ring = (lightness(look.border) - card).abs();
            assert!(ring >= 4.0, "{name}: border vs card: ΔL* {ring:.1}");
        }
    }
}
