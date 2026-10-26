//! «Atajos de teclado» (atajos-y-navegacion): the table of
//! [`crate::keybinds`], each action with its keys, recorded anew with a
//! click and reset to the default; then the keys that do not change.

use limn::Element;

use rune_components::keybinds::{self as rows, Row};
use rune_components::user_settings::{self as parts, Shortcut};
use rune_i18n::{es, t, tr};

use super::{Ctx, Section};
use crate::desktop::Shortcut as Voice;
use crate::keybinds::{self, Action, Group, ACTIONS, FIXED};
use crate::settings::SettingsPage::Keybinds;
use crate::GlobalKeys;

pub(super) const SECTIONS: &[Section] = &[Section {
    page: Keybinds,
    key: "shortcuts",
    title: es("Atajos de teclado"),
    rows: &[
        (
            "keys-navigation",
            es("atajos de teclado navegación cambiar reasignar buscador rápido canales historial atrás adelante región lista métricas ctrl k alt flechas f6 f12"),
        ),
        (
            "keys-messages",
            es("atajos de teclado mensajes enviar nueva línea editar último enter shift"),
        ),
        (
            "keys-formatting",
            es("atajos de teclado formato negrita cursiva subrayado tachado código bloque spoiler ctrl"),
        ),
        (
            "keys-voice",
            es("atajos de teclado voz pulsar para hablar silenciar ensordecer micrófono global fuera de la ventana tecla combinación push to mute ptt"),
        ),
        (
            "keys-fixed",
            es("atajos de teclado fijos cerrar escape esc mención tab desplazar página menú contextual zoom acercar alejar"),
        ),
    ],
}];

/// The setting `key` of the section; `None` for the rest.
pub(super) fn setting(ctx: &Ctx, key: &str) -> Option<Element> {
    let group = match key {
        "keys-navigation" => Group::Navigation,
        "keys-messages" => Group::Messages,
        "keys-formatting" => Group::Formatting,
        "keys-voice" => Group::Voice,
        "keys-fixed" => return Some(fixed(ctx)),
        _ => return None,
    };
    let rows: Vec<Row> = ACTIONS
        .into_iter()
        .filter(|action| action.group() == group)
        .map(|action| row(ctx, action))
        .collect();
    let mut column = vec![parts::subtitle(group.title(), ctx.look)];
    if group == Group::Messages {
        column.push(note(
            t("Si Enviar deja de ser Enter, Enter escribe una línea nueva."),
            ctx,
        ));
    }
    column.push(rows::list(&rows, ctx.look));
    Some(Element::column(column).gap(4.0))
}

fn note(text: &str, ctx: &Ctx) -> Element {
    rune_components::text(text, 14.0, ctx.look.muted)
}

fn row(ctx: &Ctx, action: Action) -> Row {
    let ui = ctx.ui;
    let prefs = &ui.prefs;
    let state = &ui.settings_state;
    let recording = state.recording.is_some() && state.recording_for == action;
    let caps = keybinds::caps(prefs, action);
    let reset = match action {
        Action::Voice(_) => caps.is_some().then(|| t("Quitar").to_owned()),
        _ => (keybinds::binding(prefs, action) != action.default_chord())
            .then(|| t("Restablecer").to_owned()),
    };
    let note = if recording {
        Some(t("Pulsa la combinación. Esc cancela.").to_owned())
    } else {
        match action {
            Action::Voice(voice) => voice_note(ctx, voice, caps.is_some()),
            _ => None,
        }
    };
    Row {
        key: action.key().into(),
        label: action.label().into(),
        keys: caps,
        recording,
        reset,
        note,
        problem: state
            .key_notice
            .as_ref()
            .filter(|(refused, _)| *refused == action)
            .map(|(_, why)| why.clone()),
    }
}

/// Where a voice shortcut works: in the window, or also outside it as the
/// desktop's portal bound it.
fn voice_note(ctx: &Ctx, voice: Voice, bound: bool) -> Option<String> {
    Some(match (&ctx.ui.audio.global_keys, bound) {
        (_, false) => match voice {
            Voice::PushToMute => t("Mientras lo mantienes, el micrófono no pasa.").to_owned(),
            _ => return None,
        },
        (GlobalKeys::Unavailable(reason), true) => {
            tr!("Sólo en la ventana: {reason}", reason = t(reason))
        }
        (GlobalKeys::Waiting, true) => t("Pidiendo el atajo global al escritorio…").to_owned(),
        (_, true) => t("También fuera de RUNE, como el escritorio lo asigne.").to_owned(),
    })
}

/// The keys that are not in the table: the fields', the lists' and the
/// window's own.
fn fixed(ctx: &Ctx) -> Element {
    let rows: Vec<Shortcut> = FIXED
        .iter()
        .map(|(action, keys)| Shortcut {
            action: t(action).to_owned(),
            keys: keys
                .split(' ')
                .map(|chord| chord.split('+').map(|key| t(key).to_owned()).collect())
                .collect(),
        })
        .collect();
    Element::column(vec![
        parts::subtitle(t("Fijos"), ctx.look),
        note(t("Estos no se cambian."), ctx),
        parts::shortcuts(&rows, ctx.look),
    ])
    .gap(4.0)
}
