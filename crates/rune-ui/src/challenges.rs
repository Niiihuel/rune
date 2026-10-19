//! A verification the provider asks for before an action goes out: the
//! question over the window and the person's answer. Nothing is solved
//! here: «Verificar» asks the provider to show its own window, «Cancelar»
//! gives the action up. The action ends as any other, with its answer or
//! its failure. Actions start with `challenge:`.

use limn::{Element, UiAction};
use rune_components::challenge::{self as parts, ChallengeProps};
use rune_components::Look;
use rune_i18n::t;
use rune_provider::{Command, Failure, ProviderKind, RequestId};
use rune_state::{Applied, Store};

use crate::{Intent, Ui};

/// The question shown, until the person answers it or the action ends.
#[derive(Debug)]
pub struct Waiting {
    challenge: RequestId,
    failure: Failure,
}

pub(crate) fn on_applied(ui: &mut Ui, applied: &Applied) {
    match applied {
        Applied::Challenge { challenge, failure } => {
            ui.challenge = Some(Waiting {
                challenge: *challenge,
                failure: failure.clone(),
            });
        }
        Applied::Failed { failure, .. }
            if ui
                .challenge
                .as_ref()
                .is_some_and(|waiting| waiting.failure == *failure) =>
        {
            ui.challenge = None;
        }
        Applied::Ready | Applied::Authentication => ui.challenge = None,
        _ => {}
    }
}

/// The answer to the question shown; `None` when the action is not one.
pub(crate) fn update(ui: &mut Ui, action: &UiAction) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    let (challenge, answer) = id.strip_prefix("challenge:")?.split_once(':')?;
    let challenge = challenge.parse().ok()?;
    let solve = match answer {
        "solve" => true,
        "cancel" => false,
        _ => return None,
    };
    // Only the question shown answers: a late click sends nothing.
    if ui
        .challenge
        .take_if(|waiting| waiting.challenge == challenge)
        .is_none()
    {
        return Some(Vec::new());
    }
    Some(vec![Intent::Command(Command::SolveChallenge {
        challenge,
        solve,
    })])
}

/// The question over `root`, while one waits.
pub(crate) fn overlay(ui: &Ui, store: &Store, root: Element, look: &Look) -> Element {
    let Some(waiting) = &ui.challenge else {
        return root;
    };
    let provider = store.provider.as_ref();
    parts::dialog(
        root,
        &ChallengeProps {
            challenge: waiting.challenge,
            service: provider.map_or_else(
                || t("El servicio").to_owned(),
                |info| t(&info.name).to_owned(),
            ),
            action: waiting.failure.action().to_owned(),
            simulated: provider.is_some_and(|info| info.kind == ProviderKind::Simulated),
        },
        look,
    )
}
