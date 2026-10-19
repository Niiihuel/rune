//! The editable list of keyboard shortcuts in the settings
//! (atajos-y-navegacion), as Serein's: one row per action, what it does, and
//! its keys in a box that records others when clicked; a reset beside it
//! when the keys are not the default ones, and why a key was refused.

use limn::{Button, ButtonVariant, Component, ControlSize, Element, FontFamily};
use rune_i18n::{t, tr};

use crate::user_settings::keycap;
use crate::{gap, text, Look};

/// One action of the list.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Row {
    /// The action's key, in its controls' IDs ([`ids`]).
    pub key: String,
    pub label: String,
    /// Its keys, one cap each (`["Ctrl", "K"]`); `None` without a shortcut.
    pub keys: Option<Vec<String>>,
    /// Its next key press is being recorded.
    pub recording: bool,
    /// The reset's label (`Restablecer`, or `Quitar` for a shortcut that has
    /// none by default), while the keys are not the default ones.
    pub reset: Option<String>,
    /// A quiet line under the label.
    pub note: Option<String>,
    /// Why the key last recorded was refused, in the danger color.
    pub problem: Option<String>,
}

/// The IDs a row reports.
pub mod ids {
    /// The box of keys: records new ones, or cancels while recording.
    pub fn record(key: &str) -> String {
        format!("us:key-record={key}")
    }

    pub fn reset(key: &str) -> String {
        format!("us:key-reset={key}")
    }
}

/// The rows in a card, one under the other.
pub fn list(rows: &[Row], look: &Look) -> Element {
    let mut lines = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            lines.push(
                Element::panel(Element::spacer())
                    .size(None, Some(1.0))
                    .background(look.border),
            );
        }
        lines.push(line(row, look));
    }
    Element::panel(Element::column(lines))
        .background(look.raised)
        .border(look.border, 1.0)
        .radius(8.0)
}

fn line(row: &Row, look: &Look) -> Element {
    let mut copy = vec![text(&row.label, 16.0, look.text)];
    if let Some(problem) = &row.problem {
        copy.push(text(problem, 14.0, look.danger));
    } else if let Some(note) = &row.note {
        copy.push(text(note, 14.0, look.muted));
    }
    let mut controls = vec![keys(row, look)];
    if let Some(reset) = &row.reset {
        controls.push(
            Button::new(ids::reset(&row.key), reset, look.palette())
                .variant(ButtonVariant::Ghost)
                .size(ControlSize::Compact)
                .accessibility_label(format!("{reset}: {}", row.label))
                .render()
                .centered_y(),
        );
    }
    Element::column(vec![
        gap(0.0, 9.0),
        Element::row(vec![
            gap(18.0, 0.0),
            Element::column(copy).gap(2.0).flex(1.0).centered_y(),
            gap(16.0, 0.0),
            Element::row(controls).gap(8.0).centered_y(),
            gap(14.0, 0.0),
        ])
        .min_size(0.0, 41.5),
        gap(0.0, 9.0),
    ])
}

/// The box of keys, a button: the caps, «Sin atajo», or a prompt while
/// recording, outlined in the accent.
fn keys(row: &Row, look: &Look) -> Element {
    let inside = match (&row.keys, row.recording) {
        (_, true) => text(t("Pulsá las teclas…"), 14.0, look.text)
            .font_family(FontFamily::SansMedium)
            .centered_y(),
        (Some(keys), false) => Element::row(
            keys.iter()
                .map(|key| keycap(key, look).centered_y())
                .collect(),
        )
        .gap(4.0)
        .centered_y(),
        (None, false) => text(t("Sin atajo"), 14.0, look.muted).centered_y(),
    };
    let name = if row.recording {
        tr!(
            "Cancelar la grabación de {action}",
            action = row.label.as_str()
        )
    } else {
        tr!(
            "Grabar otro atajo para {action}",
            action = row.label.as_str()
        )
    };
    let ring = if row.recording {
        look.accent
    } else if row.problem.is_some() {
        look.danger
    } else {
        look.border
    };
    Element::clickable(
        ids::record(&row.key),
        Element::row(vec![
            Element::spacer().flex(1.0),
            inside,
            Element::spacer().flex(1.0),
        ]),
    )
    .size(Some(168.0), Some(34.0))
    .radius(6.0)
    .background(look.canvas)
    .border(ring, if row.recording { 1.5 } else { 1.0 })
    .hover_background(look.hover)
    .accessible(limn::Access::new(limn::Role::Button).name(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};

    fn row(key: &str) -> Row {
        Row {
            key: key.into(),
            label: "Negrita".into(),
            keys: Some(vec!["Ctrl".into(), "B".into()]),
            ..Row::default()
        }
    }

    #[test]
    fn a_row_shows_its_keys_its_reset_and_why_a_key_was_refused() {
        let rows = [
            row("bold"),
            Row {
                reset: Some("Restablecer".into()),
                problem: Some("Ya lo usa «Cursiva».".into()),
                ..row("italic")
            },
            Row {
                recording: true,
                keys: None,
                ..row("mute")
            },
        ];
        let (_, frame) = frame(list(&rows, &Look::DARK), limn::Size::new(760.0, 400.0));
        let ids = controls(&frame);
        for id in [
            "us:key-record=bold",
            "us:key-record=italic",
            "us:key-reset=italic",
            "us:key-record=mute",
        ] {
            assert!(ids.contains(&id.to_owned()), "{id} in {ids:?}");
        }
        assert!(!ids.contains(&"us:key-reset=bold".to_owned()));
        let shown = texts(&frame);
        for text in ["CTRL", "B", "Ya lo usa «Cursiva».", "Pulsá las teclas…"] {
            assert!(
                shown.iter().any(|shown| shown == text),
                "{text} in {shown:?}"
            );
        }
    }
}
