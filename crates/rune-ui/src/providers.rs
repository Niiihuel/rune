//! Choosing the provider from the interface. The host lists what it can
//! start ([`ProviderOption`]); the interface shows each one's origin, warns
//! before an unofficial one, asks before leaving the current one and asks
//! the host with [`Intent::UseProvider`]. It names no service: names and
//! notices come from the host. Its actions start with `provider:`.

use limn::{
    Access, Button, ButtonVariant, Component, ControlSize, Element, RadioGroup, RadioOption,
    RadioVariant, Role, UiAction,
};
use rune_components::user_settings::setting;
use rune_components::{line, text, Look};
use rune_i18n::{es, t, tr};
use rune_provider::{ProviderInfo, ProviderKind};
use rune_state::Store;

use crate::{Intent, Ui};

/// A provider the host can start, as the interface offers it.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderOption {
    /// What the host knows it by; [`Intent::UseProvider`] names it.
    pub key: &'static str,
    pub name: String,
    pub kind: ProviderKind,
    /// One sentence on what it is and is not.
    pub notice: String,
    /// Why it cannot be started here, when it cannot.
    pub unavailable: Option<String>,
}

/// What the interface knows of the providers.
#[derive(Clone, Debug, Default)]
pub struct Providers {
    /// What the host can start, in the order it lists them.
    pub options: Vec<ProviderOption>,
    /// The one in use.
    pub current: Option<&'static str>,
    /// The list is open on the sign-in page.
    pub open: bool,
    /// The one chosen, waiting for the person to confirm leaving the current.
    pub confirming: Option<&'static str>,
}

/// The warning every unofficial provider carries, before it is chosen and
/// while it is in use. Spanish: translated with `t` where shown.
pub const UNOFFICIAL: &str = es("Experimento personal no oficial: el servicio no lo aprobó, puede dejar de funcionar y usarlo puede tener consecuencias para tu cuenta.");

const USE: &str = "provider:use=";
const CONFIRM: &str = "provider:confirm";
const CANCEL: &str = "provider:cancel";
const QUESTION: &str = "provider:question";
/// Opens and closes the list on the sign-in page.
pub(crate) const LIST: &str = "provider:list";

/// How a kind of provider says where its data comes from.
pub fn kind_label(kind: ProviderKind) -> &'static str {
    match kind {
        ProviderKind::Simulated => t("sin conexión real"),
        ProviderKind::Official => t("oficial"),
        ProviderKind::Unofficial => t("no oficial"),
    }
}

/// The origin of `info`, as the foot of the column and the sign-in page say it.
pub fn origin(info: &ProviderInfo) -> String {
    format!("{} · {}", t(&info.name), kind_label(info.kind))
}

/// Clicks on the provider's choice; `None` for the rest of the window.
pub fn update(ui: &mut Ui, action: &UiAction) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    let providers = &mut ui.providers;
    if let Some(key) = id.strip_prefix(USE) {
        let chosen = providers
            .options
            .iter()
            .find(|option| option.key == key && option.unavailable.is_none())
            .map(|option| option.key);
        providers.confirming = chosen.filter(|key| Some(*key) != providers.current);
        // The question may open below what is in view.
        return Some(
            providers
                .confirming
                .map(|_| Intent::Reveal {
                    id: QUESTION.into(),
                    margin: 120.0,
                })
                .into_iter()
                .collect(),
        );
    }
    match id.as_str() {
        LIST => {
            providers.open = !providers.open;
            providers.confirming = None;
            Some(Vec::new())
        }
        CANCEL => {
            providers.confirming = None;
            Some(Vec::new())
        }
        CONFIRM => Some(
            providers
                .confirming
                .take()
                .map(|key| vec![Intent::UseProvider(key)])
                .unwrap_or_default(),
        ),
        _ => None,
    }
}

/// The host started the provider `key` in place of the one before: what
/// belonged to that one goes, as when the account changes; what the person
/// set for the window stays.
pub fn provider_changed(ui: &mut Ui, key: &'static str) -> Vec<Intent> {
    let mut intents = crate::search::dismiss(ui);
    intents.extend(crate::context::cancel(ui));
    intents.extend(crate::call_sounds::stop_choosing(ui));
    *ui = crate::auth::fresh(ui, false);
    ui.providers.current = Some(key);
    intents.push(Intent::StopMeter);
    intents
}

/// The providers to choose from, each with its origin, what it is and why it
/// cannot be chosen, and the question before leaving the current one.
pub(crate) fn choices(ui: &Ui, width: f32, look: &Look) -> Vec<Element> {
    let providers = &ui.providers;
    let mut rows = Vec::new();
    for option in &providers.options {
        let current = providers.current == Some(option.key);
        let mut copy = vec![
            line(
                format!("{} · {}", t(&option.name), kind_label(option.kind)),
                15.0,
                look.text,
            ),
            text(t(&option.notice), 13.0, look.muted).size(Some(width - 140.0), None),
        ];
        if option.kind == ProviderKind::Unofficial {
            copy.push(text(t(UNOFFICIAL), 13.0, look.idle).size(Some(width - 140.0), None));
        }
        if let Some(reason) = &option.unavailable {
            copy.push(text(t(reason), 13.0, look.danger).size(Some(width - 140.0), None));
        }
        let action = if current {
            line(t("En uso"), 13.0, look.muted)
        } else {
            Button::new(
                format!("{USE}{}", option.key),
                t("Usar"),
                look.sidebar_palette(),
            )
            .variant(ButtonVariant::Secondary)
            .enabled(option.unavailable.is_none())
            .render()
        };
        rows.push(
            Element::row(vec![
                Element::column(copy).gap(4.0).flex(1.0),
                action.centered_y(),
            ])
            .gap(12.0)
            .padding(12.0)
            .size(Some(width), None)
            .background(look.raised)
            .radius(8.0),
        );
        if providers.confirming == Some(option.key) {
            rows.push(question(option, width, look));
        }
    }
    rows
}

/// The question before leaving the provider in use for `option`.
fn question(option: &ProviderOption, width: f32, look: &Look) -> Element {
    let question = tr!(
        "¿Cambiar a {name}? Se cierra la sesión actual y se borran de la ventana sus conversaciones, borradores, búsquedas, visores y trabajos en curso. Las cuentas recordadas y tus preferencias se conservan.",
        name = t(&option.name)
    );
    Element::column(vec![
        text(question.clone(), 14.0, look.text).size(Some(width - 24.0), None),
        Element::row(vec![
            Button::new(CANCEL, t("Cancelar"), look.sidebar_palette())
                .variant(ButtonVariant::Secondary)
                .render(),
            Button::new(CONFIRM, t("Cambiar"), look.accent_palette()).render(),
        ])
        .gap(8.0),
    ])
    .gap(10.0)
    .padding(12.0)
    .background(look.raised)
    .radius(8.0)
    .accessible(Access::new(Role::Alert).id(QUESTION).name(question))
    .anchor(QUESTION)
}

/// The providers as cards of one choice, for Settings › Conexiones: the one
/// in use is selected, one that cannot start is disabled, and a change asks
/// first as in [`choices`]. The unofficial one's warning sits under them.
pub(crate) fn cards(ui: &Ui, store: &Store, width: f32, look: &Look) -> Vec<Element> {
    let providers = &ui.providers;
    let options = providers
        .options
        .iter()
        .map(|option| {
            let note = if option.unavailable.is_some() {
                t("No disponible ahora. El motivo está en Avanzado.")
            } else {
                t(&option.notice)
            };
            RadioOption::new(
                format!("{USE}{}", option.key),
                format!("{} · {}", t(&option.name), kind_label(option.kind)),
            )
            .description(note)
            .enabled(option.unavailable.is_none())
        })
        .collect();
    let current = providers.current.map(|key| format!("{USE}{key}"));
    let mut rows = vec![RadioGroup::new(
        "provider:choose",
        options,
        current.as_deref(),
        look.palette(),
    )
    .variant(RadioVariant::Card)
    .size(ControlSize::Spacious)
    .accessibility_label(t("Cambiar de proveedor"))
    .render()];
    let confirming = providers
        .confirming
        .and_then(|key| providers.options.iter().find(|option| option.key == key));
    if let Some(option) = confirming {
        rows.push(question(option, width, look));
    }
    let in_use = store
        .provider
        .as_ref()
        .is_some_and(|info| info.kind == ProviderKind::Unofficial);
    let chosen = confirming.is_some_and(|option| option.kind == ProviderKind::Unofficial);
    if in_use || chosen {
        rows.push(warning(t(UNOFFICIAL), look));
    }
    rows
}

/// A note that warns, in the warning tone of the provider's notices.
fn warning(message: &str, look: &Look) -> Element {
    Element::panel(
        Element::row(vec![text(message, 14.0, look.idle).flex(1.0).centered_y()]).padding(12.0),
    )
    .background(Look::alpha(look.idle, 14))
    .border(Look::alpha(look.idle, 150), 1.0)
    .radius(8.0)
    .accessible(Access::new(Role::Status).name(message.to_owned()))
}

/// The origin of a provider, as a badge beside its name.
pub(crate) fn badge(kind: ProviderKind, look: &Look) -> Element {
    let (label, ink) = match kind {
        ProviderKind::Simulated => (t("Simulación"), look.muted),
        ProviderKind::Official => (t("Oficial"), look.accent),
        ProviderKind::Unofficial => (t("No oficial"), look.idle),
    };
    Element::row(vec![line(label, 12.0, ink)])
        .padding(6.0)
        .background(Look::alpha(ink, 30))
        .radius(4.0)
}

/// What the host says the provider in use is, in one sentence.
pub(crate) fn current_notice(ui: &Ui) -> Option<&str> {
    let current = ui.providers.current?;
    ui.providers
        .options
        .iter()
        .find(|option| option.key == current)
        .map(|option| option.notice.as_str())
}

/// Why the providers that cannot start here cannot, for Avanzado: the
/// developer's way to fix it, kept off the connection page.
pub(crate) fn unavailable_notes(ui: &Ui, look: &Look) -> Option<Element> {
    let rows: Vec<Element> = ui
        .providers
        .options
        .iter()
        .filter_map(|option| {
            let reason = option.unavailable.as_deref()?;
            Some(setting(t(&option.name), Some(t(reason)), None, true, look))
        })
        .collect();
    (!rows.is_empty()).then(|| Element::column(rows))
}

/// Whether the host offers more than the provider in use.
pub(crate) fn choosable(ui: &Ui) -> bool {
    ui.providers.options.len() > 1
}

/// The origin line of the store's provider, if any.
/// The foot of the channel column: only simulated data says where it comes
/// from (AGENTS.md); a real account's column ends at its channels (the
/// owner's choice, 2026-10-19). The sign-in page and Settings › Connection
/// still name every provider's origin.
pub(crate) fn origin_of(store: &Store) -> Option<String> {
    store
        .provider
        .as_ref()
        .filter(|info| info.kind == ProviderKind::Simulated)
        .map(origin)
}
