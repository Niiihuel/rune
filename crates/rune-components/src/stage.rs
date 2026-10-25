//! The account's place on the stage of its call, over its voice panel: where
//! it sits and what it can do there (raise or lower its hand, accept an
//! invitation, step down). Actions start with `stage:`.

use limn::{Button, ButtonVariant, Component, ControlSize, Element, Icon};

use crate::{icon, strong_line, text, Look};

/// The actions of a stage.
pub mod ids {
    /// Raise (`up`) or lower (`down`) the account's hand on `channel`.
    pub fn hand(channel: impl std::fmt::Display, up: bool) -> String {
        format!("stage:hand:{channel}:{}", if up { "up" } else { "down" })
    }

    /// Speak on `channel`: accept an invitation, or step up as moderation.
    pub fn speak(channel: impl std::fmt::Display) -> String {
        format!("stage:speak:{channel}")
    }

    /// Go back to the audience of `channel`, or decline an invitation.
    pub fn step_down(channel: impl std::fmt::Display) -> String {
        format!("stage:down:{channel}")
    }

    /// Invite `user` to speak on `channel`.
    pub fn invite(channel: impl std::fmt::Display, user: impl std::fmt::Display) -> String {
        format!("stage:invite:{channel}:{user}")
    }

    /// Send `user` to the audience of `channel`.
    pub fn audience(channel: impl std::fmt::Display, user: impl std::fmt::Display) -> String {
        format!("stage:audience:{channel}:{user}")
    }
}

/// One button of the strip: its action, label, and whether it leads.
#[derive(Clone, Debug, PartialEq)]
pub struct StageAction {
    pub id: String,
    pub label: String,
    pub primary: bool,
}

/// What the strip shows.
#[derive(Clone, Debug, PartialEq)]
pub struct StageStrip {
    /// Where the account sits: «Estás en el público».
    pub seat: String,
    /// A line under it: why it is not heard, or that it is simulated.
    pub detail: Option<String>,
    pub actions: Vec<StageAction>,
}

/// The strip over the voice panel: a section of its card, so it draws no box.
pub fn strip(view: &StageStrip, look: &Look) -> Element {
    let mut lines = vec![strong_line(&view.seat, 13.0, look.text)];
    if let Some(detail) = &view.detail {
        lines.push(text(detail.clone(), 12.0, look.muted));
    }
    let buttons = view
        .actions
        .iter()
        .map(|action| {
            Button::new(
                action.id.clone(),
                action.label.clone(),
                if action.primary {
                    look.accent_palette()
                } else {
                    look.menu_palette()
                },
            )
            .variant(if action.primary {
                ButtonVariant::Default
            } else {
                ButtonVariant::Secondary
            })
            .size(ControlSize::Compact)
            .render()
        })
        .collect();
    Element::panel(
        Element::column(vec![
            Element::row(vec![
                icon(Icon::Users, look.faint).centered_y(),
                Element::column(lines).gap(2.0).flex(1.0),
            ])
            .gap(8.0),
            Element::row(buttons).gap(6.0).wrap(),
        ])
        .gap(8.0),
    )
    .padding(12.0)
    .accessible(limn::Access::new(limn::Role::Group).name(view.seat.clone()))
}

/// A person's name on a stage, with a raised hand when they asked to speak.
pub fn name(name: &str, hand: bool) -> String {
    if hand {
        format!("✋ {name}")
    } else {
        name.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    #[test]
    fn the_strip_says_where_the_account_sits_and_offers_its_actions() {
        let view = StageStrip {
            seat: "Te invitaron a hablar".into(),
            detail: Some("Aceptá para que te escuchen".into()),
            actions: vec![
                StageAction {
                    id: ids::speak(30),
                    label: "Aceptar".into(),
                    primary: true,
                },
                StageAction {
                    id: ids::step_down(30),
                    label: "Rechazar".into(),
                    primary: false,
                },
            ],
        };
        let (_, laid) = frame(strip(&view, &Look::DARK), Size::new(300.0, 200.0));
        let found = controls(&laid);
        assert!(
            found.contains(&"stage:speak:30".to_owned())
                && found.contains(&"stage:down:30".to_owned())
        );
        assert!(texts(&laid).contains(&"Te invitaron a hablar".to_owned()));
        assert_eq!(name("Ada", true), "✋ Ada");
        assert_eq!(ids::hand(30, false), "stage:hand:30:down");
    }
}
