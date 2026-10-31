//! The person menu's «Apps ›» (2026-10-31, as Discord's): the user commands of
//! the apps a conversation offers, each run on the person. The list is asked
//! for when a person's menu opens, as the composer asks for its `/` list.

use std::time::Instant;

use rune_components::voice::MenuEntry;
use rune_i18n::t;
use rune_model::interactions::{AppCommand, CommandKind, CommandUse, Interaction};
use rune_model::{ChannelId, UserId};
use rune_state::{CommandsState, Store};

use crate::person_menu::Place;
use crate::{timeline_id, Intent, Ui};

/// The conversation whose apps `user`'s menu in `place` offers: a text one,
/// of someone who is not a bot, where the account may use apps. `None` hides
/// «Apps ›».
fn conversation(ui: &Ui, store: &Store, user: UserId, place: Place) -> Option<ChannelId> {
    let channel = match place {
        // A member list is about the open conversation.
        Place::MemberList { .. } => ui.channel()?,
        // The Friends page has no conversation, as Discord's generic menu.
        Place::Friends => return None,
        Place::VoiceRow { channel }
        | Place::Tile { channel }
        | Place::Author { channel }
        | Place::Direct { channel } => channel,
    };
    let found = store.channel(channel)?;
    // A voice channel has its chat too; categories and forums hold no messages.
    let texted = found.kind.has_messages() || found.kind.is_voice();
    let bot = store.user(user).is_some_and(|account| account.bot);
    (texted && !bot && store.interaction_abilities(channel).commands.allowed()).then_some(channel)
}

/// The person menu open, whom it is for and where: outside a call, or in a
/// voice channel's column or tile.
fn open(ui: &Ui) -> Option<(UserId, Place)> {
    if let Some(menu) = ui.person_menu {
        return Some((menu.user, menu.place));
    }
    let menu = ui.voice.menu?;
    Some((menu.user, crate::voice::menu_place(menu)))
}

/// The «Apps ›» entry of `user`'s menu in `place`; `None` where the
/// conversation has no apps for the account.
pub(crate) fn entry(ui: &Ui, store: &Store, user: UserId, place: Place) -> Option<MenuEntry> {
    let channel = conversation(ui, store, user, place)?;
    let rows = match store.commands_state(channel) {
        CommandsState::Unknown | CommandsState::Loading => {
            vec![MenuEntry::Inert(
                format!("pm:{}:apps:loading", user.0),
                t("Cargando…").into(),
            )]
        }
        CommandsState::Failed(_) => vec![MenuEntry::Inert(
            format!("pm:{}:apps:failed", user.0),
            t("No se pudieron cargar los comandos").into(),
        )],
        CommandsState::Loaded => command_rows(store, channel, user),
    };
    Some(MenuEntry::Submenu(t("Apps").into(), rows))
}

/// The user commands `channel` lets the account run: one submenu per app, in
/// name order, each holding its commands by name; a row that says there are none.
fn command_rows(store: &Store, channel: ChannelId, user: UserId) -> Vec<MenuEntry> {
    let mut apps: Vec<(String, Vec<&AppCommand>)> = Vec::new();
    let user_commands = store
        .usable_commands(channel)
        .into_iter()
        .filter(|command| command.kind == CommandKind::User);
    for command in user_commands {
        match apps.iter_mut().find(|(name, _)| *name == command.app_name) {
            Some((_, found)) => found.push(command),
            None => apps.push((command.app_name.clone(), vec![command])),
        }
    }
    if apps.is_empty() {
        return vec![MenuEntry::Inert(
            format!("pm:{}:apps:none", user.0),
            t("No hay comandos disponibles").into(),
        )];
    }
    apps.sort_by(|a, b| a.0.cmp(&b.0));
    apps.into_iter()
        .map(|(name, mut found)| {
            found.sort_by(|a, b| a.name.cmp(&b.name));
            let items = found
                .into_iter()
                .map(|command| {
                    MenuEntry::Item(
                        format!("pm:{}:app-command:{}:{}", user.0, channel.0, command.id),
                        command.name.clone(),
                        None,
                    )
                })
                .collect();
            MenuEntry::Submenu(name, items)
        })
        .collect()
}

/// A command chosen from `user`'s menu, as `rest` says: `{channel}:{command}`.
/// It runs on them in that conversation, as the composer runs one.
pub(crate) fn choose(
    ui: &mut Ui,
    store: &mut Store,
    user: UserId,
    rest: &str,
    now: Instant,
) -> Vec<Intent> {
    let Some((channel, command)) = rest.split_once(':') else {
        return Vec::new();
    };
    let (Ok(channel), Ok(command)) = (channel.parse::<u64>(), command.parse::<u64>()) else {
        return Vec::new();
    };
    let channel = ChannelId(channel);
    // Choosing closes the voice menu too, as any of its items does.
    ui.voice.menu = None;
    let found = store
        .usable_commands(channel)
        .into_iter()
        .find(|found| found.id == command && found.kind == CommandKind::User);
    let Some(found) = found else {
        return Vec::new();
    };
    let used = CommandUse {
        target: Some(user),
        ..CommandUse::of(found, Vec::new(), Vec::new())
    };
    let request = ui.fresh_request();
    match store.begin_interaction(request, channel, Interaction::Command(used), now) {
        Ok(sent) => {
            let mut intents = vec![Intent::Command(sent)];
            if ui.channel() == Some(channel) {
                intents.push(Intent::ScrollToEnd(timeline_id(channel)));
            }
            intents
        }
        Err(reason) => {
            ui.notify(t("No se pudo usar el comando"), Some(reason), true, now);
            Vec::new()
        }
    }
}

/// After a layout: an open person's menu asks its conversation's commands once,
/// when it opens. `load_commands` keeps a list for `COMMANDS_KEPT`, so a menu
/// opened again soon does not ask again.
pub(crate) fn after_layout(ui: &mut Ui, store: &mut Store) -> Vec<Intent> {
    let open = open(ui).filter(|(user, place)| conversation(ui, store, *user, *place).is_some());
    if open == ui.apps.menu_asked {
        return Vec::new();
    }
    ui.apps.menu_asked = open;
    let Some((user, place)) = open else {
        return Vec::new();
    };
    let Some(channel) = conversation(ui, store, user, place) else {
        return Vec::new();
    };
    let request = ui.fresh_request();
    store
        .load_commands(channel, request, Instant::now(), false)
        .map(Intent::Command)
        .into_iter()
        .collect()
}
