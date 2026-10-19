//! The question over the window when the service asks for a verification
//! before an action goes out: solve it in the provider's own window, or give
//! the action up. Nothing opens until the person says so.

use limn::{AlertDialog, Component, Element};
use rune_i18n::{t, tr};

use crate::Look;

/// `challenge:{id}:solve` and `challenge:{id}:cancel`.
pub mod ids {
    pub fn solve(challenge: impl std::fmt::Display) -> String {
        format!("challenge:{challenge}:solve")
    }

    pub fn cancel(challenge: impl std::fmt::Display) -> String {
        format!("challenge:{challenge}:cancel")
    }
}

pub const WIDTH: f32 = 440.0;

/// What the question says.
#[derive(Clone, Debug, PartialEq)]
pub struct ChallengeProps {
    pub challenge: u64,
    /// Who asks: the service's name, as the provider gives it.
    pub service: String,
    /// What waits, as "enviar el mensaje".
    pub action: String,
    /// A simulation asks: no window opens, and solving only pretends.
    pub simulated: bool,
}

/// `root` with the question over it.
pub fn dialog(root: Element, props: &ChallengeProps, look: &Look) -> Element {
    let body = if props.simulated {
        tr!(
            "La simulación pide una verificación para {action}. Es simulada: no se abre ninguna ventana y «Verificar» la da por resuelta. Si cancelás, no se envía nada.",
            action = props.action
        )
    } else {
        tr!(
            "{service} pide una verificación para {action}. Resolvela vos en la ventana de verificación de RUNE: nada se resuelve solo, y con tu respuesta la acción se envía una sola vez. Si cancelás o cerrás la ventana, no se envía nada.",
            service = props.service,
            action = props.action
        )
    };
    AlertDialog::new(
        root,
        true,
        if props.simulated {
            t("Verificación simulada")
        } else {
            t("Verificación requerida")
        },
        body,
        ids::solve(props.challenge),
        ids::cancel(props.challenge),
        look.menu_palette(),
    )
    .width(WIDTH)
    .confirm_label(t("Verificar"))
    .cancel_label(t("Cancelar"))
    .render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{controls, frame, texts};
    use limn::Size;

    fn props(simulated: bool) -> ChallengeProps {
        ChallengeProps {
            challenge: 7,
            service: "Servicio".into(),
            action: "enviar el mensaje".into(),
            simulated,
        }
    }

    #[test]
    fn the_question_says_what_waits_and_offers_to_solve_or_cancel() {
        let (_, laid) = frame(
            dialog(Element::spacer(), &props(false), &Look::DARK),
            Size::new(900.0, 700.0),
        );
        let ids = controls(&laid);
        assert!(ids.contains(&"challenge:7:solve".to_owned()));
        assert!(ids.contains(&"challenge:7:cancel".to_owned()));
        let painted = texts(&laid).join(" ");
        assert!(painted.contains("Verificación requerida"));
        assert!(painted.contains("enviar el mensaje"));
        assert!(!painted.contains("simulada"));
    }

    #[test]
    fn a_simulated_one_says_so() {
        let (_, laid) = frame(
            dialog(Element::spacer(), &props(true), &Look::DARK),
            Size::new(900.0, 700.0),
        );
        let painted = texts(&laid).join(" ");
        assert!(painted.contains("Verificación simulada"));
        assert!(painted.contains("no se abre ninguna ventana"));
    }
}
