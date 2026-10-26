//! The settings of one channel or category, over the whole window, on the
//! frame of the server's settings: its overview (name, topic, slow mode, age
//! restriction), its permissions (private or not, in sync with its category
//! or not, and every overwrite with a control of three states for each
//! permission), and deleting it. Opened by the action `cs:open:{channel}`
//! from wherever a channel offers its settings; every action of its own
//! starts with `cs:`.
//!
//! As in the server's settings, nothing changes before the provider says so.
//! The drafts keep only what differs from the store: a [`ChannelEdit`] and
//! the overwrites changed, added or removed. While they differ, the bar of
//! unsaved changes stays, and leaving the page or the settings is refused
//! (the bar insists) until they are saved or reset. Unlike the reference,
//! Enter never deletes: the confirmation starts on «Cancelar».

mod view;
// Server administration, rest (P7).
pub(crate) mod forum;

pub use view::view;
pub(crate) use view::wanted;

use std::time::Instant;

use limn::{Key, UiAction, UiEvent};
use rune_components::channel_settings::TOPIC_LIMIT;
use rune_components::server_menu::channel_name;
use rune_components::server_settings::permission_of;
use rune_i18n::t;
use rune_model::{
    Channel, ChannelId, ChannelKind, Overwrite, OverwriteTarget, Permissions, RoleId, UserId,
};
use rune_provider::{ChannelEdit, Command, Failure};
use rune_state::{Applied, Store};

use crate::{Intent, Ui};

/// The fields of the settings, whose text limn keeps by id.
pub(crate) const NAME: &str = "cs:name";
pub(crate) const TOPIC: &str = "cs:topic";
pub(crate) const ADD_QUERY: &str = "cs:add-query";
pub(crate) const ACCESS_QUERY: &str = "cs:access-query";

/// A page of the settings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Page {
    #[default]
    Overview,
    Permissions,
    // Server administration, rest (P7).
    Forum,
}

/// Choosing who may see a private channel: what is searched, and the roles
/// and people chosen.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Access {
    pub query: String,
    pub roles: Vec<RoleId>,
    pub members: Vec<UserId>,
}

/// The settings of one channel while they are open.
#[derive(Debug, Default)]
pub struct ChannelSettings {
    pub channel: ChannelId,
    pub page: Page,
    /// What the overview changed and is not saved yet, as typed.
    pub edit: ChannelEdit,
    /// Overwrites changed here, by target, in the order first touched. An
    /// empty one is a target added with nothing set yet, or all set back.
    pub changes: Vec<Overwrite>,
    /// Targets whose overwrite is taken away here.
    pub removed: Vec<OverwriteTarget>,
    /// Whose permissions the advanced view shows; `None` is `@everyone`.
    pub selected: Option<OverwriteTarget>,
    pub slowmode_open: bool,
    /// The popout that adds a role or a person to the advanced view.
    pub adding: bool,
    pub add_query: String,
    /// The dialog that lets roles and people see a private channel.
    pub access: Option<Access>,
    /// The confirmation to take the category's permissions.
    pub sync: bool,
    /// The confirmation to delete the channel.
    pub delete: bool,
    /// Someone tried to leave with unsaved changes: the bar insists.
    pub nudged: bool,
    /// Commands sent for this channel and not answered yet.
    pending: usize,
    // Channels and voice administration (admin-canales-y-voz).
    /// The name last looked up at the provider for the people searched.
    lookup: Option<String>,
    // A forum's archive time (2026-10-19).
    /// Its list of archive times is open.
    pub archive_open: bool,
}

impl ChannelSettings {
    fn new(channel: ChannelId) -> Self {
        Self {
            channel,
            ..Self::default()
        }
    }

    /// Whether anything shown differs from what the provider has.
    pub fn dirty(&self, store: &Store) -> bool {
        store.channel(self.channel).is_some_and(|channel| {
            channel_changes(&self.edit, channel) != ChannelEdit::default()
                || !self.overwrite_commands(channel).is_empty()
        })
    }

    /// Changes sent and not answered yet.
    pub fn saving(&self) -> bool {
        self.pending > 0
    }

    /// Close the innermost dialog, popout or list; false when none was open.
    fn close_overlay(&mut self) -> bool {
        std::mem::take(&mut self.delete)
            || std::mem::take(&mut self.sync)
            || self.access.take().is_some()
            || std::mem::take(&mut self.adding)
            || std::mem::take(&mut self.slowmode_open)
            || std::mem::take(&mut self.archive_open)
    }

    /// The overwrite of `target` as shown: with what was changed here.
    pub fn shown(&self, channel: &Channel, target: OverwriteTarget) -> Overwrite {
        let empty = Overwrite {
            target,
            allow: Permissions::NONE,
            deny: Permissions::NONE,
        };
        if self.removed.contains(&target) {
            return empty;
        }
        self.changes
            .iter()
            .chain(&channel.overwrites)
            .find(|overwrite| overwrite.target == target)
            .copied()
            .unwrap_or(empty)
    }

    /// The overwrite `target` would have once saved; `None` for none.
    fn desired(&self, channel: &Channel, target: OverwriteTarget) -> Option<Overwrite> {
        Some(self.shown(channel, target)).filter(|overwrite| !overwrite.is_empty())
    }

    /// The roles and people with an overwrite, or added here: never those
    /// removed here. `@everyone` is listed only when it has one.
    pub fn listed(&self, channel: &Channel) -> Vec<OverwriteTarget> {
        let mut targets = Vec::new();
        for target in channel
            .overwrites
            .iter()
            .chain(&self.changes)
            .map(|overwrite| overwrite.target)
        {
            if !self.removed.contains(&target) && !targets.contains(&target) {
                targets.push(target);
            }
        }
        targets
    }

    /// The commands that make the provider's overwrites what is shown here.
    fn overwrite_commands(&self, channel: &Channel) -> Vec<Command> {
        let mut targets: Vec<OverwriteTarget> = Vec::new();
        for target in channel
            .overwrites
            .iter()
            .chain(&self.changes)
            .map(|overwrite| overwrite.target)
            .chain(self.removed.iter().copied())
        {
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
        targets
            .into_iter()
            .filter_map(|target| overwrite_command(channel, target, self.desired(channel, target)))
            .collect()
    }

    /// Put `overwrite` in the draft.
    fn set(&mut self, overwrite: Overwrite) {
        self.removed.retain(|target| *target != overwrite.target);
        match self
            .changes
            .iter_mut()
            .find(|change| change.target == overwrite.target)
        {
            Some(change) => *change = overwrite,
            None => self.changes.push(overwrite),
        }
    }

    /// Take `target` out of the list, and its overwrite away once saved.
    fn remove(&mut self, channel: &Channel, target: OverwriteTarget) {
        self.changes.retain(|change| change.target != target);
        let stored = channel
            .overwrites
            .iter()
            .any(|overwrite| overwrite.target == target);
        if stored && !self.removed.contains(&target) {
            self.removed.push(target);
        }
        if self.selected == Some(target) {
            self.selected = None;
        }
    }

    /// Whether the channel is private as shown: `@everyone` may not see it.
    pub fn is_private(&self, channel: &Channel, everyone: OverwriteTarget) -> bool {
        Store::private_by(&self.shown(channel, everyone))
    }
}

/// The overwrite of `target` the provider has, `None` when it has none.
pub(crate) fn stored(channel: &Channel, target: OverwriteTarget) -> Option<Overwrite> {
    channel
        .overwrites
        .iter()
        .find(|overwrite| overwrite.target == target && !overwrite.is_empty())
        .copied()
}

/// The command that gives `target` the overwrite `wanted` in `channel`;
/// `None` when it already has it.
fn overwrite_command(
    channel: &Channel,
    target: OverwriteTarget,
    wanted: Option<Overwrite>,
) -> Option<Command> {
    if wanted == stored(channel, target) {
        return None;
    }
    Some(match wanted {
        Some(overwrite) => Command::SetOverwrite {
            channel: channel.id,
            overwrite,
        },
        None => Command::RemoveOverwrite {
            channel: channel.id,
            target,
        },
    })
}

/// The commands that give `channel` the overwrites of its `category`.
/// None when it already has them: they are in sync.
pub(crate) fn sync_commands(channel: &Channel, category: &Channel) -> Vec<Command> {
    let mut targets: Vec<OverwriteTarget> = Vec::new();
    for target in channel
        .overwrites
        .iter()
        .chain(&category.overwrites)
        .map(|overwrite| overwrite.target)
    {
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
        .into_iter()
        .filter_map(|target| overwrite_command(channel, target, stored(category, target)))
        .collect()
}

/// A channel's name as it would be saved: a text channel's lowercase and
/// dashed.
pub(crate) fn final_name(kind: ChannelKind, name: &str) -> String {
    if kind == ChannelKind::Text {
        channel_name(name)
    } else {
        name.trim().to_owned()
    }
}

/// `edit` as it would be saved, without what `channel` already is.
pub(crate) fn channel_changes(edit: &ChannelEdit, channel: &Channel) -> ChannelEdit {
    let topic = |text: &Option<String>| text.clone().filter(|text| !text.trim().is_empty());
    ChannelEdit {
        name: edit
            .name
            .as_deref()
            .map(|name| final_name(channel.kind, name))
            .filter(|name| *name != channel.name),
        topic: edit
            .topic
            .as_ref()
            .map(topic)
            .filter(|text| *text != channel.topic),
        parent: None,
        position: None,
        slowmode_secs: edit
            .slowmode_secs
            .filter(|secs| *secs != channel.slowmode_secs),
        nsfw: edit.nsfw.filter(|nsfw| *nsfw != channel.nsfw),
        // Server administration, rest (P7).
        tags: forum::changes(edit, channel).0,
        forum: forum::changes(edit, channel).1,
    }
}

/// `@everyone` of the channel's server.
pub(crate) fn everyone(channel: &Channel) -> OverwriteTarget {
    Store::everyone(channel)
}

/// Why the account cannot change `target`'s overwrite in `channel`, when it
/// cannot: it needs to manage the channel's permissions, and a role must be
/// below its own.
pub(crate) fn target_lock(
    store: &Store,
    channel: &Channel,
    target: OverwriteTarget,
) -> Option<String> {
    let abilities = store.abilities(channel.id)?;
    if let Some(reason) = abilities.manage_permissions.reason() {
        return Some(reason.to_owned());
    }
    let server = channel.server?;
    let OverwriteTarget::Role(role) = target else {
        return None;
    };
    let position = store
        .server(server)?
        .roles
        .iter()
        .find(|candidate| candidate.id == role && role.0 != server.0)?
        .position;
    (position >= store.my_rank(server)).then(|| {
        t("Este rol está a la altura del tuyo o por encima: no puedes cambiar sus permisos.").into()
    })
}

/// Why the account cannot allow or deny `permission` in `channel`, when it
/// cannot: nobody hands out, or takes away, what they do not have.
pub(crate) fn permission_lock(channel: &Channel, permission: Permissions) -> Option<String> {
    (!channel.permissions.contains(permission))
        .then(|| t("No lo tienes en este canal, así que no puedes cambiarlo.").into())
}

fn set_input(id: &str, value: &str) -> Intent {
    Intent::SetInput {
        id: id.to_owned(),
        value: value.to_owned(),
    }
}

/// Open the settings of `channel`, a channel or a category of a server.
fn open(ui: &mut Ui, store: &Store, channel: ChannelId) -> Vec<Intent> {
    let Some(found) = store.channel(channel).filter(|found| {
        found.server.is_some()
            && matches!(
                found.kind,
                ChannelKind::Text
                    | ChannelKind::Voice
                    | ChannelKind::Category
                    // Server administration, rest (P7).
                    | ChannelKind::Forum
            )
    }) else {
        return Vec::new();
    };
    ui.channel_settings = Some(ChannelSettings::new(channel));
    // What floated over the window goes: the settings cover it.
    ui.profile = None;
    ui.status_menu = false;
    ui.emoji_open = false;
    ui.reacting = None;
    ui.switcher = None;
    ui.settings = None;
    let mut intents = vec![
        set_input(NAME, &found.name),
        set_input(TOPIC, found.topic.as_deref().unwrap_or("")),
        set_input(ADD_QUERY, ""),
        set_input(ACCESS_QUERY, ""),
    ];
    // A voice channel and a category have little more than their name.
    let manage = store
        .abilities(channel)
        .is_some_and(|abilities| abilities.manage.allowed());
    if found.kind != ChannelKind::Text && manage {
        intents.push(Intent::Focus(NAME.into()));
    }
    intents
}

/// Apply a `cs:` action of the settings; `None` for the rest of the window.
pub fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let id = match action {
        UiAction::Clicked { id }
        | UiAction::InputChanged { id, .. }
        | UiAction::Submitted { id, .. } => id,
        _ => return None,
    };
    let rest = id.strip_prefix("cs:")?;
    if let Some(channel) = rest.strip_prefix("open:") {
        return Some(
            channel
                .parse()
                .map(|channel| open(ui, store, ChannelId(channel)))
                .unwrap_or_default(),
        );
    }
    let Some(mut settings) = ui.channel_settings.take() else {
        return Some(Vec::new());
    };
    settings.nudged = false;
    let mut intents = Vec::new();
    let keep = match action {
        UiAction::Clicked { .. } => clicked(&mut settings, ui, store, rest, now, &mut intents),
        UiAction::InputChanged { value, .. } => {
            // Server administration, rest (P7): a forum's fields.
            let forum = store
                .channel(settings.channel)
                .is_some_and(|channel| forum::typed(&mut settings, store, channel, id, value));
            if !forum {
                typed(&mut settings, id, value, &mut intents);
                intents.extend(lookup(&mut settings, store, id, value));
            }
            true
        }
        _ => true,
    };
    if keep {
        ui.channel_settings = Some(settings);
    }
    Some(intents)
}

/// Esc while the settings are open: close what floats inside them, else the
/// settings, unless something is not saved: then the bar insists instead.
pub fn intercept(ui: &mut Ui, store: &Store, event: &UiEvent) -> Option<Vec<Intent>> {
    let settings = ui.channel_settings.as_mut()?;
    if !matches!(
        event,
        UiEvent::Key {
            key: Key::Escape,
            ..
        }
    ) {
        return None;
    }
    if !settings.close_overlay() {
        leave(ui, store);
    }
    Some(Vec::new())
}

/// Close the settings, unless something is not saved.
fn leave(ui: &mut Ui, store: &Store) {
    let Some(settings) = ui.channel_settings.as_mut() else {
        return;
    };
    if settings.dirty(store) {
        settings.nudged = true;
    } else {
        ui.channel_settings = None;
    }
}

/// Answer what an event of the provider changed. A channel gone, here or
/// elsewhere, or out of the account's sight, closes its settings; a change
/// of it shows in every field not being edited.
pub fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied) -> Vec<Intent> {
    let Some(mut settings) = ui.channel_settings.take() else {
        return Vec::new();
    };
    let Some(channel) = store
        .channel(settings.channel)
        .filter(|channel| Store::visible(channel))
    else {
        return Vec::new();
    };
    let mut intents = Vec::new();
    match applied {
        Applied::Ready => {
            settings.pending = 0;
            refresh(&mut settings, store, &mut intents);
        }
        Applied::Channel(changed) if *changed == channel.id => {
            settings.pending = settings.pending.saturating_sub(1);
            refresh(&mut settings, store, &mut intents);
        }
        // Roles made, changed or deleted.
        Applied::Server(server) if Some(*server) == channel.server => {
            refresh(&mut settings, store, &mut intents);
        }
        Applied::Failed {
            failure: Failure::Channel { channel: Some(of) },
            ..
        } if *of == channel.id => settings.pending = 0,
        _ => {}
    }
    ui.channel_settings = Some(settings);
    intents
}

/// Keep of the drafts only what still differs from the store, and show the
/// store's values in the fields nobody edited.
fn refresh(settings: &mut ChannelSettings, store: &Store, intents: &mut Vec<Intent>) {
    let Some(channel) = store.channel(settings.channel) else {
        return;
    };
    let changed = channel_changes(&settings.edit, channel);
    if changed.name.is_none() {
        settings.edit.name = None;
        intents.push(set_input(NAME, &channel.name));
    }
    if changed.topic.is_none() {
        settings.edit.topic = None;
        intents.push(set_input(TOPIC, channel.topic.as_deref().unwrap_or("")));
    }
    settings.edit.slowmode_secs = changed.slowmode_secs;
    settings.edit.nsfw = changed.nsfw;
    // Server administration, rest (P7).
    let emojis = channel
        .server
        .and_then(|server| store.server(server))
        .map_or(&[][..], |server| &server.emojis);
    forum::refresh(settings, channel, emojis, intents);
    // An empty change stays: a target added and still unset, or set back.
    settings
        .changes
        .retain(|change| change.is_empty() || Some(*change) != stored(channel, change.target));
    settings.removed.retain(|target| {
        channel
            .overwrites
            .iter()
            .any(|overwrite| overwrite.target == *target)
    });
    if settings
        .selected
        .is_some_and(|target| !settings.listed(channel).contains(&target))
    {
        settings.selected = None;
    }
}

/// What a field's new text changes.
fn typed(settings: &mut ChannelSettings, id: &str, value: &str, intents: &mut Vec<Intent>) {
    match id {
        NAME => settings.edit.name = Some(value.to_owned()),
        TOPIC => {
            let kept: String = value.chars().take(TOPIC_LIMIT).collect();
            if kept.len() != value.len() {
                intents.push(set_input(TOPIC, &kept));
            }
            settings.edit.topic = Some(Some(kept));
        }
        ADD_QUERY => settings.add_query = value.to_owned(),
        ACCESS_QUERY => {
            if let Some(access) = &mut settings.access {
                access.query = value.to_owned();
            }
        }
        _ => {}
    }
}

/// Look up at the provider the people named like what the ⊕ popout or the
/// private channel's dialog searches, beyond the members it sent (the
/// Gateway's member search); the same name asks once.
fn lookup(settings: &mut ChannelSettings, store: &Store, id: &str, value: &str) -> Option<Intent> {
    if id != ADD_QUERY && id != ACCESS_QUERY {
        return None;
    }
    let query = value.trim().trim_start_matches('@').to_lowercase();
    let server = store.channel(settings.channel)?.server?;
    if query.is_empty()
        || !store.member_search(server).allowed()
        || settings.lookup.as_ref() == Some(&query)
    {
        return None;
    }
    settings.lookup = Some(query.clone());
    Some(Intent::Command(Command::SearchMembers { server, query }))
}

/// The role or person a pair of an action's parts names: `role:{id}` or
/// `member:{id}`.
fn target_of(kind: &str, id: &str) -> Option<OverwriteTarget> {
    let id: u64 = id.parse().ok()?;
    match kind {
        "role" => Some(OverwriteTarget::Role(RoleId(id))),
        "member" => Some(OverwriteTarget::Member(UserId(id))),
        _ => None,
    }
}

/// A click inside the settings. False when it closes them.
fn clicked(
    settings: &mut ChannelSettings,
    ui: &mut Ui,
    store: &Store,
    rest: &str,
    now: Instant,
    intents: &mut Vec<Intent>,
) -> bool {
    let Some(channel) = store.channel(settings.channel) else {
        return false;
    };
    let Some(abilities) = store.abilities(channel.id) else {
        return false;
    };
    let Some(server) = channel.server else {
        return false;
    };
    let everyone = everyone(channel);
    let manage = abilities.manage.allowed();
    let editable = |target: OverwriteTarget| target_lock(store, channel, target).is_none();
    let parts: Vec<&str> = rest.split(':').collect();
    match parts.as_slice() {
        ["close"] => {
            if settings.dirty(store) {
                settings.nudged = true;
            } else {
                return false;
            }
        }
        ["page", key] => {
            let page = match *key {
                "overview" => Page::Overview,
                "permissions" => Page::Permissions,
                "forum" if channel.kind == ChannelKind::Forum => Page::Forum,
                _ => return true,
            };
            if settings.page != page {
                if settings.dirty(store) {
                    settings.nudged = true;
                } else {
                    while settings.close_overlay() {}
                    settings.page = page;
                }
            }
        }
        ["reset"] => {
            settings.edit = ChannelEdit::default();
            settings.changes.clear();
            settings.removed.clear();
            settings.slowmode_open = false;
            refresh(settings, store, intents);
        }
        ["save"] => save(settings, store, intents),

        // The overview.
        ["slowmode"] if manage => settings.slowmode_open = !settings.slowmode_open,
        ["slowmode", "dismiss"] => settings.slowmode_open = false,
        ["slowmode", secs] if manage => {
            if let Ok(secs) = secs.parse() {
                settings.edit.slowmode_secs = Some(secs);
            }
            settings.slowmode_open = false;
        }
        ["nsfw"] if manage => {
            settings.edit.nsfw = Some(!settings.edit.nsfw.unwrap_or(channel.nsfw));
        }

        // Private or not, and who may see it.
        ["private"]
            if editable(everyone) && permission_lock(channel, Permissions::VIEW).is_none() =>
        {
            let on = !settings.is_private(channel, everyone);
            let shown = settings.shown(channel, everyone);
            settings.set(shown.with_state(Permissions::VIEW, if on { Some(false) } else { None }));
            // Who owns the server or administers it sees it anyway; anyone
            // else making it private keeps seeing it.
            let sees_all = store.owns(server)
                || store
                    .server_permissions(server)
                    .contains(Permissions::ADMINISTRATOR);
            if let (true, false, Some(me)) = (on, sees_all, store.me()) {
                let mine = settings.shown(channel, OverwriteTarget::Member(me));
                settings.set(mine.with_state(Permissions::VIEW, Some(true)));
            }
        }
        ["access", "add"] if abilities.manage_permissions.allowed() => {
            settings.access = Some(Access::default());
            intents.push(set_input(ACCESS_QUERY, ""));
        }
        ["access", kind, id, "remove"] => {
            if let Some(target) = target_of(kind, id).filter(|target| editable(*target)) {
                let shown = settings.shown(channel, target);
                settings.set(shown.with_state(Permissions::VIEW, None));
            }
        }
        ["pick", "cancel"] => settings.access = None,
        ["pick", "done"] => {
            if let Some(access) = settings.access.take() {
                let targets = access
                    .roles
                    .iter()
                    .map(|role| OverwriteTarget::Role(*role))
                    .chain(
                        access
                            .members
                            .iter()
                            .map(|user| OverwriteTarget::Member(*user)),
                    );
                for target in targets.filter(|target| editable(*target)) {
                    let shown = settings.shown(channel, target);
                    settings.set(shown.with_state(Permissions::VIEW, Some(true)));
                }
            }
        }
        ["pick", kind, id] => {
            if let (Some(access), Ok(id)) = (&mut settings.access, id.parse::<u64>()) {
                fn flip<T: PartialEq>(list: &mut Vec<T>, item: T) {
                    match list.iter().position(|candidate| *candidate == item) {
                        Some(index) => {
                            list.remove(index);
                        }
                        None => list.push(item),
                    }
                }
                match *kind {
                    "role" => flip(&mut access.roles, RoleId(id)),
                    "member" => flip(&mut access.members, UserId(id)),
                    _ => {}
                }
            }
        }

        // The advanced view.
        ["advanced"] => ui.advanced_permissions = !ui.advanced_permissions,
        ["target", kind, id] => {
            if let Some(target) = target_of(kind, id) {
                if target == everyone || settings.listed(channel).contains(&target) {
                    settings.selected = (target != everyone).then_some(target);
                }
            }
        }
        ["target", kind, id, "remove"] => {
            if let Some(target) =
                target_of(kind, id).filter(|target| *target != everyone && editable(*target))
            {
                settings.remove(channel, target);
            }
        }
        ["target", _, id, "copy-id"] => {
            ui.notify(t("ID copiado"), None, false, now);
            intents.push(Intent::Copy((*id).to_owned()));
        }
        ["add"] if abilities.manage_permissions.allowed() => {
            settings.adding = !settings.adding;
            if settings.adding {
                settings.add_query.clear();
                intents.push(set_input(ADD_QUERY, ""));
                intents.push(Intent::Focus(ADD_QUERY.into()));
            }
        }
        ["add", "dismiss"] => settings.adding = false,
        ["add", kind, id] => {
            if let Some(target) = target_of(kind, id).filter(|target| editable(*target)) {
                if !settings.listed(channel).contains(&target) {
                    settings.set(Overwrite {
                        target,
                        allow: Permissions::NONE,
                        deny: Permissions::NONE,
                    });
                }
                settings.selected = (target != everyone).then_some(target);
                settings.adding = false;
            }
        }
        ["perm", bit, which] => {
            let target = settings.selected.unwrap_or(everyone);
            let state = match *which {
                "deny" => Some(false),
                "inherit" => None,
                "allow" => Some(true),
                _ => return true,
            };
            if let Some(permission) = permission_of(bit).filter(|permission| {
                editable(target) && permission_lock(channel, *permission).is_none()
            }) {
                let shown = settings.shown(channel, target);
                settings.set(shown.with_state(permission, state));
            }
        }

        // In sync with the category.
        ["sync"] => {
            let out_of_sync = channel
                .parent
                .and_then(|parent| store.channel(parent))
                .is_some_and(|category| !sync_commands(channel, category).is_empty());
            settings.sync = out_of_sync && abilities.manage_permissions.allowed();
        }
        ["sync", "confirm"] => {
            settings.sync = false;
            if let Some(category) = channel.parent.and_then(|parent| store.channel(parent)) {
                let commands = sync_commands(channel, category);
                // The category's overwrites replace these, and whatever was drafted.
                settings.changes.clear();
                settings.removed.clear();
                settings.selected = None;
                settings.pending += commands.len();
                intents.extend(commands.into_iter().map(Intent::Command));
            }
        }
        ["sync", "cancel"] => settings.sync = false,

        // Deleting it.
        ["delete"] if manage => settings.delete = true,
        ["delete", "confirm"] => {
            settings.delete = false;
            if manage {
                intents.push(Intent::Command(Command::DeleteChannel {
                    channel: channel.id,
                }));
            }
        }
        ["delete", "cancel"] => settings.delete = false,
        // Server administration, rest (P7).
        _ => {
            forum::clicked(settings, channel, &abilities, &parts, intents);
        }
    }
    true
}

/// Send what differs from the store: the overview in one edit, and one
/// command per overwrite. A category's change goes as well to the channels
/// in sync with it, which keep taking its permissions.
fn save(settings: &mut ChannelSettings, store: &Store, intents: &mut Vec<Intent>) {
    if settings.saving() {
        return;
    }
    let Some(channel) = store.channel(settings.channel) else {
        return;
    };
    let mut edit = channel_changes(&settings.edit, channel);
    if edit.name.as_deref() == Some("") {
        // The field says why.
        return;
    }
    // Server administration, rest (P7): checked again as it leaves.
    if !store
        .abilities(channel.id)
        .is_some_and(|abilities| abilities.forum_settings.allowed())
    {
        edit.tags = None;
        edit.forum = None;
    }
    if edit != ChannelEdit::default() {
        settings.pending += 1;
        intents.push(Intent::Command(Command::EditChannel {
            channel: channel.id,
            edit,
        }));
    }
    let commands = settings.overwrite_commands(channel);
    settings.pending += commands.len();
    if channel.kind == ChannelKind::Category && !commands.is_empty() {
        for child in children(store, channel) {
            let allowed = store
                .abilities(child.id)
                .is_some_and(|abilities| abilities.manage_permissions.allowed());
            if allowed && sync_commands(child, channel).is_empty() {
                intents.extend(
                    commands
                        .iter()
                        .map(|command| Intent::Command(aimed_at(command.clone(), child.id))),
                );
            }
        }
    }
    intents.extend(commands.into_iter().map(Intent::Command));
}

/// The channels of `category` the account sees.
pub(crate) fn children<'a>(store: &'a Store, category: &Channel) -> Vec<&'a Channel> {
    category
        .server
        .map(|server| store.channel_tree(server))
        .unwrap_or_default()
        .into_iter()
        .find(|(heading, _)| heading.is_some_and(|heading| heading.id == category.id))
        .map(|(_, channels)| channels)
        .unwrap_or_default()
}

/// An overwrite command for `channel` instead.
fn aimed_at(command: Command, channel: ChannelId) -> Command {
    match command {
        Command::SetOverwrite { overwrite, .. } => Command::SetOverwrite { channel, overwrite },
        Command::RemoveOverwrite { target, .. } => Command::RemoveOverwrite { channel, target },
        other => other,
    }
}
