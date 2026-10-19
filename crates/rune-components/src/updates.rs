//! RUNE's own updates (actualizaciones, 2026-10-19): the screen of a
//! mandatory one, shown before the main window, and the notice of an
//! optional one over it. Actions start with `update:`.

use limn::{
    Access, Button, ButtonVariant, Component, ControlSize, Element, FontFamily, Icon,
    Progress as Bar, Role, ScrollArea, Size, Spinner,
};
use rune_i18n::{t, tr};

use crate::{centered, file_size, line, strong_line, text, Look};

/// The actions of the screen, the notice and their settings.
pub mod ids {
    /// Download, install and start again.
    pub const INSTALL: &str = "update:install";
    /// Put the notice off until the next start.
    pub const LATER: &str = "update:later";
    /// Show or hide the notes in the notice.
    pub const NOTES: &str = "update:notes";
    /// Try a failed download or installation again.
    pub const RETRY: &str = "update:retry";
    /// Leave RUNE from the mandatory screen.
    pub const QUIT: &str = "update:quit";
    /// Look for a newer RUNE now (settings).
    pub const CHECK: &str = "update:check";
    /// Look when RUNE starts, or no longer (settings).
    pub const AT_START: &str = "update:at-start";
}

/// Where the update offered stands.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Stage {
    /// Offered, not started.
    #[default]
    Offered,
    Downloading {
        have: u64,
        whole: u64,
    },
    Installing,
    /// Why it stopped; RUNE is as it was.
    Failed(String),
}

/// An update as the screen and the notice show it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdateView {
    pub version: String,
    pub installed: String,
    pub notes: String,
    pub stage: Stage,
}

/// The window of a mandatory update, before anything else opens: the new
/// version and its notes, the download's progress, and why it failed with
/// «Reintentar» and «Salir».
pub fn screen(view: &UpdateView, window: Size, look: &Look) -> Element {
    let title = t("Actualización obligatoria");
    let mut parts = vec![
        centered(
            Element::icon(Icon::Custom(crate::MARK))
                .large_icon(72.0)
                .foreground(look.accent),
        )
        .size(None, Some(80.0)),
        centered(
            line(title, 20.0, look.text)
                .font_family(FontFamily::SansMedium)
                .accessible(Access::new(Role::Heading).name(title)),
        )
        .size(None, Some(28.0)),
        text(
            tr!(
                "RUNE {version} reemplaza a la {installed} que tenés instalada, que ya no se puede usar. RUNE se actualiza antes de abrir.",
                version = view.version,
                installed = view.installed
            ),
            14.0,
            look.muted,
        ),
    ];
    if !view.notes.trim().is_empty() {
        parts.push(notes(&view.notes, 180.0, look));
    }
    parts.push(stage(view, look, true));
    let middle = Element::column(parts).gap(14.0).size(Some(460.0), None);
    Element::panel(centered(middle))
        .background(look.canvas)
        .size(Some(window.width), Some(window.height))
}

/// The card of an optional update, over the window until it is installed
/// or put off: «Actualizar y reiniciar», «Más tarde» and its notes.
pub fn notice(view: &UpdateView, notes_open: bool, look: &Look) -> Element {
    let mut parts = vec![
        strong_line(
            tr!("RUNE {version} está disponible", version = view.version),
            14.0,
            look.text,
        ),
        text(
            tr!(
                "Tenés la {installed}. Se instala y RUNE vuelve a abrir.",
                installed = view.installed
            ),
            12.0,
            look.muted,
        ),
    ];
    if notes_open && !view.notes.trim().is_empty() {
        parts.push(notes(&view.notes, 140.0, look));
    }
    parts.push(stage(view, look, false));
    Element::column(parts)
        .gap(8.0)
        .padding(14.0)
        .size(Some(360.0), None)
        .background(look.menu)
        .border(look.border, 1.0)
        .radius(8.0)
        .accessible(Access::new(Role::Status).name(tr!(
            "RUNE {version} está disponible",
            version = view.version
        )))
}

/// The release's notes, scrolled when long.
fn notes(notes: &str, height: f32, look: &Look) -> Element {
    ScrollArea::new("update:notes-text", text(notes.trim(), 13.0, look.text))
        .appearance(look.scrollbar(true))
        .render()
        .max_height(height)
}

/// The progress, the failure or the buttons of where it stands.
fn stage(view: &UpdateView, look: &Look, mandatory: bool) -> Element {
    let button = |id: &str, label: &str, primary: bool| {
        Button::new(
            id,
            label,
            if primary {
                look.accent_palette()
            } else {
                look.menu_palette()
            },
        )
        .variant(if primary {
            ButtonVariant::Default
        } else {
            ButtonVariant::Ghost
        })
        .size(ControlSize::Compact)
        .render()
    };
    let buttons = |list: Vec<Element>| Element::row(list).gap(8.0);
    match &view.stage {
        Stage::Offered if mandatory => {
            centered(Spinner::new("update:waiting", look.accent).render()).size(None, Some(24.0))
        }
        Stage::Offered => buttons(vec![
            button(ids::INSTALL, t("Actualizar y reiniciar"), true),
            button(ids::LATER, t("Más tarde"), false),
            button(ids::NOTES, t("Notas"), false),
        ]),
        Stage::Downloading { have, whole } => {
            let fraction = if *whole == 0 {
                0.0
            } else {
                *have as f32 / *whole as f32
            };
            let said = tr!(
                "Descargando… {have} de {whole}",
                have = file_size(*have),
                whole = file_size(*whole)
            );
            Element::column(vec![
                Bar::new(fraction, look.accent_palette())
                    .id("update:progress")
                    .render()
                    .accessibility_label(said.clone()),
                line(said, 12.0, look.muted),
            ])
            .gap(6.0)
        }
        Stage::Installing => Element::row(vec![
            Spinner::new("update:installing", look.accent).render(),
            line(t("Instalando…"), 12.0, look.muted),
        ])
        .gap(8.0),
        Stage::Failed(reason) => Element::column(vec![
            text(reason.clone(), 13.0, look.danger).accessible(
                Access::new(Role::Alert)
                    .id("update:error")
                    .name(reason.clone()),
            ),
            if mandatory {
                buttons(vec![
                    button(ids::RETRY, t("Reintentar"), true),
                    button(ids::QUIT, t("Salir"), false),
                ])
            } else {
                buttons(vec![
                    button(ids::RETRY, t("Reintentar"), true),
                    button(ids::LATER, t("Más tarde"), false),
                ])
            },
        ])
        .gap(8.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::frame;

    fn texts(element: Element) -> Vec<String> {
        crate::testing::texts(&frame(element, Size::new(1280.0, 800.0)).1)
    }

    fn view(stage: Stage) -> UpdateView {
        UpdateView {
            version: "1.2.0".into(),
            installed: "1.1.0".into(),
            notes: "Arregla el inicio.".into(),
            stage,
        }
    }

    #[test]
    fn a_mandatory_update_shows_its_progress_and_on_failure_retry_and_quit() {
        let window = Size::new(1280.0, 800.0);
        let shown = texts(screen(
            &view(Stage::Downloading {
                have: 512 * 1024,
                whole: 2 * 1024 * 1024,
            }),
            window,
            &Look::DARK,
        ));
        assert!(shown
            .iter()
            .any(|text| text.contains("1.2.0") && text.contains("1.1.0")));
        assert!(shown.iter().any(|text| text == "Arregla el inicio."));
        assert!(shown.iter().any(|text| text.starts_with("Descargando…")));
        assert!(!shown.iter().any(|text| text == "Reintentar"));
        let failed = texts(screen(
            &view(Stage::Failed("Sin conexión.".into())),
            window,
            &Look::DARK,
        ));
        for wanted in ["Sin conexión.", "Reintentar", "Salir"] {
            assert!(
                failed.iter().any(|text| text == wanted),
                "{wanted}: {failed:?}"
            );
        }
    }

    #[test]
    fn an_optional_update_offers_to_install_later_or_its_notes() {
        let closed = texts(notice(&view(Stage::Offered), false, &Look::DARK));
        for wanted in ["Actualizar y reiniciar", "Más tarde", "Notas"] {
            assert!(
                closed.iter().any(|text| text == wanted),
                "{wanted}: {closed:?}"
            );
        }
        assert!(!closed.iter().any(|text| text == "Arregla el inicio."));
        let open = texts(notice(&view(Stage::Offered), true, &Look::DARK));
        assert!(open.iter().any(|text| text == "Arregla el inicio."));
        let failed = texts(notice(
            &view(Stage::Failed("No.".into())),
            false,
            &Look::DARK,
        ));
        assert!(failed.iter().any(|text| text == "Reintentar"));
        assert!(!failed.iter().any(|text| text == "Salir"));
    }
}
