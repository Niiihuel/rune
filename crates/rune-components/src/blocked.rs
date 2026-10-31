//! Messages of people the account blocked or ignores, folded where they would
//! show, as the official client folds them: how many, and a way to see them
//! anyway.

use limn::{Button, ButtonVariant, Component, ControlSize, Element, Icon};
use rune_i18n::{t, tr};

use crate::{gap, icon, line, Look};

/// The ID a run's button reports: `blocked:{first message}`.
pub fn id(first: impl std::fmt::Display) -> String {
    format!("blocked:{first}")
}

/// The ID of a run of ignored messages' button: `ignored:{first message}`.
pub fn ignored_id(first: impl std::fmt::Display) -> String {
    format!("ignored:{first}")
}

/// The line over a run of `count` blocked messages, `shown` or folded.
pub fn run(first: impl std::fmt::Display, count: usize, shown: bool, look: &Look) -> Element {
    let said = if count == 1 {
        t("1 mensaje bloqueado").to_owned()
    } else {
        tr!("{count} mensajes bloqueados", count)
    };
    folded(id(first), said, shown, look)
}

/// The line over a run of `count` ignored messages, `shown` or folded.
pub fn ignored_run(
    first: impl std::fmt::Display,
    count: usize,
    shown: bool,
    look: &Look,
) -> Element {
    let said = if count == 1 {
        t("1 mensaje ignorado").to_owned()
    } else {
        tr!("{count} mensajes ignorados", count)
    };
    folded(ignored_id(first), said, shown, look)
}

fn folded(button: String, said: String, shown: bool, look: &Look) -> Element {
    Element::row(vec![
        gap(20.0, 0.0),
        icon(Icon::Lock, look.faint)
            .size(Some(14.0), Some(14.0))
            .centered_y(),
        gap(10.0, 0.0),
        line(said, 14.0, look.muted).centered_y(),
        gap(4.0, 0.0),
        Button::new(
            button,
            if shown { t("Ocultar") } else { t("Mostrar") },
            look.menu_palette(),
        )
        .variant(ButtonVariant::Link)
        .size(ControlSize::Compact)
        .render()
        .centered_y(),
        Element::spacer().flex(1.0),
    ])
    .size(None, Some(32.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    #[test]
    fn a_run_says_how_many_and_offers_to_show_them() {
        let (_, laid) = frame(run(42, 3, false, &Look::DARK), Size::new(600.0, 40.0));
        assert!(controls(&laid).contains(&"blocked:42".to_owned()));
        let painted = texts(&laid);
        assert!(painted.contains(&"3 mensajes bloqueados".to_owned()));
        assert!(painted.contains(&"Mostrar".to_owned()));
        let (_, laid) = frame(run(42, 1, true, &Look::DARK), Size::new(600.0, 40.0));
        let painted = texts(&laid);
        assert!(
            painted.contains(&"1 mensaje bloqueado".to_owned())
                && painted.contains(&"Ocultar".to_owned())
        );
    }

    #[test]
    fn an_ignored_run_has_its_own_line_and_button() {
        let (_, laid) = frame(
            ignored_run(42, 2, false, &Look::DARK),
            Size::new(600.0, 40.0),
        );
        assert!(controls(&laid).contains(&"ignored:42".to_owned()));
        let painted = texts(&laid);
        assert!(painted.contains(&"2 mensajes ignorados".to_owned()));
        assert!(painted.contains(&"Mostrar".to_owned()));
        let (_, laid) = frame(
            ignored_run(42, 1, false, &Look::DARK),
            Size::new(600.0, 40.0),
        );
        assert!(texts(&laid).contains(&"1 mensaje ignorado".to_owned()));
    }
}
