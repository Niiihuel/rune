//! The composer's `/` (P3): RUNE's own commands, which work with any
//! provider, mixed with the apps' commands the provider lists. An app's
//! command, once chosen, shows its options over the composer, checked here
//! before it goes; the app may suggest values while one is written.

use std::collections::HashMap;
use std::time::Instant;

use limn::{Element, Icon, Key, UiAction};
use rune_components::composer::{ids as composer_ids, Completion, Completions};
use rune_components::interactions::{command_form, command_ids, CommandForm, OptionField};
use rune_components::Look;
use rune_i18n::{es, t};
use rune_model::components::{MenuKind, Select};
use rune_model::interactions::{
    AppCommand, CommandOption, CommandUse, Interaction, OptionKind, OptionValue,
};
use rune_model::{ChannelId, UserId};
use rune_provider::{Command, RequestId};
use rune_state::{Applied, CommandsState, Store};

use crate::{editor_id, timeline_id, Intent, Ui};

/// RUNE's own commands.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Local {
    Gif,
    Me,
    Msg,
    Shrug,
    Spoiler,
    Sticker,
    Tableflip,
    Unflip,
}

/// Each local command, its name and what it does.
const LOCAL: [(Local, &str, &str); 8] = [
    (
        Local::Gif,
        "gif",
        es("Buscá un GIF animado: /gif [búsqueda]"),
    ),
    (
        Local::Me,
        "me",
        es("Escribí en cursiva, como una acción: /me texto"),
    ),
    (
        Local::Msg,
        "msg",
        es("Escribile por privado a alguien: /msg @persona [mensaje]"),
    ),
    (Local::Shrug, "shrug", es("Agregá ¯\\_(ツ)_/¯ a tu mensaje")),
    (
        Local::Spoiler,
        "spoiler",
        es("Ocultá el texto como spoiler: /spoiler texto"),
    ),
    (
        Local::Sticker,
        "sticker",
        es("Buscá un sticker: /sticker [búsqueda]"),
    ),
    (
        Local::Tableflip,
        "tableflip",
        es("Agregá (╯°□°)╯︵ ┻━┻ a tu mensaje"),
    ),
    (
        Local::Unflip,
        "unflip",
        es("Agregá ┬─┬ ノ( ゜-゜ノ) a tu mensaje"),
    ),
];

/// What a completion inserts to start an app's command.
const APP_PICK: &str = "/@app:";
/// Rows the `/` list shows at most.
const LISTED: usize = 30;
/// The longest a message may be, as the composer sends it.
const MAX_CONTENT: usize = 2000;

/// An option's menu open: its name, what it searches, and what it offers
/// (value, label).
type OptionMenu = (String, String, Vec<(String, String)>);

/// An app's command being run over the composer.
#[derive(Clone, Debug)]
pub(crate) struct Running {
    channel: ChannelId,
    command: AppCommand,
    path: Vec<String>,
    /// What is written, or picked, for each option, by name.
    written: HashMap<String, String>,
    tried: bool,
    /// The option whose menu is open, what it searches and offers.
    menu: Option<OptionMenu>,
    /// The last suggestions asked for: request, option and text.
    asked: Option<(RequestId, String, String)>,
}

impl Running {
    /// `/name sub` as it is written.
    fn shown(&self) -> String {
        let mut said = format!("/{}", self.command.name);
        for part in &self.path {
            said.push(' ');
            said.push_str(part);
        }
        said
    }

    fn options(&self) -> &[CommandOption] {
        self.command.options_at(&self.path).unwrap_or_default()
    }
}

/// The local command `draft` runs, and what follows its name.
fn local(draft: &str) -> Option<(Local, &str)> {
    let rest = draft.strip_prefix('/')?;
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let (name, arguments) = rest.split_at(end);
    let (command, ..) = LOCAL.iter().find(|(_, known, _)| *known == name)?;
    Some((*command, arguments.trim()))
}

/// What `/` completes in `channel`: RUNE's commands and the apps' ones the
/// account may run there, by what follows the `/`.
pub(crate) fn completions(ui: &Ui, store: &Store, channel: ChannelId) -> Option<Completions> {
    if ui.completions_closed
        || ui
            .apps
            .command
            .as_ref()
            .is_some_and(|running| running.channel == channel)
    {
        return None;
    }
    let query = ui.drafts.get(&channel)?.strip_prefix('/')?;
    if query.chars().any(char::is_whitespace) || query.chars().count() > 64 {
        return None;
    }
    let query = query.to_lowercase();
    let writes = store
        .abilities(channel)
        .is_some_and(|abilities| abilities.write.allowed());
    let mut groups: Vec<(String, Vec<Completion>)> = Vec::new();
    if writes {
        let own = LOCAL
            .iter()
            .filter(|(_, name, _)| name.starts_with(&query))
            .map(|(_, name, detail)| Completion {
                insert: format!("/{name}"),
                label: format!("/{name}"),
                detail: format!("{} · RUNE", t(detail)),
                icon: Icon::Code,
                section: None,
                face: None,
                ink: None,
                aside: String::new(),
            })
            .collect();
        groups.push((t("COMANDOS DE RUNE").into(), own));
    }
    let mut apps: Vec<(String, Vec<Completion>)> = Vec::new();
    for command in store.usable_commands(channel) {
        for (path, description) in command.leaves() {
            let name = std::iter::once(command.name.as_str())
                .chain(path.iter().map(String::as_str))
                .collect::<Vec<_>>()
                .join(" ");
            if !name.to_lowercase().contains(&query)
                && !command.app_name.to_lowercase().contains(&query)
            {
                continue;
            }
            let item = Completion {
                insert: format!("{APP_PICK}{}:{}", command.id, path.join(" ")),
                label: format!("/{name}"),
                detail: description.to_owned(),
                icon: Icon::Puzzle,
                section: None,
                face: None,
                ink: None,
                aside: String::new(),
            };
            let heading = command.app_name.to_uppercase();
            match apps.iter_mut().find(|(known, _)| *known == heading) {
                Some((_, items)) => items.push(item),
                None => apps.push((heading, vec![item])),
            }
        }
    }
    apps.sort_by(|a, b| a.0.cmp(&b.0));
    groups.extend(apps);
    groups.retain(|(_, items)| !items.is_empty());
    let note = match store.commands_state(channel) {
        CommandsState::Loading => Some(t(" · CARGANDO LOS DE APLICACIONES…")),
        CommandsState::Failed(_) => Some(t(" · NO SE PUDIERON CARGAR LOS DE APLICACIONES")),
        _ => None,
    };
    let mut title = groups.first()?.0.clone();
    if let Some(note) = note {
        title.push_str(note);
    }
    let mut items = Vec::new();
    for (index, (heading, group)) in groups.into_iter().enumerate() {
        for (row, mut item) in group.into_iter().enumerate() {
            if index > 0 && row == 0 {
                item.section = Some(heading.clone());
            }
            items.push(item);
        }
    }
    items.truncate(LISTED);
    let selected = ui.completion % items.len();
    Some(Completions {
        title,
        items,
        selected,
    })
}

/// Put `value` in the composer of `channel`, focused.
fn set_composer(ui: &mut Ui, channel: ChannelId, value: String) -> Vec<Intent> {
    ui.set_draft(channel, (!value.is_empty()).then(|| value.clone()));
    ui.completion = 0;
    vec![
        Intent::SetInput {
            id: editor_id(channel),
            value,
        },
        Intent::Focus(editor_id(channel)),
    ]
}

/// A row of the `/` list chosen: a local command waits for its text; an
/// app's command shows its options.
fn chosen(ui: &mut Ui, store: &Store, channel: ChannelId, insert: &str) -> Vec<Intent> {
    let Some(rest) = insert.strip_prefix(APP_PICK) else {
        return set_composer(ui, channel, format!("{insert} "));
    };
    let Some((id, path)) = rest.split_once(':') else {
        return Vec::new();
    };
    let Some(command) = id.parse::<u64>().ok().and_then(|id| {
        store
            .usable_commands(channel)
            .into_iter()
            .find(|command| command.id == id)
            .cloned()
    }) else {
        return Vec::new();
    };
    let path: Vec<String> = path
        .split(' ')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect();
    if command.options_at(&path).is_none() {
        return Vec::new();
    }
    let running = Running {
        channel,
        command,
        path,
        written: HashMap::new(),
        tried: false,
        menu: None,
        asked: None,
    };
    let shown = format!("{} ", running.shown());
    let first = running
        .options()
        .first()
        .filter(|option| !menu_option(option))
        .map(|option| command_ids::option(&option.name));
    ui.apps.command = Some(running);
    let mut intents = set_composer(ui, channel, shown);
    if let Some(first) = first {
        intents.push(Intent::Focus(first));
    }
    intents
}

/// Whether an option is picked from a menu instead of written.
fn menu_option(option: &CommandOption) -> bool {
    (!option.choices.is_empty() && !option.autocomplete)
        || matches!(
            option.kind,
            OptionKind::Boolean
                | OptionKind::User
                | OptionKind::Role
                | OptionKind::Channel
                | OptionKind::Mentionable
        )
}

/// What the menu of `option` offers for `query`: value, as the option
/// reads it, and label.
fn offered(
    store: &Store,
    channel: ChannelId,
    option: &CommandOption,
    query: &str,
) -> Vec<(String, String)> {
    let menu = |kind| Select {
        kind,
        channels: option.channels.clone(),
        ..Select::default()
    };
    match option.kind {
        OptionKind::Boolean => vec![
            ("true".into(), t("Sí").into()),
            ("false".into(), t("No").into()),
        ],
        OptionKind::User => {
            crate::interactions::candidates(store, channel, &menu(MenuKind::User), query)
        }
        OptionKind::Role => {
            crate::interactions::candidates(store, channel, &menu(MenuKind::Role), query)
                .into_iter()
                .map(|(id, name)| (format!("role:{id}"), name))
                .collect()
        }
        OptionKind::Channel => {
            crate::interactions::candidates(store, channel, &menu(MenuKind::Channel), query)
        }
        OptionKind::Mentionable => {
            let mut both =
                crate::interactions::candidates(store, channel, &menu(MenuKind::User), query);
            both.extend(
                crate::interactions::candidates(store, channel, &menu(MenuKind::Role), query)
                    .into_iter()
                    .map(|(id, name)| (format!("role:{id}"), name)),
            );
            both.truncate(25);
            both
        }
        _ => option
            .choices
            .iter()
            .map(|choice| (choice.value.written(), choice.name.clone()))
            .collect(),
    }
}

/// How a picked value reads.
fn shown(
    store: &Store,
    channel: ChannelId,
    option: &CommandOption,
    written: &str,
) -> Option<String> {
    if written.is_empty() {
        return None;
    }
    if let Some(choice) = option
        .choices
        .iter()
        .find(|choice| choice.value.written() == written)
    {
        return Some(choice.name.clone());
    }
    let server = store.channel(channel).and_then(|found| found.server);
    Some(match option.parse(written) {
        Ok(Some(OptionValue::Boolean(on))) => if on { t("Sí") } else { t("No") }.into(),
        Ok(Some(OptionValue::User(user))) => format!("@{}", store.display_name(user, server)),
        Ok(Some(OptionValue::Channel(id))) => store
            .channel(id)
            .map_or_else(|| t("#canal").into(), |found| format!("#{}", found.name)),
        Ok(Some(OptionValue::Role(role))) => server
            .and_then(|server| store.server(server))
            .and_then(|server| server.roles.iter().find(|found| found.id == role))
            .map_or_else(|| t("@rol").into(), |found| format!("@{}", found.name)),
        _ => written.to_owned(),
    })
}

/// Run the command over the composer: its options checked, then sent.
fn send(ui: &mut Ui, store: &mut Store, now: Instant) -> Vec<Intent> {
    let Some(running) = ui.apps.command.as_mut() else {
        return Vec::new();
    };
    running.tried = true;
    running.menu = None;
    let channel = running.channel;
    let mut values = Vec::new();
    let mut blocked = false;
    for option in running.options() {
        let written = running.written.get(&option.name).map_or("", String::as_str);
        if option.kind == OptionKind::Attachment {
            blocked |= option.required;
            continue;
        }
        match option.parse(written) {
            Ok(Some(value)) => values.push((option.name.clone(), value)),
            Ok(None) => {}
            Err(_) => blocked = true,
        }
    }
    if blocked {
        return Vec::new();
    }
    let used = CommandUse::of(&running.command, running.path.clone(), values);
    let request = ui.fresh_request();
    match store.begin_interaction(request, channel, Interaction::Command(used), now) {
        Ok(command) => {
            ui.apps.command = None;
            let mut intents = vec![
                Intent::Command(command),
                Intent::ScrollToEnd(timeline_id(channel)),
            ];
            intents.extend(set_composer(ui, channel, String::new()));
            intents
        }
        Err(reason) => {
            ui.notify(t("No se pudo usar el comando"), Some(reason), true, now);
            Vec::new()
        }
    }
}

/// Ask the app what it suggests for `option`, as written, unless that was
/// asked last or an answer is awaited.
fn suggest(ui: &mut Ui, store: &Store, option: &str) -> Vec<Intent> {
    let Some(running) = ui.apps.command.as_ref() else {
        return Vec::new();
    };
    let Some(found) = running
        .options()
        .iter()
        .find(|known| known.name == option && known.autocomplete)
    else {
        return Vec::new();
    };
    let text = running.written.get(option).cloned().unwrap_or_default();
    if let Some((request, asked, said)) = &running.asked {
        let answered = store.suggested(*request).is_some();
        if (asked == option && *said == text) || !answered {
            return Vec::new();
        }
    }
    let values = running
        .options()
        .iter()
        .filter(|other| other.name != found.name)
        .filter_map(|other| {
            let written = running.written.get(&other.name)?;
            Some((other.name.clone(), other.parse(written).ok()??))
        })
        .collect();
    let mut used = CommandUse::of(&running.command, running.path.clone(), values);
    used.focused = Some((option.to_owned(), text.clone()));
    let channel = running.channel;
    let request = ui.fresh_request();
    if let Some(running) = ui.apps.command.as_mut() {
        running.asked = Some((request, option.to_owned(), text));
    }
    vec![Intent::Command(Command::SuggestChoices {
        request,
        channel,
        command: used,
    })]
}

/// A local command sent from the composer of `channel`.
fn run_local(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    command: Local,
    arguments: &str,
    now: Instant,
) -> Vec<Intent> {
    let refuse = |ui: &mut Ui, reason: &str| {
        ui.notify(
            t("No se pudo usar el comando"),
            Some(reason.into()),
            true,
            now,
        );
        Vec::new()
    };
    let content = match command {
        Local::Gif => return open_expressions(ui, store, channel, "gif", arguments, now),
        Local::Sticker => return open_expressions(ui, store, channel, "stickers", arguments, now),
        Local::Msg => return message_someone(ui, store, channel, arguments, now),
        Local::Me | Local::Spoiler if arguments.is_empty() => {
            return refuse(ui, t("Escribí algo después del comando."))
        }
        Local::Me => format!("*{}*", crate::emoticons::sent(ui, arguments)),
        Local::Spoiler => format!("||{}||", crate::emoticons::sent(ui, arguments)),
        Local::Shrug | Local::Tableflip | Local::Unflip => {
            // Escaped so the face survives Markdown.
            let face = match command {
                Local::Shrug => r"¯\\\_(ツ)\_/¯",
                Local::Tableflip => "(╯°□°)╯︵ ┻━┻",
                _ => "┬─┬ ノ( ゜-゜ノ)",
            };
            if arguments.is_empty() {
                face.to_owned()
            } else {
                format!("{} {face}", crate::emoticons::sent(ui, arguments))
            }
        }
    };
    if content.chars().count() > MAX_CONTENT {
        return refuse(
            ui,
            t("El resultado supera los 2000 caracteres de un mensaje."),
        );
    }
    // As the composer, its emoji are checked before it goes (2026-10-18).
    if crate::expressions::refuse_unusable(
        ui,
        store,
        channel,
        &content,
        t("No se pudo usar el comando"),
        now,
    ) {
        return Vec::new();
    }
    if let Some(intents) = crate::uploads::submit(ui, store, channel, &content) {
        return intents;
    }
    let mut intents = vec![
        Intent::Send {
            channel,
            content,
            reply_to: ui.reply_to.take(),
            mention_reply: crate::message_actions::submit_options(ui),
        },
        Intent::ScrollToEnd(timeline_id(channel)),
    ];
    intents.extend(set_composer(ui, channel, String::new()));
    intents
}

/// `/gif` and `/sticker`: the picker opens on its tab, searching `query`.
fn open_expressions(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    tab: &str,
    query: &str,
    now: Instant,
) -> Vec<Intent> {
    let mut intents = set_composer(ui, channel, String::new());
    ui.emoji_open = true;
    intents.extend(
        crate::expressions::clicked(
            ui,
            store,
            &format!("{}:tab={tab}", composer_ids::EMOJI),
            now,
        )
        .unwrap_or_default(),
    );
    let query: String = query.chars().take(50).collect();
    if tab == "gif" {
        if ui.messages.gifs.available && !query.is_empty() {
            let typed = UiAction::InputChanged {
                id: crate::gifs::QUERY.into(),
                value: query.clone(),
            };
            crate::gifs::update(ui, store, &typed, now);
            intents.push(Intent::SetInput {
                id: crate::gifs::QUERY.into(),
                value: query.clone(),
            });
            let searched = UiAction::Submitted {
                id: crate::gifs::QUERY.into(),
                value: query,
            };
            intents.extend(crate::gifs::update(ui, store, &searched, now).unwrap_or_default());
        }
    } else {
        ui.messages.query = query.clone();
        intents.push(Intent::SetInput {
            id: rune_components::emoji::ids::search(composer_ids::EMOJI),
            value: query,
        });
    }
    intents
}

/// `/msg @persona [texto]`: the direct conversation with someone known
/// opens with the text in its composer, to send as usual.
fn message_someone(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    arguments: &str,
    now: Instant,
) -> Vec<Intent> {
    let end = arguments
        .find(char::is_whitespace)
        .unwrap_or(arguments.len());
    let (who, text) = arguments.split_at(end);
    let text = text.trim().to_owned();
    let refuse = |ui: &mut Ui, reason: &str| {
        ui.notify(t("No se pudo usar /msg"), Some(reason.into()), true, now);
        Vec::new()
    };
    if who.is_empty() {
        return refuse(ui, t("Elegí a quién: /msg @persona [mensaje]"));
    }
    let Some(user) = recipient(store, who) else {
        return refuse(
            ui,
            t("No encontramos a esa persona: usá su nombre de usuario o su mención."),
        );
    };
    let mut intents = set_composer(ui, channel, String::new());
    intents.extend(crate::conversations::open_direct(ui, store, user, now));
    if text.is_empty() {
        return intents;
    }
    match store.direct_with(user) {
        Some(direct) if ui.channel() == Some(direct) => {
            intents.extend(set_composer(ui, direct, text));
        }
        _ => ui.apps.waiting_direct = Some((user, text)),
    }
    intents
}

/// Someone known by `who`: a mention, an id or a handle, with or without @.
fn recipient(store: &Store, who: &str) -> Option<UserId> {
    let me = store.me();
    let id = who
        .strip_prefix("<@")
        .and_then(|rest| rest.strip_suffix('>'))
        .map(|rest| rest.trim_start_matches('!'))
        .unwrap_or(who)
        .parse::<u64>()
        .ok()
        .map(UserId);
    let name = who.trim_start_matches('@').to_lowercase();
    let mut found = store.users().filter(|user| {
        Some(user.id) != me
            && (Some(user.id) == id || user.name.to_lowercase() == name)
            && !store.blocked(user.id)
    });
    let first = found.next()?.id;
    found.next().is_none().then_some(first)
}

/// Answer the composer's `/` and the options of the command being run.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    let channel = ui.channel()?;
    match action {
        UiAction::Clicked { id } => {
            if let Some(insert) = id
                .strip_prefix(composer_ids::MENTION_PICK)
                .filter(|insert| insert.starts_with('/'))
            {
                return Some(chosen(ui, store, channel, insert));
            }
            if !id.starts_with("cmd:") {
                return None;
            }
            Some(clicked(ui, store, id, now))
        }
        UiAction::InputChanged { id, value } if *id == editor_id(channel) => {
            let mut intents = crate::messages::update(ui, store, action, now).unwrap_or_default();
            // The command over the composer ends once its name is changed.
            if ui.apps.command.as_ref().is_some_and(|running| {
                running.channel != channel || !value.starts_with(&running.shown())
            }) {
                ui.apps.command = None;
            }
            if value.starts_with('/') {
                let request = ui.fresh_request();
                if let Some(command) = store.load_commands(channel, request, now, false) {
                    intents.push(Intent::Command(command));
                }
            }
            Some(intents)
        }
        UiAction::InputChanged { id, value } => {
            if let Some(name) = id.strip_prefix("cmd:opt:") {
                let running = ui.apps.command.as_mut()?;
                running
                    .written
                    .insert(name.to_owned(), value.chars().take(6000).collect());
                return Some(suggest(ui, store, name));
            }
            let name = id.strip_prefix("cmd:query:")?;
            let running = ui.apps.command.as_ref()?;
            let option = running
                .options()
                .iter()
                .find(|option| option.name == name)?
                .clone();
            let offered = offered(store, running.channel, &option, value);
            let running = ui.apps.command.as_mut()?;
            if let Some((open, query, options)) = running.menu.as_mut() {
                if open == name {
                    *query = value.chars().take(100).collect();
                    *options = offered;
                }
            }
            Some(Vec::new())
        }
        UiAction::Submitted { id, value } if *id == editor_id(channel) => {
            if ui
                .apps
                .command
                .as_ref()
                .is_some_and(|running| running.channel == channel)
            {
                return Some(send(ui, store, now));
            }
            let (command, arguments) = local(value.trim())?;
            Some(run_local(ui, store, channel, command, arguments, now))
        }
        UiAction::Submitted { id, .. } if id.starts_with("cmd:opt:") => Some(send(ui, store, now)),
        _ => None,
    }
}

fn clicked(ui: &mut Ui, store: &mut Store, id: &str, now: Instant) -> Vec<Intent> {
    let Some(channel) = ui.apps.command.as_ref().map(|running| running.channel) else {
        return Vec::new();
    };
    if id == command_ids::CANCEL {
        ui.apps.command = None;
        return set_composer(ui, channel, String::new());
    }
    if id == command_ids::SEND {
        return send(ui, store, now);
    }
    let Some(running) = ui.apps.command.as_ref() else {
        return Vec::new();
    };
    let option_of = |name: &str| {
        running
            .options()
            .iter()
            .find(|option| option.name == name)
            .cloned()
    };
    if let Some(name) = id.strip_prefix("cmd:menu-close:") {
        if let Some(running) = ui.apps.command.as_mut() {
            running.menu.take_if(|(open, ..)| open == name);
        }
        return Vec::new();
    }
    if let Some(name) = id.strip_prefix("cmd:menu:") {
        let Some(option) = option_of(name) else {
            return Vec::new();
        };
        let options = offered(store, channel, &option, "");
        if let Some(running) = ui.apps.command.as_mut() {
            running.menu = match running.menu.take() {
                Some((open, ..)) if open == name => None,
                _ => Some((name.to_owned(), String::new(), options)),
            };
        }
        return Vec::new();
    }
    let Some((index, name)) = id
        .strip_prefix("cmd:pick:")
        .and_then(|rest| rest.split_once(':'))
    else {
        return Vec::new();
    };
    let Ok(index) = index.parse::<usize>() else {
        return Vec::new();
    };
    let Some(option) = option_of(name) else {
        return Vec::new();
    };
    let picked = if menu_option(&option) {
        running
            .menu
            .as_ref()
            .filter(|(open, ..)| open == name)
            .and_then(|(_, _, options)| options.get(index))
            .map(|(value, _)| value.clone())
    } else {
        // An app's suggestion takes the place of what was written.
        running
            .asked
            .as_ref()
            .and_then(|(request, _, _)| store.suggested(*request))
            .and_then(|choices| choices.get(index))
            .map(|choice| choice.value.written())
    };
    let Some(value) = picked else {
        return Vec::new();
    };
    let Some(running) = ui.apps.command.as_mut() else {
        return Vec::new();
    };
    running.menu = None;
    running.written.insert(name.to_owned(), value.clone());
    if menu_option(&option) {
        return Vec::new();
    }
    vec![Intent::SetInput {
        id: command_ids::option(name),
        value,
    }]
}

/// Keys of the composer while `/` lists commands, or a command is run.
pub(crate) fn keys(
    ui: &mut Ui,
    store: &Store,
    key: Key,
    ctrl: bool,
    focused: Option<&str>,
) -> Option<Vec<Intent>> {
    let channel = ui.channel()?;
    let in_form = focused.is_some_and(|id| id.starts_with("cmd:"));
    if key == Key::Escape && (in_form || focused == Some(editor_id(channel).as_str())) {
        if let Some(running) = ui.apps.command.as_mut() {
            // A menu open closes first; then the command.
            if running.menu.take().is_none() {
                ui.apps.command = None;
                return Some(set_composer(ui, channel, String::new()));
            }
            return Some(Vec::new());
        }
    }
    if focused != Some(editor_id(channel).as_str()) {
        return None;
    }
    let found = completions(ui, store, channel)?;
    let count = found.items.len();
    match key {
        Key::ArrowDown => ui.completion = (found.selected + 1) % count,
        Key::ArrowUp => ui.completion = (found.selected + count - 1) % count,
        Key::Enter | Key::Tab if !ctrl => {
            let insert = found.items[found.selected].insert.clone();
            return Some(chosen(ui, store, channel, &insert));
        }
        Key::Escape => ui.completions_closed = true,
        _ => return None,
    }
    Some(Vec::new())
}

/// After an event: a suggestion answered while more was written is asked
/// again; a direct conversation `/msg` opened takes its text.
pub(crate) fn on_applied(ui: &mut Ui, store: &mut Store, _applied: &Applied) -> Vec<Intent> {
    let mut intents = Vec::new();
    if let Some((_, option, said)) = ui.apps.command.as_ref().and_then(|running| {
        let (request, option, said) = running.asked.clone()?;
        store.suggested(request)?;
        Some((request, option, said))
    }) {
        let now = ui
            .apps
            .command
            .as_ref()
            .and_then(|running| running.written.get(&option).cloned())
            .unwrap_or_default();
        if now != said {
            intents.extend(suggest(ui, store, &option));
        }
    }
    if let Some((user, _)) = &ui.apps.waiting_direct {
        if let Some(direct) = store
            .direct_with(*user)
            .filter(|direct| ui.channel() == Some(*direct))
        {
            let (_, text) = ui.apps.waiting_direct.take().unwrap_or_default();
            intents.extend(set_composer(ui, direct, text));
        }
    }
    intents
}

/// The composer of `channel` with the command being run over it, or the
/// state of the last one sent.
pub(crate) fn with_form(
    surface: Element,
    ui: &Ui,
    store: &Store,
    channel: ChannelId,
    width: f32,
    now: Instant,
    look: &Look,
) -> Element {
    let pending = store
        .interaction_pending(now)
        .filter(|pending| pending.channel == channel && pending.part.is_none() && !pending.modal);
    let problem = store.interaction_problem(now).filter(|problem| {
        problem.channel == channel && problem.part.is_none() && store.modal().is_none()
    });
    let app = |id: UserId| store.display_name(id, None);
    let waiting = pending.map(|pending| {
        if pending.received {
            t("La aplicación está pensando…").to_owned()
        } else {
            t("Enviando el comando…").to_owned()
        }
    });
    let Some(running) = ui
        .apps
        .command
        .as_ref()
        .filter(|running| running.channel == channel)
    else {
        let line = match (&waiting, &problem) {
            (Some(waiting), _) => Some((waiting.clone(), false)),
            (None, Some(problem)) => Some((problem.reason.clone(), true)),
            _ => None,
        };
        let Some((said, failed)) = line else {
            return surface;
        };
        let ink = if failed { look.danger } else { look.muted };
        let mark = if failed {
            rune_components::icon(Icon::WarningTriangle, ink)
        } else {
            Element::spinner("cmd:waiting", ink, limn::SpinnerVariant::Circle)
                .size(Some(14.0), Some(14.0))
        };
        return Element::column(vec![
            Element::row(vec![
                mark.centered_y(),
                rune_components::line(said, 12.0, ink)
                    .flex(1.0)
                    .centered_y(),
            ])
            .gap(6.0)
            .padding(4.0),
            surface,
        ]);
    };
    let name = running.shown();
    let app_name = app(running.command.app);
    let description = running
        .command
        .leaves()
        .into_iter()
        .find(|(path, _)| *path == running.path)
        .map(|(_, description)| description.to_owned())
        .unwrap_or_default();
    let empty = String::new();
    let suggestions: Vec<(String, Vec<String>)> = running
        .asked
        .as_ref()
        .and_then(|(request, option, _)| {
            let choices = store.suggested(*request)?;
            Some(vec![(
                option.clone(),
                choices.iter().map(|choice| choice.name.clone()).collect(),
            )])
        })
        .unwrap_or_default();
    let fields = running
        .options()
        .iter()
        .map(|option| {
            let written = running.written.get(&option.name).unwrap_or(&empty);
            let problem = match option.kind {
                OptionKind::Attachment if option.required => {
                    Some(t("RUNE todavía no adjunta archivos a comandos").to_owned())
                }
                OptionKind::Attachment => None,
                _ if running.tried => option.parse(written).err(),
                _ => None,
            };
            OptionField {
                name: &option.name,
                description: &option.description,
                kind: option.kind,
                required: option.required,
                written,
                shown: shown(store, channel, option, written),
                problem,
                menu: menu_option(option),
                open: running
                    .menu
                    .as_ref()
                    .filter(|(open, ..)| *open == option.name)
                    .map(|(_, _, options)| options.as_slice()),
                query: running
                    .menu
                    .as_ref()
                    .filter(|(open, ..)| *open == option.name)
                    .map_or("", |(_, query, _)| query.as_str()),
                suggestions: suggestions
                    .iter()
                    .find(|(name, _)| *name == option.name)
                    .map_or(&[][..], |(_, said)| said.as_slice()),
                unsupported: (option.kind == OptionKind::Attachment)
                    .then(|| t("RUNE todavía no adjunta archivos a comandos")),
            }
        })
        .collect();
    let form = CommandForm {
        name: &name,
        app: &app_name,
        description: &description,
        fields,
        waiting: waiting.as_deref(),
        problem: problem.as_ref().map(|problem| problem.reason.as_str()),
    };
    Element::column(vec![command_form(&form, width, look), surface]).gap(8.0)
}
