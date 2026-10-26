//! Account-bound invitation previews and explicit, session-scoped acceptance.

use std::time::{Duration, Instant};

use limn::{
    Access, Button, ButtonVariant, Component, Element, Key, Modal, Role, Spinner, TextField,
    Tooltip, UiAction, UiEvent,
};
use rune_components::{avatar_with_image, line, text, Look};
use rune_i18n::{es, t, tr};
use rune_model::{RemoteImage, Timestamp, UserId};
use rune_provider::{
    Command, ConnectionState, Failure, InvitationDestination, InvitationError, InvitationJoinError,
    InvitationPreview, InvitationResult, InvitationTarget, ProviderKind, RequestId,
};
use rune_state::{Applied, Store};

use crate::{editor_id, Intent, Ui, ViewContext};

const CLOSE: &str = "invitation:close";
const COPY: &str = "invitation:copy";
const OPEN: &str = "invitation:open";
const RETRY: &str = "invitation:retry";
const ACCEPT: &str = "invitation:accept";
const VERIFY: &str = "invitation:verify";
const BROWSER: &str = "invitation:browser";
const INPUT_OPEN: &str = rune_components::server_menu::ids::SERVER_REVIEW_INVITATION;
const INPUT: &str = "invitation:input";
const INPUT_SUBMIT: &str = "invitation:input-submit";
const INPUT_CLOSE: &str = "invitation:input-close";
const INPUT_LIMIT: usize = 2048;
const WAIT_LIMIT: Duration = Duration::from_secs(60);
/// Solving a challenge in the provider's window, then its one retry.
const VERIFY_LIMIT: Duration = Duration::from_secs(360);

pub(crate) struct InvitationInput {
    account: Option<UserId>,
    value: String,
    error: Option<&'static str>,
}

impl std::fmt::Debug for InvitationInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("InvitationInput([REDACTED])")
    }
}

pub(crate) fn visible(ui: &Ui) -> bool {
    ui.invitation_input.is_some() || ui.invitation.is_some()
}

/// Recognition is local and never falls back to opening an unrecognized URL.
pub fn invitation_input_result(
    ui: &mut Ui,
    store: &Store,
    input: &str,
    target: Option<InvitationTarget>,
) -> Vec<Intent> {
    let Some(dialog) = ui.invitation_input.as_mut() else {
        return Vec::new();
    };
    if dialog.account != store.me() {
        return close(ui);
    }
    if dialog.value.trim() != input || !store.invitation_preview().allowed() {
        return Vec::new();
    }
    match target {
        Some(target) => open_invitation(ui, store, target),
        None => {
            dialog.error = Some(es(
                "Pega el enlace completo de una invitación compatible con esta conexión.",
            ));
            Vec::new()
        }
    }
}

fn change_input(ui: &mut Ui, value: &str) -> Vec<Intent> {
    let dialog = ui.invitation_input.as_mut().expect("the input is open");
    if value.len() > INPUT_LIMIT {
        dialog.error = Some(es("El enlace supera el límite de 2048 bytes."));
        return vec![Intent::SetInput {
            id: INPUT.into(),
            value: dialog.value.clone(),
        }];
    }
    dialog.value = value.into();
    dialog.error = None;
    Vec::new()
}

fn submit_input(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let dialog = ui.invitation_input.as_mut().expect("the input is open");
    if !store.invitation_preview().allowed() || dialog.value.trim().is_empty() {
        return Vec::new();
    }
    vec![Intent::InspectInvitation(dialog.value.trim().into())]
}

fn update_input(ui: &mut Ui, store: &Store, action: &UiAction) -> Option<Vec<Intent>> {
    if matches!(action, UiAction::Clicked { id } if id == INPUT_OPEN) {
        if !store.invitation_preview().allowed()
            || !matches!(ui.server_menu.dialog, Some(crate::Dialog::Server(None)))
            || visible(ui)
        {
            return Some(Vec::new());
        }
        let mut intents = close(ui);
        ui.invitation_input = Some(InvitationInput {
            account: store.me(),
            value: String::new(),
            error: None,
        });
        ui.server_menu.dialog = None;
        ui.server_menu.open = false;
        intents.extend(crate::search::dismiss(ui));
        intents.extend(crate::context::cancel(ui));
        ui.switcher = None;
        intents.extend([
            Intent::SetInput {
                id: INPUT.into(),
                value: String::new(),
            },
            Intent::Focus(INPUT.into()),
        ]);
        return Some(intents);
    }
    let dialog = ui.invitation_input.as_ref()?;
    if dialog.account != store.me() {
        return Some(close(ui));
    }
    Some(match action {
        UiAction::InputChanged { id, value } if id == INPUT => change_input(ui, value),
        UiAction::Submitted { id, value } if id == INPUT => {
            let intents = change_input(ui, value);
            if intents.is_empty() {
                submit_input(ui, store)
            } else {
                intents
            }
        }
        UiAction::Clicked { id } if id == INPUT_SUBMIT => submit_input(ui, store),
        UiAction::Clicked { id } if id == INPUT_CLOSE => close(ui),
        _ => Vec::new(),
    })
}

/// Closing a preview never forgets an in-flight write or its rate limit.
#[derive(Debug, Default)]
pub(crate) struct InvitationJoinState {
    account: Option<UserId>,
    active: Option<Acceptance>,
    cooldown: Option<(Instant, Duration)>,
}

#[derive(Debug)]
struct Acceptance {
    request: RequestId,
    destination: InvitationDestination,
    target: InvitationTarget,
    phase: Phase,
    since: Instant,
    navigate: bool,
}

#[derive(Clone, Copy, Debug)]
enum Phase {
    Sending,
    /// The provider's window shows the challenge; nothing is sent until the
    /// person solves it.
    Verifying,
    Waiting,
    /// HTTP may still resolve, or membership did not arrive in time.
    Unconfirmed {
        pending: bool,
    },
    Failed(InvitationJoinError),
}

impl InvitationJoinState {
    fn remaining(&self, now: Instant) -> Option<Duration> {
        self.cooldown
            .map(|(at, duration)| duration.saturating_sub(now.saturating_duration_since(at)))
            .filter(|left| !left.is_zero())
    }

    fn busy(&self) -> bool {
        self.active.as_ref().is_some_and(|active| {
            matches!(
                active.phase,
                Phase::Sending | Phase::Verifying | Phase::Unconfirmed { pending: true }
            )
        })
    }

    fn for_dialog(&self, dialog: &InvitationDialog) -> Option<&Acceptance> {
        self.active
            .as_ref()
            .filter(|active| self.account == dialog.account && active.target == dialog.target)
    }
}

/// How long a phase may wait for the provider before it becomes uncertain.
fn limit(phase: Phase) -> Option<Duration> {
    match phase {
        Phase::Sending | Phase::Waiting => Some(WAIT_LIMIT),
        Phase::Verifying => Some(VERIFY_LIMIT),
        _ => None,
    }
}

pub(crate) fn expire(ui: &mut Ui, now: Instant) -> bool {
    let state = &mut ui.invitation_join;
    let mut changed = false;
    if state.cooldown.is_some() && state.remaining(now).is_none() {
        state.cooldown = None;
        changed = true;
    }
    if let Some(active) = &mut state.active {
        if limit(active.phase)
            .is_some_and(|limit| now.saturating_duration_since(active.since) >= limit)
        {
            active.phase = Phase::Unconfirmed {
                pending: !matches!(active.phase, Phase::Waiting),
            };
            active.navigate = false;
            changed = true;
        }
    }
    if let Some(dialog) = &mut ui.invitation {
        if dialog.preview_cooldown.is_some() && dialog.preview_remaining(now).is_none() {
            dialog.preview_cooldown = None;
            changed = true;
        }
    }
    changed
}

pub(crate) fn next_expiry(ui: &Ui, now: Instant) -> Option<Duration> {
    let state = &ui.invitation_join;
    let pending = state.active.as_ref().and_then(|active| {
        limit(active.phase)
            .map(|limit| limit.saturating_sub(now.saturating_duration_since(active.since)))
    });
    let cooldown = state
        .cooldown
        .map(|(at, duration)| duration.saturating_sub(now.saturating_duration_since(at)));
    let preview = ui
        .invitation
        .as_ref()
        .and_then(|dialog| dialog.preview_remaining(now))
        .map(|left| left.min(Duration::from_secs(1)));
    let expiration = ui
        .invitation
        .as_ref()
        .and_then(InvitationDialog::expires_at)
        .map(|at| at.0.saturating_sub(crate::server_menu::wall_now().0))
        .filter(|left| *left > 0)
        .map(|left| Duration::from_millis(left as u64).min(Duration::from_secs(60)));
    pending
        .into_iter()
        .chain(cooldown)
        .chain(preview)
        .chain(expiration)
        .min()
}

#[derive(Debug)]
pub struct InvitationDialog {
    request: RequestId,
    account: Option<UserId>,
    target: InvitationTarget,
    result: Option<Result<InvitationPreview, InvitationError>>,
    preview_cooldown: Option<(Instant, Duration)>,
}

impl InvitationDialog {
    fn expires_at(&self) -> Option<Timestamp> {
        self.result.as_ref()?.as_ref().ok()?.expires_at
    }

    fn expired(&self, wall: Timestamp) -> bool {
        self.expires_at().is_some_and(|at| at <= wall)
    }

    /// A command deadline within `limit` that cannot outlive the public
    /// invitation. Wall time is read again so crossing its expiration after
    /// the last check sends nothing.
    fn deadline(&self, now: Instant, limit: Duration) -> Option<Instant> {
        let lifetime = self.expires_at().map_or(limit, |expiration| {
            Duration::from_millis(
                expiration
                    .0
                    .saturating_sub(crate::server_menu::wall_now().0)
                    .max(0) as u64,
            )
            .min(limit)
        });
        now.checked_add(lifetime).filter(|_| !lifetime.is_zero())
    }

    fn preview_remaining(&self, now: Instant) -> Option<Duration> {
        self.preview_cooldown
            .map(|(at, duration)| duration.saturating_sub(now.saturating_duration_since(at)))
            .filter(|left| !left.is_zero())
    }

    fn can_retry_preview(&self, store: &Store, now: Instant) -> bool {
        store.invitation_preview().allowed()
            && match &self.result {
                Some(Err(InvitationError::Invalid | InvitationError::Unavailable)) => true,
                Some(Err(InvitationError::RateLimited { .. })) => {
                    self.preview_remaining(now).is_none()
                }
                Some(Ok(_)) => self.expired(crate::server_menu::wall_now()),
                _ => false,
            }
    }
}

pub fn open_invitation(ui: &mut Ui, store: &Store, target: InvitationTarget) -> Vec<Intent> {
    let mut intents = close(ui);
    if ui.invitation_join.account.is_some() && ui.invitation_join.account != store.me() {
        ui.invitation_join = InvitationJoinState::default();
    }
    let request = ui.fresh_request();
    let supported = store.invitation_preview().allowed();
    ui.invitation = Some(InvitationDialog {
        request,
        account: store.me(),
        target: target.clone(),
        result: (!supported).then_some(Err(InvitationError::Unsupported)),
        preview_cooldown: None,
    });
    intents.extend(crate::search::dismiss(ui));
    intents.extend(crate::context::cancel(ui));
    ui.switcher = None;
    if supported {
        intents.push(Intent::PreviewInvitation { request, target });
    }
    intents.push(Intent::Focus(CLOSE.into()));
    intents
}

pub fn invitation_result(ui: &mut Ui, store: &Store, result: InvitationResult) -> Vec<Intent> {
    let Some(dialog) = ui.invitation.as_mut() else {
        return Vec::new();
    };
    if dialog.account != store.me() {
        return close(ui);
    }
    if dialog.request == result.request && dialog.target == result.target && dialog.result.is_none()
    {
        dialog.preview_cooldown = match &result.result {
            Err(InvitationError::RateLimited { retry_after_secs }) => {
                Some((Instant::now(), Duration::from_secs(*retry_after_secs)))
            }
            _ => None,
        };
        dialog.result = Some(result.result);
    }
    Vec::new()
}

/// An HTTP acknowledgement is not membership. Only the normal account events
/// may supply the server and its visible channels.
pub(crate) fn on_applied(
    ui: &mut Ui,
    store: &mut Store,
    applied: &Applied,
    now: Instant,
) -> Vec<Intent> {
    expire(ui, now);
    if ui.invitation_join.account.is_some() && ui.invitation_join.account != store.me() {
        ui.invitation_join = InvitationJoinState::default();
    }
    if ui
        .invitation_input
        .as_ref()
        .is_some_and(|dialog| dialog.account != store.me())
        || ui
            .invitation
            .as_ref()
            .is_some_and(|dialog| dialog.account != store.me())
    {
        return close(ui);
    }
    let result = match applied {
        Applied::InvitationAcceptance {
            request,
            destination,
            result,
        } => Some((*request, *destination, *result, true)),
        Applied::Failed {
            failure:
                Failure::Invitation {
                    request,
                    destination,
                },
            ambiguous,
            ..
        } => Some((
            *request,
            *destination,
            Err(if *ambiguous {
                InvitationJoinError::Ambiguous
            } else {
                InvitationJoinError::Unavailable
            }),
            false,
        )),
        _ => None,
    };
    if let Some((request, destination, result, terminal)) = result {
        let state = &mut ui.invitation_join;
        if let Some(active) = state.active.as_mut().filter(|active| {
            active.request == request
                && active.destination == destination
                && matches!(
                    active.phase,
                    Phase::Sending | Phase::Verifying | Phase::Unconfirmed { pending: true }
                )
        }) {
            match result {
                Ok(()) => {
                    active.phase = Phase::Waiting;
                    active.since = now;
                }
                Err(error) => {
                    active.navigate = false;
                    active.phase = if error == InvitationJoinError::Ambiguous {
                        Phase::Unconfirmed { pending: !terminal }
                    } else {
                        Phase::Failed(error)
                    };
                    if let InvitationJoinError::RateLimited { retry_after_secs } = error {
                        let previous = state
                            .cooldown
                            .map(|(at, duration)| {
                                duration.saturating_sub(now.saturating_duration_since(at))
                            })
                            .unwrap_or_default();
                        state.cooldown =
                            Some((now, previous.max(Duration::from_secs(retry_after_secs))));
                    }
                }
            }
        }
    }
    if let Applied::ServerRemoved { server, .. } = applied {
        if let Some(active) = ui
            .invitation_join
            .active
            .as_mut()
            .filter(|active| active.destination.server() == Some(*server))
        {
            active.navigate = false;
            if matches!(active.phase, Phase::Waiting) {
                active.phase = Phase::Failed(InvitationJoinError::Denied);
            }
        }
    }
    if let Some(active) = &mut ui.invitation_join.active {
        if matches!(active.phase, Phase::Unconfirmed { pending: true })
            && !store.from_cache
            && store.invitation_joined(active.destination)
        {
            // Membership resolves the uncertain outcome without repeating the
            // write or restoring navigation that a timeout/close disabled.
            active.phase = Phase::Unconfirmed { pending: false };
        }
        if matches!(applied, Applied::Connection) && store.connection != ConnectionState::Connected
        {
            active.navigate = false;
        }
    }
    let navigate = ui
        .invitation_join
        .active
        .as_ref()
        .filter(|active| {
            active.navigate
                && matches!(active.phase, Phase::Waiting)
                && ui.invitation.as_ref().is_some_and(|dialog| {
                    dialog.target == active.target && dialog.account == store.me()
                })
                && !store.from_cache
                && store.connection == ConnectionState::Connected
                && arrived(store, active.destination)
        })
        .map(|active| active.destination);
    if let Some(destination) = navigate {
        // Scheduled events (eventos-programados): the event it names, after.
        let event = named_event(ui);
        let mut intents = close(ui);
        intents.extend(open_destination(ui, store, destination, now));
        intents.extend(crate::events::open_invited(ui, store, event));
        return intents;
    }
    Vec::new()
}

/// Whether what the invitation gave can be opened by itself: a server with a
/// conversation to show, the guest's server, the group, or the direct
/// conversation with the new friend (opening one is a write: never automatic).
fn arrived(store: &Store, destination: InvitationDestination) -> bool {
    store.invitation_joined(destination)
        && match destination {
            InvitationDestination::Server(server) => store.first_text_channel(server).is_some(),
            InvitationDestination::Guest { .. } | InvitationDestination::Group(_) => true,
            InvitationDestination::Friend(user) => store.direct_with(user).is_some(),
        }
}

// Scheduled events (eventos-programados).
/// The event the invitation open in the dialog names, if it names one.
fn named_event(ui: &Ui) -> Option<(rune_model::ServerId, rune_model::ScheduledEventId)> {
    let event = ui
        .invitation
        .as_ref()?
        .result
        .as_ref()?
        .as_ref()
        .ok()?
        .event
        .as_ref()?;
    Some((event.server, event.id))
}

/// Open what an invitation gave the account: the server, the group, or the
/// direct conversation with the friend (asking for one when there is none).
pub(crate) fn open_destination(
    ui: &mut Ui,
    store: &mut Store,
    destination: InvitationDestination,
    now: Instant,
) -> Vec<Intent> {
    match destination {
        InvitationDestination::Server(server) => {
            let mut intents = crate::open_server(ui, store, Some(server));
            // Rules and onboarding (2026-10-18): what it asks, at once.
            intents.extend(crate::onboarding::open(ui, store, server));
            intents
        }
        // A guest is never onboarded, as the official client lets one in.
        InvitationDestination::Guest { server, .. } => crate::open_server(ui, store, Some(server)),
        InvitationDestination::Group(channel) => crate::open(ui, store, None, channel),
        InvitationDestination::Friend(user) => {
            crate::conversations::open_direct(ui, store, user, now)
        }
    }
}

pub(crate) fn shows_failure(ui: &Ui, failure: &Failure) -> bool {
    let Failure::Invitation {
        request,
        destination,
    } = failure
    else {
        return false;
    };
    ui.invitation.as_ref().is_some_and(|dialog| {
        ui.invitation_join
            .for_dialog(dialog)
            .is_some_and(|active| active.request == *request && active.destination == *destination)
    })
}

fn close(ui: &mut Ui) -> Vec<Intent> {
    ui.invitation_input = None;
    if let Some(active) = &mut ui.invitation_join.active {
        active.navigate = false;
    }
    let mut intents = ui
        .invitation
        .take()
        .map(|dialog| vec![Intent::CancelInvitation(dialog.request)])
        .unwrap_or_default();
    if let Some(channel) = ui.channel() {
        intents.push(Intent::Focus(editor_id(channel)));
    }
    intents
}

pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    if let Some(intents) = update_input(ui, store, action) {
        return Some(intents);
    }
    let UiAction::Clicked { id } = action else {
        return None;
    };
    if !id.starts_with("invitation:") {
        return None;
    }
    expire(ui, now);
    let Some(dialog) = &ui.invitation else {
        return Some(Vec::new());
    };
    if dialog.account != store.me() {
        return Some(close(ui));
    }
    Some(match id.as_str() {
        CLOSE => close(ui),
        COPY => vec![Intent::Copy(dialog.target.url.clone())],
        // The service's own page, with whatever account the browser has.
        BROWSER => vec![Intent::OpenInBrowser(dialog.target.url.clone())],
        ACCEPT if can_accept(ui, store, dialog, now) => {
            let destination = dialog
                .result
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .expect("accept requires a preview")
                .destination;
            let Some(expires_at) = dialog.deadline(now, WAIT_LIMIT) else {
                return Some(Vec::new());
            };
            let target = dialog.target.clone();
            let account = store.me().expect("accept requires an account");
            let request = ui.fresh_request();
            ui.invitation_join.account = Some(account);
            ui.invitation_join.active = Some(Acceptance {
                request,
                destination,
                target: target.clone(),
                phase: Phase::Sending,
                since: now,
                navigate: true,
            });
            vec![Intent::Command(Command::AcceptInvitation {
                request,
                target,
                destination,
                account,
                expires_at,
            })]
        }
        // Opens the provider's own window; only the person's answer there
        // lets the provider repeat this same acceptance, once.
        VERIFY if can_verify(ui, store, dialog, now) => {
            let Some(expires_at) = dialog.deadline(now, VERIFY_LIMIT) else {
                return Some(Vec::new());
            };
            let account = store.me().expect("verify requires an account");
            let active = ui
                .invitation_join
                .active
                .as_mut()
                .expect("verify requires an attempt");
            active.phase = Phase::Verifying;
            active.since = now;
            active.navigate = true;
            vec![Intent::Command(Command::VerifyInvitation {
                request: active.request,
                destination: active.destination,
                account,
                expires_at,
            })]
        }
        OPEN => {
            let destination = dialog
                .result
                .as_ref()
                .and_then(|result| result.as_ref().ok())
                .map(|preview| preview.destination)
                .filter(|destination| store.invitation_joined(*destination));
            if let Some(destination) = destination {
                if let Some(active) = &mut ui.invitation_join.active {
                    if active.destination == destination {
                        active.phase = Phase::Unconfirmed { pending: false };
                    }
                }
                // Scheduled events (eventos-programados): the event it names.
                let event = named_event(ui);
                let mut intents = close(ui);
                intents.extend(open_destination(ui, store, destination, now));
                intents.extend(crate::events::open_invited(ui, store, event));
                intents
            } else {
                Vec::new()
            }
        }
        RETRY if dialog.can_retry_preview(store, now) => {
            let target = dialog.target.clone();
            open_invitation(ui, store, target)
        }
        _ => Vec::new(),
    })
}

/// Only the challenge that answered this dialog's own attempt, while that
/// attempt could still be accepted.
fn can_verify(ui: &Ui, store: &Store, dialog: &InvitationDialog, now: Instant) -> bool {
    let Some(preview) = dialog
        .result
        .as_ref()
        .and_then(|result| result.as_ref().ok())
    else {
        return false;
    };
    dialog.account == store.me()
        && !dialog.expired(crate::server_menu::wall_now())
        && store.accept_invitation(preview.destination).allowed()
        && ui.invitation_join.remaining(now).is_none()
        && ui.invitation_join.for_dialog(dialog).is_some_and(|active| {
            active.destination == preview.destination
                && matches!(
                    active.phase,
                    Phase::Failed(InvitationJoinError::ChallengeRequired)
                )
        })
}

fn can_accept(ui: &Ui, store: &Store, dialog: &InvitationDialog, now: Instant) -> bool {
    let Some(preview) = dialog
        .result
        .as_ref()
        .and_then(|result| result.as_ref().ok())
    else {
        return false;
    };
    if dialog.account != store.me()
        || dialog.expired(crate::server_menu::wall_now())
        || store.invitation_joined(preview.destination)
        || !store.accept_invitation(preview.destination).allowed()
        || ui.invitation_join.busy()
        || ui.invitation_join.remaining(now).is_some()
    {
        return false;
    }
    ui.invitation_join.for_dialog(dialog).is_none_or(|active| {
        active.destination == preview.destination
            && matches!(active.phase, Phase::Failed(error) if error.can_retry())
    })
}

pub(crate) fn intercept(ui: &mut Ui, event: &UiEvent) -> Option<Vec<Intent>> {
    if !visible(ui) {
        return None;
    }
    let UiEvent::Key { key, .. } = event else {
        return None;
    };
    if *key == Key::Escape {
        return Some(close(ui));
    }
    if ui.invitation_input.is_some() {
        return None;
    }
    if matches!(key, Key::Tab | Key::Enter | Key::Space) {
        None
    } else {
        Some(Vec::new())
    }
}

pub(crate) fn image<'a>(ui: &'a Ui, store: &Store) -> Option<&'a RemoteImage> {
    ui.invitation
        .as_ref()
        .filter(|dialog| dialog.account == store.me())?
        .result
        .as_ref()?
        .as_ref()
        .ok()?
        .image
        .as_ref()
}

pub(crate) fn overlay(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    if let Some(dialog) = ui
        .invitation_input
        .as_ref()
        .filter(|dialog| dialog.account == store.me())
    {
        return input_overlay(dialog, store, root, look, context);
    }
    let Some(dialog) = ui
        .invitation
        .as_ref()
        .filter(|dialog| dialog.account == store.me())
    else {
        return root;
    };
    let width = Modal::fit_width(context.window.width, 480.0);
    let inner = Modal::body_width(width);
    let mut body = Vec::new();
    if let Some(account) = store.me_user() {
        body.push(line(t("Cuenta conectada en RUNE"), 12.0, look.muted));
        body.push(
            Element::row(vec![
                avatar_with_image(
                    &account.name,
                    look.accent,
                    32.0,
                    ui.picture(account.picture()),
                ),
                line(format!("@{}", account.name), 14.0, look.text)
                    .flex(1.0)
                    .centered_y(),
            ])
            .gap(10.0),
        );
    } else {
        body.push(line(t("No hay una cuenta conectada"), 14.0, look.muted));
    }
    let mut actions = Vec::new();
    match &dialog.result {
        None => body.push(
            Element::row(vec![
                Spinner::new("invitation:loading", look.accent).render(),
                line(t("Consultando invitación…"), 14.0, look.muted).centered_y(),
            ])
            .gap(8.0),
        ),
        Some(Err(error)) => {
            body.push(text(error.label(), 14.0, look.muted).size(Some(inner), None));
            if let Some(left) = dialog.preview_remaining(context.now) {
                let seconds = left
                    .as_secs()
                    .saturating_add(u64::from(left.subsec_nanos() != 0));
                body.push(status(
                    &tr!("Puedes volver a consultar en {seconds} s.", seconds),
                    false,
                    inner,
                    look,
                ));
            }
            if dialog.can_retry_preview(store, context.now) {
                actions.push(
                    Button::new(RETRY, t("Volver a consultar"), look.menu_palette())
                        .variant(ButtonVariant::Secondary)
                        .render(),
                );
            }
        }
        Some(Ok(preview)) => {
            let destination = preview.destination;
            let name = if preview.name.is_empty() {
                t("Grupo sin nombre")
            } else {
                preview.name.as_str()
            };
            body.push(
                Element::row(vec![
                    avatar_with_image(name, look.accent, 48.0, ui.image(&preview.image)),
                    line(name, 18.0, look.text).flex(1.0).centered_y(),
                ])
                .gap(12.0),
            );
            if let Some(description) = &preview.description {
                body.push(text(description, 14.0, look.muted).size(Some(inner), None));
            }
            let expiration = match preview.expires_at {
                Some(_) if dialog.expired(context.wall) => t("Esta invitación venció.").into(),
                Some(at) => tr!(
                    "Vence: {when}.",
                    when = rune_components::time::stamp(at, context.wall)
                ),
                None => t("Caducidad no informada por el servicio.").into(),
            };
            body.push(
                text(&expiration, 13.0, look.muted)
                    .size(Some(inner), None)
                    .accessible(
                        Access::new(Role::Status)
                            .id("invitation:expiration")
                            .name(expiration),
                    ),
            );
            if dialog.can_retry_preview(store, context.now) {
                actions.push(
                    Button::new(RETRY, t("Volver a consultar"), look.menu_palette())
                        .variant(ButtonVariant::Secondary)
                        .render(),
                );
            }
            let stats = match (preview.members, preview.online) {
                (Some(members), Some(online)) => {
                    tr!("{members} miembros · {online} en línea", members, online)
                }
                (Some(members), None) => tr!("{members} miembros", members),
                (None, Some(online)) => tr!("{online} en línea", online),
                _ => String::new(),
            };
            if !stats.is_empty() {
                body.push(
                    Tooltip::new(
                        text(&stats, 13.0, look.muted).accessible(
                            Access::new(Role::Status)
                                .id("invitation:counts")
                                .name(&stats)
                                .description(t("Cantidades aproximadas")),
                        ),
                        "invitation:counts:hint",
                        t("Cantidades aproximadas"),
                        look.menu_palette(),
                    )
                    .render(),
                );
            }
            if let InvitationDestination::Guest { .. } = destination {
                body.push(status(
                    t("Como invitado entras sólo a un canal de voz, y sales del servidor al dejar la llamada."),
                    false,
                    inner,
                    look,
                ));
            }
            if store.invitation_joined(destination) {
                let open = match destination {
                    InvitationDestination::Group(_) => t("Abrir grupo"),
                    InvitationDestination::Friend(_) => t("Enviar mensaje"),
                    _ => t("Abrir servidor"),
                };
                actions.push(Button::new(OPEN, open, look.accent_palette()).render());
                match destination {
                    InvitationDestination::Server(server)
                        if ui.invitation_join.for_dialog(dialog).is_some()
                            && store.first_text_channel(server).is_none() =>
                    {
                        body.push(status(
                            t("Ya eres miembro. Todavía no hay canales visibles."),
                            false,
                            inner,
                            look,
                        ));
                    }
                    InvitationDestination::Friend(_) => {
                        body.push(status(t("Ya son amigos."), false, inner, look))
                    }
                    _ => {}
                }
            } else {
                let acceptance = ui.invitation_join.for_dialog(dialog);
                match acceptance.map(|active| active.phase) {
                    Some(Phase::Sending) => body.push(progress(t("Entrando…"), inner, look)),
                    Some(Phase::Verifying) => body.push(progress(
                        t("Completa la verificación en la ventana que se abrió…"),
                        inner,
                        look,
                    )),
                    Some(Phase::Failed(InvitationJoinError::ChallengeRequired)) => {
                        let service = store
                            .provider
                            .as_ref()
                            .map_or(t("El servicio"), |provider| t(&provider.name));
                        body.push(status(
                            &tr!("{service} pide una verificación para entrar.", service),
                            false,
                            inner,
                            look,
                        ));
                        body.push(
                            text(
                                t("Se abre en una ventana aparte. Cerrarla no envía nada."),
                                13.0,
                                look.muted,
                            )
                            .size(Some(inner), None),
                        );
                        actions.push(
                            Button::new(VERIFY, t("Verificar"), look.accent_palette())
                                .enabled(can_verify(ui, store, dialog, context.now))
                                .render(),
                        );
                    }
                    Some(Phase::Waiting) => body.push(progress(
                        match destination {
                            InvitationDestination::Group(_) => t("Esperando el grupo…"),
                            InvitationDestination::Friend(_) => t("Esperando la amistad…"),
                            _ => t("Esperando acceso al servidor…"),
                        },
                        inner,
                        look,
                    )),
                    Some(Phase::Unconfirmed { .. }) => body.push(status(
                        InvitationJoinError::Ambiguous.label(),
                        true,
                        inner,
                        look,
                    )),
                    // Not a refusal: once the account's events bring it,
                    // the dialog offers to open it.
                    Some(Phase::Failed(InvitationJoinError::AlreadyJoined)) => body.push(status(
                        match destination {
                            InvitationDestination::Group(_) => t("Ya estás en este grupo."),
                            InvitationDestination::Friend(_) => t("Ya son amigos."),
                            _ => t("Ya eres miembro de este servidor."),
                        },
                        false,
                        inner,
                        look,
                    )),
                    Some(Phase::Failed(error)) => {
                        body.push(status(error.label(), true, inner, look))
                    }
                    None if ui.invitation_join.busy() => body.push(status(
                        t("Hay otra solicitud de ingreso sin confirmar."),
                        false,
                        inner,
                        look,
                    )),
                    None => {}
                }
                let permit = store.accept_invitation(destination);
                if permit.allowed()
                    && acceptance.is_none_or(
                        |active| matches!(active.phase, Phase::Failed(error) if error.can_retry()),
                    )
                {
                    let label = if acceptance.is_some() {
                        t("Volver a intentar")
                    } else {
                        t("Aceptar invitación")
                    };
                    actions.push(
                        Button::new(ACCEPT, label, look.accent_palette())
                            .enabled(can_accept(ui, store, dialog, context.now))
                            .render(),
                    );
                    if ui.invitation_join.remaining(context.now).is_some() {
                        body.push(status(
                            t("Espera un momento antes de volver a intentarlo."),
                            false,
                            inner,
                            look,
                        ));
                    }
                } else if acceptance.is_none() {
                    if let Some(reason) = permit.reason() {
                        body.push(status(reason, false, inner, look));
                    }
                }
            }
        }
    }
    // A way out that never shares RUNE's session: always with an unofficial
    // provider, and after any refusal.
    let refused = ui.invitation_join.for_dialog(dialog).is_some_and(|active| {
        matches!(active.phase, Phase::Failed(error) if error != InvitationJoinError::AlreadyJoined)
    });
    let unofficial = store
        .provider
        .as_ref()
        .is_some_and(|provider| provider.kind == ProviderKind::Unofficial);
    if refused || unofficial {
        actions.push(
            Tooltip::new(
                Button::new(BROWSER, t("Abrir en el navegador"), look.menu_palette())
                    .variant(ButtonVariant::Secondary)
                    .render(),
                "invitation:browser:hint",
                t("Se abre con la cuenta de tu navegador, no con la de RUNE."),
                look.menu_palette(),
            )
            .render(),
        );
    }
    let footer = actions_footer(actions, inner, look);
    let title = match dialog
        .result
        .as_ref()
        .and_then(|result| result.as_ref().ok())
        .map(|preview| preview.destination)
    {
        Some(InvitationDestination::Server(_)) => t("Invitación a un servidor"),
        Some(InvitationDestination::Guest { .. }) => t("Invitación como invitado"),
        Some(InvitationDestination::Group(_)) => t("Invitación a un grupo"),
        Some(InvitationDestination::Friend(_)) => t("Invitación de amistad"),
        None => t("Invitación"),
    };
    Modal::new(root, true, title, "", CLOSE, look.menu_palette())
        .width(width)
        .body(Element::column(body).gap(12.0))
        .footer(footer)
        .initial_focus(CLOSE)
        .render()
}

fn input_overlay(
    dialog: &InvitationInput,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let width = Modal::fit_width(context.window.width, 480.0);
    let inner = Modal::body_width(width);
    let permit = store.invitation_preview();
    let mut body = vec![
        line(t("Enlace de invitación"), 14.0, look.text),
        TextField::new(
            INPUT,
            dialog.value.clone(),
            t("Pega el enlace completo"),
            look.menu_palette(),
        )
        .render()
        .accessibility_label(t("Enlace de invitación")),
        text(
            t("Revisar el destino no te une al servidor."),
            13.0,
            look.muted,
        )
        .size(Some(inner), None),
    ];
    if let Some(error) = permit.reason().or(dialog.error.map(t)) {
        body.push(status(error, true, inner, look));
    }
    let cancel = Button::new(INPUT_CLOSE, t("Cancelar"), look.menu_palette())
        .variant(ButtonVariant::Secondary)
        .render();
    let review = Button::new(INPUT_SUBMIT, t("Revisar invitación"), look.accent_palette())
        .enabled(permit.allowed() && !dialog.value.trim().is_empty())
        .render();
    let footer = if inner >= 360.0 {
        Element::row(vec![cancel, Element::spacer().flex(1.0), review])
    } else {
        Element::column(vec![review, cancel])
    };
    Modal::new(
        root,
        true,
        t("Revisar una invitación"),
        "",
        INPUT_CLOSE,
        look.menu_palette(),
    )
    .width(width)
    .body(Element::column(body).gap(12.0))
    .footer(footer.gap(8.0))
    .initial_focus(INPUT)
    .render()
}

fn status(label: &str, error: bool, width: f32, look: &Look) -> Element {
    text(label, 13.0, if error { look.danger } else { look.muted })
        .size(Some(width), None)
        .accessible(
            Access::new(if error { Role::Alert } else { Role::Status })
                .id(if error {
                    "invitation:error"
                } else {
                    "invitation:status"
                })
                .name(label),
        )
}

fn progress(label: &str, width: f32, look: &Look) -> Element {
    Element::row(vec![
        Spinner::new("invitation:joining", look.accent).render(),
        status(label, false, (width - 28.0).max(0.0), look).centered_y(),
    ])
    .gap(8.0)
}

/// Keep secondary actions quiet and fixed while the modal body scrolls.
fn actions_footer(mut actions: Vec<Element>, width: f32, look: &Look) -> Element {
    let copy = Button::new(COPY, t("Copiar enlace"), look.menu_palette())
        .variant(ButtonVariant::Secondary)
        .render();
    let close = Button::new(CLOSE, t("Cerrar"), look.menu_palette())
        .variant(ButtonVariant::Ghost)
        .render();
    if actions.is_empty() {
        Element::row(vec![copy.flex(1.0), close.flex(1.0)]).gap(8.0)
    } else if width >= 400.0 && actions.len() == 1 {
        actions.push(copy.flex(1.0));
        actions.push(close.flex(1.0));
        Element::row(actions.into_iter().map(|action| action.flex(1.0)).collect()).gap(8.0)
    } else {
        actions.push(Element::row(vec![copy.flex(1.0), close.flex(1.0)]).gap(8.0));
        Element::column(actions).gap(8.0)
    }
}
