//! What a server asks of someone who joins it (2026-10-18): its questions
//! and its rules, in one dialog. It opens by itself after an invitation
//! joins a server that asks either, and from the bar over the composer
//! while the account owes something. The questions go first and the rules
//! after them, as Serein and the official client send them; each answer
//! goes once and counts once the provider says so. Every action starts
//! with `onb:`.

use limn::{Element, Key, Modal, UiAction, UiEvent};
use rune_components::onboarding::{self as parts, JoinView};
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::{ChoiceId, JoinOutcome, RulesAnswer, ServerId, UserId};
use rune_provider::{Command, Failure, ProviderKind, RequestId};
use rune_state::{Applied, Store};

use crate::{Intent, Ui, ViewContext};

/// The dialog while it is open.
#[derive(Debug)]
pub(crate) struct JoinDialog {
    pub server: ServerId,
    account: Option<UserId>,
    /// Reads of the questions and the rules in flight.
    loading_onboarding: Option<RequestId>,
    loading_rules: Option<RequestId>,
    /// Answers in flight.
    sending_onboarding: Option<RequestId>,
    sending_rules: Option<RequestId>,
    /// What was chosen and answered, once each part arrived.
    chosen: Option<Vec<ChoiceId>>,
    pub(crate) answers: Option<Vec<RulesAnswer>>,
    tried: bool,
    problem: Option<String>,
    done: Option<String>,
}

impl JoinDialog {
    fn busy(&self) -> bool {
        self.sending_onboarding.is_some() || self.sending_rules.is_some()
    }

    fn loading(&self) -> bool {
        self.loading_onboarding.is_some() || self.loading_rules.is_some()
    }
}

/// Whether a part the account owes never arrived: it is asked for again
/// before anything goes, so the questions never go without the rules.
fn missing(store: &Store, dialog: &JoinDialog) -> bool {
    let steps = store.join_steps(dialog.server);
    (steps.onboarding && dialog.chosen.is_none())
        || (steps.rules && dialog.answers.is_none() && store.rules_outcome(dialog.server).is_none())
}

/// What the account chose among the questions asked, as the provider sent
/// it: it may name options of questions asked later, which never show.
fn answered(store: &Store, server: ServerId) -> Vec<ChoiceId> {
    store
        .onboarding(server)
        .flatten()
        .map(rune_model::Onboarding::answered)
        .unwrap_or_default()
}

/// Open the dialog for `server`, asking for what the account still owes
/// it; nothing when it owes nothing.
pub(crate) fn open(ui: &mut Ui, store: &Store, server: ServerId) -> Vec<Intent> {
    if !store.join_steps(server).any() {
        return Vec::new();
    }
    if ui
        .join
        .as_ref()
        .is_some_and(|dialog| dialog.server == server)
    {
        return Vec::new();
    }
    let mut dialog = JoinDialog {
        server,
        account: store.me(),
        loading_onboarding: None,
        loading_rules: None,
        sending_onboarding: None,
        sending_rules: None,
        chosen: None,
        answers: None,
        tried: false,
        problem: None,
        done: None,
    };
    let intents = load(ui, store, &mut dialog);
    ui.join = Some(dialog);
    intents
}

/// Ask for each part the account owes that the dialog does not have yet;
/// what the store already has is used as it is.
fn load(ui: &mut Ui, store: &Store, dialog: &mut JoinDialog) -> Vec<Intent> {
    let server = dialog.server;
    let steps = store.join_steps(server);
    let mut intents = Vec::new();
    if steps.onboarding && dialog.chosen.is_none() && dialog.loading_onboarding.is_none() {
        match store.onboarding(server) {
            Some(_) => dialog.chosen = Some(answered(store, server)),
            None => {
                let request = ui.fresh_request();
                dialog.loading_onboarding = Some(request);
                intents.push(Intent::Command(Command::LoadOnboarding { request, server }));
            }
        }
    }
    if steps.rules && dialog.answers.is_none() && dialog.loading_rules.is_none() {
        match store.server_rules(server) {
            Some(known) => {
                dialog.answers = Some(
                    known
                        .map(|rules| rules.fields.iter().map(|field| field.starting()).collect())
                        .unwrap_or_default(),
                )
            }
            None => {
                let request = ui.fresh_request();
                dialog.loading_rules = Some(request);
                intents.push(Intent::Command(Command::LoadServerRules {
                    request,
                    server,
                }));
            }
        }
    }
    intents
}

/// Send what is owed: the questions first; the rules once they are
/// answered (see [`on_applied`]), or now when only the rules are owed.
fn send(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let Some(mut dialog) = ui.join.take() else {
        return Vec::new();
    };
    let intents = if dialog.busy() || dialog.loading() {
        Vec::new()
    } else if missing(store, &dialog) {
        // A part owed never arrived: ask for it again.
        dialog.problem = None;
        load(ui, store, &mut dialog)
    } else {
        dialog.tried = true;
        dialog.problem = None;
        let server = dialog.server;
        let questions = shown_onboarding(store, &dialog);
        let rules = shown_rules(store, &dialog);
        let questions_ok = questions.is_none_or(|onboarding| {
            onboarding
                .answer_problem(dialog.chosen.as_deref().unwrap_or_default())
                .is_none()
        });
        let rules_ok = rules.is_none_or(|rules| {
            rules
                .problem(dialog.answers.as_deref().unwrap_or_default())
                .is_none()
        });
        match (questions, questions_ok && rules_ok) {
            (_, false) => Vec::new(),
            (Some(onboarding), true) => {
                let request = ui.fresh_request();
                dialog.sending_onboarding = Some(request);
                vec![Intent::Command(Command::AnswerOnboarding {
                    request,
                    server,
                    onboarding: onboarding.clone(),
                    chosen: dialog.chosen.clone().unwrap_or_default(),
                })]
            }
            (None, true) => send_rules(ui, store, &mut dialog),
        }
    };
    ui.join = Some(dialog);
    intents
}

fn send_rules(ui: &mut Ui, store: &Store, dialog: &mut JoinDialog) -> Vec<Intent> {
    let Some(rules) = shown_rules(store, dialog) else {
        return Vec::new();
    };
    let request = ui.fresh_request();
    dialog.sending_rules = Some(request);
    vec![Intent::Command(Command::AnswerServerRules {
        request,
        server: dialog.server,
        rules: rules.clone(),
        answers: dialog.answers.clone().unwrap_or_default(),
    })]
}

/// The questions still owed, once they arrived and while they ask anything.
fn shown_onboarding<'a>(
    store: &'a Store,
    dialog: &JoinDialog,
) -> Option<&'a rune_model::Onboarding> {
    dialog.chosen.as_ref()?;
    store
        .join_steps(dialog.server)
        .onboarding
        .then(|| store.onboarding(dialog.server).flatten())
        .flatten()
        .filter(|onboarding| onboarding.asks())
}

/// The rules still owed, once they arrived.
fn shown_rules<'a>(store: &'a Store, dialog: &JoinDialog) -> Option<&'a rune_model::ServerRules> {
    dialog.answers.as_ref()?;
    store
        .join_steps(dialog.server)
        .rules
        .then(|| store.server_rules(dialog.server).flatten())
        .flatten()
        .filter(|_| store.rules_outcome(dialog.server).is_none())
}

/// Apply an `onb:` action; `None` for the rest of the window.
pub(crate) fn update(ui: &mut Ui, store: &Store, action: &UiAction) -> Option<Vec<Intent>> {
    let (id, value) = match action {
        UiAction::Clicked { id } => (id.as_str(), None),
        UiAction::InputChanged { id, value } => (id.as_str(), Some(value)),
        _ => return None,
    };
    let rest = id.strip_prefix("onb:")?;
    if let Some(server) = rest.strip_prefix("open:") {
        return Some(
            server
                .parse()
                .map(|server| open(ui, store, ServerId(server)))
                .unwrap_or_default(),
        );
    }
    let parts: Vec<&str> = rest.split(':').collect();
    let busy = ui.join.as_ref().is_some_and(JoinDialog::busy);
    match (parts.as_slice(), value) {
        // Answers on their way keep it open, and as they were sent.
        (["close"], _) if !busy => ui.join = None,
        (["send"] | ["retry"], _) => return Some(send(ui, store)),
        _ if busy => {}
        (["text", field], Some(value)) => {
            if let (Some(dialog), Ok(field)) = (ui.join.as_mut(), field.parse::<usize>()) {
                if let Some(answer) = dialog.answers.as_mut().and_then(|all| all.get_mut(field)) {
                    *answer = RulesAnswer::Text(value.clone());
                }
            }
        }
        (["agree", field], None) => {
            if let (Some(dialog), Ok(field)) = (ui.join.as_mut(), field.parse::<usize>()) {
                if let Some(RulesAnswer::Agreed(agreed)) =
                    dialog.answers.as_mut().and_then(|all| all.get_mut(field))
                {
                    *agreed = !*agreed;
                }
            }
        }
        (["choice", field, index], None) => {
            if let (Some(dialog), Ok(field), Ok(index)) = (
                ui.join.as_mut(),
                field.parse::<usize>(),
                index.parse::<usize>(),
            ) {
                if let Some(answer) = dialog.answers.as_mut().and_then(|all| all.get_mut(field)) {
                    *answer = RulesAnswer::Choice(Some(index));
                }
            }
        }
        (["option", choice], None) => {
            let choice = choice.parse().map(ChoiceId).ok();
            if let (Some(dialog), Some(choice)) = (ui.join.as_mut(), choice) {
                choose(store, dialog, choice);
            }
        }
        _ => {}
    }
    Some(Vec::new())
}

/// Choose `choice`, or let it go; in a question of one answer, choosing
/// one lets the others go.
fn choose(store: &Store, dialog: &mut JoinDialog, choice: ChoiceId) {
    let Some(prompt) = store
        .onboarding(dialog.server)
        .flatten()
        .and_then(|onboarding| {
            onboarding
                .asked()
                .find(|prompt| prompt.options.iter().any(|option| option.id == choice))
        })
    else {
        return;
    };
    let chosen = dialog.chosen.get_or_insert_with(Vec::new);
    if chosen.contains(&choice) {
        if !prompt.single {
            chosen.retain(|known| *known != choice);
        }
        return;
    }
    if prompt.single {
        chosen.retain(|known| !prompt.options.iter().any(|option| option.id == *known));
    }
    chosen.push(choice);
}

/// Esc closes the dialog, unless answers are on their way; a verification
/// one of them meets is over it and takes the key itself.
pub(crate) fn intercept(ui: &mut Ui, event: &UiEvent) -> Option<Vec<Intent>> {
    let dialog = ui.join.as_ref().filter(|_| ui.challenge.is_none())?;
    if !matches!(
        event,
        UiEvent::Key {
            key: Key::Escape,
            ..
        }
    ) {
        return None;
    }
    if !dialog.busy() {
        ui.join = None;
    }
    Some(Vec::new())
}

/// Follow what the provider answered: seed each part as it arrives, send
/// the rules once the questions are answered, and say what came of it.
pub(crate) fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied) -> Vec<Intent> {
    let Some(mut dialog) = ui.join.take() else {
        return Vec::new();
    };
    if dialog.account != store.me() || store.server(dialog.server).is_none() {
        return Vec::new();
    }
    let mut intents = Vec::new();
    let server = dialog.server;
    let name = || {
        store
            .server(server)
            .map(|server| server.shown_name().to_owned())
            .unwrap_or_default()
    };
    match applied {
        Applied::Onboarding { request, .. } if dialog.loading_onboarding == Some(*request) => {
            dialog.loading_onboarding = None;
            dialog.chosen = Some(answered(store, server));
        }
        Applied::ServerRules { request, .. } if dialog.loading_rules == Some(*request) => {
            dialog.loading_rules = None;
            dialog.answers = Some(
                store
                    .server_rules(dialog.server)
                    .flatten()
                    .map(|rules| rules.fields.iter().map(|field| field.starting()).collect())
                    .unwrap_or_default(),
            );
        }
        Applied::OnboardingAnswered { request, .. }
            if dialog.sending_onboarding == Some(*request) =>
        {
            dialog.sending_onboarding = None;
            intents = send_rules(ui, store, &mut dialog);
            if intents.is_empty() {
                dialog.done = Some(tr!(
                    "Listo: tus respuestas ya están en {server}.",
                    server = name()
                ));
            }
        }
        Applied::RulesAnswered {
            request, outcome, ..
        } if dialog.sending_rules == Some(*request) => {
            dialog.sending_rules = None;
            let server = name();
            dialog.done = Some(match outcome {
                JoinOutcome::Approved => tr!("Listo: ya puedes participar en {server}.", server),
                JoinOutcome::InReview => tr!(
                    "Enviado: la moderación de {server} va a revisar tus respuestas.",
                    server
                ),
                JoinOutcome::Rejected => {
                    tr!("La moderación de {server} rechazó tu solicitud.", server)
                }
            });
        }
        Applied::Failed {
            failure: Failure::Rules { request, .. } | Failure::Onboarding { request, .. },
            reason,
            ..
        } if [
            dialog.loading_onboarding,
            dialog.loading_rules,
            dialog.sending_onboarding,
            dialog.sending_rules,
        ]
        .contains(&Some(*request)) =>
        {
            for slot in [
                &mut dialog.loading_onboarding,
                &mut dialog.loading_rules,
                &mut dialog.sending_onboarding,
                &mut dialog.sending_rules,
            ] {
                if *slot == Some(*request) {
                    *slot = None;
                }
            }
            dialog.problem = Some(reason.clone());
        }
        // A new session forgot what was asked: ask again; what was on its
        // way says it did not arrive.
        Applied::Ready => {
            if dialog.busy() {
                dialog.problem = Some(t("Se perdió la conexión: vuelve a intentarlo.").into());
            }
            dialog.loading_onboarding = None;
            dialog.loading_rules = None;
            dialog.sending_onboarding = None;
            dialog.sending_rules = None;
            dialog.chosen = None;
            dialog.answers = None;
            if dialog.done.is_none() && store.join_steps(dialog.server).any() {
                intents = load(ui, store, &mut dialog);
            }
        }
        _ => {}
    }
    // Owed nothing any more (answered elsewhere, or it asks nothing), and
    // nothing to say: there is nothing to show.
    let idle = !dialog.busy() && !dialog.loading();
    if idle && dialog.done.is_none() && dialog.problem.is_none() && !store.join_steps(server).any()
    {
        return intents;
    }
    ui.join = Some(dialog);
    intents
}

/// The dialog over `root`, while it is open.
pub(crate) fn overlay(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let Some(dialog) = ui
        .join
        .as_ref()
        .filter(|dialog| dialog.account == store.me())
    else {
        return root;
    };
    let Some(server) = store.server(dialog.server) else {
        return root;
    };
    let onboarding = shown_onboarding(store, dialog);
    let rules = shown_rules(store, dialog);
    let simulated = store
        .provider
        .as_ref()
        .is_some_and(|info| info.kind == ProviderKind::Simulated);
    let in_review = store
        .rules_outcome(dialog.server)
        .filter(|outcome| *outcome != JoinOutcome::Approved)
        .map(|_| {
            tr!(
                "La moderación de {server} está revisando tus respuestas.",
                server = server.shown_name()
            )
        });
    let width = Modal::fit_width(context.window.width, 520.0);
    let empty = Vec::new();
    parts::join_dialog(
        root,
        &JoinView {
            server: server.shown_name(),
            onboarding: onboarding.filter(|_| dialog.done.is_none()),
            chosen: dialog.chosen.as_ref().unwrap_or(&empty),
            rules: rules.filter(|_| dialog.done.is_none()),
            answers: dialog.answers.as_deref().unwrap_or_default(),
            loading: dialog.loading(),
            sending: dialog.busy(),
            tried: dialog.tried,
            problem: dialog.problem.as_deref(),
            done: dialog.done.as_deref().or(in_review.as_deref()),
            retry: missing(store, dialog) && !dialog.loading(),
            note: simulated.then(|| t("Simulación local: no se envía nada a ningún servicio.")),
        },
        width,
        look,
    )
}

/// The composer of a conversation in `server`, with a bar over it while
/// the account still owes the server something.
pub(crate) fn with_steps(
    surface: Element,
    ui: &Ui,
    store: &Store,
    server: Option<ServerId>,
    look: &Look,
) -> Element {
    let Some(server) = server else {
        return surface;
    };
    let steps = store.join_steps(server);
    if !steps.any() || ui.join.is_some() {
        return surface;
    }
    let Some(found) = store.server(server) else {
        return surface;
    };
    let name = found.shown_name();
    let bar = match (steps.rules, store.rules_outcome(server)) {
        (true, Some(JoinOutcome::InReview)) => return surface,
        (true, Some(JoinOutcome::Rejected)) => return surface,
        (true, _) => parts::steps_bar(
            &tr!(
                "Antes de participar, {server} pide que aceptes sus reglas.",
                server = name
            ),
            t("Ver las reglas"),
            server,
            true,
            look,
        ),
        (false, _) => parts::steps_bar(
            &tr!(
                "Responde las preguntas de {server} para elegir tus roles y canales.",
                server = name
            ),
            t("Responder las preguntas"),
            server,
            false,
            look,
        ),
    };
    Element::column(vec![bar, surface])
}

/// Whether the dialog covers the window.
pub(crate) fn visible(ui: &Ui) -> bool {
    ui.join.is_some()
}
