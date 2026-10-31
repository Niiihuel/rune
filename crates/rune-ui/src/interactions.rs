//! Apps in the open conversation (P3): their buttons and menus in
//! messages, the modal one opens, and the replies only the account sees.
//! What may go is the store's to say ([`Store::interaction_abilities`],
//! [`Store::begin_interaction`]); this keeps what is being picked or written.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use limn::{Element, Modal, UiAction};
use rune_components::interactions::{modal_dialog, modal_ids, ModalView};
use rune_components::message::{AppParts, OpenMenu};
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::components::{Component, MenuKind, Select};
use rune_model::interactions::{FieldValue, Interaction, ModalInput};
use rune_model::{ChannelId, ChannelKind, MessageId, PendingAttachment, UploadToken};
use rune_provider::UploadError;
use rune_state::Store;

use crate::{Intent, Ui, ViewContext};

/// People, roles or channels a menu offers at once, as the service allows
/// options in a menu.
const OFFERED: usize = 25;

/// What the interface keeps of apps while they are used.
#[derive(Debug, Default)]
pub struct Apps {
    /// The menu of an app's message open, and the message.
    menu: Option<(MessageId, OpenMenu)>,
    /// The modal being filled.
    form: Option<ModalForm>,
    /// The command being run over the composer.
    pub(crate) command: Option<crate::commands::Running>,
    /// What `/msg` writes to someone once their conversation opens.
    pub(crate) waiting_direct: Option<(rune_model::UserId, String)>,
    /// The person menu whose conversation's commands were asked for: a menu asks once.
    pub(crate) menu_asked: Option<(rune_model::UserId, crate::person_menu::Place)>,
}

#[cfg(test)]
impl Apps {
    /// What the menu open offers now.
    pub(crate) fn menu_options(&self) -> Vec<(String, String)> {
        self.menu
            .as_ref()
            .map(|(_, open)| open.options.clone())
            .unwrap_or_default()
    }
}

/// A modal being filled.
#[derive(Debug, Default)]
struct ModalForm {
    id: u64,
    values: HashMap<String, FieldValue>,
    tried: bool,
    menu: Option<OpenMenu>,
    /// Where it is open, and the files chosen for its file fields, which
    /// name them by token (comandos-de-apps).
    channel: ChannelId,
    files: Vec<PendingAttachment>,
}

/// What the rows of the timeline need to draw apps' parts.
pub(crate) struct AppRows {
    denied: Option<String>,
    busy: bool,
    waiting: Option<(MessageId, String, bool)>,
    problem: Option<(MessageId, String, String)>,
    menu: Option<(MessageId, OpenMenu)>,
}

impl AppRows {
    /// The parts of `message`.
    pub(crate) fn parts(&self, message: MessageId) -> AppParts<'_> {
        AppParts {
            denied: self.denied.as_deref(),
            busy: self.busy,
            waiting: self
                .waiting
                .as_ref()
                .filter(|(at, ..)| *at == message)
                .map(|(_, custom_id, received)| (custom_id.as_str(), *received)),
            problem: self
                .problem
                .as_ref()
                .filter(|(at, ..)| *at == message)
                .map(|(_, custom_id, reason)| (custom_id.as_str(), reason.as_str())),
            menu: self
                .menu
                .as_ref()
                .filter(|(at, _)| *at == message)
                .map(|(_, open)| open),
        }
    }
}

/// What apps' parts of `channel` may do now.
pub(crate) fn rows(ui: &Ui, store: &Store, channel: ChannelId) -> AppRows {
    let now = Instant::now();
    let pending = store
        .interaction_pending(now)
        .filter(|pending| pending.channel == channel);
    AppRows {
        denied: store
            .interaction_abilities(channel)
            .parts
            .reason()
            .map(str::to_owned),
        busy: store.interaction_pending(now).is_some(),
        waiting: pending.and_then(|pending| {
            let (message, custom_id) = pending.part.clone()?;
            Some((message, custom_id, pending.received))
        }),
        problem: store
            .interaction_problem(now)
            .filter(|problem| problem.channel == channel)
            .and_then(|problem| {
                let (message, custom_id) = problem.part?;
                Some((message, custom_id, problem.reason))
            }),
        menu: ui.apps.menu.clone(),
    }
}

/// The people the menus of `message`'s app start with picked.
pub(crate) fn menu_people(message: &rune_model::Message) -> Vec<rune_model::UserId> {
    fn walk(parts: &[Component], found: &mut Vec<rune_model::UserId>) {
        for part in parts {
            match part {
                Component::Select(menu) => {
                    found.extend(menu.defaults.iter().filter_map(|picked| match picked {
                        rune_model::components::Picked::User(user) => Some(*user),
                        _ => None,
                    }))
                }
                Component::Row(parts) | Component::Container { parts, .. } => walk(parts, found),
                Component::Section { parts, accessory } => {
                    walk(parts, found);
                    walk(std::slice::from_ref(accessory), found);
                }
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    if message.components.app.is_some() {
        walk(&message.components.parts, &mut found);
    }
    found
}

/// When the interaction in flight stops being waited for, so an idle host
/// wakes then to say so.
pub fn interaction_tick(store: &Store, now: Instant) -> Option<Duration> {
    store.interaction_expires(now)
}

/// Tell an app what the account did, or say why it cannot.
pub(crate) fn begin(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    interaction: Interaction,
    now: Instant,
) -> Vec<Intent> {
    let request = ui.fresh_request();
    let files = interaction.files();
    match store.begin_interaction(request, channel, interaction, now) {
        // A modal's files join it away from the interface.
        Ok(command) if !files.is_empty() => vec![Intent::InteractWithFiles { command, files }],
        Ok(command) => vec![Intent::Command(command)],
        Err(reason) => {
            ui.notify(t("No se pudo usar la aplicación"), Some(reason), true, now);
            Vec::new()
        }
    }
}

/// Answer what apps' parts, their modal and replies report.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    match action {
        UiAction::Clicked { id } if id.starts_with("modal:") => modal_clicked(ui, store, id, now),
        UiAction::InputChanged { id, value } if id.starts_with("modal:") => {
            modal_input(ui, store, id, value)
        }
        UiAction::Clicked { id } => {
            let rest = id.strip_prefix("msg:")?;
            let (message, what) = rest.split_once(':')?;
            let message = MessageId(message.parse().ok()?);
            let channel = ui.channel()?;
            if what == "dismiss" {
                store.dismiss_ephemeral(channel, message);
                return Some(Vec::new());
            }
            if let Some(custom_id) = what.strip_prefix("app:") {
                return Some(part(ui, store, channel, message, custom_id, now));
            }
            if let Some(rest) = what.strip_prefix("app-pick:") {
                let (index, custom_id) = rest.split_once(':')?;
                return Some(pick(
                    ui,
                    store,
                    channel,
                    message,
                    index.parse().ok()?,
                    custom_id,
                    now,
                ));
            }
            let custom_id = what.strip_prefix("app-close:")?;
            Some(close_menu(ui, store, channel, message, custom_id, now))
        }
        UiAction::InputChanged { id, value } => {
            let rest = id.strip_prefix("msg:")?;
            let (message, what) = rest.split_once(':')?;
            what.strip_prefix("app-query:")?;
            let message = MessageId(message.parse().ok()?);
            let channel = ui.channel()?;
            let menu = store
                .timeline(channel)
                .and_then(|timeline| timeline.get(message))
                .and_then(|found| {
                    let (_, open) = ui.apps.menu.as_ref()?;
                    match rune_model::components::control(&found.components.parts, &open.custom_id)
                    {
                        Some(Component::Select(menu)) => Some(menu.clone()),
                        _ => None,
                    }
                })?;
            let (_, open) = ui.apps.menu.as_mut()?;
            open.query = value.chars().take(100).collect();
            open.options = candidates(store, channel, &menu, &open.query);
            Some(Vec::new())
        }
        _ => None,
    }
}

/// The menu of `message` named `custom_id`, as the timeline has it.
fn menu_of(
    store: &Store,
    channel: ChannelId,
    message: MessageId,
    custom_id: &str,
) -> Option<Select> {
    let found = store.timeline(channel)?.get(message)?;
    match rune_model::components::control(&found.components.parts, custom_id)? {
        Component::Select(menu) => Some(menu.clone()),
        _ => None,
    }
}

/// A button pressed goes to its app; a menu opens or closes.
fn part(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    message: MessageId,
    custom_id: &str,
    now: Instant,
) -> Vec<Intent> {
    let Some(found) = store
        .timeline(channel)
        .and_then(|timeline| timeline.get(message))
        .cloned()
    else {
        return Vec::new();
    };
    match rune_model::components::control(&found.components.parts, custom_id) {
        Some(Component::Button(_)) => {
            ui.apps.menu = None;
            let press = Interaction::Part {
                message,
                custom_id: custom_id.to_owned(),
                values: Vec::new(),
            };
            begin(ui, store, channel, press, now)
        }
        Some(Component::Select(menu)) => {
            let open = ui
                .apps
                .menu
                .as_ref()
                .is_some_and(|(at, open)| *at == message && open.custom_id == custom_id);
            if open {
                return close_menu(ui, store, channel, message, custom_id, now);
            }
            if !store.interaction_abilities(channel).parts.allowed() {
                return Vec::new();
            }
            ui.apps.menu = Some((
                message,
                OpenMenu {
                    custom_id: custom_id.to_owned(),
                    picked: menu.starting(),
                    query: String::new(),
                    options: candidates(store, channel, menu, ""),
                },
            ));
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// The `index`th option of the open menu, picked: one alone goes at once;
/// with several, it is marked or unmarked until the menu closes.
fn pick(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    message: MessageId,
    index: usize,
    custom_id: &str,
    now: Instant,
) -> Vec<Intent> {
    let Some(menu) = menu_of(store, channel, message, custom_id) else {
        return Vec::new();
    };
    let Some((at, open)) = ui.apps.menu.as_mut() else {
        return Vec::new();
    };
    if *at != message || open.custom_id != custom_id {
        return Vec::new();
    }
    let value = match menu.kind {
        MenuKind::Text => menu.options.get(index).map(|option| option.value.clone()),
        _ => open.options.get(index).map(|(value, _)| value.clone()),
    };
    let Some(value) = value else {
        return Vec::new();
    };
    if menu.max <= 1 {
        ui.apps.menu = None;
        let picked = Interaction::Part {
            message,
            custom_id: custom_id.to_owned(),
            values: vec![value],
        };
        return begin(ui, store, channel, picked, now);
    }
    if let Some(found) = open.picked.iter().position(|kept| *kept == value) {
        open.picked.remove(found);
    } else if open.picked.len() < usize::from(menu.max) {
        open.picked.push(value);
    }
    Vec::new()
}

/// The open menu closes; what several picks changed goes to its app.
fn close_menu(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    message: MessageId,
    custom_id: &str,
    now: Instant,
) -> Vec<Intent> {
    let Some((at, open)) = ui.apps.menu.take() else {
        return Vec::new();
    };
    if at != message || open.custom_id != custom_id {
        return Vec::new();
    }
    let Some(menu) = menu_of(store, channel, message, custom_id) else {
        return Vec::new();
    };
    if menu.max <= 1 || open.picked == menu.starting() || !menu.accepts(&open.picked) {
        return Vec::new();
    }
    let picked = Interaction::Part {
        message,
        custom_id: custom_id.to_owned(),
        values: open.picked,
    };
    begin(ui, store, channel, picked, now)
}

/// What a menu of people, roles or channels offers in `channel` for
/// `query`: value and label; a text menu offers its own options.
pub(crate) fn candidates(
    store: &Store,
    channel: ChannelId,
    menu: &Select,
    query: &str,
) -> Vec<(String, String)> {
    let query = query.trim().to_lowercase();
    let matches = |name: &str| query.is_empty() || name.to_lowercase().contains(&query);
    let server = store.channel(channel).and_then(|found| found.server);
    let people = || -> Vec<(String, String)> {
        let users: Vec<_> = match server {
            Some(server) => store
                .member_groups(server)
                .into_iter()
                .flat_map(|group| group.members)
                .collect(),
            None => store.direct_members(channel),
        };
        users
            .into_iter()
            .filter_map(|user| {
                let found = store.user(user)?;
                let shown = store.display_name(user, server);
                (matches(&shown) || matches(&found.name))
                    .then(|| (user.0.to_string(), format!("@{shown}")))
            })
            .collect()
    };
    let roles = || -> Vec<(String, String)> {
        server
            .and_then(|server| store.server(server))
            .map(|server| {
                server
                    .roles
                    .iter()
                    .filter(|role| role.id != server.everyone_id() && matches(&role.name))
                    .map(|role| (role.id.0.to_string(), format!("@{}", role.name)))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut found = match menu.kind {
        MenuKind::Text => menu
            .options
            .iter()
            .map(|option| (option.value.clone(), option.label.clone()))
            .collect(),
        MenuKind::User => people(),
        MenuKind::Role => roles(),
        MenuKind::Mentionable => {
            let mut both = people();
            both.extend(roles());
            both
        }
        MenuKind::Channel => server
            .map(|server| {
                store
                    .channel_tree(server)
                    .into_iter()
                    .flat_map(|(_, channels)| channels)
                    .filter(|found| {
                        if menu.channels.is_empty() {
                            found.kind != ChannelKind::Category
                        } else {
                            menu.channels.contains(&found.kind)
                        }
                    })
                    .filter(|found| matches(&found.name))
                    .map(|found| (found.id.0.to_string(), format!("#{}", found.name)))
                    .collect()
            })
            .unwrap_or_default(),
    };
    found.truncate(OFFERED);
    found
}

/// The form of the modal open, made current: a new modal starts from its
/// fields' values.
fn form<'a>(ui: &'a mut Ui, store: &Store) -> Option<&'a mut ModalForm> {
    let Some((channel, modal)) = store.modal() else {
        ui.apps.form = None;
        return None;
    };
    if ui.apps.form.as_ref().is_none_or(|form| form.id != modal.id) {
        ui.apps.form = Some(ModalForm {
            id: modal.id,
            channel,
            files: Vec::new(),
            values: modal
                .fields
                .iter()
                .filter_map(|field| {
                    Some((field.input.custom_id()?.to_owned(), field.input.starting()))
                })
                .collect(),
            tried: false,
            menu: None,
        });
    }
    ui.apps.form.as_mut()
}

/// The files chosen for the modal open, by any of its fields.
pub(crate) fn modal_files<'a>(ui: &'a Ui, store: &Store) -> &'a [PendingAttachment] {
    let open = store.modal().map(|(_, modal)| modal.id);
    ui.apps
        .form
        .as_ref()
        .filter(|form| Some(form.id) == open)
        .map_or(&[], |form| form.files.as_slice())
}

/// `files`, chosen for the field `custom_id` of the modal open: added to
/// it, unless the modal closed, the field takes no more or not their kind.
pub(crate) fn files_chosen(
    ui: &mut Ui,
    store: &Store,
    custom_id: &str,
    files: Vec<PendingAttachment>,
) -> Result<(), UploadError> {
    let (_, modal) = store.modal().ok_or(UploadError::Cancelled)?;
    let (input, max) = modal
        .fields
        .iter()
        .find_map(|field| match &field.input {
            ModalInput::Files {
                custom_id: id, max, ..
            } if id == custom_id => Some((&field.input, *max)),
            _ => None,
        })
        .ok_or(UploadError::Cancelled)?;
    if let Some(refused) = files.iter().find(|file| !input.accepts(&file.filename)) {
        ui.notify(
            tr!(
                "{name} no es de un tipo que acepte este campo",
                name = refused.filename
            ),
            None,
            true,
            Instant::now(),
        );
        // Said above.
        return Err(UploadError::Cancelled);
    }
    let form = ui
        .apps
        .form
        .as_mut()
        .filter(|form| form.id == modal.id)
        .ok_or(UploadError::Cancelled)?;
    let Some(FieldValue::Files(named)) = form.values.get_mut(custom_id) else {
        return Err(UploadError::Cancelled);
    };
    if named.len() + files.len() > usize::from(max) {
        return Err(UploadError::TooMany);
    }
    named.extend(files.iter().map(|file| file.token));
    form.files.extend(files);
    Ok(())
}

/// Let go of what the modal `form` held: its files, and a choice of them
/// under way.
fn release(ui: &mut Ui, form: ModalForm) -> Vec<Intent> {
    let mut intents: Vec<Intent> = crate::uploads::cancel_modal_pick(ui).into_iter().collect();
    intents.extend(form.files.into_iter().map(|file| Intent::RemoveAttachment {
        channel: form.channel,
        token: file.token,
    }));
    intents
}

/// A modal that closed, answered or not, lets go of its files: the ones
/// its app got are gone already, and letting them go again does nothing.
pub(crate) fn on_applied(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let open = store.modal().map(|(_, modal)| modal.id);
    match ui.apps.form.take_if(|form| Some(form.id) != open) {
        Some(form) => release(ui, form),
        None => Vec::new(),
    }
}

fn modal_input(ui: &mut Ui, store: &Store, id: &str, value: &str) -> Option<Vec<Intent>> {
    let channel = store.modal()?.0;
    let (_, modal) = store.modal()?;
    let form = form(ui, store)?;
    if let Some(custom_id) = id.strip_prefix("modal:text:") {
        form.values.insert(
            custom_id.to_owned(),
            FieldValue::Text(value.chars().take(4000).collect()),
        );
        return Some(Vec::new());
    }
    let custom_id = id.strip_prefix("modal:query:")?;
    let menu = modal.fields.iter().find_map(|field| match &field.input {
        ModalInput::Menu(menu) if menu.custom_id.as_deref() == Some(custom_id) => Some(menu),
        _ => None,
    })?;
    let open = form.menu.as_mut()?;
    open.query = value.chars().take(100).collect();
    open.options = candidates(store, channel, menu, &open.query);
    Some(Vec::new())
}

fn modal_clicked(ui: &mut Ui, store: &mut Store, id: &str, now: Instant) -> Option<Vec<Intent>> {
    if id == modal_ids::CLOSE {
        // Files on their way reach the app anyway: it is waited for.
        let uploading = store
            .interaction_pending(now)
            .is_some_and(|pending| pending.modal)
            && ui
                .apps
                .form
                .as_ref()
                .is_some_and(|form| !form.files.is_empty());
        if uploading {
            ui.notify(
                t("Los archivos ya se están enviando"),
                Some(t("Espera la respuesta de la aplicación.").into()),
                true,
                now,
            );
            return Some(Vec::new());
        }
        store.close_modal();
        return Some(match ui.apps.form.take() {
            Some(form) => release(ui, form),
            None => Vec::new(),
        });
    }
    let (channel, modal) = store
        .modal()
        .map(|(channel, modal)| (channel, modal.clone()))?;
    form(ui, store)?;
    if let Some(custom_id) = id.strip_prefix("modal:files:") {
        let takes_files = modal.fields.iter().any(|field| {
            matches!(&field.input, ModalInput::Files { custom_id: id, .. } if id == custom_id)
        });
        if !takes_files || store.interaction_pending(now).is_some() {
            return Some(Vec::new());
        }
        return Some(
            match crate::uploads::begin_modal_import(ui, store, channel, custom_id) {
                Ok(request) => vec![Intent::PickAttachment { request, channel }],
                Err(failure) => {
                    crate::uploads::error(ui, failure);
                    Vec::new()
                }
            },
        );
    }
    // Files still being chosen would miss the modal.
    let picking = crate::uploads::modal_picking(ui).is_some();
    let sending = store.interaction_pending(now).is_some();
    let form = form(ui, store)?;
    if let Some(rest) = id.strip_prefix("modal:unfile:") {
        let (token, custom_id) = rest.split_once(':')?;
        let token = UploadToken(token.parse().ok()?);
        if sending {
            return Some(Vec::new());
        }
        if let Some(FieldValue::Files(named)) = form.values.get_mut(custom_id) {
            named.retain(|held| *held != token);
        }
        let before = form.files.len();
        form.files.retain(|file| file.token != token);
        return Some(if form.files.len() < before {
            vec![Intent::RemoveAttachment { channel, token }]
        } else {
            Vec::new()
        });
    }
    if id == modal_ids::SEND {
        form.tried = true;
        form.menu = None;
        let blocked = modal.fields.iter().any(|field| {
            let value = field
                .input
                .custom_id()
                .and_then(|custom_id| form.values.get(custom_id).cloned())
                .unwrap_or_else(|| field.input.starting());
            field.input.problem(&value).is_some()
        });
        if blocked || picking {
            return Some(Vec::new());
        }
        let answers = modal
            .fields
            .iter()
            .filter_map(|field| {
                let custom_id = field.input.custom_id()?;
                let value = form.values.get(custom_id).cloned()?;
                Some((custom_id.to_owned(), value))
            })
            .collect();
        let sent = Interaction::Modal {
            id: modal.id,
            app: modal.app,
            custom_id: modal.custom_id.clone(),
            answers,
        };
        return Some(begin(ui, store, channel, sent, now));
    }
    let field_of = |custom_id: &str| {
        modal
            .fields
            .iter()
            .find(|field| field.input.custom_id() == Some(custom_id))
            .map(|field| field.input.clone())
    };
    if let Some(custom_id) = id.strip_prefix("modal:check:") {
        if let Some(FieldValue::Check(on)) = form.values.get_mut(custom_id) {
            *on = !*on;
        }
        return Some(Vec::new());
    }
    if let Some(custom_id) = id.strip_prefix("modal:menu-close:") {
        form.menu.take_if(|open| open.custom_id == custom_id);
        return Some(Vec::new());
    }
    if let Some(custom_id) = id.strip_prefix("modal:menu:") {
        if form
            .menu
            .take_if(|open| open.custom_id == custom_id)
            .is_some()
        {
            return Some(Vec::new());
        }
        let Some(ModalInput::Menu(menu)) = field_of(custom_id) else {
            return Some(Vec::new());
        };
        form.menu = Some(OpenMenu {
            custom_id: custom_id.to_owned(),
            picked: Vec::new(),
            query: String::new(),
            options: candidates(store, channel, &menu, ""),
        });
        return Some(Vec::new());
    }
    let rest = id.strip_prefix("modal:pick:")?;
    let (index, custom_id) = rest.split_once(':')?;
    let index: usize = index.parse().ok()?;
    let picked = match form.values.get_mut(custom_id)? {
        FieldValue::Values(picked) => picked,
        _ => return Some(Vec::new()),
    };
    match field_of(custom_id)? {
        ModalInput::Menu(menu) => {
            let value = match menu.kind {
                MenuKind::Text => menu.options.get(index).map(|option| option.value.clone()),
                _ => form
                    .menu
                    .as_ref()
                    .and_then(|open| open.options.get(index))
                    .map(|(value, _)| value.clone()),
            }?;
            if menu.max <= 1 {
                *picked = vec![value];
                form.menu = None;
            } else if let Some(found) = picked.iter().position(|kept| *kept == value) {
                picked.remove(found);
            } else if picked.len() < usize::from(menu.max) {
                picked.push(value);
            }
        }
        ModalInput::Radio { options, .. } => {
            *picked = vec![options.get(index)?.value.clone()];
        }
        ModalInput::Checks { options, max, .. } => {
            let value = options.get(index)?.value.clone();
            if let Some(found) = picked.iter().position(|kept| *kept == value) {
                picked.remove(found);
            } else if picked.len() < usize::from(max.max(1)) {
                picked.push(value);
            }
        }
        _ => {}
    }
    Some(Vec::new())
}

/// The modal an app opened, over `root`.
pub(crate) fn overlay(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let Some((channel, modal)) = store.modal() else {
        return root;
    };
    let empty = HashMap::new();
    let form = ui.apps.form.as_ref().filter(|form| form.id == modal.id);
    let pending = store
        .interaction_pending(context.now)
        .filter(|pending| pending.modal);
    let problem = store
        .interaction_problem(context.now)
        .filter(|problem| problem.channel == channel && problem.part.is_none());
    let names = crate::timeline::names_for(store, channel, look);
    let app = store.display_name(modal.app, None);
    let denied = store.interaction_abilities(channel).files;
    let view = ModalView {
        modal,
        app: &app,
        values: form.map_or(&empty, |form| &form.values),
        tried: form.is_some_and(|form| form.tried),
        menu: form.and_then(|form| form.menu.as_ref()),
        sending: pending.is_some(),
        problem: problem.as_ref().map(|problem| problem.reason.as_str()),
        names: Some(&names),
        files: form.map_or(&[], |form| form.files.as_slice()),
        picking: crate::uploads::modal_picking(ui),
        files_denied: denied.reason(),
    };
    let width = Modal::fit_width(context.window.width, 520.0);
    modal_dialog(root, &view, width, look)
}
