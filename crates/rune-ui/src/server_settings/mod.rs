//! The settings of one server, over the whole window: its profile, its roles
//! and what each grants, its members, its invitations, its emojis and
//! stickers, its bans, its audit log, and deleting it. Opened by the action `ss:open:{server}` from wherever a server offers
//! its settings; every action of its own starts with `ss:`.
//!
//! Nothing changes before the provider says so. The pages keep drafts of
//! only what differs from the store (a [`ServerEdit`], a [`RoleEdit`], an
//! order of roles), so a change made elsewhere shows at once wherever nothing
//! was edited here. While a draft differs, the bar of unsaved changes stays,
//! and leaving the page or the settings is refused (the bar says so) until it
//! is saved or reset: unlike the reference, Esc never drops changes silently.
//!
//! Bans and the audit log come a page at a time («Cargar más»). Revoking,
//! unbanning, renaming and deleting ask first, and the permission is checked
//! again when confirmed: if it was lost meanwhile, nothing is sent and a
//! notice says why. What is confirmed shows at once and comes back if the
//! provider refuses it (see `Store::ahead`).

pub(crate) mod advanced;
// Server administration, rest (P7).
pub(crate) mod rest;
// Rules and onboarding (2026-10-18).
pub(crate) mod onboarding;
// AutoMod (automod).
pub(crate) mod automod;
mod view;

pub use advanced::expression_image_picked;
pub use view::view;
pub(crate) use view::wanted;

use std::collections::HashSet;
use std::time::Instant;

use limn::{Key, UiAction, UiEvent};
use rune_components::server_settings::{hex, parse_hex, permission_of, AUDIT_ACTIONS};
use rune_i18n::{es, t, tr};
use rune_model::{
    valid_emoji_name, valid_sticker_name, AuditActionKind, AuditQuery, EmojiId, ExpressionId,
    Permissions, Role, RoleId, Server, ServerId, StickerId, UserId,
};
use rune_provider::{Command, Created, Failure, RequestId, RoleEdit, ServerEdit};
use rune_state::{Applied, Permit, Store};

use crate::{Intent, Ui};

/// The fields of the settings, whose text limn keeps by id.
pub(crate) const NAME: &str = "ss:name";
pub(crate) const DESCRIPTION: &str = "ss:description";
pub(crate) const ROLE_NAME: &str = "ss:role-name";
pub(crate) const ACCENT_HEX: &str = "ss:accent:hex";
pub(crate) const COLOR_HEX: &str = "ss:color:hex";
pub(crate) const NICK: &str = "ss:nick";
pub(crate) const REASON: &str = "ss:reason";
pub(crate) const DELETE_NAME: &str = "ss:delete-server:name";
pub(crate) const ROLES_QUERY: &str = "ss:roles:query";
pub(crate) const PERMISSIONS_QUERY: &str = "ss:perms:query";
pub(crate) const ROLE_MEMBERS_QUERY: &str = "ss:role-members:query";
pub(crate) const ADD_QUERY: &str = "ss:add:query";
pub(crate) const MEMBERS_QUERY: &str = "ss:members:query";
pub(crate) const BANS_QUERY: &str = "ss:bans:query";
pub(crate) const EXPRESSION_NAME: &str = "ss:expression-name";
/// What one page of bans or of the audit log asks for.
pub(crate) const PAGE: usize = 50;
/// What a new role is called until it is renamed.
const NEW_ROLE: &str = es("nuevo rol");

/// A page of the settings.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Page {
    #[default]
    Profile,
    Members,
    Roles,
    Invites,
    Emojis,
    Stickers,
    Audit,
    Bans,
    // Administration (19).
    Integrations,
    // Server administration, rest (P7).
    Safety,
    Channels,
    // Rules and onboarding (2026-10-18).
    Onboarding,
    // AutoMod (automod).
    AutoMod,
}

impl Page {
    fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "profile" => Self::Profile,
            "members" => Self::Members,
            "roles" => Self::Roles,
            "invites" => Self::Invites,
            "emojis" => Self::Emojis,
            "stickers" => Self::Stickers,
            "audit" => Self::Audit,
            "bans" => Self::Bans,
            "integrations" => Self::Integrations,
            "safety" => Self::Safety,
            "channels" => Self::Channels,
            "onboarding" => Self::Onboarding,
            // AutoMod (automod).
            "automod" => Self::AutoMod,
            _ => return None,
        })
    }
}

/// A tab of the role editor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RoleTab {
    #[default]
    Display,
    Permissions,
    Members,
}

/// Kicking or banning someone, with the reason being written.
#[derive(Clone, Debug, PartialEq)]
pub struct Removal {
    pub user: UserId,
    pub ban: bool,
    pub reason: String,
    /// The bans' history to delete: an index of `advanced::BAN_HISTORY`.
    pub history: usize,
}

/// Which filter of the audit log is open.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuditFilter {
    User,
    Action,
}

/// The settings of one server while they are open.
#[derive(Debug, Default)]
pub struct ServerSettings {
    pub server: ServerId,
    pub page: Page,
    /// What the profile page changed and is not saved yet.
    pub profile: ServerEdit,
    /// The custom color field of the profile, while it is open.
    pub accent_hex: Option<String>,
    /// The role whose editor is open, on the roles page.
    pub role: Option<RoleId>,
    pub tab: RoleTab,
    /// What the editor changed in `role` and is not saved yet.
    pub role_edit: RoleEdit,
    pub color_hex: Option<String>,
    /// The roles in the order dragged here, highest first, not saved yet.
    pub order: Option<Vec<RoleId>>,
    /// The order being saved: the provider moves one role at a time, so the
    /// next move goes once the last one is answered.
    saving_order: Option<Vec<RoleId>>,
    /// Edits sent and not answered yet.
    pending: usize,
    pub dragging: Option<RoleId>,
    pub drop_over: Option<RoleId>,
    pub role_menu: Option<RoleId>,
    pub editor_menu: bool,
    pub delete_role: Option<RoleId>,
    /// The dialog that gives the open role to people, and who is chosen.
    pub add_members: Option<Vec<UserId>>,
    /// The role being made, to open its editor once it exists.
    creating: Option<RequestId>,
    /// Someone tried to leave with unsaved changes: the bar insists.
    pub nudged: bool,
    pub roles_query: String,
    pub permissions_query: String,
    pub role_members_query: String,
    pub add_query: String,
    pub members_query: String,
    pub bans_query: String,
    /// The members page shows only those with this role.
    pub role_filter: Option<RoleId>,
    pub filter_open: bool,
    pub member_menu: Option<UserId>,
    /// Whose roles are being picked on the members page.
    pub role_picker: Option<UserId>,
    /// Whose nickname is being written, and what.
    pub nick: Option<(UserId, String)>,
    pub removal: Option<Removal>,
    pub unban: Option<UserId>,
    /// The audit log entries opened to their details.
    pub expanded: HashSet<u64>,
    pub audit_user: Option<UserId>,
    pub audit_action: Option<AuditActionKind>,
    pub audit_filter: Option<AuditFilter>,
    /// The invitation being revoked, while that dialog is open.
    pub revoke_invite: Option<String>,
    /// The expression being renamed, and the name written.
    pub rename: Option<(ExpressionId, String)>,
    pub delete_expression: Option<ExpressionId>,
    /// The name typed to confirm deleting the server, while that dialog is open.
    pub delete_server: Option<String>,
    /// Timeouts, new invitations and expressions, pruning, webhooks and
    /// integrations: see `advanced`.
    pub advanced: advanced::Advanced,
    // Server administration, rest (P7).
    /// Pictures, traits, safety, channels' order, role styles and member
    /// queries: see `rest`.
    pub rest: rest::Rest,
    // Rules and onboarding (2026-10-18).
    /// The onboarding's page: see `onboarding`.
    pub onboarding: onboarding::OnboardingPage,
    // AutoMod (automod).
    /// The AutoMod rules' page: see `automod`.
    pub automod: automod::AutoModPage,
}

impl ServerSettings {
    fn new(server: ServerId) -> Self {
        Self {
            server,
            ..Self::default()
        }
    }

    /// Whether anything shown differs from what the provider has.
    pub fn dirty(&self, store: &Store) -> bool {
        let Some(server) = store.server(self.server) else {
            return false;
        };
        server_changes(&self.profile, server) != ServerEdit::default()
            || self.role_changes(server) != RoleEdit::default()
            || self.order_changed(store)
            || rest::layout_changed(self, store)
            || onboarding::changed(self, store)
    }

    /// The changes to the open role, without what already matches it.
    fn role_changes(&self, server: &Server) -> RoleEdit {
        self.role
            .and_then(|id| server.roles.iter().find(|role| role.id == id))
            .map_or_else(RoleEdit::default, |role| {
                role_changes(&self.role_edit, role)
            })
    }

    fn order_changed(&self, store: &Store) -> bool {
        self.order
            .as_ref()
            .is_some_and(|order| *order != ranked(store, self.server))
    }

    /// What the audit log's filters ask for.
    pub fn audit_query(&self) -> AuditQuery {
        AuditQuery {
            actor: self.audit_user,
            action: self.audit_action,
        }
    }

    /// Changes left and not answered yet.
    pub fn saving(&self) -> bool {
        self.pending > 0
            || self.saving_order.is_some()
            || self.rest.saving()
            || self.onboarding.saving()
    }

    /// Close the innermost dialog, menu or popover; false when none was open.
    fn close_overlay(&mut self) -> bool {
        if self.rest.close()
            || self.onboarding.close()
            // AutoMod (automod).
            || self.automod.close()
            || self.advanced.close()
            || self.delete_server.take().is_some()
            || self.delete_role.take().is_some()
            || self.add_members.take().is_some()
            || self.nick.take().is_some()
            || self.removal.take().is_some()
            || self.unban.take().is_some()
            || self.revoke_invite.take().is_some()
            || self.rename.take().is_some()
            || self.delete_expression.take().is_some()
            || self.role_menu.take().is_some()
            || self.member_menu.take().is_some()
            || self.role_picker.take().is_some()
            || self.audit_filter.take().is_some()
            || self.color_hex.take().is_some()
            || self.accent_hex.take().is_some()
        {
            return true;
        }
        std::mem::take(&mut self.editor_menu) || std::mem::take(&mut self.filter_open)
    }
}

/// The roles that can be given, highest first: all but `@everyone`.
pub(crate) fn ranked(store: &Store, server: ServerId) -> Vec<RoleId> {
    store.server(server).map_or_else(Vec::new, |found| {
        found
            .assignable_roles()
            .iter()
            .map(|role| role.id)
            .collect()
    })
}

/// The roles as the settings show them: in the order dragged here, or as
/// the provider has them.
pub(crate) fn shown_order(settings: &ServerSettings, store: &Store) -> Vec<RoleId> {
    let ranked = ranked(store, settings.server);
    settings
        .order
        .clone()
        .filter(|order| same_roles(order, &ranked))
        .unwrap_or(ranked)
}

fn same_roles(a: &[RoleId], b: &[RoleId]) -> bool {
    a.len() == b.len() && a.iter().all(|role| b.contains(role))
}

/// `edit` without what `server` already is.
pub(crate) fn server_changes(edit: &ServerEdit, server: &Server) -> ServerEdit {
    let description = |text: &Option<String>| text.clone().filter(|text| !text.trim().is_empty());
    ServerEdit {
        name: edit.name.clone().filter(|name| name.trim() != server.name),
        description: edit
            .description
            .clone()
            .filter(|text| description(text) != server.description),
        accent: edit.accent.filter(|accent| *accent != server.accent),
        // Server administration, rest (P7).
        ..rest::changes(edit, server)
    }
}

/// `edit` without what `role` already is.
pub(crate) fn role_changes(edit: &RoleEdit, role: &Role) -> RoleEdit {
    RoleEdit {
        name: edit.name.clone().filter(|name| name.trim() != role.name),
        color: edit.color.filter(|color| *color != role.color),
        hoisted: edit.hoisted.filter(|hoisted| *hoisted != role.hoisted),
        mentionable: edit
            .mentionable
            .filter(|mentionable| *mentionable != role.mentionable),
        permissions: edit
            .permissions
            .filter(|permissions| *permissions != role.permissions),
        position: None,
        // Server administration, rest (P7).
        icon: rest::role_changes(edit, role).0,
        gradient: rest::role_changes(edit, role).1,
    }
}

/// `role` as the editor shows it: with what was changed here.
pub(crate) fn shown_role(role: &Role, edit: &RoleEdit) -> Role {
    let mut shown = Role {
        name: edit.name.clone().unwrap_or_else(|| role.name.clone()),
        color: edit.color.unwrap_or(role.color),
        hoisted: edit.hoisted.unwrap_or(role.hoisted),
        mentionable: edit.mentionable.unwrap_or(role.mentionable),
        permissions: edit.permissions.unwrap_or(role.permissions),
        ..role.clone()
    };
    // Server administration, rest (P7).
    rest::shown_style(&mut shown, edit);
    shown
}

/// `server` as the profile page shows it: with what was changed here.
pub(crate) fn shown_server(server: &Server, edit: &ServerEdit) -> Server {
    Server {
        name: edit.name.clone().unwrap_or_else(|| server.name.clone()),
        description: edit
            .description
            .clone()
            .unwrap_or_else(|| server.description.clone()),
        accent: edit.accent.unwrap_or(server.accent),
        ..server.clone()
    }
}

/// Why the account cannot change `role`, when it cannot. `@everyone` only
/// needs the permission to manage roles.
pub(crate) fn role_lock(store: &Store, server: ServerId, role: &Role) -> Option<String> {
    let abilities = store.server_abilities(server)?;
    if let Some(reason) = abilities.manage_roles.reason() {
        return Some(reason.to_owned());
    }
    (role.id != RoleId(server.0) && role.position >= store.my_rank(server))
        .then(|| t("Este rol está a la altura del tuyo o por encima: no puedes cambiarlo.").into())
}

/// Why the account cannot give or take `role`, when it cannot.
pub(crate) fn grant_lock(store: &Store, server: ServerId, role: &Role) -> Option<String> {
    let abilities = store.server_abilities(server)?;
    if let Some(reason) = abilities.manage_roles.reason() {
        return Some(reason.to_owned());
    }
    (role.position >= store.my_rank(server))
        .then(|| t("Este rol está a la altura del tuyo o por encima.").into())
}

/// Why the account cannot turn `permission` on for a role, when it cannot:
/// nobody can grant what they do not have.
pub(crate) fn permission_lock(
    store: &Store,
    server: ServerId,
    permission: Permissions,
) -> Option<String> {
    (!store.server_permissions(server).contains(permission))
        .then(|| t("No lo tienes, así que no puedes darlo.").into())
}

/// Why the account cannot do what `permit` allows to `user`, when it cannot:
/// nobody acts on who owns the server, nor on someone ranked as high.
pub(crate) fn member_lock(
    store: &Store,
    server: ServerId,
    user: UserId,
    permit: &Permit,
) -> Option<String> {
    if let Some(reason) = permit.reason() {
        return Some(reason.to_owned());
    }
    if store.server(server)?.owner == Some(user) {
        return Some(t("Nadie puede hacerle eso a quien es dueño del servidor.").into());
    }
    (store.rank_of(server, user) >= store.my_rank(server))
        .then(|| t("Su rol está a la altura del tuyo o por encima.").into())
}

/// Why the account cannot change `user`'s nickname, when it cannot.
pub(crate) fn nick_lock(store: &Store, server: ServerId, user: UserId) -> Option<String> {
    let abilities = store.server_abilities(server)?;
    if Some(user) == store.me() {
        return abilities.change_nick.reason().map(str::to_owned);
    }
    member_lock(store, server, user, &abilities.manage_nicks)
}

/// Whether the account may drag `role` to another place.
fn movable(store: &Store, server: ServerId, role: RoleId) -> bool {
    store
        .server(server)
        .and_then(|found| found.roles.iter().find(|candidate| candidate.id == role))
        .is_some_and(|found| role_lock(store, server, found).is_none() && role != RoleId(server.0))
}

/// The server's own entries of `user`'s menu outside the settings (a voice
/// channel): nickname, roles, timeout, kick and ban, only those the account
/// may use. Each opens the members page with its dialog, as there.
pub(crate) fn member_menu_group(
    store: &Store,
    server: ServerId,
    user: UserId,
) -> Vec<rune_components::voice::MenuEntry> {
    use rune_components::voice::MenuEntry;
    let (Some(abilities), Some(found), Some(member)) = (
        store.server_abilities(server),
        store.server(server),
        store.member_of(server, user),
    ) else {
        return Vec::new();
    };
    let id = |what: &str| format!("ss:member-of:{server}:{user}:{what}");
    let name = store.display_name(user, Some(server));
    let mut items = Vec::new();
    // Only what the account may do now: no permission or a higher rank hides it.
    let nick_permit = if Some(user) == store.me() {
        &abilities.change_nick
    } else {
        &abilities.manage_nicks
    };
    if nick_permit.allowed() && nick_lock(store, server, user).is_none() {
        items.push(MenuEntry::Item(id("nick"), t("Cambiar apodo").into(), None));
    }
    if abilities.manage_roles.allowed() {
        let mut roles: Vec<&Role> = found
            .roles
            .iter()
            .filter(|role| role.id != RoleId(server.0) && grant_lock(store, server, role).is_none())
            .collect();
        // Highest first, as the official client lists them.
        roles.sort_by_key(|role| std::cmp::Reverse(role.position));
        let roles: Vec<MenuEntry> = roles
            .into_iter()
            .map(|role| MenuEntry::Role {
                id: id(&format!("role:{}", role.id)),
                name: role.name.clone(),
                color: role.color,
                has: member.roles.contains(&role.id),
            })
            .collect();
        if !roles.is_empty() {
            items.push(MenuEntry::Submenu(t("Roles").into(), roles));
        }
    }
    if Some(user) != store.me() {
        if abilities.timeout.allowed()
            && member_lock(store, server, user, &abilities.timeout).is_none()
        {
            let timed_out = advanced::timed_out(store, server, user);
            let (action, label) = if timed_out {
                ("untimeout", tr!("Quitar el aislamiento a {name}", name))
            } else {
                ("timeout", tr!("Aislar a {name}", name))
            };
            items.push(if timed_out {
                MenuEntry::Item(id(action), label, None)
            } else {
                MenuEntry::Danger(id(action), label)
            });
        }
        if abilities.kick.allowed() && member_lock(store, server, user, &abilities.kick).is_none() {
            let label = tr!("Expulsar a {name}", name);
            items.push(MenuEntry::Danger(id("kick"), label));
        }
        if abilities.ban.allowed() && member_lock(store, server, user, &abilities.ban).is_none() {
            let label = tr!("Vetar a {name}", name);
            items.push(MenuEntry::Danger(id("ban"), label));
        }
    }
    items
}

/// `{server}:{user}:{what}` from [`member_menu_group`]: a role changes right
/// away, with the menu left open for the next, as the official client does;
/// the rest close it and open the members page and the same dialog as there.
fn member_from_outside(ui: &mut Ui, store: &Store, member: &str, now: Instant) -> Vec<Intent> {
    let Some((server, rest)) = member.split_once(':') else {
        return Vec::new();
    };
    let Some(server) = server.parse().ok().map(ServerId) else {
        return Vec::new();
    };
    let Some((user, what)) = rest.split_once(':') else {
        return Vec::new();
    };
    let Some(user) = user.parse().ok().map(UserId) else {
        return Vec::new();
    };
    let mut intents = Vec::new();
    if let Some(role) = what.strip_prefix("role:") {
        let role = role.parse().ok().map(RoleId);
        if let Some(target) = store.server(server).and_then(|found| {
            found
                .roles
                .iter()
                .find(|candidate| Some(candidate.id) == role)
        }) {
            let has = store
                .member_of(server, user)
                .is_some_and(|member| member.roles.contains(&target.id));
            give(store, server, user, target, !has, &mut intents);
        }
        return intents;
    }
    ui.voice.menu = None;
    intents.extend(open(ui, store, server));
    let Some(mut settings) = ui.server_settings.take() else {
        return intents;
    };
    clicked(&mut settings, ui, store, "page:members", now, &mut intents);
    clicked(
        &mut settings,
        ui,
        store,
        &format!("member:{user}:{what}"),
        now,
        &mut intents,
    );
    ui.server_settings = Some(settings);
    intents
}

/// Open the settings of `server`.
fn open(ui: &mut Ui, store: &Store, server: ServerId) -> Vec<Intent> {
    let Some(found) = store.server(server) else {
        return Vec::new();
    };
    let mut settings = ServerSettings::new(server);
    // Server administration, rest (P7): the profile's traits are read.
    let loads = rest::load(&mut settings, Page::Profile, ui, store);
    ui.server_settings = Some(settings);
    // What floated over the window goes: the settings cover it.
    ui.profile = None;
    ui.status_menu = false;
    ui.emoji_open = false;
    ui.reacting = None;
    ui.switcher = None;
    ui.settings = None;
    let mut intents = vec![
        set_input(NAME, &found.name),
        set_input(DESCRIPTION, found.description.as_deref().unwrap_or("")),
    ];
    for query in [
        ROLES_QUERY,
        PERMISSIONS_QUERY,
        ROLE_MEMBERS_QUERY,
        MEMBERS_QUERY,
        BANS_QUERY,
    ] {
        intents.push(set_input(query, ""));
    }
    intents.extend(loads.into_iter().map(Intent::Command));
    intents
}

fn set_input(id: &str, value: &str) -> Intent {
    Intent::SetInput {
        id: id.to_owned(),
        value: value.to_owned(),
    }
}

/// Apply an `ss:` action of the settings; `None` for the rest of the window.
pub fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let id = match action {
        UiAction::Clicked { id }
        | UiAction::InputChanged { id, .. }
        | UiAction::Submitted { id, .. }
        | UiAction::DragStarted { id, .. }
        | UiAction::DragMoved { id, .. }
        | UiAction::Dropped { id, .. }
        | UiAction::DragCancelled { id } => id,
        _ => return None,
    };
    let rest = id.strip_prefix("ss:")?;
    if let Some(member) = rest.strip_prefix("member-of:") {
        return Some(member_from_outside(ui, store, member, now));
    }
    if let Some(open_rest) = rest.strip_prefix("open:") {
        // `ss:open:{server}:delete` opens them with the deletion asked for.
        let (server, delete) = match open_rest.split_once(':') {
            Some((server, "delete")) => (server, true),
            _ => (open_rest, false),
        };
        let Ok(server) = server.parse::<u64>() else {
            return Some(Vec::new());
        };
        let server = ServerId(server);
        let mut intents = open(ui, store, server);
        if delete && store.owns(server) {
            if let Some(settings) = ui.server_settings.as_mut() {
                settings.delete_server = Some(String::new());
            }
            intents.push(set_input(DELETE_NAME, ""));
            intents.push(Intent::Focus(DELETE_NAME.into()));
        }
        return Some(intents);
    }
    let Some(mut settings) = ui.server_settings.take() else {
        return Some(Vec::new());
    };
    settings.nudged = false;
    let mut intents = Vec::new();
    let keep = match action {
        UiAction::Clicked { .. } => clicked(&mut settings, ui, store, rest, now, &mut intents),
        UiAction::InputChanged { value, .. } => {
            if !onboarding::typed(&mut settings, id, value)
                // AutoMod (automod).
                && !automod::typed(&mut settings, id, value)
                && !advanced::typed(&mut settings, id, value)
                && !rest::typed(&mut settings, store, id, value)
            {
                typed(&mut settings, id, value);
            }
            true
        }
        UiAction::Submitted { .. } => {
            // Enter in a dialog's field does what its main button does.
            let confirm = match id.as_str() {
                NICK => Some("nick:save"),
                REASON => Some("removal:confirm"),
                DELETE_NAME => Some("delete-server:confirm"),
                EXPRESSION_NAME => Some("rename:save"),
                other => advanced::submitted(other),
            };
            match confirm {
                Some(confirm) => clicked(&mut settings, ui, store, confirm, now, &mut intents),
                None => true,
            }
        }
        // Server administration, rest (P7): channels are dragged too.
        UiAction::DragStarted { .. }
        | UiAction::DragMoved { .. }
        | UiAction::Dropped { .. }
        | UiAction::DragCancelled { .. }
            if rest.starts_with("chan:") =>
        {
            rest::dragged(&mut settings, store, action);
            true
        }
        UiAction::DragStarted { .. } => {
            settings.dragging = role_of(rest).filter(|role| movable(store, settings.server, *role));
            true
        }
        UiAction::DragMoved { over, .. } => {
            settings.drop_over = over
                .as_deref()
                .and_then(|over| over.strip_prefix("ss:"))
                .and_then(role_of);
            true
        }
        UiAction::Dropped { target, .. } => {
            let target = target
                .as_deref()
                .and_then(|target| target.strip_prefix("ss:"))
                .and_then(role_of);
            if let (Some(role), Some(target)) = (settings.dragging, target) {
                let order = shown_order(&settings, store);
                if let Some(to) = order.iter().position(|id| *id == target) {
                    move_role(&mut settings, store, role, to);
                }
            }
            settings.dragging = None;
            settings.drop_over = None;
            true
        }
        UiAction::DragCancelled { .. } => {
            settings.dragging = None;
            settings.drop_over = None;
            true
        }
        _ => true,
    };
    if keep {
        ui.server_settings = Some(settings);
    }
    // What is confirmed shows before the provider answers.
    for intent in &intents {
        if let Intent::Command(command) = intent {
            store.ahead(command);
        }
    }
    Some(intents)
}

/// The role a row's id names: `role:{id}`.
fn role_of(rest: &str) -> Option<RoleId> {
    rest.strip_prefix("role:")?.parse().ok().map(RoleId)
}

/// Esc while the settings are open: close what floats inside them, else the
/// settings, unless something is not saved: then the bar insists instead.
/// A drag in course is limn's to cancel.
pub fn intercept(ui: &mut Ui, store: &Store, event: &UiEvent) -> Option<Vec<Intent>> {
    let settings = ui.server_settings.as_mut()?;
    if !matches!(
        event,
        UiEvent::Key {
            key: Key::Escape,
            ..
        }
    ) || settings.dragging.is_some()
        || settings.rest.dragging.is_some()
    {
        return None;
    }
    if !settings.close_overlay() {
        leave(ui, store);
    }
    Some(Vec::new())
}

/// Close the settings, unless something is not saved.
fn leave(ui: &mut Ui, store: &Store) {
    let Some(settings) = ui.server_settings.as_mut() else {
        return;
    };
    if settings.dirty(store) {
        settings.nudged = true;
    } else {
        ui.server_settings = None;
    }
}

/// Answer what an event of the provider changed. A removed server closes its
/// settings; a change of the server updates the drafts and continues a
/// reorder; a made role opens in the editor.
pub fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied, now: Instant) -> Vec<Intent> {
    let Some(mut settings) = ui.server_settings.take() else {
        return Vec::new();
    };
    let server = settings.server;
    if store.server(server).is_none() {
        // Removed, or gone from a new session: there is nothing to set.
        return Vec::new();
    }
    let mut intents = Vec::new();
    advanced::on_applied(&mut settings, ui, store, applied, now);
    rest::on_applied(&mut settings, ui, store, applied, now);
    // Rules and onboarding (2026-10-18).
    intents.extend(
        onboarding::on_applied(&mut settings, ui, store, applied, now)
            .into_iter()
            .map(Intent::Command),
    );
    // AutoMod (automod).
    intents.extend(
        automod::on_applied(&mut settings, ui, store, applied)
            .into_iter()
            .map(Intent::Command),
    );
    match applied {
        Applied::Ready => {
            settings.pending = 0;
            settings.saving_order = None;
            settings.creating = None;
            refresh(&mut settings, store, &mut intents);
            // A new session forgot the lists: the open page asks again.
            let page = settings.page;
            reload(&mut settings, ui, store, &mut intents, page);
            for command in advanced::load(&settings, page, ui, store) {
                intents.push(Intent::Command(command));
            }
            for command in rest::load(&mut settings, page, ui, store) {
                intents.push(Intent::Command(command));
            }
        }
        Applied::Server(changed) if *changed == server => {
            settings.pending = settings.pending.saturating_sub(1);
            refresh(&mut settings, store, &mut intents);
            if settings.saving_order.is_some() {
                match next_move(&settings, store) {
                    Some(command) => intents.push(Intent::Command(command)),
                    None => settings.saving_order = None,
                }
            }
            reload(&mut settings, ui, store, &mut intents, Page::Audit);
            let gone =
                |expression: &ExpressionId| expression_name(store, server, *expression).is_none();
            if settings.rename.as_ref().is_some_and(|(of, _)| gone(of)) {
                settings.rename = None;
            }
            if settings.delete_expression.as_ref().is_some_and(gone) {
                settings.delete_expression = None;
            }
        }
        Applied::Members(changed) if *changed == server => {
            let gone = |user: &UserId| store.member_of(server, *user).is_none();
            if settings.member_menu.as_ref().is_some_and(gone) {
                settings.member_menu = None;
            }
            if settings.role_picker.as_ref().is_some_and(gone) {
                settings.role_picker = None;
            }
            if settings.nick.as_ref().is_some_and(|(user, _)| gone(user)) {
                settings.nick = None;
            }
            if settings
                .removal
                .as_ref()
                .is_some_and(|removal| gone(&removal.user))
            {
                settings.removal = None;
            }
            if let Some(chosen) = &mut settings.add_members {
                chosen.retain(|user| !gone(user));
            }
            reload(&mut settings, ui, store, &mut intents, Page::Audit);
        }
        Applied::Bans(changed) | Applied::Invites(changed) if *changed == server => {
            reload(&mut settings, ui, store, &mut intents, Page::Audit);
        }
        Applied::Failed { failure, .. } => {
            let ours = match failure {
                Failure::Server { server: Some(of) } | Failure::Role { server: of } => {
                    *of == server
                }
                Failure::Member { server: of, .. } => *of == server,
                _ => false,
            };
            if ours {
                settings.pending = 0;
                settings.saving_order = None;
                if matches!(failure, Failure::Role { .. }) {
                    settings.creating = None;
                }
            }
        }
        Applied::Created {
            request,
            created: Created::Role(of, role),
        } if settings.creating == Some(*request) && *of == server => {
            settings.creating = None;
            let name = store
                .server(server)
                .and_then(|found| found.roles.iter().find(|candidate| candidate.id == *role))
                .map(|found| found.name.clone());
            // Only where it was asked for, and without hiding changes not saved.
            let here = settings.page == Page::Roles
                && store
                    .server(server)
                    .is_some_and(|found| settings.role_changes(found) == RoleEdit::default());
            if let Some(name) = name.filter(|_| here) {
                settings.role = Some(*role);
                settings.role_edit = RoleEdit::default();
                settings.color_hex = None;
                intents.push(set_input(ROLE_NAME, &name));
            }
        }
        _ => {}
    }
    ui.server_settings = Some(settings);
    intents
}

/// Ask again for what `page` shows, while it is the one open.
fn reload(
    settings: &mut ServerSettings,
    ui: &mut Ui,
    store: &Store,
    intents: &mut Vec<Intent>,
    page: Page,
) {
    if settings.page == page {
        if let Some(command) = load(settings, page, ui, store, false) {
            intents.push(Intent::Command(command));
        }
    }
}

/// The command that brings what `page` lists: bans, invitations and the
/// audit log are asked for, the rest is already in the store. `more` asks
/// for the page after the last one loaded, if there is one.
fn load(
    settings: &ServerSettings,
    page: Page,
    ui: &mut Ui,
    store: &Store,
    more: bool,
) -> Option<Command> {
    let server = settings.server;
    let abilities = store.server_abilities(server)?;
    if !match page {
        Page::Bans => abilities.ban.allowed(),
        Page::Audit => abilities.view_audit_log.allowed(),
        Page::Invites => abilities.manage_invites.allowed() && !more,
        _ => false,
    } {
        return None;
    }
    let query = settings.audit_query();
    // The cursor is the end of what is loaded; nothing more past the end.
    let (after, before) = if more {
        match page {
            Page::Bans => {
                let list = store.bans(server).filter(|list| !list.reached_end)?;
                (Some(list.items.last()?.user), None)
            }
            _ => {
                let list = store
                    .audit(server, &query)
                    .filter(|list| !list.reached_end && list.items.len() < rune_state::AUDIT_MAX)?;
                (None, Some(list.items.last()?.id))
            }
        }
    } else {
        (None, None)
    };
    let request = ui.fresh_request();
    match page {
        Page::Bans => Some(Command::LoadBans {
            request,
            server,
            after,
            limit: PAGE,
        }),
        Page::Audit => Some(Command::LoadAudit {
            request,
            server,
            query,
            before,
            limit: PAGE,
        }),
        Page::Invites => Some(Command::LoadInvites { request, server }),
        _ => None,
    }
}

/// The name of `expression` in `server`, while it exists.
pub(crate) fn expression_name(
    store: &Store,
    server: ServerId,
    expression: ExpressionId,
) -> Option<String> {
    let found = store.server(server)?;
    match expression {
        ExpressionId::Emoji(id) => found
            .emojis
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.name.clone()),
        ExpressionId::Sticker(id) => found
            .stickers
            .iter()
            .find(|s| s.id == id)
            .map(|s| s.name.clone()),
    }
}

/// The expression a row's key names: `emoji` or `sticker`, and its id.
fn expression_of(kind: &str, id: &str) -> Option<ExpressionId> {
    let id = id.parse().ok()?;
    match kind {
        "emoji" => Some(ExpressionId::Emoji(EmojiId(id))),
        "sticker" => Some(ExpressionId::Sticker(StickerId(id))),
        _ => None,
    }
}

/// Keep of the drafts only what still differs from the store, and show the
/// store's values in the fields nobody edited.
fn refresh(settings: &mut ServerSettings, store: &Store, intents: &mut Vec<Intent>) {
    let Some(server) = store.server(settings.server) else {
        return;
    };
    settings.profile = server_changes(&settings.profile, server);
    if settings.profile.name.is_none() {
        intents.push(set_input(NAME, &server.name));
    }
    if settings.profile.description.is_none() {
        intents.push(set_input(
            DESCRIPTION,
            server.description.as_deref().unwrap_or(""),
        ));
    }
    // Server administration, rest (P7).
    rest::refresh(settings, store, intents);
    let exists = |role: &RoleId| server.roles.iter().any(|candidate| candidate.id == *role);
    match settings
        .role
        .and_then(|id| server.roles.iter().find(|role| role.id == id))
    {
        Some(role) => {
            settings.role_edit = role_changes(&settings.role_edit, role);
            if settings.role_edit.name.is_none() {
                intents.push(set_input(ROLE_NAME, &role.name));
            }
        }
        // Deleted, here or elsewhere: back to the list.
        None if settings.role.is_some() => {
            settings.role = None;
            settings.role_edit = RoleEdit::default();
            settings.editor_menu = false;
            settings.add_members = None;
            settings.color_hex = None;
        }
        None => {}
    }
    let ranked = ranked(store, settings.server);
    if settings
        .order
        .as_ref()
        .is_some_and(|order| *order == ranked || !same_roles(order, &ranked))
    {
        settings.order = None;
    }
    if !settings.role_menu.as_ref().is_none_or(exists) {
        settings.role_menu = None;
    }
    if !settings.delete_role.as_ref().is_none_or(exists) {
        settings.delete_role = None;
    }
    if settings
        .role_filter
        .as_ref()
        .is_some_and(|role| !exists(role))
    {
        settings.role_filter = None;
    }
}

/// The next move of the order being saved: the first role out of place goes
/// where the order wants it. `None` once the store has that order.
fn next_move(settings: &ServerSettings, store: &Store) -> Option<Command> {
    let target = settings.saving_order.as_ref()?;
    let current = ranked(store, settings.server);
    // Roles made or deleted meanwhile are left where they are.
    let mine: Vec<RoleId> = current
        .iter()
        .copied()
        .filter(|role| target.contains(role))
        .collect();
    let wanted: Vec<RoleId> = target
        .iter()
        .copied()
        .filter(|role| current.contains(role))
        .collect();
    let slot = mine.iter().zip(&wanted).position(|(is, want)| is != want)?;
    let role = wanted[slot];
    // It takes the place of the role in that slot now: positions count up
    // from the bottom, from 1.
    let place = current.iter().position(|id| *id == mine[slot])?;
    let position = (current.len() - place) as i32;
    // Recheck every move: membership can change while an earlier move is in flight.
    if !movable(store, settings.server, role) || position >= store.my_rank(settings.server) {
        return None;
    }
    Some(Command::EditRole {
        server: settings.server,
        role,
        edit: RoleEdit {
            position: Some(position),
            ..RoleEdit::default()
        },
    })
}

/// Put `role` at `to` in the shown order, if the account may: never above a
/// role it cannot change.
fn move_role(settings: &mut ServerSettings, store: &Store, role: RoleId, to: usize) {
    let server = settings.server;
    if !movable(store, server, role) {
        return;
    }
    let mut order = shown_order(settings, store);
    let Some(from) = order.iter().position(|id| *id == role) else {
        return;
    };
    let floor = order
        .iter()
        .take_while(|id| !movable(store, server, **id))
        .count();
    if from == to || to < floor || to >= order.len() {
        return;
    }
    order.remove(from);
    order.insert(to, role);
    settings.order = Some(order);
}

/// What a field's new text changes.
fn typed(settings: &mut ServerSettings, id: &str, value: &str) {
    let value = value.to_owned();
    match id {
        NAME => settings.profile.name = Some(value),
        DESCRIPTION => settings.profile.description = Some(Some(value)),
        ROLE_NAME => settings.role_edit.name = Some(value),
        ACCENT_HEX => {
            if let Some(color) = parse_hex(&value) {
                settings.profile.accent = Some(color);
            }
            settings.accent_hex = Some(value);
        }
        COLOR_HEX => {
            if let Some(color) = parse_hex(&value) {
                settings.role_edit.color = Some(Some(color));
            }
            settings.color_hex = Some(value);
        }
        NICK => {
            if let Some((_, nick)) = &mut settings.nick {
                *nick = value;
            }
        }
        REASON => {
            if let Some(removal) = &mut settings.removal {
                removal.reason = value;
            }
        }
        DELETE_NAME if settings.delete_server.is_some() => settings.delete_server = Some(value),
        ROLES_QUERY => settings.roles_query = value,
        PERMISSIONS_QUERY => settings.permissions_query = value,
        ROLE_MEMBERS_QUERY => settings.role_members_query = value,
        ADD_QUERY => settings.add_query = value,
        MEMBERS_QUERY => settings.members_query = value,
        BANS_QUERY => settings.bans_query = value,
        EXPRESSION_NAME => {
            if let Some((_, name)) = &mut settings.rename {
                *name = value;
            }
        }
        _ => {}
    }
}

/// A click inside the settings. False when it closes them.
fn clicked(
    settings: &mut ServerSettings,
    ui: &mut Ui,
    store: &Store,
    rest: &str,
    now: Instant,
    intents: &mut Vec<Intent>,
) -> bool {
    let server = settings.server;
    let Some(found) = store.server(server) else {
        return false;
    };
    let Some(abilities) = store.server_abilities(server) else {
        return false;
    };
    let parts: Vec<&str> = rest.split(':').collect();
    let user = |part: &str| part.parse().ok().map(UserId);
    let role = |part: &str| {
        part.parse()
            .ok()
            .map(RoleId)
            .and_then(|id| found.roles.iter().find(|role| role.id == id))
    };
    let editing = settings
        .role
        .and_then(|id| found.roles.iter().find(|role| role.id == id));
    let editable = editing.filter(|role| role_lock(store, server, role).is_none());
    match parts.as_slice() {
        ["close"] => {
            if settings.dirty(store) {
                settings.nudged = true;
            } else {
                return false;
            }
        }
        ["page", key] => {
            if let Some(page) = Page::from_key(key) {
                go_to(settings, ui, store, page, intents);
            }
        }
        ["reset"] => {
            settings.profile = ServerEdit::default();
            settings.role_edit = RoleEdit::default();
            settings.order = None;
            settings.rest.layout = None;
            settings.accent_hex = None;
            settings.color_hex = None;
            onboarding::reset(settings, store);
            refresh(settings, store, intents);
        }
        ["save"] => {
            // Rules and onboarding (2026-10-18): an onboarding Discord
            // would refuse is not sent.
            if let Some(problem) = onboarding::problem(settings, store) {
                ui.notify(t("No se guardó la incorporación"), Some(problem), true, now);
            } else if let Some(problem) = rest::community_problem(settings, store) {
                // Icon, banner and community (2026-10-19).
                ui.notify(t("No se guardó la comunidad"), Some(problem), true, now);
            } else if !save(settings, store, intents)
                || !onboarding::save(settings, store, intents, ui)
            {
                ui.notify(
                    t("Tus permisos cambiaron"),
                    Some(t("Revisa los cambios antes de guardar.").into()),
                    true,
                    now,
                );
            }
        }
        // Rules and onboarding (2026-10-18).
        ["onb", "reload"] => {
            settings.onboarding.failed = false;
            for command in onboarding::load(settings, Page::Onboarding, ui, store) {
                intents.push(Intent::Command(command));
            }
        }

        // The profile.
        ["accent", "custom"] if abilities.manage_server.allowed() => {
            let shown = shown_server(found, &settings.profile).accent;
            settings.accent_hex = match settings.accent_hex {
                Some(_) => None,
                None => {
                    intents.push(set_input(ACCENT_HEX, &hex(shown)));
                    Some(hex(shown))
                }
            };
        }
        ["accent", "dismiss"] => settings.accent_hex = None,
        ["accent", color] if abilities.manage_server.allowed() => {
            if let Some(color) = parse_hex(color) {
                settings.profile.accent = Some(color);
            }
        }

        // The roles page.
        ["role", "create"] | ["editor", "create"] => {
            if settings.role_changes(found) != RoleEdit::default() {
                settings.nudged = true;
            } else if abilities.manage_roles.allowed() && settings.creating.is_none() {
                let request = ui.fresh_request();
                settings.creating = Some(request);
                intents.push(Intent::Command(Command::CreateRole {
                    request,
                    server,
                    name: t(NEW_ROLE).into(),
                }));
            }
        }
        ["role", "everyone"] => open_role(settings, found, found.everyone_id(), intents),
        ["role", id] | ["role", id, "edit"] => {
            if let Some(target) = role(id) {
                open_role(settings, found, target.id, intents);
            }
        }
        ["role", id, "menu"] => {
            let id = role(id).map(|role| role.id);
            settings.role_menu = if settings.role_menu == id { None } else { id };
        }
        ["role-menu", "dismiss"] => settings.role_menu = None,
        ["role", id, step @ ("up" | "down")] => {
            settings.role_menu = None;
            if let Some(target) = role(id) {
                let order = shown_order(settings, store);
                if let Some(from) = order.iter().position(|role| *role == target.id) {
                    let to = if *step == "up" {
                        from.checked_sub(1)
                    } else {
                        Some(from + 1)
                    };
                    if let Some(to) = to {
                        move_role(settings, store, target.id, to);
                    }
                }
            }
        }
        ["role", id, "copy-id"] => {
            settings.role_menu = None;
            settings.editor_menu = false;
            if let Some(target) = role(id) {
                ui.notify(t("ID copiado"), None, false, now);
                intents.push(Intent::Copy(target.id.to_string()));
            }
        }
        ["role", id, "delete"] => {
            settings.role_menu = None;
            settings.editor_menu = false;
            settings.delete_role = role(id)
                .filter(|target| {
                    target.id != found.everyone_id() && role_lock(store, server, target).is_none()
                })
                .map(|target| target.id);
        }
        ["role-delete", "confirm"] => {
            if let Some(target) = settings.delete_role.take() {
                let lock = found
                    .roles
                    .iter()
                    .find(|candidate| candidate.id == target)
                    .and_then(|known| role_lock(store, server, known));
                match lock {
                    None if movable(store, server, target) => {
                        intents.push(Intent::Command(Command::DeleteRole {
                            server,
                            role: target,
                        }));
                    }
                    reason => refused(ui, t("No se eliminó el rol"), reason, now),
                }
            }
        }
        ["role-delete", "cancel"] => settings.delete_role = None,

        // The role editor.
        ["editor", "back"] => {
            if settings.role_changes(found) != RoleEdit::default() {
                settings.nudged = true;
            } else {
                settings.role = None;
                settings.role_edit = RoleEdit::default();
                settings.editor_menu = false;
                settings.color_hex = None;
            }
        }
        ["editor", "menu"] => settings.editor_menu = !settings.editor_menu,
        ["editor-menu", "dismiss"] => settings.editor_menu = false,
        ["tab", key] => {
            let everyone = settings.role == Some(found.everyone_id());
            settings.tab = match *key {
                "display" if !everyone => RoleTab::Display,
                "members" if !everyone => RoleTab::Members,
                "permissions" => RoleTab::Permissions,
                _ => settings.tab,
            };
            settings.color_hex = None;
        }
        ["color", "default"] if editable.is_some() => settings.role_edit.color = Some(None),
        ["color", "custom"] if editable.is_some() => {
            let shown = editable
                .map(|role| shown_role(role, &settings.role_edit))
                .and_then(|role| role.color)
                .unwrap_or(rune_components::server_settings::DEFAULT_ROLE_COLOR);
            settings.color_hex = match settings.color_hex {
                Some(_) => None,
                None => {
                    intents.push(set_input(COLOR_HEX, &hex(shown)));
                    Some(hex(shown))
                }
            };
        }
        ["color", "dismiss"] => settings.color_hex = None,
        ["color", color] if editable.is_some() => {
            if let Some(color) = parse_hex(color) {
                settings.role_edit.color = Some(Some(color));
            }
        }
        ["hoist"] => {
            if let Some(role) = editable {
                settings.role_edit.hoisted = Some(!shown_role(role, &settings.role_edit).hoisted);
            }
        }
        ["mentionable"] => {
            if let Some(role) = editable {
                settings.role_edit.mentionable =
                    Some(!shown_role(role, &settings.role_edit).mentionable);
            }
        }
        ["perm", bit] => {
            if let (Some(role), Some(permission)) = (editable, permission_of(bit)) {
                let shown = shown_role(role, &settings.role_edit).permissions;
                let on = !shown.contains(permission);
                if !on || permission_lock(store, server, permission).is_none() {
                    settings.role_edit.permissions = Some(shown.set(permission, on));
                }
            }
        }
        ["perms", "clear"] if editable.is_some() => {
            settings.role_edit.permissions = Some(Permissions::NONE);
        }
        ["role-members", "add"]
            if editing.is_some_and(|role| grant_lock(store, server, role).is_none()) =>
        {
            settings.add_members = Some(Vec::new());
            settings.add_query.clear();
            intents.push(set_input(ADD_QUERY, ""));
        }
        ["role-member", id, "remove"] => {
            if let (Some(target), Some(user)) = (editing, user(id)) {
                give(store, server, user, target, false, intents);
            }
        }
        ["add", "confirm"] => {
            if let (Some(target), Some(chosen)) = (editing, settings.add_members.take()) {
                for user in chosen {
                    give(store, server, user, target, true, intents);
                }
            }
        }
        ["add", "cancel"] => settings.add_members = None,
        ["add", id] => {
            if let (Some(chosen), Some(user)) = (&mut settings.add_members, user(id)) {
                match chosen.iter().position(|candidate| *candidate == user) {
                    Some(index) => {
                        chosen.remove(index);
                    }
                    None => chosen.push(user),
                }
            }
        }

        // The members page.
        ["members", "filter"] => settings.filter_open = !settings.filter_open,
        ["members-filter", "dismiss"] => settings.filter_open = false,
        ["members-filter", "all"] => {
            settings.role_filter = None;
            settings.filter_open = false;
        }
        ["members-filter", id] => {
            settings.role_filter = role(id).map(|role| role.id);
            settings.filter_open = false;
        }
        ["member", id, "roles"] => {
            let id = user(id);
            settings.role_picker = if settings.role_picker == id { None } else { id };
        }
        ["role-picker", "dismiss"] => settings.role_picker = None,
        ["member", id, "role", role_id] => {
            if let (Some(user), Some(target)) = (user(id), role(role_id)) {
                let has = store
                    .member_of(server, user)
                    .is_some_and(|member| member.roles.contains(&target.id));
                give(store, server, user, target, !has, intents);
            }
        }
        ["member", id, "menu"] => {
            let id = user(id);
            settings.member_menu = if settings.member_menu == id { None } else { id };
        }
        ["member-menu", "dismiss"] => settings.member_menu = None,
        ["member", id, "nick"] => {
            settings.member_menu = None;
            if let Some(user) = user(id).filter(|user| nick_lock(store, server, *user).is_none()) {
                let nick = store
                    .member_of(server, user)
                    .and_then(|member| member.nick.clone())
                    .unwrap_or_default();
                intents.push(set_input(NICK, &nick));
                intents.push(Intent::Focus(NICK.into()));
                settings.nick = Some((user, nick));
            }
        }
        ["member", id, what @ ("kick" | "ban")] => {
            settings.member_menu = None;
            let ban = *what == "ban";
            let permit = if ban { &abilities.ban } else { &abilities.kick };
            if let Some(user) = user(id).filter(|user| {
                Some(*user) != store.me() && member_lock(store, server, *user, permit).is_none()
            }) {
                intents.push(set_input(REASON, ""));
                settings.removal = Some(Removal {
                    user,
                    ban,
                    reason: String::new(),
                    history: 0,
                });
            }
        }
        ["member", id, "copy-id"] => {
            settings.member_menu = None;
            if let Some(user) = user(id) {
                ui.notify(t("ID copiado"), None, false, now);
                intents.push(Intent::Copy(user.to_string()));
            }
        }
        ["nick", "save"] => {
            if let Some((user, nick)) = settings.nick.take() {
                match nick_lock(store, server, user) {
                    None => {
                        let nick = Some(nick.trim().to_owned()).filter(|nick| !nick.is_empty());
                        intents.push(Intent::Command(Command::SetNick { server, user, nick }));
                    }
                    reason => refused(ui, t("No se cambió el apodo"), reason, now),
                }
            }
        }
        ["nick", "clear"] => {
            if let Some((user, _)) = settings
                .nick
                .take()
                .filter(|(user, _)| nick_lock(store, server, *user).is_none())
            {
                intents.push(Intent::Command(Command::SetNick {
                    server,
                    user,
                    nick: None,
                }));
            }
        }
        ["nick", "cancel"] => settings.nick = None,
        ["removal", "confirm"] => {
            if let Some(removal) = settings.removal.take().filter(|removal| {
                let permit = if removal.ban {
                    &abilities.ban
                } else {
                    &abilities.kick
                };
                let lock = member_lock(store, server, removal.user, permit);
                let allowed = Some(removal.user) != store.me() && lock.is_none();
                if !allowed {
                    refused(ui, t("No se sacó a la persona"), lock, now);
                }
                allowed
            }) {
                let reason = Some(removal.reason.trim().to_owned()).filter(|text| !text.is_empty());
                let (user, command) = (removal.user, removal.ban);
                let delete_message_seconds =
                    advanced::BAN_HISTORY[removal.history.min(advanced::BAN_HISTORY.len() - 1)].0;
                intents.push(Intent::Command(if command {
                    Command::Ban {
                        server,
                        user,
                        reason,
                        delete_message_seconds,
                    }
                } else {
                    Command::Kick {
                        server,
                        user,
                        reason,
                    }
                }));
            }
        }
        ["removal", "cancel"] => settings.removal = None,

        // The bans page.
        ["ban", id, "revoke"] if abilities.ban.allowed() => settings.unban = user(id),
        ["unban", "confirm"] => {
            if let Some(user) = settings.unban.take() {
                match abilities.ban.reason() {
                    None => intents.push(Intent::Command(Command::Unban { server, user })),
                    reason => refused(
                        ui,
                        t("No se revocó el veto"),
                        reason.map(str::to_owned),
                        now,
                    ),
                }
            }
        }
        ["unban", "cancel"] => settings.unban = None,
        [list @ ("bans" | "audit"), "more"] => {
            let page = if *list == "bans" {
                Page::Bans
            } else {
                Page::Audit
            };
            if let Some(command) = load(settings, page, ui, store, true) {
                intents.push(Intent::Command(command));
            }
        }

        // The invitations page.
        ["invite", code, "revoke"] if abilities.manage_invites.allowed() => {
            settings.revoke_invite = store
                .invites(server)
                .and_then(|invites| invites.iter().find(|invite| invite.code == *code))
                .map(|invite| invite.code.clone());
        }
        ["invite-revoke", "confirm"] => {
            if let Some(code) = settings.revoke_invite.take() {
                let listed = store
                    .invites(server)
                    .is_some_and(|invites| invites.iter().any(|invite| invite.code == code));
                match abilities.manage_invites.reason() {
                    None if listed => {
                        intents.push(Intent::Command(Command::RevokeInvite { server, code }))
                    }
                    None => refused(
                        ui,
                        t("No se revocó la invitación"),
                        Some(t("Esa invitación ya no existe.").into()),
                        now,
                    ),
                    reason => refused(
                        ui,
                        t("No se revocó la invitación"),
                        reason.map(str::to_owned),
                        now,
                    ),
                }
            }
        }
        ["invite-revoke", "cancel"] => settings.revoke_invite = None,

        // The emoji and sticker pages.
        [kind @ ("emoji" | "sticker"), id, "rename"] if abilities.manage_expressions.allowed() => {
            if let Some(expression) = expression_of(kind, id) {
                if let Some(name) = expression_name(store, server, expression) {
                    intents.push(set_input(EXPRESSION_NAME, &name));
                    intents.push(Intent::Focus(EXPRESSION_NAME.into()));
                    settings.rename = Some((expression, name));
                }
            }
        }
        [kind @ ("emoji" | "sticker"), id, "delete"] if abilities.manage_expressions.allowed() => {
            settings.delete_expression = expression_of(kind, id)
                .filter(|expression| expression_name(store, server, *expression).is_some());
        }
        ["rename", "save"] => {
            if let Some((expression, name)) = settings.rename.clone() {
                let name = name.trim().to_owned();
                let valid = match expression {
                    ExpressionId::Emoji(_) => valid_emoji_name(&name),
                    ExpressionId::Sticker(_) => valid_sticker_name(&name),
                };
                let current = expression_name(store, server, expression);
                match abilities.manage_expressions.reason() {
                    // Kept open: the dialog says what a name needs.
                    None if !valid => {}
                    None if current.is_none() => {
                        settings.rename = None;
                        refused(
                            ui,
                            t("No se renombró"),
                            Some(t("Ya no existe.").into()),
                            now,
                        );
                    }
                    None => {
                        settings.rename = None;
                        if current.as_deref() != Some(name.as_str()) {
                            intents.push(Intent::Command(Command::RenameExpression {
                                server,
                                expression,
                                name,
                            }));
                        }
                    }
                    reason => {
                        settings.rename = None;
                        refused(ui, t("No se renombró"), reason.map(str::to_owned), now);
                    }
                }
            }
        }
        ["rename", "cancel"] => settings.rename = None,
        ["expression-delete", "confirm"] => {
            if let Some(expression) = settings.delete_expression.take() {
                match abilities.manage_expressions.reason() {
                    None if expression_name(store, server, expression).is_some() => {
                        intents.push(Intent::Command(Command::DeleteExpression {
                            server,
                            expression,
                        }));
                    }
                    None => refused(ui, t("No se eliminó"), Some(t("Ya no existe.").into()), now),
                    reason => refused(ui, t("No se eliminó"), reason.map(str::to_owned), now),
                }
            }
        }
        ["expression-delete", "cancel"] => settings.delete_expression = None,

        // The audit log.
        ["audit", "user"] => toggle_filter(settings, AuditFilter::User),
        ["audit", "action"] => toggle_filter(settings, AuditFilter::Action),
        ["audit-filter", "dismiss"] => settings.audit_filter = None,
        ["audit-user", id] => {
            settings.audit_user = user(id);
            settings.audit_filter = None;
            refilter(settings, ui, store, intents);
        }
        ["audit-action", index] => {
            settings.audit_action = index
                .parse::<usize>()
                .ok()
                .and_then(|index| AUDIT_ACTIONS.get(index))
                .map(|(kind, _)| *kind);
            settings.audit_filter = None;
            refilter(settings, ui, store, intents);
        }
        ["audit", entry] => {
            if let Ok(entry) = entry.parse() {
                if !settings.expanded.remove(&entry) {
                    settings.expanded.insert(entry);
                }
            }
        }

        // Deleting the server.
        ["delete-server"] if store.owns(server) => {
            settings.delete_server = Some(String::new());
            intents.push(set_input(DELETE_NAME, ""));
            intents.push(Intent::Focus(DELETE_NAME.into()));
        }
        ["delete-server", "confirm"] => {
            let typed = settings.delete_server.as_deref().map(str::trim);
            if typed == Some(found.name.as_str()) && abilities.delete.allowed() {
                settings.delete_server = None;
                intents.push(Intent::Command(Command::DeleteServer { server }));
            }
        }
        ["delete-server", "cancel"] => settings.delete_server = None,
        _ => {
            if !onboarding::clicked(settings, store, parts.as_slice(), intents)
                // AutoMod (automod).
                && !automod::clicked(settings, ui, store, parts.as_slice(), now, intents)
                && !advanced::clicked(settings, ui, store, parts.as_slice(), now, intents)
            {
                rest::clicked(settings, ui, store, parts.as_slice(), now, intents);
            }
        }
    }
    true
}

/// The provider filters the log: a new filter asks for its first page.
fn refilter(settings: &mut ServerSettings, ui: &mut Ui, store: &Store, intents: &mut Vec<Intent>) {
    settings.expanded.clear();
    if let Some(command) = load(settings, Page::Audit, ui, store, false) {
        intents.push(Intent::Command(command));
    }
}

/// A confirmed change was not sent: the permission or the target changed
/// since the dialog opened. Say why.
fn refused(ui: &mut Ui, title: &str, reason: Option<String>, now: Instant) {
    ui.notify(
        title,
        Some(reason.unwrap_or_else(|| t("Tus permisos cambiaron.").into())),
        true,
        now,
    );
}

fn toggle_filter(settings: &mut ServerSettings, filter: AuditFilter) {
    settings.audit_filter = if settings.audit_filter == Some(filter) {
        None
    } else {
        Some(filter)
    };
}

/// Give `role` to `user`, or take it away, if the account may.
fn give(
    store: &Store,
    server: ServerId,
    user: UserId,
    role: &Role,
    on: bool,
    intents: &mut Vec<Intent>,
) {
    if grant_lock(store, server, role).is_some() || role.id == RoleId(server.0) {
        return;
    }
    let Some(member) = store.member_of(server, user) else {
        return;
    };
    if member.roles.contains(&role.id) == on {
        return;
    }
    let mut roles: Vec<RoleId> = member
        .roles
        .iter()
        .copied()
        .filter(|candidate| *candidate != role.id)
        .collect();
    if on {
        roles.push(role.id);
    }
    intents.push(Intent::Command(Command::SetMemberRoles {
        server,
        user,
        roles,
    }));
}

/// Go to `page`, unless the one open has changes not saved.
fn go_to(
    settings: &mut ServerSettings,
    ui: &mut Ui,
    store: &Store,
    page: Page,
    intents: &mut Vec<Intent>,
) {
    if settings.page == page && settings.role.is_none() {
        return;
    }
    if settings.dirty(store) {
        settings.nudged = true;
        return;
    }
    while settings.close_overlay() {}
    settings.page = page;
    settings.role = None;
    settings.role_edit = RoleEdit::default();
    settings.profile = ServerEdit::default();
    settings.order = None;
    settings.rest.layout = None;
    settings.onboarding.draft = None;
    if let Some(command) = load(settings, page, ui, store, false) {
        intents.push(Intent::Command(command));
    }
    for command in advanced::load(settings, page, ui, store) {
        intents.push(Intent::Command(command));
    }
    for command in rest::load(settings, page, ui, store) {
        intents.push(Intent::Command(command));
    }
    // Rules and onboarding (2026-10-18).
    for command in onboarding::load(settings, page, ui, store) {
        intents.push(Intent::Command(command));
    }
    // AutoMod (automod).
    for command in automod::load(settings, page, ui, store) {
        intents.push(Intent::Command(command));
    }
    refresh(settings, store, intents);
}

/// Open the editor on `role`, unless the open one has changes not saved.
fn open_role(
    settings: &mut ServerSettings,
    server: &Server,
    role: RoleId,
    intents: &mut Vec<Intent>,
) {
    if settings.role == Some(role) {
        return;
    }
    let Some(found) = server.roles.iter().find(|candidate| candidate.id == role) else {
        return;
    };
    if settings.role_changes(server) != RoleEdit::default() {
        settings.nudged = true;
        return;
    }
    settings.page = Page::Roles;
    settings.role = Some(role);
    settings.role_edit = RoleEdit::default();
    settings.role_menu = None;
    settings.editor_menu = false;
    settings.color_hex = None;
    settings.rest.gradient_hex = None;
    settings.rest.role_icon_preview = None;
    intents.push(set_input(
        rest::ROLE_EMOJI,
        found.style.emoji.as_deref().unwrap_or(""),
    ));
    settings.role_members_query.clear();
    if role == server.everyone_id() {
        settings.tab = RoleTab::Permissions;
    }
    intents.push(set_input(ROLE_NAME, &found.name));
    intents.push(set_input(ROLE_MEMBERS_QUERY, ""));
}

/// Send what differs from the store: the profile, the open role, and the
/// order of the roles, one move at a time.
fn save(settings: &mut ServerSettings, store: &Store, intents: &mut Vec<Intent>) -> bool {
    if settings.saving() {
        return true;
    }
    let Some(server) = store.server(settings.server) else {
        return false;
    };
    let profile = server_changes(&settings.profile, server);
    let edit = settings.role_changes(server);
    // Validate the whole draft against current permissions before emitting anything.
    let Some(abilities) = store.server_abilities(server.id) else {
        return false;
    };
    let basic = ServerEdit {
        name: profile.name.clone(),
        description: profile.description.clone(),
        accent: profile.accent,
        ..ServerEdit::default()
    };
    if (basic != ServerEdit::default() && !abilities.manage_server.allowed())
        // Server administration, rest (P7).
        || (rest::touches_identity(&profile) && !abilities.edit_identity.allowed())
        || (rest::touches_safety(&profile) && !abilities.edit_safety.allowed())
        || (edit.icon.is_some() && !abilities.role_icons.allowed())
        || (edit.gradient.is_some() && !abilities.role_gradients.allowed())
        || (rest::layout_changed(settings, store) && !abilities.reorder_channels.allowed())
        // Icon, banner and community (2026-10-19).
        || rest::denied_looks(&profile, &abilities)
    {
        return false;
    }
    if edit != RoleEdit::default() {
        let Some(role) = server
            .roles
            .iter()
            .find(|role| Some(role.id) == settings.role)
        else {
            return false;
        };
        if role_lock(store, server.id, role).is_some()
            || edit.permissions.is_some_and(|permissions| {
                !store
                    .server_permissions(server.id)
                    .contains(permissions.without(role.permissions))
            })
        {
            return false;
        }
    }
    if settings.order_changed(store) {
        // Fixed roles must retain their slots even after a demotion during editing.
        let current = ranked(store, server.id);
        let wanted = shown_order(settings, store);
        if current.iter().zip(&wanted).any(|(before, after)| {
            before != after
                && (!movable(store, server.id, *before) || !movable(store, server.id, *after))
        }) {
            return false;
        }
    }
    // Server administration, rest (P7): pictures are let go once answered.
    settings.rest.pictures_sent |= profile.icon.is_some()
        || profile.banner.is_some()
        || matches!(edit.icon, Some(Some(rune_provider::RoleIcon::Image(_))));
    if profile != ServerEdit::default() {
        settings.pending += 1;
        intents.push(Intent::Command(Command::EditServer {
            server: server.id,
            edit: profile,
        }));
    }
    if let (Some(role), true) = (settings.role, edit != RoleEdit::default()) {
        settings.pending += 1;
        intents.push(Intent::Command(Command::EditRole {
            server: server.id,
            role,
            edit,
        }));
    }
    if settings.order_changed(store) {
        settings.saving_order = settings.order.clone();
        match next_move(settings, store) {
            Some(command) => intents.push(Intent::Command(command)),
            None => settings.saving_order = None,
        }
    }
    // Server administration, rest (P7).
    rest::save_layout(settings, store, intents)
}
