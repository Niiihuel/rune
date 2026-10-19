//! «Cosméticos de los perfiles»: a switch for each cosmetic people wear —
//! avatar decorations, nameplates and profile effects —, as Serein offers
//! them. A hidden one is neither drawn nor downloaded; it changes only what
//! this device shows. Profile frames are not drawn by RUNE yet, so they
//! have no switch.

use limn::Element;

use rune_components::user_settings as parts;
use rune_i18n::{es, t};

use super::Ctx;
use crate::settings::{Cosmetics, Prefs};

/// The section's settings, each with the words it is found by.
pub(super) const ROWS: &[(&str, &str)] = &[
    (
        "cosmetic:decorations",
        es("decoraciones de avatar marco cosméticos ocultar descargar"),
    ),
    (
        "cosmetic:nameplates",
        es("placas de nombre nameplates lista de miembros cosméticos ocultar"),
    ),
    (
        "cosmetic:effects",
        es("efectos de perfil animaciones tarjeta cosméticos ocultar"),
    ),
];

/// Each cosmetic: its key, what the switch says and what it explains.
const SWITCHES: [(&str, &str, &str); 3] = [
    (
        "decorations",
        es("Decoraciones de avatar"),
        es("Los marcos que se dibujan alrededor de las caras."),
    ),
    (
        "nameplates",
        es("Placas de nombre"),
        es("El arte detrás de los nombres en la lista de miembros."),
    ),
    (
        "effects",
        es("Efectos de perfil"),
        es("Las animaciones sobre las tarjetas de perfil."),
    ),
];

/// The switch of `cosmetic` in `cosmetics`.
fn switch<'a>(cosmetics: &'a mut Cosmetics, cosmetic: &str) -> Option<&'a mut bool> {
    match cosmetic {
        "decorations" => Some(&mut cosmetics.decorations),
        "nameplates" => Some(&mut cosmetics.nameplates),
        "effects" => Some(&mut cosmetics.effects),
        _ => None,
    }
}

/// The setting `key` of the section; `None` for the rest.
pub(super) fn setting(ctx: &Ctx, key: &str) -> Option<Element> {
    let cosmetic = key.strip_prefix("cosmetic:")?;
    let (_, label, note) = SWITCHES.iter().find(|(key, ..)| *key == cosmetic)?;
    let mut cosmetics = ctx.ui.prefs.cosmetics;
    let on = *switch(&mut cosmetics, cosmetic)?;
    let switch = parts::toggle(
        &format!("us:cosmetic:{cosmetic}"),
        t(label),
        Some(t(note)),
        on,
        None,
        ctx.look,
    );
    // The section says once what hiding does, under its first switch.
    Some(if cosmetic == "decorations" {
        Element::column(vec![
            parts::subtitle(
                t("Lo oculto no se descarga. Sólo cambia lo que ves en este equipo."),
                ctx.look,
            ),
            switch,
        ])
    } else {
        switch
    })
}

/// A click on a switch of the section (`us:` taken off); `None` for the rest.
pub(crate) fn clicked(ui: &mut crate::Ui, rest: &str) -> Option<Vec<crate::Intent>> {
    let cosmetic = rest.strip_prefix("cosmetic:")?;
    let on = switch(&mut ui.prefs.cosmetics, cosmetic)?;
    *on = !*on;
    Some(vec![crate::Intent::SaveSetting {
        key: format!("cosmetic_{cosmetic}"),
        value: Some(on.to_string()),
    }])
}

/// The switches as settings to keep.
pub(crate) fn kept(prefs: &Prefs) -> Vec<(&'static str, Option<String>)> {
    let Cosmetics {
        decorations,
        nameplates,
        effects,
    } = prefs.cosmetics;
    vec![
        ("cosmetic_decorations", Some(decorations.to_string())),
        ("cosmetic_nameplates", Some(nameplates.to_string())),
        ("cosmetic_effects", Some(effects.to_string())),
    ]
}

/// Take back what [`kept`] kept; what is missing or wrong stays shown.
pub(crate) fn restore(prefs: &mut Prefs, setting: &impl Fn(&str) -> Option<String>) {
    let shown = |key: &str| setting(key).as_deref() != Some("false");
    prefs.cosmetics = Cosmetics {
        decorations: shown("cosmetic_decorations"),
        nameplates: shown("cosmetic_nameplates"),
        effects: shown("cosmetic_effects"),
    };
}
