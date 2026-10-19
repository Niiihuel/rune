//! Themes of one's own, the main color and the window's transparency
//! (temas-propios), as Serein offers them (`docs/theme-api.md`): a theme is
//! a built-in look with some colors replaced, made in Ajustes › Apariencia
//! with a preview, kept on this device and shared as a file of data
//! ([`rune_components::theme`]); the main color tints whatever look is in
//! use; and the desktop may show through the window. No extensions, code,
//! images or remote catalog.

use std::time::Instant;

use limn::{Color, UiAction};
use rune_components::server_settings::{hex as rgb_hex, parse_hex};
use rune_components::theme::{self, CustomTheme};
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::Rgb;

use crate::{Intent, Ui};

/// Themes kept, at most, as Serein bounds its installed ones.
pub const MAX_THEMES: usize = 8;
/// The most of the desktop that may show through: past it text floats.
pub const MAX_TRANSPARENCY: u8 = 90;
/// The steps of its slider, in percent.
pub const TRANSPARENCY_STEP: u8 = 5;

const THEMES_SETTING: &str = "custom_themes";
const ACTIVE_SETTING: &str = "custom_theme";
const ACCENT_SETTING: &str = "accent_color";

pub(crate) const CREATE: &str = "us:theme-create";
pub(crate) const IMPORT: &str = "us:theme-import";
pub(crate) const EDIT: &str = "us:theme-edit";
pub(crate) const EXPORT: &str = "us:theme-export";
pub(crate) const DELETE: &str = "us:theme-delete";
pub(crate) const DELETE_CONFIRM: &str = "us:theme-delete:confirm";
pub(crate) const DELETE_CANCEL: &str = "us:theme-delete:cancel";
/// `{USE}{index}` chooses a saved theme.
pub(crate) const USE: &str = "us:theme-use=";
pub(crate) const NAME: &str = "us:theme-name";
/// `{BASE}{key}` changes the draft's base.
pub(crate) const BASE: &str = "us:theme-base=";
pub(crate) const SAVE: &str = "us:theme-save";
pub(crate) const CANCEL: &str = "us:theme-cancel";
/// `{TOKEN}{key}` is a color's swatch, and the prefix of its picker.
pub(crate) const TOKEN: &str = "us:theme-token:";
/// The main color's swatches, as [`rune_components::server_settings::Swatches`]
/// names them.
pub(crate) const ACCENT: &str = "us:theme-accent";
pub(crate) const TRANSPARENCY: &str = "us:theme-transparency";

/// What the person made of the look, on this device.
#[derive(Debug, Default)]
pub struct Themes {
    pub saved: Vec<CustomTheme>,
    /// The saved theme in use; `None` is the built-in [`Ui::theme`].
    pub active: Option<usize>,
    /// The main color over any look; `None` is the look's own.
    pub accent: Option<Color>,
    /// The main color's hex field, while its picker is open.
    pub accent_hex: Option<String>,
    /// How much of the desktop shows through the window, in percent.
    pub transparency: u8,
    /// The window opened able to show the desktop: the transparency
    /// applies now; else from the next start.
    pub see_through: bool,
    pub editor: Option<Editor>,
    /// «Borrar» was pressed once: it asks before the theme goes.
    pub deleting: bool,
    /// The import or export under way.
    pub waiting: Option<u64>,
}

/// A theme being made or changed.
#[derive(Clone, Debug, PartialEq)]
pub struct Editor {
    pub draft: CustomTheme,
    /// The name as typed.
    pub name: String,
    /// The saved theme it replaces; `None` for a new one.
    pub replaces: Option<usize>,
    /// The color whose picker is open.
    pub picking: Option<&'static str>,
    /// Why it was not saved.
    pub error: Option<String>,
}

impl Ui {
    /// The look the window paints with: the theme in use, in the main color
    /// chosen. Opaque: see [`Self::panes_look`] for the window's columns.
    pub fn look(&self) -> Look {
        let look = match self.active_theme() {
            Some(theme) => theme.look(),
            None => *self.theme.look(),
        };
        match self.themes.accent.or(self.system_accent()) {
            Some(accent) => look.with_accent(accent),
            None => look,
        }
    }

    /// The desktop's accent color, while the theme follows the system's
    /// (windows-base); the person's own main color goes over it.
    pub(crate) fn system_accent(&self) -> Option<Color> {
        if self.prefs.system_theme && self.active_theme().is_none() {
            self.desktop.system_accent
        } else {
            None
        }
    }

    /// The look of the rail, the columns and the conversation, and the
    /// window's ground under them: see-through while the desktop shows
    /// through the window ([`Look::see_through`]), else [`Self::look`]'s.
    pub fn panes_look(&self) -> (Look, Color) {
        let look = self.look();
        if self.see_through() {
            (look.see_through(), look.backdrop(self.themes.transparency))
        } else {
            (look, look.canvas)
        }
    }

    /// Whether the desktop shows through the window now.
    pub fn see_through(&self) -> bool {
        self.themes.see_through && self.themes.transparency > 0
    }

    pub fn active_theme(&self) -> Option<&CustomTheme> {
        self.themes.saved.get(self.themes.active?)
    }
}

impl Themes {
    /// What stays for another account or provider: all of it, the device's.
    pub(crate) fn kept(&mut self) -> Self {
        std::mem::take(self)
    }
}

/// What the host read before the window opened: the transparency kept,
/// and whether the window can show the desktop through it.
pub fn window_known(ui: &mut Ui, transparency: u8, see_through: bool) {
    ui.themes.transparency = transparency.min(MAX_TRANSPARENCY);
    ui.themes.see_through = see_through;
}

/// The settings to keep.
pub(crate) fn kept(ui: &Ui) -> Vec<(&'static str, Option<String>)> {
    let themes = &ui.themes;
    vec![
        (
            THEMES_SETTING,
            (!themes.saved.is_empty()).then(|| theme::list_json(&themes.saved)),
        ),
        (
            ACTIVE_SETTING,
            ui.active_theme().map(|theme| theme.name.clone()),
        ),
        (ACCENT_SETTING, themes.accent.map(theme::hex)),
    ]
}

/// Take back what [`kept`] kept; what does not read is left out.
pub(crate) fn restore(ui: &mut Ui, setting: &impl Fn(&str) -> Option<String>) {
    let themes = &mut ui.themes;
    themes.saved = setting(THEMES_SETTING)
        .map(|kept| theme::parse_list(&kept))
        .unwrap_or_default();
    themes.saved.truncate(MAX_THEMES);
    themes.active = setting(ACTIVE_SETTING)
        .and_then(|name| themes.saved.iter().position(|theme| theme.name == name));
    themes.accent = setting(ACCENT_SETTING)
        .and_then(|kept| theme::parse_color(&kept))
        .map(|color| Look::alpha(color, 255));
}

fn save_themes(ui: &Ui) -> Vec<Intent> {
    kept(ui)
        .into_iter()
        .filter(|(key, _)| *key != ACCENT_SETTING)
        .map(|(key, value)| Intent::SaveSetting {
            key: key.into(),
            value,
        })
        .collect()
}

fn save_accent(ui: &Ui) -> Intent {
    Intent::SaveSetting {
        key: ACCENT_SETTING.into(),
        value: ui.themes.accent.map(theme::hex),
    }
}

/// A built-in theme, or the system's, was chosen: no saved one is in use.
pub(crate) fn built_in_chosen(ui: &mut Ui) -> Vec<Intent> {
    ui.themes.deleting = false;
    if ui.themes.active.take().is_none() {
        return Vec::new();
    }
    vec![Intent::SaveSetting {
        key: ACTIVE_SETTING.into(),
        value: None,
    }]
}

/// Apply an action of the theme's settings; `None` for the rest.
pub(crate) fn update(ui: &mut Ui, action: &UiAction, now: Instant) -> Option<Vec<Intent>> {
    match action {
        UiAction::Clicked { id } if id.starts_with("us:theme-") => Some(clicked(ui, id, now)),
        UiAction::InputChanged { id, value } if id.starts_with("us:theme-") => {
            Some(typed(ui, id, value))
        }
        UiAction::Submitted { id, .. } if id == NAME => Some(save(ui, now)),
        UiAction::Submitted { id, .. } if id.starts_with(ACCENT) => {
            ui.themes.accent_hex = None;
            Some(vec![save_accent(ui)])
        }
        UiAction::SliderChanged {
            id,
            fraction,
            committed,
        } if id == TRANSPARENCY => {
            let steps = f32::from(MAX_TRANSPARENCY / TRANSPARENCY_STEP);
            ui.themes.transparency =
                (fraction.clamp(0.0, 1.0) * steps).round() as u8 * TRANSPARENCY_STEP;
            Some(if *committed {
                vec![Intent::SetTransparency(ui.themes.transparency)]
            } else {
                Vec::new()
            })
        }
        _ => None,
    }
}

fn clicked(ui: &mut Ui, id: &str, now: Instant) -> Vec<Intent> {
    if let Some(index) = id.strip_prefix(USE) {
        let Some(index) = index
            .parse()
            .ok()
            .filter(|index| *index < ui.themes.saved.len())
        else {
            return Vec::new();
        };
        ui.themes.active = Some(index);
        ui.themes.deleting = false;
        ui.prefs.system_theme = false;
        let mut intents = save_themes(ui);
        // No longer the desktop's: the built-in one stays as it was.
        intents.push(Intent::SaveSetting {
            key: "theme".into(),
            value: Some(ui.theme.key().into()),
        });
        return intents;
    }
    if let Some(rest) = id.strip_prefix(TOKEN) {
        return token_clicked(ui, rest);
    }
    if let Some(rest) = id.strip_prefix(ACCENT) {
        return accent_clicked(ui, rest.trim_start_matches(':'));
    }
    if let Some(base) = id.strip_prefix(BASE) {
        if let Some(editor) = &mut ui.themes.editor {
            editor.draft.base = CustomTheme::new("", base).base;
        }
        return Vec::new();
    }
    match id {
        CREATE => {
            let draft = match ui.active_theme() {
                Some(theme) => CustomTheme {
                    name: tr!("{name} (copia)", name = theme.name),
                    ..theme.clone()
                },
                None => CustomTheme::new(t("Mi tema"), ui.theme.key()),
            };
            open_editor(ui, draft, None)
        }
        EDIT => match (ui.themes.active, ui.active_theme().cloned()) {
            (Some(index), Some(theme)) => open_editor(ui, theme, Some(index)),
            _ => Vec::new(),
        },
        CANCEL => {
            ui.themes.editor = None;
            Vec::new()
        }
        SAVE => save(ui, now),
        DELETE => {
            ui.themes.deleting = ui.themes.active.is_some();
            Vec::new()
        }
        DELETE_CANCEL => {
            ui.themes.deleting = false;
            Vec::new()
        }
        DELETE_CONFIRM => {
            ui.themes.deleting = false;
            let Some(index) = ui.themes.active.take() else {
                return Vec::new();
            };
            let gone = ui.themes.saved.remove(index);
            if ui.themes.editor.as_ref().and_then(|editor| editor.replaces) == Some(index) {
                ui.themes.editor = None;
            }
            ui.notify(tr!("Se borró «{name}»", name = gone.name), None, false, now);
            save_themes(ui)
        }
        IMPORT => {
            if ui.themes.waiting.is_some() {
                return Vec::new();
            }
            let request = ui.fresh_request();
            ui.themes.waiting = Some(request);
            vec![Intent::ImportTheme { request }]
        }
        EXPORT => {
            if ui.themes.waiting.is_some() {
                return Vec::new();
            }
            // The draft being made, or else the theme in use.
            let shared = match &ui.themes.editor {
                Some(editor) => CustomTheme {
                    name: theme::valid_name(&editor.name)
                        .unwrap_or_else(|| editor.draft.name.clone()),
                    ..editor.draft.clone()
                },
                None => match ui.active_theme() {
                    Some(theme) => theme.clone(),
                    None => return Vec::new(),
                },
            };
            let request = ui.fresh_request();
            ui.themes.waiting = Some(request);
            vec![Intent::ExportTheme {
                request,
                filename: shared.filename(),
                contents: shared.to_json(),
            }]
        }
        _ => Vec::new(),
    }
}

fn open_editor(ui: &mut Ui, draft: CustomTheme, replaces: Option<usize>) -> Vec<Intent> {
    let name = draft.name.clone();
    ui.themes.deleting = false;
    ui.themes.editor = Some(Editor {
        draft,
        name: name.clone(),
        replaces,
        picking: None,
        error: None,
    });
    vec![
        Intent::SetInput {
            id: NAME.into(),
            value: name,
        },
        Intent::Focus(NAME.into()),
    ]
}

/// The color the draft shows for `key`: its own, or its base's.
pub(crate) fn draft_color(draft: &CustomTheme, key: &str) -> Color {
    draft
        .colors
        .get(key)
        .copied()
        .or_else(|| draft.look().token(key))
        .unwrap_or(Color::TRANSPARENT)
}

fn token_clicked(ui: &mut Ui, rest: &str) -> Vec<Intent> {
    let Some(editor) = &mut ui.themes.editor else {
        return Vec::new();
    };
    let (key, part) = rest.split_once(':').unwrap_or((rest, ""));
    let Some(key) = theme::token_key(key) else {
        return Vec::new();
    };
    match part {
        "" if editor.picking == Some(key) => editor.picking = None,
        "" => {
            editor.picking = Some(key);
            let color = draft_color(&editor.draft, key);
            let hex = rgb_hex(rgb(color));
            return vec![Intent::SetInput {
                id: format!("{TOKEN}{key}:hex"),
                value: hex,
            }];
        }
        "dismiss" => editor.picking = None,
        "reset" => {
            editor.draft.colors.remove(key);
            if editor.picking == Some(key) {
                editor.picking = None;
            }
        }
        _ => {}
    }
    Vec::new()
}

fn rgb(color: Color) -> Rgb {
    Rgb(u32::from(color.r) << 16 | u32::from(color.g) << 8 | u32::from(color.b))
}

fn color_of(rgb: Rgb) -> Color {
    let (r, g, b) = rgb.channels();
    Color::rgb(r, g, b)
}

fn typed(ui: &mut Ui, id: &str, value: &str) -> Vec<Intent> {
    if id == NAME {
        if let Some(editor) = &mut ui.themes.editor {
            editor.name = value.to_owned();
            editor.error = None;
        }
        return Vec::new();
    }
    if let Some(key) = id
        .strip_prefix(TOKEN)
        .and_then(|rest| rest.strip_suffix(":hex"))
        .and_then(theme::token_key)
    {
        let (Some(editor), Some(typed)) = (&mut ui.themes.editor, parse_hex(value)) else {
            return Vec::new();
        };
        // The picker is opaque: a translucent color keeps its alpha.
        let alpha = draft_color(&editor.draft, key).a;
        let color = Look::alpha(color_of(typed), alpha);
        let base = Look::named(editor.draft.base).and_then(|look| look.token(key));
        if base == Some(color) {
            editor.draft.colors.remove(key);
        } else {
            editor.draft.colors.insert(key, color);
        }
        return Vec::new();
    }
    if id == format!("{ACCENT}:hex") {
        ui.themes.accent_hex = Some(value.to_owned());
        if let Some(color) = parse_hex(value) {
            ui.themes.accent = Some(color_of(color));
        }
    }
    Vec::new()
}

fn accent_clicked(ui: &mut Ui, what: &str) -> Vec<Intent> {
    let themes = &mut ui.themes;
    match what {
        "default" => {
            themes.accent = None;
            themes.accent_hex = None;
        }
        "custom" => {
            let current = rgb_hex(rgb(ui.look().accent));
            ui.themes.accent_hex = Some(current.clone());
            return vec![
                Intent::SetInput {
                    id: format!("{ACCENT}:hex"),
                    value: current,
                },
                Intent::Focus(format!("{ACCENT}:hex")),
            ];
        }
        "dismiss" => themes.accent_hex = None,
        preset => match parse_hex(preset) {
            Some(color) => {
                themes.accent = Some(color_of(color));
                themes.accent_hex = None;
            }
            None => return Vec::new(),
        },
    }
    vec![save_accent(ui)]
}

/// «Guardar»: the draft, named as typed, replaces the theme it was made
/// from or joins the list, and is used.
fn save(ui: &mut Ui, now: Instant) -> Vec<Intent> {
    let themes = &mut ui.themes;
    let Some(editor) = &mut themes.editor else {
        return Vec::new();
    };
    let Some(name) = theme::valid_name(&editor.name) else {
        editor.error = Some(tr!(
            "Escribí un nombre de hasta {max} caracteres.",
            max = theme::MAX_NAME
        ));
        return Vec::new();
    };
    let taken = themes
        .saved
        .iter()
        .enumerate()
        .any(|(at, theme)| theme.name == name && Some(at) != editor.replaces);
    if taken {
        editor.error = Some(t("Ya tenés un tema con ese nombre.").into());
        return Vec::new();
    }
    if editor.replaces.is_none() && themes.saved.len() >= MAX_THEMES {
        editor.error = Some(tr!(
            "Podés guardar hasta {max} temas: borrá uno antes.",
            max = MAX_THEMES
        ));
        return Vec::new();
    }
    let theme = CustomTheme {
        name,
        ..editor.draft.clone()
    };
    let index = match editor.replaces {
        Some(index) if index < themes.saved.len() => {
            themes.saved[index] = theme;
            index
        }
        _ => {
            themes.saved.push(theme);
            themes.saved.len() - 1
        }
    };
    themes.active = Some(index);
    themes.editor = None;
    ui.prefs.system_theme = false;
    ui.notify(t("Tema guardado"), None, false, now);
    let mut intents = save_themes(ui);
    intents.push(Intent::SaveSetting {
        key: "theme".into(),
        value: Some(ui.theme.key().into()),
    });
    intents
}

/// What the file chosen for `request` holds, as the host read it: a theme
/// that joins the list and is used, or why not; nothing when the person
/// cancelled. A name already taken gets a number.
pub fn imported(
    ui: &mut Ui,
    request: u64,
    read: Result<Option<String>, String>,
    now: Instant,
) -> Vec<Intent> {
    if ui.themes.waiting != Some(request) {
        return Vec::new();
    }
    ui.themes.waiting = None;
    let text = match read {
        Ok(Some(text)) => text,
        Ok(None) => return Vec::new(),
        Err(reason) => {
            ui.notify(t("No se pudo importar el tema"), Some(reason), true, now);
            return Vec::new();
        }
    };
    let mut theme = match CustomTheme::parse(&text) {
        Ok(theme) => theme,
        Err(reason) => {
            ui.notify(t("No se pudo importar el tema"), Some(reason), true, now);
            return Vec::new();
        }
    };
    let themes = &mut ui.themes;
    if themes.saved.len() >= MAX_THEMES {
        ui.notify(
            t("No se pudo importar el tema"),
            Some(tr!(
                "Podés guardar hasta {max} temas: borrá uno antes.",
                max = MAX_THEMES
            )),
            true,
            now,
        );
        return Vec::new();
    }
    let name = theme.name.clone();
    let mut number = 2;
    while themes.saved.iter().any(|known| known.name == theme.name) {
        theme.name = format!("{name} ({number})");
        number += 1;
    }
    themes.saved.push(theme);
    themes.active = Some(themes.saved.len() - 1);
    themes.deleting = false;
    ui.prefs.system_theme = false;
    let name = ui.themes.saved.last().map(|theme| theme.name.clone());
    ui.notify(
        tr!("Se importó «{name}»", name = name.unwrap_or_default()),
        None,
        false,
        now,
    );
    let mut intents = save_themes(ui);
    intents.push(Intent::SaveSetting {
        key: "theme".into(),
        value: Some(ui.theme.key().into()),
    });
    intents
}

/// What came of saving the theme file of `request`: its file's name, or
/// why not; nothing when the person cancelled.
pub fn exported(ui: &mut Ui, request: u64, saved: Result<Option<String>, String>, now: Instant) {
    if ui.themes.waiting != Some(request) {
        return;
    }
    ui.themes.waiting = None;
    match saved {
        Ok(Some(filename)) => ui.notify(
            t("Tema exportado"),
            Some(tr!("Se guardó como {filename}.", filename)),
            false,
            now,
        ),
        Ok(None) => {}
        Err(reason) => ui.notify(t("No se pudo exportar el tema"), Some(reason), true, now),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn click(ui: &mut Ui, id: &str) -> Vec<Intent> {
        update(ui, &UiAction::Clicked { id: id.into() }, Instant::now()).unwrap()
    }

    fn input(ui: &mut Ui, id: &str, value: &str) {
        update(
            ui,
            &UiAction::InputChanged {
                id: id.into(),
                value: value.into(),
            },
            Instant::now(),
        );
    }

    #[test]
    fn a_theme_is_made_on_its_base_saved_used_and_kept() {
        let mut ui = Ui {
            theme: crate::Theme::Dark,
            ..Ui::default()
        };
        let asked = click(&mut ui, CREATE);
        assert!(asked.contains(&Intent::Focus(NAME.into())));
        assert_eq!(ui.themes.editor.as_ref().unwrap().draft.base, "dark");
        input(&mut ui, NAME, "  Bosque ");
        click(&mut ui, &format!("{BASE}light"));
        // A color from its picker, then one back to the base's.
        click(&mut ui, &format!("{TOKEN}canvas"));
        assert_eq!(ui.themes.editor.as_ref().unwrap().picking, Some("canvas"));
        input(&mut ui, &format!("{TOKEN}canvas:hex"), "102030");
        input(&mut ui, &format!("{TOKEN}hover:hex"), "102030");
        let draft = &ui.themes.editor.as_ref().unwrap().draft;
        assert_eq!(draft.colors["canvas"], Color::rgb(16, 32, 48));
        assert_eq!(
            draft.colors["hover"],
            Color::rgba(16, 32, 48, Look::LIGHT.hover.a),
            "a translucent color keeps its alpha"
        );
        input(&mut ui, &format!("{TOKEN}text:hex"), "181b22");
        assert!(
            !ui.themes
                .editor
                .as_ref()
                .unwrap()
                .draft
                .colors
                .contains_key("text"),
            "the base's own color is no change"
        );
        click(&mut ui, &format!("{TOKEN}hover:reset"));
        // Nothing of it shows until it is saved.
        assert_eq!(ui.look(), Look::DARK);
        let intents = click(&mut ui, SAVE);
        assert!(ui.themes.editor.is_none());
        assert_eq!(ui.look().canvas, Color::rgb(16, 32, 48));
        assert_eq!(ui.look().sidebar, Look::LIGHT.sidebar);
        assert!(intents.contains(&Intent::SaveSetting {
            key: ACTIVE_SETTING.into(),
            value: Some("Bosque".into())
        }));
        // Read back as the next run would.
        let kept = ui.settings_to_keep();
        let mut again = Ui::default();
        again.restore(|key| {
            kept.iter()
                .find(|(found, _)| *found == key)
                .and_then(|(_, value)| value.clone())
        });
        assert_eq!(again.themes.saved, ui.themes.saved);
        assert_eq!(again.look(), ui.look());
        // A built-in one leaves it unused, kept.
        assert_eq!(built_in_chosen(&mut ui).len(), 1);
        assert_eq!(ui.look(), Look::DARK);
        assert_eq!(ui.themes.saved.len(), 1);
    }

    #[test]
    fn a_name_must_be_valid_and_free_and_the_list_is_bounded() {
        let mut ui = Ui::default();
        click(&mut ui, CREATE);
        input(&mut ui, NAME, " ");
        assert!(click(&mut ui, SAVE).is_empty());
        assert!(ui.themes.editor.as_ref().unwrap().error.is_some());
        input(&mut ui, NAME, "Uno");
        click(&mut ui, SAVE);
        // Its copy starts from it, and cannot take its name.
        click(&mut ui, CREATE);
        assert_eq!(ui.themes.editor.as_ref().unwrap().name, "Uno (copia)");
        input(&mut ui, NAME, "Uno");
        click(&mut ui, SAVE);
        assert!(ui.themes.editor.is_some(), "taken");
        click(&mut ui, CANCEL);
        // Editing keeps its own name.
        click(&mut ui, EDIT);
        click(&mut ui, SAVE);
        assert_eq!(ui.themes.saved.len(), 1);
        for number in 2..=MAX_THEMES {
            click(&mut ui, CREATE);
            input(&mut ui, NAME, &format!("Tema {number}"));
            click(&mut ui, SAVE);
        }
        click(&mut ui, CREATE);
        input(&mut ui, NAME, "Uno más");
        click(&mut ui, SAVE);
        assert_eq!(ui.themes.saved.len(), MAX_THEMES);
        assert!(ui.themes.editor.as_ref().unwrap().error.is_some());
    }

    #[test]
    fn a_theme_is_deleted_only_once_confirmed() {
        let mut ui = Ui::default();
        click(&mut ui, CREATE);
        click(&mut ui, SAVE);
        click(&mut ui, DELETE);
        assert!(ui.themes.deleting);
        click(&mut ui, DELETE_CANCEL);
        assert_eq!(ui.themes.saved.len(), 1);
        click(&mut ui, DELETE);
        let intents = click(&mut ui, DELETE_CONFIRM);
        assert!(ui.themes.saved.is_empty() && ui.themes.active.is_none());
        assert!(intents.contains(&Intent::SaveSetting {
            key: THEMES_SETTING.into(),
            value: None
        }));
    }

    #[test]
    fn a_file_is_imported_checked_and_one_exported_as_used() {
        let mut ui = Ui::default();
        let now = Instant::now();
        let asked = click(&mut ui, IMPORT);
        let [Intent::ImportTheme { request }] = asked[..] else {
            panic!("{asked:?}")
        };
        assert!(click(&mut ui, IMPORT).is_empty(), "one dialog at a time");
        // Another request's answer is not this one's.
        let file = r##"{"rune_theme":1,"name":"Mar","base":"dark","colors":{"accent":"#3366cc"}}"##;
        assert!(imported(&mut ui, request + 1, Ok(Some(file.into())), now).is_empty());
        assert!(!imported(&mut ui, request, Ok(Some(file.into())), now).is_empty());
        assert_eq!(ui.look().accent, Color::rgb(51, 102, 204));
        // The same again takes a number; a bad one says why.
        let Intent::ImportTheme { request } = click(&mut ui, IMPORT)[0] else {
            unreachable!()
        };
        imported(&mut ui, request, Ok(Some(file.into())), now);
        assert_eq!(ui.active_theme().unwrap().name, "Mar (2)");
        let Intent::ImportTheme { request } = click(&mut ui, IMPORT)[0] else {
            unreachable!()
        };
        ui.notices.clear();
        imported(&mut ui, request, Ok(Some("{}".into())), now);
        assert_eq!(ui.themes.saved.len(), 2);
        assert!(ui.notices.iter().any(|notice| notice.error));
        // Export: the theme in use, as a file.
        let asked = click(&mut ui, EXPORT);
        let [Intent::ExportTheme {
            request,
            ref filename,
            ref contents,
        }] = asked[..]
        else {
            panic!("{asked:?}")
        };
        assert_eq!(filename, "Mar (2).rune-theme.json");
        assert_eq!(CustomTheme::parse(contents).unwrap().name, "Mar (2)");
        exported(&mut ui, request, Ok(Some(filename.clone())), now);
        assert!(ui.themes.waiting.is_none());
    }

    #[test]
    fn the_main_color_tints_any_look_and_goes_back_to_its_own() {
        let mut ui = Ui::default();
        let intents = click(&mut ui, &format!("{ACCENT}:3498db"));
        assert_eq!(ui.look().accent, Color::rgb(0x34, 0x98, 0xdb));
        assert_eq!(
            intents,
            [Intent::SaveSetting {
                key: ACCENT_SETTING.into(),
                value: Some("#3498db".into())
            }]
        );
        click(&mut ui, &format!("{ACCENT}:custom"));
        assert_eq!(ui.themes.accent_hex.as_deref(), Some("3498db"));
        input(&mut ui, &format!("{ACCENT}:hex"), "ff0000");
        assert_eq!(ui.look().accent, Color::rgb(255, 0, 0), "live");
        input(&mut ui, &format!("{ACCENT}:hex"), "ff00");
        assert_eq!(ui.look().accent, Color::rgb(255, 0, 0));
        click(&mut ui, &format!("{ACCENT}:default"));
        assert_eq!(ui.look(), Look::ONYX);
    }

    #[test]
    fn transparency_moves_in_steps_and_applies_only_to_a_see_through_window() {
        let mut ui = Ui::default();
        let slide = |ui: &mut Ui, fraction, committed| {
            update(
                ui,
                &UiAction::SliderChanged {
                    id: TRANSPARENCY.into(),
                    fraction,
                    committed,
                },
                Instant::now(),
            )
            .unwrap()
        };
        assert!(slide(&mut ui, 0.34, false).is_empty());
        assert_eq!(ui.themes.transparency, 30);
        assert_eq!(slide(&mut ui, 0.34, true), [Intent::SetTransparency(30)]);
        // Opened opaque: from the next start.
        assert!(!ui.see_through());
        assert_eq!(ui.panes_look().0, ui.look());
        window_known(&mut ui, 30, true);
        let (panes, ground) = ui.panes_look();
        assert_eq!(panes.canvas.a, 0);
        assert_eq!(ground, Look::ONYX.backdrop(30));
        slide(&mut ui, 1.0, true);
        assert_eq!(ui.themes.transparency, MAX_TRANSPARENCY);
        window_known(&mut ui, 250, false);
        assert_eq!(ui.themes.transparency, MAX_TRANSPARENCY);
    }
}
