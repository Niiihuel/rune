//! «Sonidos de la llamada» (audio work): how loud, one switch and a «Probar»
//! for each event, and the folder of one's own sounds with what the host
//! found in it.

use limn::{Button, ButtonVariant, Component, ControlSize, Element, Slider, Switch, TextField};

use rune_components::{line, text, user_settings as parts};
use rune_i18n::{es, t, tr};

use super::Ctx;
use crate::call_sounds::FOLDER;
use crate::CallSound;

/// The section's settings, each with the words it is found by.
pub(super) const ROWS: &[(&str, &str)] = &[
    (
        "sound-volume",
        es("volumen de los sonidos avisos llamada ensordecido probar"),
    ),
    (
        "sound:join",
        es("sonido entrar a la llamada unirse conectar"),
    ),
    ("sound:leave", es("sonido salir de la llamada colgar")),
    (
        "sound:user_join",
        es("sonido alguien entra se une otra persona"),
    ),
    (
        "sound:user_leave",
        es("sonido alguien sale se va otra persona"),
    ),
    ("sound:mute", es("sonido silenciar micrófono mute")),
    (
        "sound:unmute",
        es("sonido dejar de silenciar micrófono unmute"),
    ),
    (
        "sound:deafen",
        es("sonido ensordecer auriculares sonido apagado"),
    ),
    (
        "sound:undeafen",
        es("sonido dejar de ensordecer auriculares"),
    ),
    (
        "sound:stream_start",
        es("sonido empezar a transmitir compartir pantalla en vivo"),
    ),
    (
        "sound:stream_stop",
        es("sonido dejar de transmitir compartir pantalla terminar"),
    ),
    (
        "sound:user_stream",
        es("sonido alguien transmite comparte pantalla en vivo"),
    ),
    (
        "sound:disconnect",
        es("sonido desconexión error se cortó la llamada"),
    ),
    (
        "sound:message",
        es("sonido mensaje nuevo mención mensaje directo"),
    ),
    // Private calls.
    (
        "sound:ring",
        es("sonido llamada entrante timbre sonar privado"),
    ),
    (
        "sound:ring_out",
        es("sonido llamando a alguien timbre esperar privado"),
    ),
    (
        "sound-folder",
        es("carpeta de sonidos propios archivos personalizados reemplazar ogg opus mp3 wav"),
    ),
];

/// The setting `key` of the section; `None` for the rest.
pub(super) fn setting(ctx: &Ctx, key: &str) -> Option<Element> {
    if let Some(sound) = key.strip_prefix("sound:") {
        return CallSound::from_key(sound).map(|sound| event(ctx, sound));
    }
    match key {
        "sound-volume" => Some(volume(ctx)),
        "sound-folder" => Some(folder(ctx)),
        _ => None,
    }
}

/// One event: its name, «Probar» and its switch.
fn event(ctx: &Ctx, sound: CallSound) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let on = ui.prefs.sound_on(sound);
    let own = ui
        .call_sounds
        .found
        .as_ref()
        .and_then(|found| found.taken.iter().find(|(taken, _)| *taken == sound))
        .map(|(_, file)| tr!("Suena {file}, de tu carpeta.", file = file.as_str()));
    let controls = Element::row(vec![
        Button::new(
            format!("us:sound-test={}", sound.key()),
            t("Probar"),
            look.palette(),
        )
        .variant(ButtonVariant::Secondary)
        .size(ControlSize::Compact)
        .accessibility_label(tr!("Probar «{sound}»", sound = sound.label()))
        .render(),
        Switch::new(format!("us:sound={}", sound.key()), on, look.palette())
            .accessibility_label(sound.label())
            .render(),
    ])
    .gap(12.0);
    parts::setting(sound.label(), own.as_deref(), Some(controls), true, look)
}

/// How loud they play, and what deafening leaves.
fn volume(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let volume = ui.prefs.sound_volume;
    Element::column(vec![
        parts::setting(
            t("Volumen de los sonidos"),
            Some(t("Bajo el volumen de salida. Ensordecido solo suenan ensordecer y dejar de ensordecer, como en Discord, y una llamada entrante.")),
            None,
            true,
            look,
        ),
        Slider::new("us:sound-volume", volume, look.palette())
            .width(ctx.geometry.column)
            .ink(look.accent)
            .accessibility_label(t("Volumen de los sonidos"))
            .render(),
        line(
            tr!("{percent} %", percent = format!("{:.0}", volume * 100.0)),
            13.0,
            look.muted,
        ),
        Element::spacer().size(None, Some(8.0)),
    ])
    .gap(8.0)
}

/// The folder of one's own sounds, its names, and what the host found.
fn folder(ctx: &Ctx) -> Element {
    let (ui, look) = (ctx.ui, ctx.look);
    let sounds = &ui.call_sounds;
    let current = ui.prefs.sound_folder.as_deref().unwrap_or_default();
    let typed = sounds.typed.clone().unwrap_or_else(|| current.to_owned());
    let names: Vec<&str> = CallSound::ALL.iter().map(|sound| sound.key()).collect();
    let mut column = vec![
        parts::subtitle(t("Carpeta de sonidos"), look),
        text(
            t("Si tiene un archivo con el nombre de un evento (.ogg, .opus, .mp3 o .wav, hasta 1 MB y 5 s), suena ese en lugar del de RUNE. Los demás siguen siendo los de RUNE."),
            13.0,
            look.muted,
        ),
        text(
            tr!("Nombres: {names}", names = names.join(", ")),
            13.0,
            look.muted,
        ),
        Element::row(vec![
            TextField::new(FOLDER, typed, t("/home/vos/sonidos o ~/sonidos"), look.palette())
                .accessibility_label(t("Carpeta de sonidos"))
                .render()
                .flex(1.0),
            // Linux desktop (escritorio-linux): or the system's dialog.
            parts::secondary_button(
                "us:sound-folder:choose",
                if sounds.choosing.is_some() {
                    t("Eligiendo…")
                } else {
                    t("Elegir…")
                },
                sounds.choosing.is_none(),
                look,
            ),
            parts::primary_button("us:sound-folder:use", t("Usar"), true, look),
            parts::secondary_button(
                "us:sound-folder:clear",
                t("Quitar"),
                ui.prefs.sound_folder.is_some(),
                look,
            ),
        ])
        .gap(8.0),
    ];
    let mut status = match (&sounds.found, ui.prefs.sound_folder.as_ref()) {
        (Some(found), _) if found.error.is_some() => vec![found.error.clone().unwrap_or_default()],
        (Some(found), Some(_)) => {
            let taken: Vec<&str> = found.taken.iter().map(|(_, file)| file.as_str()).collect();
            let mut lines = vec![if taken.is_empty() {
                t("No hay sonidos con esos nombres: suenan los de RUNE.").to_owned()
            } else {
                tr!(
                    "Usa {count} de {all}: {files}.",
                    count = taken.len(),
                    all = CallSound::ALL.len(),
                    files = taken.join(", ")
                )
            }];
            lines.extend(found.refused.iter().map(|(file, reason)| {
                tr!(
                    "{file}: {reason}. Suena el de RUNE.",
                    file = file.as_str(),
                    reason = reason.as_str()
                )
            }));
            lines
        }
        (None, Some(_)) => vec![t("Leyendo la carpeta…").to_owned()],
        _ => Vec::new(),
    };
    // Linux desktop (escritorio-linux): the dialog's, under the folder's.
    status.extend(sounds.dialog_problem.clone());
    column.extend(
        status
            .into_iter()
            .map(|status| text(status, 13.0, look.text)),
    );
    column.push(Element::spacer().size(None, Some(12.0)));
    Element::column(column).gap(8.0)
}
