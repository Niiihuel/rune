//! Ajustes › Actualizaciones (actualizaciones, 2026-10-19): the version
//! installed and where, «Buscar ahora» with the last check, the update
//! offered, and whether RUNE looks when it starts.

use limn::Element;

use rune_components::updates::{ids, progress, release_notes, Stage};
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
            let status = if updates.checking {
                t("Buscando…").to_owned()
            } else if let Some(error) = &updates.error {
                error.clone()
            } else if let Some(offered) = offered {
                match &offered.stage {
                    Stage::Installing => tr!(
                        "Instalando RUNE {version}…",
                        version = offered.version
                    ),
                    Stage::Restarting | Stage::Installed => tr!(
                        "RUNE {version} está instalada.",
                        version = offered.version
                    ),
                    Stage::Offered | Stage::Downloading { .. } | Stage::Failed(_) => tr!(
                        "RUNE {version} está disponible.",
                        version = offered.version
                    ),
                }
            } else if updates.checked.is_some() {
                t("Tenés la última versión.").to_owned()
            } else {
                t("Todavía no se buscó.").to_owned()
            };
            let note = match updates.checked {
                Some(at) => tr!(
                    "{status} Última verificación: {when}.",
                    status = status,
                    when = time::stamp(at, ctx.context.wall)
                ),
                None => status,
            };
            let mut controls = vec![parts::secondary_button(
                ids::CHECK,
                t("Buscar ahora"),
                !updates.checking && !updates.busy,
                look,
            )];
            match offered.map(|offered| &offered.stage) {
                Some(Stage::Failed(_)) => controls.push(parts::primary_button(
                    ids::RETRY,
                    t("Reintentar"),
                    true,
                    look,
                )),
                Some(Stage::Installed) => controls.push(parts::primary_button(
                    ids::RESTART,
                    t("Reiniciar ahora"),
                    true,
                    look,
                )),
                _ if updates.installable() => controls.push(parts::primary_button(
                    ids::INSTALL,
                    t("Actualizar y reiniciar"),
                    true,
                    look,
                )),
                _ => {}
            }
            let mut rows = vec![parts::setting(
                t("Buscar actualizaciones"),
                Some(&note),
                None,
                true,
                look,
            )];
            // The bar under the note, across the page, as the screens show it.
            rows.extend(offered.and_then(|offered| progress(&offered.stage, look)));
            // The buttons under it: beside, two of them leave a narrow
            // window's label no room.
            rows.push(Element::row(controls).gap(12.0));
            rows.push(Element::spacer().size(None, Some(12.0)));
            Element::column(rows)
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
