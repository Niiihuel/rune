//! The screen RUNE opens on until its session is loaded: the mark and the
//! stage it is in, in the window itself. After a while, or at once when the
//! connection gave up, why and what to do about it. It fades into the
//! interface when the session is ready; «Reconectando» is left for a
//! connection lost once the session was loaded.
//!
//! The official client (static REA, 2026-10-17) covers the window with its
//! spinning mark and a tip until READY, adds a section about connection
//! problems after 10 s and fades out over 200 ms; with reduced motion its
//! mark stands still. Serein has no such screen: it shows a banner. RUNE
//! says the stage it is in instead of a tip, and offers what can be done.
//! While it only says its stage the host keeps the window as small as the
//! official client's splash ([`compact`]).

use std::time::{Duration, Instant};

use limn::{Access, Button, ButtonVariant, Component, Element, FontFamily, Icon, Role, Spinner};
use rune_components::{avatar_with_image, centered, line, text, Look};
use rune_i18n::{t, tr};
use rune_provider::{AuthenticationState, Command, ConnectionState};
use rune_state::Store;

use crate::{Intent, Ui, ViewContext};

/// How long the screen waits before it says why and offers a way out.
pub const TROUBLE_AFTER: Duration = Duration::from_secs(10);

pub mod ids {
    pub const RETRY: &str = "splash:retry";
    pub const SWITCH: &str = "splash:switch";
    pub const SIGN_OUT: &str = "splash:sign-out";
    /// The Gateway's wait between attempts, cut short: here or in the line
    /// over the conversation of a loaded session.
    pub const RETRY_NOW: &str = "connection:retry-now";
    /// A remembered account to go on with: `splash:account={id}`.
    pub const ACCOUNT: &str = "splash:account=";
    pub const ADD_ACCOUNT: &str = "splash:add-account";
    pub const SIGN_OUT_YES: &str = "splash:sign-out-yes";
    /// Back to the ways out, from the accounts or the question.
    pub const BACK: &str = "splash:back";
}

/// What the screen shows instead of its ways out once one asks first.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Asking {
    /// The remembered accounts, and adding another.
    Accounts,
    /// Whether to sign out, which forgets the account on this device.
    SignOut,
}

/// Whether the screen stands in for the interface: nothing of a session is
/// here yet (neither one ready nor what this device kept of the last one),
/// and the person is not in the middle of signing in.
pub fn visible(store: &Store) -> bool {
    store.provider.is_some()
        && store.me().is_none()
        && !matches!(
            store.authentication,
            AuthenticationState::SignedOut
                | AuthenticationState::Failed { .. }
                | AuthenticationState::AwaitingBrowser { .. }
                | AuthenticationState::AwaitingExternal
        )
}

/// After any event: when the screen started showing, or none once gone.
pub fn on_applied(ui: &mut Ui, store: &Store, now: Instant) {
    if !visible(store) {
        ui.splash_asking = None;
    }
    ui.splash_since = match ui.splash_since {
        _ if !visible(store) => None,
        Some(since) => Some(since),
        None => Some(now),
    };
}

/// When an idle host must wake for the screen to say why it is taking long.
pub fn expiry(ui: &Ui, now: Instant) -> Option<Duration> {
    let due = ui.splash_since? + TROUBLE_AFTER;
    (due > now).then(|| due - now)
}

/// The stage the session is in, as far as the provider said.
fn stage(store: &Store) -> &'static str {
    match store.authentication {
        AuthenticationState::Exchanging => t("Iniciando sesión…"),
        AuthenticationState::SignedIn { .. } => t("Cargando servidores…"),
        _ => t("Conectando…"),
    }
}

/// Why it is taking long, once it is.
fn reason(store: &Store) -> String {
    match &store.connection {
        ConnectionState::Disconnected { reason } => {
            tr!("Sin conexión: {reason}", reason = t(reason))
        }
        ConnectionState::Reconnecting {
            attempt,
            retry_in_ms,
        } => tr!(
            "Reintentando (intento {attempt}) en {seconds} s…",
            attempt,
            seconds = format!("{:.1}", *retry_in_ms as f32 / 1000.0)
        ),
        _ => t("Está tardando más de lo normal.").into(),
    }
}

/// Whether the window can stay as small as the official client's splash:
/// while the screen only says the stage it is in. Once it says why it is
/// taking long and offers ways out, or gives way to the interface or the
/// login, the window takes its usual size.
pub fn compact(ui: &Ui, store: &Store, now: Instant) -> bool {
    visible(store) && !troubled(ui, store, now)
}

/// The notice's first sentence: the whole of what the provider can do is
/// too long for a screen that shows for a moment.
fn first_sentence(notice: &str) -> &str {
    notice.find(". ").map_or(notice, |end| &notice[..=end])
}

/// Whether the screen says why and offers a way out.
fn troubled(ui: &Ui, store: &Store, now: Instant) -> bool {
    matches!(store.connection, ConnectionState::Disconnected { .. })
        || ui
            .splash_since
            .is_some_and(|since| now.saturating_duration_since(since) >= TROUBLE_AFTER)
}

pub fn view(ui: &Ui, store: &Store, look: &Look, context: &ViewContext) -> Element {
    let stage = stage(store);
    let notice = store
        .provider
        .as_ref()
        .map(|info| first_sentence(t(&info.notice)).to_owned())
        .unwrap_or_default();
    // As wide as it was, or the window less its sides while it is a splash.
    let width = (context.window.width - 32.0).clamp(0.0, 420.0);
    let mut parts = vec![
        centered(
            Element::icon(Icon::Custom(rune_components::MARK))
                .large_icon(88.0)
                .foreground(rune_components::BRAND),
        )
        .size(None, Some(96.0)),
        centered(Spinner::new("splash:progress", look.accent).render()).size(None, Some(24.0)),
        centered(
            line(stage, 16.0, look.text)
                .font_family(FontFamily::SansMedium)
                .accessible(Access::new(Role::Status).name(stage)),
        )
        .size(None, Some(24.0)),
        text(notice, 12.0, look.muted)
            .center_content()
            .max_lines(2)
            .size(Some(width), None),
    ];
    // While the account is checked, signing in can still be called off
    // (as Escape does): `auth` answers it.
    if matches!(store.authentication, AuthenticationState::Exchanging) {
        parts.push(centered(
            Button::new("auth:cancel", t("Cancelar"), look.sidebar_palette())
                .variant(ButtonVariant::Ghost)
                .render(),
        ));
    }
    let middle = Element::column(parts).gap(14.0).size(Some(width), None);
    let mut column = vec![
        Element::spacer().flex(1.0),
        centered(middle),
        Element::spacer().flex(1.0),
    ];
    if troubled(ui, store, context.now) {
        column.push(trouble(ui, store, look));
    }
    Element::panel(Element::column(column))
        .background(look.canvas)
        .size(Some(context.window.width), Some(context.window.height))
        // It fades out in place once the session is ready (limn keeps it
        // still under reduced motion).
        .animate_presence("splash")
}

/// Why it is taking long and the ways out the provider allows: try the
/// connection again now, start the provider again, go on with another
/// account, or sign out; the last two ask first.
fn trouble(ui: &Ui, store: &Store, look: &Look) -> Element {
    let reason = reason(store);
    let mut rows = vec![centered(
        text(reason.clone(), 14.0, look.muted)
            .accessible(Access::new(Role::Alert).id("splash:reason").name(reason)),
    )];
    match ui.splash_asking {
        Some(Asking::Accounts) => rows.extend(accounts(ui, store, look)),
        Some(Asking::SignOut) => rows.push(sign_out_question(look)),
        None => rows.push(ways_out(ui, store, look)),
    }
    Element::column(rows)
        .gap(14.0)
        .padding(32.0)
        .animate_enter("splash:trouble")
}

/// Inside the Gateway's wait, when the provider can: the same session, at
/// once. The session provider knows the account before its Gateway starts,
/// so its wait shows in the line over the conversation, not here.
pub fn retry_now(store: &Store, look: &Look) -> Option<Element> {
    let able = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.reconnect_now);
    (able && matches!(store.connection, ConnectionState::Reconnecting { .. })).then(|| {
        Button::new(ids::RETRY_NOW, t("Reintentar ya"), look.accent_palette())
            .variant(ButtonVariant::Default)
            .render()
    })
}

fn ways_out(ui: &Ui, store: &Store, look: &Look) -> Element {
    let capabilities = store.provider.as_ref().map(|info| info.capabilities);
    let mut buttons: Vec<Element> = retry_now(store, look).into_iter().collect();
    if ui.providers.current.is_some() {
        buttons.push(
            Button::new(ids::RETRY, t("Reintentar"), look.accent_palette())
                .variant(ButtonVariant::Default)
                .render(),
        );
    }
    if capabilities.is_some_and(|it| it.saved_accounts) {
        buttons.push(
            Button::new(ids::SWITCH, t("Cambiar de cuenta"), look.sidebar_palette())
                .variant(ButtonVariant::Secondary)
                .render(),
        );
    }
    if capabilities.is_some_and(|it| it.authentication) {
        buttons.push(
            Button::new(ids::SIGN_OUT, t("Cerrar sesión"), look.sidebar_palette())
                .variant(ButtonVariant::Secondary)
                .render(),
        );
    }
    centered(Element::row(buttons).gap(10.0)).size(None, Some(40.0))
}

/// The remembered accounts to go on with, and adding another, as the login
/// page lists them; the one loading stays remembered whichever is chosen.
fn accounts(ui: &Ui, store: &Store, look: &Look) -> Vec<Element> {
    const WIDTH: f32 = 360.0;
    let mut rows: Vec<Element> = store
        .accounts
        .iter()
        .map(|account| {
            let name = crate::auth::account_name(account);
            let label = tr!("Continuar como {name}", name);
            centered(
                Element::clickable(
                    format!("{}{}", ids::ACCOUNT, account.id.0),
                    Element::row(vec![
                        avatar_with_image(
                            name,
                            look.accent,
                            32.0,
                            ui.picture(account.avatar.as_ref()),
                        )
                        .centered_y(),
                        line(label.clone(), 15.0, look.text).flex(1.0).centered_y(),
                    ])
                    .gap(12.0)
                    .padding(8.0),
                )
                .size(Some(WIDTH), None)
                .background(look.raised)
                .radius(8.0)
                .hover_background(look.hover)
                .accessible(Access::new(Role::Button).name(label)),
            )
        })
        .collect();
    rows.push(centered(
        Element::row(vec![
            Button::new(ids::BACK, t("Volver"), look.sidebar_palette())
                .variant(ButtonVariant::Ghost)
                .render(),
            Button::new(
                ids::ADD_ACCOUNT,
                t("Agregar cuenta"),
                look.sidebar_palette(),
            )
            .variant(ButtonVariant::Secondary)
            .render(),
        ])
        .gap(10.0),
    ));
    rows
}

/// Signing out forgets the account on this device, even while its saved
/// session is still being checked: asked first.
fn sign_out_question(look: &Look) -> Element {
    let question = t("¿Cerrar sesión? RUNE olvida esta cuenta en este equipo y vas a tener que volver a iniciar sesión con ella.");
    centered(
        Element::column(vec![
            text(question, 14.0, look.text).size(Some(396.0), None),
            Element::row(vec![
                Button::new(ids::BACK, t("Cancelar"), look.sidebar_palette())
                    .variant(ButtonVariant::Secondary)
                    .render(),
                Button::new(ids::SIGN_OUT_YES, t("Cerrar sesión"), look.palette())
                    .variant(ButtonVariant::Destructive)
                    .render(),
            ])
            .gap(8.0),
        ])
        .gap(10.0)
        .padding(12.0)
        .background(look.raised)
        .radius(8.0)
        .accessible(
            Access::new(Role::Alert)
                .id("splash:sign-out-question")
                .name(question),
        ),
    )
}

/// A click on the screen: its buttons, while it shows; «Reintentar ya»
/// wherever it shows.
pub fn update(store: &Store, ui: &mut Ui, action: &limn::UiAction) -> Option<Vec<Intent>> {
    let limn::UiAction::Clicked { id } = action else {
        return None;
    };
    if id == ids::RETRY_NOW {
        return Some(vec![Intent::Command(Command::ReconnectNow)]);
    }
    if !visible(store) {
        return None;
    }
    // Another account: the one loading stays remembered.
    if let Some(account) = crate::auth::remembered(store, id, ids::ACCOUNT) {
        ui.splash_asking = None;
        return Some(vec![Intent::Command(Command::UseAccount(Some(account)))]);
    }
    let (asking, intents) = match id.as_str() {
        // The provider started again, as from the beginning: what the
        // official client's reload does.
        ids::RETRY => (None, vec![Intent::UseProvider(ui.providers.current?)]),
        // The remembered accounts first, «Agregar cuenta» among them.
        ids::SWITCH => {
            let first = store
                .accounts
                .first()
                .map_or(ids::ADD_ACCOUNT.into(), |account| {
                    format!("{}{}", ids::ACCOUNT, account.id.0)
                });
            (Some(Asking::Accounts), vec![Intent::Focus(first)])
        }
        ids::ADD_ACCOUNT => (None, vec![Intent::Command(Command::UseAccount(None))]),
        ids::SIGN_OUT => (Some(Asking::SignOut), vec![Intent::Focus(ids::BACK.into())]),
        ids::SIGN_OUT_YES => (None, vec![Intent::Command(Command::SignOut)]),
        ids::BACK => (None, Vec::new()),
        _ => return None,
    };
    ui.splash_asking = asking;
    Some(intents)
}
