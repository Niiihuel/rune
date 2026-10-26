//! Themes of one's own (temas-propios): a built-in look as the base with
//! some of its colors replaced, kept on this device and shared as a small
//! JSON file of data — a name, the base and colors, never code —, checked
//! whole before it is used, as Serein checks its theme packages
//! (`docs/theme-api.md`). No images, fonts, metrics or remote catalog.
//!
//! The file:
//!
//! ```json
//! { "rune_theme": 1, "name": "Bosque", "base": "dark",
//!   "colors": { "accent": "#3fb97a", "canvas": "#101814" } }
//! ```

use std::collections::BTreeMap;

use limn::{Color, ColorPicker, Component, Element, Hsv, Icon, Popover, PopoverPlacement};
use rune_i18n::{t, tr};
use serde::{Deserialize, Serialize};

use crate::look::TOKENS;
use crate::{gap, line, strong_line, Look};

/// The version of the file this RUNE writes and reads.
pub const VERSION: u32 = 1;
/// The largest file read, in bytes: a whole theme takes about 1 KB.
pub const MAX_FILE: usize = 64 * 1024;
/// The longest name, in characters.
pub const MAX_NAME: usize = 48;
/// The built-in looks a theme starts from, by key.
pub const BASES: [&str; 3] = ["onyx", "dark", "light"];
/// What an exported file's name ends with.
pub const EXTENSION: &str = ".rune-theme.json";

/// A theme of one's own.
#[derive(Clone, Debug, PartialEq)]
pub struct CustomTheme {
    pub name: String,
    /// One of [`BASES`].
    pub base: &'static str,
    /// The colors that differ from the base's, by their key in [`TOKENS`].
    pub colors: BTreeMap<&'static str, Color>,
}

/// The file as written; anything else in it is refused.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct File {
    rune_theme: u32,
    name: String,
    base: String,
    #[serde(default)]
    colors: BTreeMap<String, String>,
}

impl CustomTheme {
    /// A theme named `name` that is `base` as it is.
    pub fn new(name: impl Into<String>, base: &str) -> Self {
        Self {
            name: name.into(),
            base: BASES.into_iter().find(|key| *key == base).unwrap_or("onyx"),
            colors: BTreeMap::new(),
        }
    }

    /// The look it paints with: the base's, its colors over it. The big
    /// surfaces stay opaque; the window's transparency is the device's.
    pub fn look(&self) -> Look {
        let mut look = Look::named(self.base).unwrap_or(Look::ONYX);
        for (key, color) in &self.colors {
            if let Some(token) = look.token_mut(key) {
                *token = *color;
            }
        }
        for surface in [&mut look.rail, &mut look.sidebar, &mut look.canvas] {
            *surface = Look::alpha(*surface, 255);
        }
        look
    }

    /// The theme in `text`, a theme file, checked whole: its version, name,
    /// base and every color; anything unknown is refused.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_FILE {
            return Err(t("El archivo pesa más de 64 KB: no es un tema de RUNE.").into());
        }
        let file: File = serde_json::from_str(text)
            .map_err(|_| t("El archivo no es un tema de RUNE o está dañado."))?;
        Self::from_file(file)
    }

    fn from_file(file: File) -> Result<Self, String> {
        if file.rune_theme != VERSION {
            return Err(t("El tema es de otra versión de RUNE.").into());
        }
        let name = valid_name(&file.name).ok_or(t(
            "El nombre del tema está vacío, es muy largo o tiene caracteres de control.",
        ))?;
        let base = BASES
            .into_iter()
            .find(|key| *key == file.base)
            .ok_or(t("La base del tema tiene que ser onyx, dark o light."))?;
        let mut colors = BTreeMap::new();
        for (key, value) in file.colors {
            let Some(token) = token_key(&key) else {
                return Err(tr!(
                    "El tema nombra un color que RUNE no conoce: {key}.",
                    key
                ));
            };
            let color = parse_color(&value).ok_or_else(|| {
                tr!(
                    "El color {key} del tema no es #RRGGBB ni #RRGGBBAA.",
                    key = token
                )
            })?;
            colors.insert(token, color);
        }
        Ok(Self { name, base, colors })
    }

    fn file(&self) -> File {
        File {
            rune_theme: VERSION,
            name: self.name.clone(),
            base: self.base.into(),
            colors: self
                .colors
                .iter()
                .map(|(key, color)| ((*key).into(), hex(*color)))
                .collect(),
        }
    }

    /// The theme as a file to share.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(&self.file()).unwrap_or_default()
    }

    /// A name for its file: the theme's own, without what a file name
    /// cannot hold, and [`EXTENSION`].
    pub fn filename(&self) -> String {
        let stem: String = self
            .name
            .chars()
            .map(|c| if "/\\:*?\"<>|".contains(c) { '-' } else { c })
            .collect();
        let stem = stem.trim().trim_start_matches('.');
        format!(
            "{}{EXTENSION}",
            if stem.is_empty() { t("tema") } else { stem }
        )
    }
}

/// `name` as a theme's name, trimmed; `None` when empty, too long or with
/// control characters.
pub fn valid_name(name: &str) -> Option<String> {
    let name = name.trim();
    (!name.is_empty() && name.chars().count() <= MAX_NAME && !name.chars().any(char::is_control))
        .then(|| name.to_owned())
}

/// The key of [`TOKENS`] that is `key`.
pub fn token_key(key: &str) -> Option<&'static str> {
    TOKENS
        .iter()
        .flat_map(|(_, tokens)| tokens.iter())
        .map(|(token, _)| *token)
        .find(|token| *token == key)
}

/// Themes as the settings keep them: a JSON list of theme files.
pub fn list_json(themes: &[CustomTheme]) -> String {
    serde_json::to_string(&themes.iter().map(CustomTheme::file).collect::<Vec<_>>())
        .unwrap_or_default()
}

/// What [`list_json`] wrote: a theme that does not read, or a name already
/// taken, is left out.
pub fn parse_list(text: &str) -> Vec<CustomTheme> {
    let files: Vec<serde_json::Value> = serde_json::from_str(text).unwrap_or_default();
    let mut themes: Vec<CustomTheme> = Vec::new();
    for file in files {
        let Ok(theme) = serde_json::from_value(file)
            .map_err(|_| String::new())
            .and_then(CustomTheme::from_file)
        else {
            continue;
        };
        if !themes.iter().any(|known| known.name == theme.name) {
            themes.push(theme);
        }
    }
    themes
}

/// `#rrggbb`, or `#rrggbbaa` when not opaque.
pub fn hex(color: Color) -> String {
    let rgb = format!("#{:02x}{:02x}{:02x}", color.r, color.g, color.b);
    if color.a == 255 {
        rgb
    } else {
        format!("{rgb}{:02x}", color.a)
    }
}

/// `#rrggbb` or `#rrggbbaa`, the `#` optional, as a color.
pub fn parse_color(text: &str) -> Option<Color> {
    let digits = text.trim().strip_prefix('#').unwrap_or(text.trim());
    if !(digits.len() == 6 || digits.len() == 8) || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    let alpha = if digits.len() == 8 { byte(6)? } else { 255 };
    Some(Color::rgba(byte(0)?, byte(2)?, byte(4)?, alpha))
}

/// A small window painted with `look`, `width` wide: the rail, a column of
/// channels, a conversation with a mention, a field and its buttons, a menu
/// and the four presences. It is not the app: nothing in it is a control.
pub fn preview(look: &Look, width: f32) -> Element {
    let dot = |fill: Color| {
        Element::panel(Element::spacer())
            .size(Some(26.0), Some(26.0))
            .radius(13.0)
            .background(fill)
    };
    let rail = Element::column(vec![dot(look.accent), dot(look.raised), dot(look.raised)])
        .gap(8.0)
        .padding(8.0)
        .size(Some(42.0), None)
        .background(look.rail);
    let channel = |name: &str, fill: Color, ink: Color| {
        Element::panel(line(format!("# {name}"), 12.0, ink))
            .padding(5.0)
            .radius(4.0)
            .background(fill)
    };
    let sidebar = Element::column(vec![
        strong_line(t("Tu servidor"), 13.0, look.text),
        gap(0.0, 6.0),
        channel("general", look.selected, look.text),
        channel(t("música"), look.hover, look.muted),
        channel(t("avisos"), Color::TRANSPARENT, look.dim),
    ])
    .gap(2.0)
    .padding(8.0)
    .size(Some(132.0), None)
    .background(look.sidebar);
    let rule = || {
        Element::panel(Element::spacer())
            .size(Some(1.0), None)
            .background(look.border)
    };
    let message = Element::row(vec![
        dot(look.accent),
        Element::column(vec![
            Element::row(vec![
                strong_line("Ana", 13.0, look.text),
                line("14:05", 11.0, look.faint).centered_y(),
            ])
            .gap(6.0),
            line(t("Así se ve tu tema."), 13.0, look.text),
            line(t("Y así, lo secundario."), 12.0, look.muted),
        ])
        .gap(2.0)
        .flex(1.0),
    ])
    .gap(8.0);
    let mention = Element::row(vec![
        Element::panel(Element::spacer())
            .size(Some(2.0), None)
            .background(look.accent),
        line(t("tú, ¿lo probaste?"), 13.0, look.text)
            .padding(5.0)
            .flex(1.0),
    ])
    .background(look.mention);
    let presence = |fill: Color| {
        Element::panel(Element::spacer())
            .size(Some(10.0), Some(10.0))
            .radius(5.0)
            .background(fill)
    };
    let actions = Element::row(vec![
        Element::panel(line(t("Enviar"), 12.0, look.on_accent))
            .padding(6.0)
            .radius(4.0)
            .background(look.accent),
        Element::panel(line(t("Borrar"), 12.0, look.danger)).padding(6.0),
        Element::spacer().flex(1.0),
        presence(look.online).centered_y(),
        presence(look.idle).centered_y(),
        presence(look.busy).centered_y(),
        presence(look.offline).centered_y(),
    ])
    .gap(6.0);
    let field = Element::panel(line(t("Escribe un mensaje"), 12.0, look.faint))
        .padding(8.0)
        .radius(6.0)
        .background(look.raised);
    let menu = Element::panel(
        Element::column(vec![
            line(t("Menú"), 12.0, look.text),
            line(t("Borrar"), 12.0, look.danger),
        ])
        .gap(2.0),
    )
    .padding(6.0)
    .radius(6.0)
    .border(look.border, 1.0)
    .background(look.menu);
    let canvas = Element::column(vec![
        Element::row(vec![message.flex(1.0), menu]).gap(8.0),
        mention,
        field,
        actions,
    ])
    .gap(10.0)
    .padding(12.0)
    .flex(1.0)
    .background(look.canvas);
    Element::row(vec![rail, rule(), sidebar, rule(), canvas])
        .size(Some(width), Some(196.0))
        .radius(8.0)
        .border(look.border, 1.0)
        .clip_children()
}

/// One color of the editor: its swatch, which opens a picker of any color,
/// its name and digits, and a way back to the base's while it differs.
#[derive(Clone, Debug, PartialEq)]
pub struct TokenRow<'a> {
    /// Prefixes the ids: the swatch is `{prefix}`, the picker's parts
    /// `{prefix}:area`, `{prefix}:hue` and `{prefix}:hex`, closing it
    /// `{prefix}:dismiss` and the way back `{prefix}:reset`.
    pub prefix: String,
    pub name: &'a str,
    pub color: Color,
    /// It differs from the base's.
    pub changed: bool,
    pub open: bool,
    /// Where the picker's thumbs stand while it is used.
    pub picked: Option<Hsv>,
}

/// The row of [`TokenRow`].
pub fn token_row(row: &TokenRow, look: &Look) -> Element {
    // A translucent color over the menu's ground, as it shows on a surface.
    let swatch = Element::clickable(
        row.prefix.clone(),
        Element::panel(Element::spacer())
            .size(Some(28.0), Some(28.0))
            .radius(6.0)
            .background(row.color),
    )
    .size(Some(28.0), Some(28.0))
    .radius(6.0)
    .border(look.border, 1.0)
    .accessible(
        limn::Access::new(limn::Role::Button)
            .name(tr!("Cambiar {name}", name = row.name))
            .expanded(row.open),
    );
    let picker = ColorPicker::new(
        row.prefix.clone(),
        row.picked
            .unwrap_or_else(|| Hsv::from(Look::alpha(row.color, 255))),
        look.menu_palette(),
    )
    .part_labels(
        t("Saturación y brillo"),
        t("Tono"),
        t("Color en hexadecimal"),
    )
    .width(208.0)
    .render()
    .padding(6.0);
    let swatch = Popover::new(
        swatch,
        picker,
        row.open,
        format!("{}:dismiss", row.prefix),
        look.menu_palette(),
    )
    .placement(PopoverPlacement::BottomStart)
    .width(220.0)
    .render();
    let mut parts = vec![
        swatch.centered_y(),
        Element::column(vec![
            line(row.name, 14.0, look.text),
            line(hex(row.color), 12.0, look.muted),
        ])
        .gap(2.0)
        .flex(1.0)
        .centered_y(),
    ];
    if row.changed {
        parts.push(
            limn::Component::render(
                limn::Button::new(
                    format!("{}:reset", row.prefix),
                    t("Restablecer"),
                    look.palette(),
                )
                .variant(limn::ButtonVariant::Ghost)
                .icon(Icon::Refresh),
            )
            .centered_y(),
        );
    }
    Element::row(parts).gap(10.0).padding(4.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};

    #[test]
    fn a_theme_file_reads_back_as_written_and_anything_else_is_refused() {
        let mut theme = CustomTheme::new("Bosque", "dark");
        theme.colors.insert("accent", Color::rgb(63, 185, 122));
        theme.colors.insert("hover", Color::rgba(255, 255, 255, 20));
        let written = theme.to_json();
        assert!(written.contains("\"rune_theme\": 1"), "{written}");
        assert!(written.contains("#ffffff14"), "alpha kept: {written}");
        assert_eq!(CustomTheme::parse(&written), Ok(theme.clone()));
        assert_eq!(theme.filename(), "Bosque.rune-theme.json");
        assert_eq!(
            CustomTheme::new("../a/b", "dark").filename(),
            "-a-b.rune-theme.json"
        );
        let refused = [
            "no es json",
            r##"{"rune_theme":2,"name":"x","base":"dark"}"##,
            r##"{"rune_theme":1,"name":"  ","base":"dark"}"##,
            r##"{"rune_theme":1,"name":"x","base":"sepia"}"##,
            r##"{"rune_theme":1,"name":"x","base":"dark","colors":{"glow":"#ffffff"}}"##,
            r##"{"rune_theme":1,"name":"x","base":"dark","colors":{"text":"red"}}"##,
            r##"{"rune_theme":1,"name":"x","base":"dark","script":"rm -rf"}"##,
        ];
        for text in refused {
            assert!(CustomTheme::parse(text).is_err(), "{text}");
        }
        let big = format!(
            r#"{{"rune_theme":1,"name":"x","base":"dark","pad":"{}"}}"#,
            " ".repeat(MAX_FILE)
        );
        assert!(CustomTheme::parse(&big).is_err());
        // The settings' list: one that does not read and a repeated name go.
        let list = list_json(&[theme.clone(), CustomTheme::new("Claro", "light")]);
        let mut kept: Vec<serde_json::Value> = serde_json::from_str(&list).unwrap();
        kept.push(serde_json::json!({"rune_theme": 1, "name": "Bosque", "base": "light"}));
        kept.push(serde_json::json!({"rune_theme": 9}));
        let themes = parse_list(&serde_json::to_string(&kept).unwrap());
        assert_eq!(themes.len(), 2);
        assert_eq!(themes[0], theme);
        assert!(parse_list("[").is_empty());
    }

    #[test]
    fn colors_read_with_or_without_alpha() {
        assert_eq!(parse_color("#E0A43C"), Some(Color::rgb(224, 164, 60)));
        assert_eq!(
            parse_color("e0a43c80"),
            Some(Color::rgba(224, 164, 60, 128))
        );
        assert_eq!(parse_color("#e0a"), None);
        assert_eq!(parse_color("#e0a43g"), None);
        assert_eq!(hex(Color::rgb(224, 164, 60)), "#e0a43c");
        assert_eq!(hex(Color::rgba(0, 0, 0, 0)), "#00000000");
    }

    #[test]
    fn a_theme_paints_its_colors_over_its_base_with_opaque_surfaces() {
        let mut theme = CustomTheme::new("x", "light");
        theme.colors.insert("text", Color::rgb(1, 2, 3));
        theme.colors.insert("canvas", Color::rgba(9, 9, 9, 10));
        let look = theme.look();
        assert_eq!(look.text, Color::rgb(1, 2, 3));
        assert_eq!(look.canvas, Color::rgb(9, 9, 9), "the window's own");
        assert_eq!(look.sidebar, Look::LIGHT.sidebar);
        assert_eq!(CustomTheme::new("x", "sepia").base, "onyx");
        // A message's cards (tarjetas-con-contraste): a file may set them, and
        // one written before they had a color of their own keeps the base's.
        let set = r##"{"rune_theme":1,"name":"x","base":"dark","colors":{"card":"#010203"}}"##;
        let set = CustomTheme::parse(set).unwrap().look();
        assert_eq!(set.card, Color::rgb(1, 2, 3));
        let older = CustomTheme::parse(r##"{"rune_theme":1,"name":"x","base":"dark"}"##);
        assert_eq!(older.unwrap().look().card, Look::DARK.card);
    }

    #[test]
    fn the_accent_brings_readable_text_and_its_mention() {
        let amber = Look::ONYX.with_accent(Color::rgb(224, 164, 60));
        assert_eq!(amber.on_accent, Color::rgb(16, 16, 18));
        assert_eq!(amber.mention.a, Look::ONYX.mention.a);
        let blue = Look::LIGHT.with_accent(Color::rgb(30, 60, 200));
        assert_eq!(blue.on_accent, Color::rgb(255, 255, 255));
        assert_eq!(
            Look::alpha(blue.mention, 255),
            Color::rgb(30, 60, 200),
            "the mention follows"
        );
        assert!(crate::look::contrast(blue.on_accent, blue.accent) > 4.5);
        let mut look = Look::DARK;
        *look.token_mut("danger_fill").unwrap() = Color::rgb(1, 1, 1);
        assert_eq!(look.token("danger_fill"), Some(Color::rgb(1, 1, 1)));
        assert_eq!(look.token("glow"), None);
        // Every key of the editor names a color.
        for (_, tokens) in TOKENS {
            for (key, _) in tokens {
                assert!(Look::ONYX.token(key).is_some(), "{key}");
            }
        }
    }

    #[test]
    fn a_see_through_look_paints_no_surface_and_keeps_its_components_opaque() {
        let glass = Look::DARK.see_through();
        assert_eq!(glass.canvas.a, 0);
        assert_eq!((glass.canvas.r, glass.sidebar.a, glass.rail.a), (26, 0, 0));
        assert_eq!(glass.menu, Look::DARK.menu, "menus stay readable");
        assert_eq!(glass.palette().surface, Look::DARK.canvas);
        assert_eq!(Look::DARK.backdrop(0), Look::DARK.canvas);
        assert_eq!(Look::DARK.backdrop(40).a, 153);
    }

    #[test]
    fn the_preview_and_a_color_lay_out() {
        let look = Look::DARK;
        let size = limn::Size::new(560.0, 400.0);
        let (_, shown) = frame(preview(&look, 520.0), size);
        assert!(texts(&shown).contains(&"Así se ve tu tema.".to_owned()));
        assert!(controls(&shown).is_empty(), "nothing in it is a control");
        let row = TokenRow {
            prefix: "us:theme-token:canvas".into(),
            name: "Conversación",
            color: Color::rgb(26, 29, 37),
            changed: true,
            open: false,
            picked: None,
        };
        let (_, shown) = frame(token_row(&row, &look), size);
        let ids = controls(&shown);
        assert!(ids.contains(&"us:theme-token:canvas".to_owned()));
        assert!(ids.contains(&"us:theme-token:canvas:reset".to_owned()));
        assert!(texts(&shown).contains(&"#1a1d25".to_owned()));
        // At the top, the picker under it.
        let open = TokenRow {
            open: true,
            changed: false,
            ..row
        };
        let (_, open) = frame(
            Element::column(vec![token_row(&open, &look), Element::spacer().flex(1.0)]),
            size,
        );
        for part in ["area", "hue", "hex"] {
            let part = format!("us:theme-token:canvas:{part}");
            assert!(
                open.regions.iter().any(|region| region.id == part),
                "{part}"
            );
        }
        assert!(!controls(&open).contains(&"us:theme-token:canvas:reset".to_owned()));
    }
}
