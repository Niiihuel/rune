//! Ajustes › Actualizaciones (actualizaciones, 2026-10-19): the version
//! installed and where, «Buscar ahora» with the last check, the update
//! offered, and whether RUNE looks when it starts.

use limn::Element;

use rune_components::updates::{check_card, ids, release_notes, CheckCard, Stage, Tone};
use rune_components::{line, time, user_settings as parts};
use rune_i18n::{es, t, tr};

use super::{Ctx, Section};
use crate::settings::SettingsPage::Updates;

pub(super) const SECTIONS: &[Section] = &[Section {
    page: Updates,
    key: "updates",
    title: es("Actualizaciones"),
    rows: &[
        (
            "update-version",
            es("versión instalada actual número dónde instalación"),
        ),
        (
            "update-check",
            es("buscar actualizaciones ahora nueva versión descargar instalar reiniciar última verificación"),
        ),
        (
            "update-at-start",
            es("buscar al iniciar abrir comprobar automáticamente obligatoria"),
        ),
        (
            "update-notes",
            es("novedades notas cambios versión github imágenes videos"),
        ),
    ],
}];

/// The setting `key` of the section; `None` for the rest.
pub(super) fn setting(ctx: &Ctx, key: &str) -> Option<Element> {
    let (updates, look) = (&ctx.ui.updates, ctx.look);
    Some(match key {
        "update-version" => {
            let place = match &updates.installation {
                Some(folder) => tr!(
                    "Instalada en {folder}. Una actualización la reemplaza entera, o no la toca.",
                    folder = folder
                ),
                None => t("Esta copia no se instaló con el instalador de RUNE: avisa si hay una versión nueva, pero se actualiza desde donde la compilaste.").into(),
            };
            parts::setting(
                t("Versión instalada"),
                Some(&place),
                Some(line(
                    format!("RUNE {}", env!("CARGO_PKG_VERSION")),
                    16.0,
                    look.muted,
                )),
                true,
                look,
            )
        }
        "update-check" => {
            let offered = updates.offered.as_ref();
            let installed = env!("CARGO_PKG_VERSION");
            // Title, pill: what the update stands at (the body says the rest).
            let (title, pill) = if updates.checking {
                (t("Buscando actualizaciones…").to_owned(), None)
            } else if let Some(offered) = offered {
                let version = offered.version.as_str();
                match &offered.stage {
                    Stage::Offered => (
                        tr!("RUNE {version} está disponible", version = version),
                        Some((t("Disponible").to_owned(), Tone::Accent)),
                    ),
                    Stage::Downloading { .. } => (
                        tr!("Descargando RUNE {version}", version = version),
                        Some((t("Descargando").to_owned(), Tone::Accent)),
                    ),
                    Stage::Installing => (
                        tr!("Instalando RUNE {version}…", version = version),
                        Some((t("Instalando").to_owned(), Tone::Accent)),
                    ),
                    Stage::Restarting => (
                        tr!("RUNE {version} está instalada", version = version),
                        None,
                    ),
                    Stage::Installed => (
                        tr!("RUNE {version} está instalada", version = version),
                        Some((t("Instalada").to_owned(), Tone::Accent)),
                    ),
                    Stage::Failed(_) => (
                        t("No se pudo actualizar").to_owned(),
                        Some((t("Error").to_owned(), Tone::Danger)),
                    ),
                }
            } else if updates.error.is_some() {
                (
                    t("No se pudo buscar actualizaciones").to_owned(),
                    Some((t("Error").to_owned(), Tone::Danger)),
                )
            } else if updates.checked.is_some() {
                (
                    t("Tenés la última versión").to_owned(),
                    Some((t("Al día").to_owned(), Tone::Online)),
                )
            } else {
                (t("Todavía no se buscó").to_owned(), None)
            };
            let subtitle = match updates.checked {
                Some(at) => tr!(
                    "Instalada: RUNE {installed}. Última verificación: {when}.",
                    installed = installed,
                    when = time::stamp(at, ctx.context.wall)
                ),
                None => tr!("Instalada: RUNE {installed}.", installed = installed),
            };
            let mut actions = vec![parts::secondary_button(
                ids::CHECK,
                t("Buscar ahora"),
                !updates.checking && !updates.busy,
                look,
            )];
            match offered.map(|offered| &offered.stage) {
                Some(Stage::Failed(_)) => actions.push(parts::primary_button(
                    ids::RETRY,
                    t("Reintentar"),
                    true,
                    look,
                )),
                Some(Stage::Installed) => actions.push(parts::primary_button(
                    ids::RESTART,
                    t("Reiniciar ahora"),
                    true,
                    look,
                )),
                _ if updates.installable() => actions.push(parts::primary_button(
                    ids::INSTALL,
                    t("Actualizar y reiniciar"),
                    true,
                    look,
                )),
                _ => {}
            }
            Element::column(vec![
                check_card(
                    CheckCard {
                        title,
                        subtitle,
                        pill,
                        stage: offered.map(|offered| offered.stage.clone()),
                        error: updates.error.clone(),
                        busy: updates.busy,
                        actions,
                    },
                    look,
                ),
                Element::spacer().size(None, Some(12.0)),
            ])
        }
        "update-at-start" => parts::toggle(
            ids::AT_START,
            t("Buscar al iniciar"),
            Some(t("Al abrir, RUNE pregunta si hay una versión nueva, con unos segundos de límite. Una obligatoria se instala antes de abrir; las demás se ofrecen sin interrumpir.")),
            updates.at_start,
            None,
            look,
        ),
        // What the release brought, as its page on GitHub says it; the
        // page itself has its pictures and videos.
        "update-notes" => {
            let (version, notes) = updates.shown_notes()?;
            if notes.trim().is_empty() {
                return None;
            }
            let width = ctx.geometry.column;
            Element::column(vec![
                parts::setting(
                    &tr!("Novedades de RUNE {version}", version = version),
                    None,
                    None,
                    true,
                    look,
                ),
                release_notes(version, notes, width, look),
                Element::row(vec![parts::secondary_button(
                    ids::ON_GITHUB,
                    t("Ver en GitHub"),
                    true,
                    look,
                )]),
                Element::spacer().size(None, Some(12.0)),
            ])
            .gap(10.0)
        }
        _ => return None,
    })
}
