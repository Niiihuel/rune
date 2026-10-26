//! The server's menu, under its name at the top of the column, and what it
//! opens: a new channel or category, the notification settings, the
//! account's name in the server, and leaving it; and a new server, from the
//! rail's "+". The server's settings belong to another window: the menu only
//! asks for them, with `ss:open:{server}`.
//!
//! Nothing here changes the domain ahead of the provider: a creation waits
//! for its `Created`, and then adds who may see a private channel and opens
//! what was made.

use std::collections::HashMap;
use std::time::Instant;

use limn::{Element, Icon, Modal};
use rune_components::channel_menu::{grouped, Entry, Item};
use rune_components::server_menu::{
    self as parts, ids, Audience, ChannelForm, Choice, NickForm, NotifyForm, ServerForm, NICK_MAX,
};
use rune_components::{sidebar, Look};
use rune_i18n::{t, tr};
use rune_model::{
    ChannelId, ChannelKind, NotificationChange, Overwrite, OverwriteTarget, Permissions, Rgb,
    RoleId, ServerId, Timestamp, UserId,
};
use rune_provider::{Command, Created, RequestId, ServerEdit};
use rune_state::{Applied, Permit, Store};

pub use rune_components::server_menu::NotifyLevel;

use crate::person_menu::INVITE_AGE_SECS;
use crate::server_settings::advanced::{invite_channels, invite_link};
use crate::{open, open_server, Intent, Ui, ViewContext};

pub(crate) const TOGGLE: &str = ids::TOGGLE;
/// The setting the notification settings of every server are kept in.
pub(crate) const NOTIFY_SETTING: &str = "server_notifications";
/// Muted until turned back on.
pub const FOREVER: Timestamp = Timestamp(i64::MAX);

/// How a server marks what is new, as the account set it on this machine.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Notify {
    pub level: NotifyLevel,
    /// Muted until then; [`FOREVER`] until turned back on.
    pub muted_until: Option<Timestamp>,
}

impl Notify {
    pub fn muted(&self, now: Timestamp) -> bool {
        self.muted_until.is_some_and(|until| until > now)
    }
}

/// The server's menu, the dialog open from it, the notification settings of
/// each server, and what was asked to be made.
#[derive(Debug, Default)]
pub struct ServerMenu {
    pub open: bool,
    pub dialog: Option<Dialog>,
    pub notify: HashMap<ServerId, Notify>,
    pending: HashMap<RequestId, Pending>,
}

/// A dialog of the server's menu.
#[derive(Clone, Debug, PartialEq)]
pub enum Dialog {
    /// A new channel or category.
    Channel(NewChannel),
    Notifications(ServerId),
    /// The account's name in the server, as written in the field.
    Nick {
        server: ServerId,
        nick: String,
    },
    Leave(ServerId),
    /// A new server: `None` while choosing how to start.
    Server(Option<ServerForm>),
}

/// A channel or category being made.
#[derive(Clone, Debug, PartialEq)]
pub struct NewChannel {
    pub server: ServerId,
    pub parent: Option<ChannelId>,
    /// `Text`, `Voice` or `Category`.
    pub kind: ChannelKind,
    pub name: String,
    pub private: bool,
    /// On the second step: choosing who may see it.
    pub choosing: bool,
    pub query: String,
    pub roles: Vec<RoleId>,
    pub members: Vec<UserId>,
    /// The name last looked up at the provider for who may see it.
    pub lookup: Option<String>,
}

impl NewChannel {
    fn form(&self) -> ChannelForm {
        ChannelForm {
            category: self.kind == ChannelKind::Category,
            voice: self.kind == ChannelKind::Voice,
            name: self.name.clone(),
            private: self.private,
            audience: None,
        }
    }
}

/// What to do once something asked for is made.
#[derive(Debug)]
enum Pending {
    /// Let `targets` see the channel, and open it if it has messages.
    Channel {
        targets: Vec<OverwriteTarget>,
        open: bool,
    },
    /// Give the server this color, and open it.
    Server { accent: Rgb },
    /// Copy its invitation's link.
    Invite,
}

/// The time on this machine's clock, for when a mute ends.
pub(crate) fn wall_now() -> Timestamp {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Timestamp(elapsed.as_millis() as i64)
}

fn set_input(id: &str, value: impl Into<String>) -> Intent {
    Intent::SetInput {
        id: id.into(),
        value: value.into(),
    }
}

/// Whether `id` is an action of this module.
pub(crate) fn owns(id: &str) -> bool {
    ["sm:", "newchan:", "newcat:", "newsrv:"]
        .iter()
        .any(|prefix| id.starts_with(prefix))
}

/// Answer a click on the menu or its dialogs.
pub(crate) fn clicked(ui: &mut Ui, store: &mut Store, id: &str, now: Instant) -> Vec<Intent> {
    let number = |prefix: &str| {
        id.strip_prefix(prefix)
            .and_then(|rest| rest.parse::<u64>().ok())
    };
    // Opening: from the menu, or from elsewhere with the same IDs.
    if let Some(rest) = id.strip_prefix("newchan:") {
        let (server, parent) = rest.split_once(':').unwrap_or((rest, ""));
        if let Ok(server) = server.parse().map(ServerId) {
            let parent = parent.parse().ok().map(ChannelId);
            return open_channel_dialog(ui, store, server, parent, ChannelKind::Text, now);
        }
    }
    if let Some(server) = number("newcat:") {
        let server = ServerId(server);
        return open_channel_dialog(ui, store, server, None, ChannelKind::Category, now);
    }
    if let Some(server) = number("sm:notify:") {
        ui.server_menu.dialog = Some(Dialog::Notifications(ServerId(server)));
        return Vec::new();
    }
    // The menu's entries of the rail and the header: a mute, a level, a check.
    if let Some((server, key)) = id.strip_prefix("sm:mute:").and_then(split_server) {
        let until = if key == "off" {
            None
        } else {
            let Some((_, _, length)) = parts::MUTE_DURATIONS
                .iter()
                .find(|(candidate, _, _)| *candidate == key)
            else {
                return Vec::new();
            };
            Some(length.map_or(FOREVER, |length| Timestamp(wall_now().0 + length)))
        };
        return change_notify_for(ui, store, server, NotificationChange::Mute(until), now);
    }
    if let Some((server, key)) = id.strip_prefix("sm:level:").and_then(split_server) {
        let Some(level) = NotifyLevel::from_key(key) else {
            return Vec::new();
        };
        return change_notify_for(ui, store, server, NotificationChange::Level(level), now);
    }
    if let Some((server, flag)) = id.strip_prefix("sm:flag:").and_then(split_server) {
        let kept = store
            .notifications(Some(server))
            .cloned()
            .unwrap_or_default();
        let change = match flag {
            "everyone" => NotificationChange::SuppressEveryone(!kept.suppress_everyone),
            "roles" => NotificationChange::SuppressRoles(!kept.suppress_roles),
            "mobile" => NotificationChange::MobilePush(!kept.mobile_push),
            "hide" => NotificationChange::HideMuted(!kept.hide_muted),
            _ => return Vec::new(),
        };
        return change_notify_for(ui, store, server, change, now);
    }
    if let Some(server) = number("sm:invite:") {
        return invite(ui, store, ServerId(server), now);
    }
    if let Some(server) = number("sm:profile:") {
        let server = ServerId(server);
        let nick = store
            .me()
            .and_then(|me| store.member_of(server, me))
            .and_then(|member| member.nick.clone())
            .unwrap_or_default();
        ui.server_menu.dialog = Some(Dialog::Nick {
            server,
            nick: nick.clone(),
        });
        return vec![set_input(ids::NICK, nick), Intent::Focus(ids::NICK.into())];
    }
    if let Some(server) = number("sm:leave:") {
        let server = ServerId(server);
        let permit = store.server_abilities(server).map(|found| found.leave);
        if let Some(reason) = permit.as_ref().and_then(Permit::reason) {
            ui.notify(
                t("No puedes abandonar el servidor"),
                Some(reason.into()),
                true,
                now,
            );
        } else if permit.is_some() {
            ui.server_menu.dialog = Some(Dialog::Leave(server));
        }
        return Vec::new();
    }
    if let Some(server) = number("sm:copy-id:") {
        ui.notify(t("ID copiado"), None, false, now);
        return vec![Intent::Copy(server.to_string())];
    }
    // Reads (P2): the whole server at once when the provider takes it.
    if let Some(server) = number("sm:read:") {
        return store
            .mark_server_read_commands(ServerId(server))
            .into_iter()
            .map(Intent::Command)
            .collect();
    }
    if let Some(role) = number(ids::CHANNEL_ROLE) {
        if let Some(Dialog::Channel(form)) = &mut ui.server_menu.dialog {
            toggle(&mut form.roles, RoleId(role));
        }
        return Vec::new();
    }
    if let Some(user) = number(ids::CHANNEL_MEMBER) {
        if let Some(Dialog::Channel(form)) = &mut ui.server_menu.dialog {
            toggle(&mut form.members, UserId(user));
        }
        return Vec::new();
    }
    if let Some(level) = id
        .strip_prefix(ids::NOTIFY_LEVEL)
        .and_then(NotifyLevel::from_key)
    {
        return change_notify(ui, store, NotificationChange::Level(level), now);
    }
    if let Some(key) = id.strip_prefix(ids::NOTIFY_FOR) {
        let Some((_, _, length)) = parts::MUTE_DURATIONS
            .iter()
            .find(|(candidate, _, _)| *candidate == key)
        else {
            return Vec::new();
        };
        let until = length.map_or(FOREVER, |length| Timestamp(wall_now().0 + length));
        return change_notify(ui, store, NotificationChange::Mute(Some(until)), now);
    }
    if let Some(color) = id
        .strip_prefix(ids::SERVER_COLOR)
        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
    {
        if let Some(Dialog::Server(Some(form))) = &mut ui.server_menu.dialog {
            form.accent = Rgb(color);
        }
        return Vec::new();
    }
    let dialog = &mut ui.server_menu.dialog;
    match id {
        ids::TOGGLE => {
            ui.server_menu.open = !ui.server_menu.open && ui.server.is_some();
            Vec::new()
        }
        ids::DISMISS => {
            ui.server_menu.open = false;
            Vec::new()
        }
        ids::CHANNEL_CLOSE
        | ids::NOTIFY_CLOSE
        | ids::NICK_CLOSE
        | ids::LEAVE_CANCEL
        | ids::SERVER_CLOSE => {
            *dialog = None;
            Vec::new()
        }
        ids::CHANNEL_TEXT | ids::CHANNEL_VOICE => {
            if let Some(Dialog::Channel(form)) = dialog {
                if form.kind != ChannelKind::Category {
                    form.kind = if id == ids::CHANNEL_VOICE {
                        ChannelKind::Voice
                    } else {
                        ChannelKind::Text
                    };
                }
            }
            Vec::new()
        }
        ids::CHANNEL_PRIVATE => {
            if let Some(Dialog::Channel(form)) = dialog {
                form.private = !form.private;
            }
            Vec::new()
        }
        ids::CHANNEL_BACK => {
            if let Some(Dialog::Channel(form)) = dialog {
                form.choosing = false;
            }
            vec![Intent::Focus(ids::CHANNEL_NAME.into())]
        }
        ids::CHANNEL_SUBMIT => submit_channel(ui),
        ids::NOTIFY_MUTE => {
            let Some(Dialog::Notifications(server)) = *dialog else {
                return Vec::new();
            };
            let muted = crate::account::server_notify(ui, store, server).muted(wall_now());
            change_notify(
                ui,
                store,
                NotificationChange::Mute((!muted).then_some(FOREVER)),
                now,
            )
        }
        ids::NOTIFY_EVERYONE | ids::NOTIFY_ROLES => {
            let Some(Dialog::Notifications(server)) = *dialog else {
                return Vec::new();
            };
            let kept = store
                .notifications(Some(server))
                .cloned()
                .unwrap_or_default();
            let change = if id == ids::NOTIFY_EVERYONE {
                NotificationChange::SuppressEveryone(!kept.suppress_everyone)
            } else {
                NotificationChange::SuppressRoles(!kept.suppress_roles)
            };
            change_notify(ui, store, change, now)
        }
        ids::NICK_SAVE => save_nick(ui, store),
        ids::LEAVE_CONFIRM => match dialog.take() {
            Some(Dialog::Leave(server)) => {
                vec![Intent::Command(Command::LeaveServer { server })]
            }
            _ => Vec::new(),
        },
        ids::SERVER_OPEN | ids::SERVER_BACK => {
            *dialog = Some(Dialog::Server(None));
            Vec::new()
        }
        ids::SERVER_OWN => {
            if !store.create_server().allowed() {
                return Vec::new();
            }
            let name = store.me_user().map_or_else(
                || t("Mi servidor").to_owned(),
                |me| tr!("Servidor de {name}", name = me.shown_name()),
            );
            *dialog = Some(Dialog::Server(Some(ServerForm {
                name: name.clone(),
                accent: parts::SERVER_COLORS[0].0,
            })));
            vec![
                set_input(ids::SERVER_NAME, name),
                Intent::Focus(ids::SERVER_NAME.into()),
            ]
        }
        ids::SERVER_CREATE => create_server(ui, store),
        _ => Vec::new(),
    }
}

/// A server's id and the rest of an action's id after it, as `{server}:{rest}`.
fn split_server(rest: &str) -> Option<(ServerId, &str)> {
    let (server, rest) = rest.split_once(':')?;
    Some((ServerId(server.parse().ok()?), rest))
}

fn toggle<T: PartialEq>(list: &mut Vec<T>, item: T) {
    match list.iter().position(|candidate| *candidate == item) {
        Some(index) => {
            list.remove(index);
        }
        None => list.push(item),
    }
}

/// Keep what is written in a field of the dialogs. False for other fields.
pub(crate) fn input_changed(ui: &mut Ui, id: &str, value: &str) -> bool {
    let value = value.to_owned();
    match (&mut ui.server_menu.dialog, id) {
        (Some(Dialog::Channel(form)), ids::CHANNEL_NAME) => form.name = value,
        (Some(Dialog::Channel(form)), ids::CHANNEL_QUERY) => form.query = value,
        (Some(Dialog::Nick { nick, .. }), ids::NICK) => *nick = value,
        (Some(Dialog::Server(Some(form))), ids::SERVER_NAME) => form.name = value,
        _ => return false,
    }
    true
}

/// Look up at the provider the people named like what the private
/// channel's audience searches, beyond the members it sent (the Gateway's
/// member search, as the channel settings do); the same name asks once.
pub(crate) fn lookup(ui: &mut Ui, store: &Store, id: &str) -> Option<Intent> {
    let Some(Dialog::Channel(form)) = &mut ui.server_menu.dialog else {
        return None;
    };
    let query = form.query.trim().trim_start_matches('@').to_lowercase();
    if id != ids::CHANNEL_QUERY
        || query.is_empty()
        || !store.member_search(form.server).allowed()
        || form.lookup.as_ref() == Some(&query)
    {
        return None;
    }
    form.lookup = Some(query.clone());
    Some(Intent::Command(Command::SearchMembers {
        server: form.server,
        query,
    }))
}

/// Enter in a field of the dialogs does what their main button does.
pub(crate) fn submitted(
    ui: &mut Ui,
    store: &mut Store,
    id: &str,
    value: &str,
) -> Option<Vec<Intent>> {
    if !input_changed(ui, id, value) {
        return None;
    }
    Some(match id {
        ids::CHANNEL_NAME | ids::CHANNEL_QUERY => submit_channel(ui),
        ids::NICK => save_nick(ui, store),
        ids::SERVER_NAME => create_server(ui, store),
        _ => Vec::new(),
    })
}

fn open_channel_dialog(
    ui: &mut Ui,
    store: &Store,
    server: ServerId,
    parent: Option<ChannelId>,
    kind: ChannelKind,
    now: Instant,
) -> Vec<Intent> {
    let Some(abilities) = store.server_abilities(server) else {
        return Vec::new();
    };
    if let Some(reason) = abilities.manage_channels.reason() {
        ui.notify(
            t("No se puede crear el canal"),
            Some(reason.into()),
            true,
            now,
        );
        return Vec::new();
    }
    ui.server_menu.dialog = Some(Dialog::Channel(NewChannel {
        server,
        parent,
        kind,
        name: String::new(),
        private: false,
        choosing: false,
        query: String::new(),
        roles: Vec::new(),
        members: Vec::new(),
        lookup: None,
    }));
    // limn keeps what a field held by its ID: a new dialog starts empty.
    vec![
        set_input(ids::CHANNEL_NAME, ""),
        set_input(ids::CHANNEL_QUERY, ""),
        Intent::Focus(ids::CHANNEL_NAME.into()),
    ]
}

/// The main button of the channel dialog: on to who may see a private one,
/// or make it.
fn submit_channel(ui: &mut Ui) -> Vec<Intent> {
    let Some(Dialog::Channel(form)) = &mut ui.server_menu.dialog else {
        return Vec::new();
    };
    let name = form.form().final_name();
    if name.is_empty() {
        return Vec::new();
    }
    if form.private && !form.choosing {
        form.choosing = true;
        return vec![Intent::Focus(ids::CHANNEL_QUERY.into())];
    }
    let form = form.clone();
    ui.server_menu.dialog = None;
    let request = ui.fresh_request();
    let targets = if form.private {
        let roles = form.roles.iter().map(|role| OverwriteTarget::Role(*role));
        let members = form
            .members
            .iter()
            .map(|user| OverwriteTarget::Member(*user));
        roles.chain(members).collect()
    } else {
        Vec::new()
    };
    ui.server_menu.pending.insert(
        request,
        Pending::Channel {
            targets,
            open: form.kind.has_messages(),
        },
    );
    vec![Intent::Command(Command::CreateChannel {
        request,
        server: form.server,
        kind: form.kind,
        name,
        parent: form.parent,
        private: form.private,
    })]
}

fn save_nick(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let Some(Dialog::Nick { server, nick }) = &ui.server_menu.dialog else {
        return Vec::new();
    };
    let (server, nick) = (*server, nick.trim().to_owned());
    let (Some(me), Some(abilities)) = (store.me(), store.server_abilities(server)) else {
        return Vec::new();
    };
    if !abilities.change_nick.allowed() || nick.chars().count() > NICK_MAX {
        return Vec::new();
    }
    ui.server_menu.dialog = None;
    let nick = (!nick.is_empty()).then_some(nick);
    let current = store
        .member_of(server, me)
        .and_then(|member| member.nick.clone());
    if nick == current {
        return Vec::new();
    }
    vec![Intent::Command(Command::SetNick {
        server,
        user: me,
        nick,
    })]
}

/// An invitation to `server` from its menu: its link is copied once made.
fn invite(ui: &mut Ui, store: &Store, server: ServerId, now: Instant) -> Vec<Intent> {
    let Some(abilities) = store.server_abilities(server) else {
        return Vec::new();
    };
    if let Some(reason) = abilities.create_invites.reason() {
        ui.notify(
            t("No se creó la invitación"),
            Some(reason.into()),
            true,
            now,
        );
        return Vec::new();
    }
    let Some(channel) = invite_channels(store, server).first().copied() else {
        return Vec::new();
    };
    let request = ui.fresh_request();
    ui.server_menu.pending.insert(request, Pending::Invite);
    vec![Intent::Command(Command::CreateInvite {
        request,
        server,
        channel,
        max_age_secs: INVITE_AGE_SECS,
        max_uses: 0,
        temporary: false,
    })]
}

fn create_server(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    if !store.create_server().allowed() {
        return Vec::new();
    }
    let Some(Dialog::Server(Some(form))) = &ui.server_menu.dialog else {
        return Vec::new();
    };
    let (name, accent) = (form.name.trim().to_owned(), form.accent);
    if name.is_empty() {
        return Vec::new();
    }
    ui.server_menu.dialog = None;
    let request = ui.fresh_request();
    ui.server_menu
        .pending
        .insert(request, Pending::Server { accent });
    vec![Intent::Command(Command::CreateServer { request, name })]
}

/// Change the notification settings of the server whose dialog is open:
/// ask the provider when it keeps them, else keep them on this machine.
fn change_notify(
    ui: &mut Ui,
    store: &Store,
    change: NotificationChange,
    now: Instant,
) -> Vec<Intent> {
    let Some(Dialog::Notifications(server)) = ui.server_menu.dialog else {
        return Vec::new();
    };
    change_notify_for(ui, store, server, change, now)
}

/// Change the notification settings of `server`, from its dialog or its menu.
fn change_notify_for(
    ui: &mut Ui,
    store: &Store,
    server: ServerId,
    change: NotificationChange,
    now: Instant,
) -> Vec<Intent> {
    if store.synced_notifications() {
        return crate::account::notify_command(ui, store, Some(server), change, now);
    }
    let default = crate::account::default_notify(store, server);
    let notify = ui.server_menu.notify.entry(server).or_insert(default);
    match change {
        NotificationChange::Level(level) => notify.level = level,
        NotificationChange::Mute(until) => notify.muted_until = until,
        _ => {}
    }
    if *notify == default {
        ui.server_menu.notify.remove(&server);
    }
    vec![Intent::SaveSetting {
        key: NOTIFY_SETTING.into(),
        value: kept(ui),
    }]
}

/// The link of an invitation made from a server's menu is copied once made.
pub(crate) fn on_invite(
    ui: &mut Ui,
    store: &Store,
    applied: &Applied,
    now: Instant,
) -> Vec<Intent> {
    let Applied::InviteCreated { request, code, .. } = applied else {
        return Vec::new();
    };
    if !matches!(ui.server_menu.pending.get(request), Some(Pending::Invite)) {
        return Vec::new();
    }
    ui.server_menu.pending.remove(request);
    ui.notify(t("Enlace de invitación copiado"), None, false, now);
    vec![Intent::Copy(invite_link(store, code))]
}

/// Something asked for here was made: let who was chosen see it, and open it.
pub(crate) fn on_created(
    ui: &mut Ui,
    store: &mut Store,
    request: RequestId,
    created: Created,
) -> Vec<Intent> {
    let Some(pending) = ui.server_menu.pending.remove(&request) else {
        return Vec::new();
    };
    match (pending, created) {
        (Pending::Channel { targets, open: go }, Created::Channel(channel)) => {
            let mut intents: Vec<Intent> = targets
                .into_iter()
                .map(|target| {
                    Intent::Command(Command::SetOverwrite {
                        channel,
                        overwrite: Overwrite {
                            target,
                            allow: Permissions::VIEW,
                            deny: Permissions::NONE,
                        },
                    })
                })
                .collect();
            if go {
                let server = store.channel(channel).and_then(|found| found.server);
                intents.extend(open(ui, store, server, channel));
            }
            intents
        }
        (Pending::Server { accent }, Created::Server(server)) => {
            let mut intents = Vec::new();
            // Making a server takes only its name; its color comes after.
            if store
                .server(server)
                .is_some_and(|found| found.accent != accent)
            {
                intents.push(Intent::Command(Command::EditServer {
                    server,
                    edit: ServerEdit {
                        accent: Some(accent),
                        ..ServerEdit::default()
                    },
                }));
            }
            intents.extend(open_server(ui, store, Some(server)));
            intents
        }
        _ => Vec::new(),
    }
}

/// What the rail marks on `server`, from what is waiting there (`unread`,
/// `mentions`) and its notification settings at `now`: whether it is
/// unread, its mentions, and whether it is muted. A muted server marks only
/// its mentions, as one that marks only mentions does; one that marks
/// nothing marks nothing.
pub(crate) fn rail_marks(
    ui: &Ui,
    store: &Store,
    server: ServerId,
    (unread, mentions): (bool, u32),
    now: Timestamp,
) -> (bool, u32, bool) {
    let notify = crate::account::server_notify(ui, store, server);
    let muted = notify.muted(now);
    match notify.level {
        NotifyLevel::Nothing => (false, 0, muted),
        _ if muted => (false, mentions, true),
        NotifyLevel::Mentions => (false, mentions, false),
        NotifyLevel::All => (unread, mentions, false),
    }
}

/// The notification settings as kept: `server:level:until` for each server
/// that differs from the default, `-` for not muted.
pub(crate) fn kept(ui: &Ui) -> Option<String> {
    let mut servers: Vec<_> = ui.server_menu.notify.iter().collect();
    servers.sort_by_key(|(server, _)| **server);
    let kept: Vec<String> = servers
        .into_iter()
        .map(|(server, notify)| {
            let until = notify
                .muted_until
                .map_or_else(|| "-".to_owned(), |until| until.0.to_string());
            format!("{}:{}:{until}", server.0, notify.level.key())
        })
        .collect();
    (!kept.is_empty()).then(|| kept.join(";"))
}

/// Take back what [`kept`] kept; what cannot be read is left out.
pub(crate) fn restore(ui: &mut Ui, kept: Option<String>) {
    ui.server_menu.notify = kept
        .unwrap_or_default()
        .split(';')
        .filter_map(|entry| {
            let mut fields = entry.split(':');
            let server = ServerId(fields.next()?.parse().ok()?);
            let level = NotifyLevel::from_key(fields.next()?)?;
            let muted_until = match fields.next()? {
                "-" => None,
                until => Some(Timestamp(until.parse().ok()?)),
            };
            Some((server, Notify { level, muted_until }))
        })
        .collect();
}

/// The bar at the top of the column with the server's menu on it.
pub(crate) fn header(ui: &Ui, store: &Store, server: ServerId, look: &Look) -> Element {
    let name = store
        .server(server)
        .map_or_else(String::new, |found| found.shown_name().into());
    let open = ui.server_menu.open;
    let entries = grouped(server_groups(ui, store, server, Vec::new()));
    let name = parts::menu(sidebar::server_name(&name, open, look), entries, open, look);
    sidebar::header_bar(name, look)
}

/// The mute durations of the menu's submenu: each one's action, then the
/// label it shows.
fn mute_entries(server: ServerId) -> Vec<Entry> {
    parts::MUTE_DURATIONS
        .iter()
        .map(|(key, label, _)| {
            let label = if *key == "always" {
                t("Hasta que lo vuelvas a activar")
            } else {
                t(label)
            };
            Item::new(format!("sm:mute:{server}:{key}"), label).into()
        })
        .collect()
}

/// The entries of `server`'s menu in groups, each disabled with its reason
/// when the account or the provider cannot do it. The header and the rail
/// share them; `folders` are the rail's groups for the server's folder, kept
/// before leaving the server.
pub(crate) fn server_groups(
    ui: &Ui,
    store: &Store,
    server: ServerId,
    folders: Vec<Vec<Entry>>,
) -> Vec<Vec<Entry>> {
    let Some(abilities) = store.server_abilities(server) else {
        return Vec::new();
    };
    let act = |id: String, label: &str, icon: Option<Icon>| Item {
        icon,
        ..Item::new(id, label)
    };
    let notify = crate::account::server_notify(ui, store, server);
    let synced = store.synced_notifications();
    let kept = store
        .notifications(Some(server))
        .cloned()
        .unwrap_or_default();
    let mut groups = Vec::new();

    let mut reads = Vec::new();
    if store.server_unread(server).0 {
        reads.push(read_item(store, server).into());
    }
    if abilities.create_invites.allowed() && !invite_channels(store, server).is_empty() {
        reads.push(
            act(
                format!("sm:invite:{server}"),
                t("Invitar a personas"),
                Some(Icon::Invite),
            )
            .into(),
        );
    }
    groups.push(reads);

    let mut notifications = Vec::new();
    if notify.muted(wall_now()) {
        notifications.push(
            act(
                format!("sm:mute:{server}:off"),
                t("Dejar de silenciar servidor"),
                Some(Icon::Bell),
            )
            .into(),
        );
    } else {
        notifications.push(
            Item {
                submenu: mute_entries(server),
                ..act(
                    format!("sm:mute:{server}"),
                    t("Silenciar servidor"),
                    Some(Icon::BellSlash),
                )
            }
            .into(),
        );
    }
    let mut levels: Vec<Entry> = [
        (NotifyLevel::All, t("Todos los mensajes")),
        (NotifyLevel::Mentions, t("Solo @menciones")),
        (NotifyLevel::Nothing, t("Nada")),
    ]
    .into_iter()
    .map(|(level, label)| {
        Item::new(format!("sm:level:{server}:{}", level.key()), label)
            .radio(notify.level == level)
            .into()
    })
    .collect();
    if synced {
        levels.push(Entry::Separator);
        levels.push(
            Item::new(
                format!("sm:flag:{server}:everyone"),
                t("Silenciar @everyone y @here"),
            )
            .checked(kept.suppress_everyone)
            .into(),
        );
        levels.push(
            Item::new(
                format!("sm:flag:{server}:roles"),
                t("Silenciar las @menciones de roles"),
            )
            .checked(kept.suppress_roles)
            .into(),
        );
        levels.push(
            Item::new(
                format!("sm:flag:{server}:mobile"),
                t("Notificaciones push para móvil"),
            )
            .checked(kept.mobile_push)
            .into(),
        );
    }
    levels.push(Entry::Separator);
    levels.push(
        act(
            format!("sm:notify:{server}"),
            t("Ajustes de notificaciones del servidor"),
            None,
        )
        .into(),
    );
    notifications.push(
        Item {
            submenu: levels,
            ..act(
                format!("sm:notifications:{server}"),
                t("Ajustes de notificaciones"),
                Some(Icon::Bell),
            )
        }
        .into(),
    );
    if synced {
        notifications.push(
            Item::new(
                format!("sm:flag:{server}:hide"),
                t("Ocultar los canales silenciados"),
            )
            .checked(kept.hide_muted)
            .into(),
        );
    }
    groups.push(notifications);

    let manages = [
        &abilities.manage_server,
        &abilities.manage_roles,
        &abilities.kick,
        &abilities.ban,
        &abilities.view_audit_log,
    ]
    .iter()
    .any(|permit| permit.allowed());
    let mut manage = Vec::new();
    if manages {
        manage.push(
            act(
                format!("ss:open:{server}"),
                t("Ajustes del servidor"),
                Some(Icon::Settings),
            )
            .into(),
        );
    }
    if abilities.manage_channels.allowed() {
        manage.push(
            act(
                format!("newchan:{server}"),
                t("Crear un canal"),
                Some(Icon::Add),
            )
            .into(),
        );
        manage.push(
            act(
                format!("newcat:{server}"),
                t("Crear categoría"),
                Some(Icon::Folder),
            )
            .into(),
        );
    }
    if store.event_abilities(server).create.allowed() {
        manage.push(act(format!("ev:new:{server}"), t("Crear evento"), None).into());
    }
    groups.push(manage);

    if abilities.change_nick.allowed() {
        groups.push(vec![act(
            format!("sm:profile:{server}"),
            t("Editar perfil de servidor"),
            Some(Icon::Edit),
        )
        .into()]);
    }
    groups.extend(folders);

    // Deleting is the owner's; the others leave.
    let mut ending = Vec::new();
    if abilities.delete.allowed() {
        ending.push(
            Item {
                danger: true,
                ..act(
                    format!("ss:open:{server}:delete"),
                    t("Eliminar servidor"),
                    Some(Icon::Trash),
                )
            }
            .into(),
        );
    } else if abilities.leave.allowed() {
        ending.push(
            Item {
                danger: true,
                ..act(
                    format!("sm:leave:{server}"),
                    t("Abandonar servidor"),
                    Some(Icon::Leave),
                )
            }
            .into(),
        );
    }
    groups.push(ending);

    // «Copiar ID» is for developer mode.
    if ui.prefs.developer {
        groups.push(vec![act(
            format!("sm:copy-id:{server}"),
            t("Copiar ID del servidor"),
            Some(Icon::Copy),
        )
        .into()]);
    }
    groups
}

/// «Marcar como leído» for `server`: what it reaches, and why not when
/// nothing waits there.
pub(crate) fn read_item(store: &Store, server: ServerId) -> Item {
    Item {
        icon: Some(Icon::Check),
        denied: (!store.server_unread(server).0).then(|| t("No hay nada sin leer").into()),
        ..Item::new(
            format!("sm:read:{server}"),
            if store.read_sync() {
                t("Marcar como leído")
            } else {
                t("Marcar como leído (solo en RUNE)")
            },
        )
    }
}

/// The dialog of the menu that is open, over `root`.
pub(crate) fn dialogs(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let width = Modal::fit_width(context.window.width, parts::DIALOG_WIDTH);
    let name_of = |server: ServerId| {
        store
            .server(server)
            .map(|found| found.shown_name().to_owned())
    };
    match &ui.server_menu.dialog {
        None => root,
        // A dialog of a server that is gone is not shown; the next one replaces it.
        Some(Dialog::Channel(form)) if store.server(form.server).is_none() => root,
        Some(Dialog::Channel(form)) => {
            let mut shown = form.form();
            shown.audience = form.choosing.then(|| audience(store, form));
            parts::channel_dialog(root, &shown, width, look)
        }
        Some(Dialog::Notifications(server)) => {
            let Some(name) = name_of(*server) else {
                return root;
            };
            let notify = crate::account::server_notify(ui, store, *server);
            let synced = store.synced_notifications();
            let kept = store.notifications(Some(*server));
            let form = NotifyForm {
                server: name,
                level: notify.level,
                muted: notify.muted(context.wall),
                until: notify
                    .muted_until
                    .filter(|until| *until != FOREVER)
                    .map(|until| rune_components::time::stamp(until, context.wall)),
                synced,
                suppress_everyone: synced.then(|| kept.is_some_and(|kept| kept.suppress_everyone)),
                suppress_roles: synced.then(|| kept.is_some_and(|kept| kept.suppress_roles)),
            };
            parts::notify_dialog(root, &form, width, look)
        }
        Some(Dialog::Nick { server, nick }) => match nick_form(store, *server, nick, look) {
            Some(form) => parts::nick_dialog(root, &form, width, look),
            None => root,
        },
        Some(Dialog::Leave(server)) => match name_of(*server) {
            Some(name) => parts::leave_dialog(root, &name, look),
            None => root,
        },
        Some(Dialog::Server(form)) => parts::server_dialog(
            root,
            form.as_ref(),
            &parts::ServerActions {
                create_denied: store.create_server().reason().map(str::to_owned),
                invitation_denied: store.invitation_preview().reason().map(str::to_owned),
            },
            width,
            look,
        ),
    }
}

/// The roles and people a private channel can be shown to: the roles below
/// the account's that match what is searched, and, once something is
/// searched (after an `@` for people only), the people that match. Who is
/// already chosen stays in view.
fn audience(store: &Store, form: &NewChannel) -> Audience {
    let query = form.query.trim().to_lowercase();
    let people_only = query.starts_with('@');
    let query = query.trim_start_matches('@');
    let rank = store.my_rank(form.server);
    let roles = match store.server(form.server) {
        Some(server) if !people_only => server
            .assignable_roles()
            .into_iter()
            .filter(|role| role.position < rank && role.name.to_lowercase().contains(query))
            .map(|role| Choice {
                id: format!("{}{}", ids::CHANNEL_ROLE, role.id),
                label: role.name.clone(),
                chosen: form.roles.contains(&role.id),
            })
            .collect(),
        _ => Vec::new(),
    };
    let searching = people_only || !query.is_empty();
    let members = (searching || !form.members.is_empty()).then(|| {
        let me = store.me();
        let mut found: Vec<Choice> = store
            .members(form.server)
            .iter()
            .filter(|member| Some(member.user) != me)
            .filter_map(|member| {
                let name = store.display_name(member.user, Some(form.server));
                let chosen = form.members.contains(&member.user);
                let matches = searching && name.to_lowercase().contains(query);
                (chosen || matches).then(|| Choice {
                    id: format!("{}{}", ids::CHANNEL_MEMBER, member.user),
                    label: name,
                    chosen,
                })
            })
            .collect();
        found.sort_by_key(|choice| !choice.chosen);
        found.truncate(8 + form.members.len());
        found
    });
    Audience {
        query: form.query.clone(),
        roles,
        members,
    }
}

fn nick_form(store: &Store, server: ServerId, nick: &str, look: &Look) -> Option<NickForm> {
    let found = store.server(server)?;
    let me = store.me_user()?;
    let denied = store
        .server_abilities(server)?
        .change_nick
        .reason()
        .map(str::to_owned);
    let current = store
        .member_of(server, me.id)
        .and_then(|member| member.nick.clone())
        .unwrap_or_default();
    let trimmed = nick.trim();
    Some(NickForm {
        server: found.name.clone(),
        nick: nick.to_owned(),
        own_name: me.shown_name().to_owned(),
        handle: me.name.clone(),
        accent: look.ink(me.accent),
        ink: store
            .top_role(me.id, server)
            .and_then(|role| role.color)
            .map_or(look.text, |color| look.ink(color)),
        can_save: denied.is_none() && trimmed.chars().count() <= NICK_MAX && trimmed != current,
        denied,
    })
}
