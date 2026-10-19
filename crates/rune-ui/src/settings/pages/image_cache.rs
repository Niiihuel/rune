//! «Caché de imágenes» (imagenes-y-lectura), as Serein's: what the
//! account's pictures on disk take of each part's budget, «Borrar» for
//! each, and how long a picture not shown again stays.

use limn::Element;

use rune_components::{file_size, user_settings as parts};
use rune_i18n::{es, t, tr};

use super::{select, Ctx, Section};
use crate::reading::{size_label, CachePart, ARTWORK_BUDGETS, KEPT_DAYS, MEDIA_BUDGETS};
use crate::settings::SettingsPage::Privacy;

pub(super) const SECTIONS: &[Section] = &[Section {
    page: Privacy,
    key: "image-cache",
    title: es("Caché de imágenes"),
    rows: &[
        (
            "cache-media",
            es("caché de medios vistas previas banners imágenes disco espacio borrar"),
        ),
        (
            "cache-artwork",
            es("caché de arte compartido emoji avatares íconos insignias stickers disco borrar"),
        ),
        (
            "cache-days",
            es("borrar las imágenes sin usar después de días retención caché"),
        ),
    ],
}];

/// The setting `key` of the section; `None` for the rest.
pub(super) fn setting(ctx: &Ctx, key: &str) -> Option<Element> {
    Some(match key {
        "cache-media" => part(ctx, CachePart::Media),
        "cache-artwork" => part(ctx, CachePart::Artwork),
        "cache-days" => days(ctx),
        _ => return None,
    })
}

/// One part: what it keeps, its budget to choose and «Borrar».
fn part(ctx: &Ctx, part: CachePart) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let (name, label, note, presets, chosen) = match part {
        CachePart::Media => (
            "cache-media",
            t("Caché de medios"),
            t("Vistas previas de adjuntos y enlaces, banners e imágenes a tamaño original."),
            &MEDIA_BUDGETS[..],
            ui.prefs.media_cache_mib,
        ),
        CachePart::Artwork => (
            "cache-artwork",
            t("Caché de arte compartido"),
            t("Avatares, emoji, stickers, íconos e insignias, aparte para que los medios grandes no los saquen."),
            &ARTWORK_BUDGETS[..],
            ui.prefs.artwork_cache_mib,
        ),
    };
    let usage = match ui.image_cache {
        Some(usage) => tr!(
            "Usa {used} de {limit}.",
            used = file_size(usage.bytes(part)),
            limit = size_label(chosen)
        ),
        None => {
            t("Sin llavero del sistema o sin cuenta, las imágenes quedan sólo en memoria.").into()
        }
    };
    let options: Vec<(String, String)> = presets
        .iter()
        .map(|mib| (format!("us:{name}={mib}"), size_label(*mib)))
        .collect();
    let options: Vec<(&str, &str, bool, bool)> = options
        .iter()
        .zip(presets)
        .map(|((id, text), mib)| (id.as_str(), text.as_str(), *mib == chosen, true))
        .collect();
    let width = 120.0_f32.min(ctx.geometry.column);
    let controls = Element::row(vec![
        select(ctx, name, label, &options, width),
        parts::secondary_button(
            &format!("us:cache-clear={}", part_key(part)),
            t("Borrar"),
            ui.image_cache.is_some(),
            look,
        ),
    ])
    .gap(12.0);
    parts::setting(
        label,
        Some(&format!("{note} {usage}")),
        Some(controls),
        true,
        look,
    )
}

fn part_key(part: CachePart) -> &'static str {
    match part {
        CachePart::Media => "media",
        CachePart::Artwork => "artwork",
    }
}

/// How long a picture not shown again stays.
fn days(ctx: &Ctx) -> Element {
    let chosen = ctx.ui.prefs.cache_days;
    let options: Vec<(String, String)> = KEPT_DAYS
        .iter()
        .map(|days| {
            (
                format!("us:cache-days={days}"),
                tr!("{count} días", count = days),
            )
        })
        .collect();
    let options: Vec<(&str, &str, bool, bool)> = options
        .iter()
        .zip(KEPT_DAYS)
        .map(|((id, text), days)| (id.as_str(), text.as_str(), days == chosen, true))
        .collect();
    let label = t("Borrar las imágenes sin usar después de");
    parts::setting(
        label,
        Some(t("Las imágenes que no se vuelven a mostrar en ese tiempo se borran del disco. Borrar estas imágenes no toca los mensajes, los borradores ni tu sesión.")),
        Some(select(ctx, "cache-days", label, &options, 120.0)),
        true,
        ctx.look,
    )
}
