//! «Juegos registrados» (escritorio-sesion-y-juegos): the game running now,
//! adding an open program as a game, and the list, as Serein lays it out
//! (`ui/src/registered_games.rs`): each game renamed, hidden or brought
//! back, with when it was last played.

use limn::{Component, Element, TextField};

use rune_components::user_settings as parts;
use rune_i18n::{t, tr};

use super::{select, Ctx};
use crate::games::{self, RegisteredGame};

/// The setting `key` of the page.
pub(super) fn setting(ctx: &Ctx, key: &str) -> Option<Element> {
    Some(match key {
        "games-now" => now(ctx),
        "games-add" => add(ctx),
        "games-list" => list(ctx),
        _ => return None,
    })
}

/// The field a game's name is written in, with its buttons.
fn renaming(ctx: &Ctx, label: &str, draft: &str) -> Element {
    let look = ctx.look;
    Element::column(vec![
        parts::setting(label, None, None, true, look),
        Element::row(vec![
            TextField::new(games::NAME, draft.to_owned(), label, look.palette())
                .accessibility_label(t("Nombre del juego"))
                .render()
                .flex(1.0),
            parts::primary_button("us:game-name:save", t("Guardar"), true, look),
            parts::secondary_button("us:game-name:cancel", t("Cancelar"), true, look),
        ])
        .gap(8.0),
        Element::spacer().size(None, Some(12.0)),
    ])
}

/// The draft name of the game of `executable`, while it is renamed.
fn draft<'a>(ctx: &'a Ctx, executable: &str) -> Option<&'a str> {
    let (renamed, draft) = ctx.ui.desktop.games.renaming.as_ref()?;
    (renamed == executable).then_some(draft.as_str())
}

fn now(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let Some(running) = &ui.desktop.games.running else {
        let note = if ui.prefs.game_activity {
            t("Cuando abras un juego que RUNE reconozca, aparece aquí.")
        } else {
            t("Activa «Detectar juegos» para que RUNE reconozca los que abres.")
        };
        return parts::setting(t("Ningún juego detectado"), Some(note), None, true, look);
    };
    let name = ui
        .prefs
        .registered_games
        .iter()
        .find(|game| game.executable == running.executable)
        .map_or(running.name.as_str(), |game| game.name.as_str());
    if let Some(draft) = draft(ctx, &running.executable) {
        return renaming(ctx, &tr!("Ahora: {name}", name), draft);
    }
    parts::setting(
        &tr!("Ahora: {name}", name),
        Some(t(
            "Detectado en este equipo. ¿No es este juego? Ocúltalo y no se detecta más.",
        )),
        Some(
            Element::row(vec![
                parts::secondary_button(
                    &format!("us:game-rename={}", running.executable),
                    t("Renombrar"),
                    true,
                    look,
                ),
                parts::secondary_button("us:game-hide-running", t("No es este juego"), true, look),
            ])
            .gap(8.0),
        ),
        true,
        look,
    )
}

fn add(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let registered = |key: &str| {
        ui.prefs
            .registered_games
            .iter()
            .any(|game| game.executable == key && !game.hidden)
    };
    let mut options: Vec<(String, String, bool, bool)> = match &ui.desktop.games.programs {
        None => vec![(
            "us:select:programs".into(),
            t(games::READING).into(),
            false,
            false,
        )],
        Some(programs) => programs
            .iter()
            .enumerate()
            .filter(|(_, (key, _))| !registered(key))
            .map(|(at, (_, path))| (format!("us:game-add={at}"), path.clone(), false, true))
            .collect(),
    };
    if options.is_empty() {
        options.push((
            "us:select:programs".into(),
            t("No hay otros programas abiertos.").into(),
            false,
            false,
        ));
    }
    let borrowed: Vec<(&str, &str, bool, bool)> = options
        .iter()
        .map(|(id, label, chosen, enabled)| (id.as_str(), label.as_str(), *chosen, *enabled))
        .collect();
    Element::column(vec![
        parts::setting(
            t("Agregar un programa"),
            Some(t("¿No ves tu juego? Elige uno de tus programas abiertos: RUNE lo detecta como juego desde ahora, sin el proveedor.")),
            None,
            true,
            look,
        ),
        select(ctx, "programs", t("Agregar un programa"), &borrowed, ctx.geometry.column),
        Element::spacer().size(None, Some(12.0)),
    ])
}

fn list(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let mut games: Vec<&RegisteredGame> = ui.prefs.registered_games.iter().collect();
    if games.is_empty() {
        return parts::setting(
            t("Juegos agregados"),
            Some(t("Todavía ninguno. Los que RUNE detecte se suman solos, para renombrarlos u ocultarlos.")),
            None,
            true,
            look,
        );
    }
    // The ones played last first, as Discord lists them.
    games.sort_by_key(|game| std::cmp::Reverse(game.last_played));
    let rows = games.into_iter().map(|game| {
        if let Some(draft) = draft(ctx, &game.executable) {
            return renaming(ctx, &game.name, draft);
        }
        let id = |action: &str| format!("us:game-{action}={}", game.executable);
        let buttons = if game.hidden {
            vec![parts::secondary_button(
                &id("restore"),
                t("Restaurar"),
                true,
                look,
            )]
        } else {
            vec![
                parts::secondary_button(&id("rename"), t("Renombrar"), true, look),
                parts::secondary_button(
                    &id("remove"),
                    if game.detected() {
                        t("Dejar de detectar")
                    } else {
                        t("Quitar")
                    },
                    true,
                    look,
                ),
            ]
        };
        parts::setting(
            &game.name,
            Some(&games::detail(game, ctx.context.wall)),
            Some(Element::row(buttons).gap(8.0)),
            !game.hidden,
            look,
        )
    });
    Element::column(rows.collect())
}
