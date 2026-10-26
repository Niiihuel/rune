//! What the account does with messages: writing and sending them, with
//! completions, emoji and the reply strip; their bar and menus; editing in
//! place; deleting, with or without asking; reactions; copying; spoilers;
//! and marking a conversation unread from a message.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use limn::{AlertDialog, Component, Element, Icon, Key, Modal, UiAction, UiEvent};
use rune_components::composer::{
    composer as composer_view, ids as composer_ids, trigger_at_end, typing_sentence, Completion,
    Completions, ComposerView,
};
use rune_components::message::{self, react_ids, MessageStyle, MessageView};
use rune_components::{emoji, text, Look};
use rune_i18n::{t, tr};
use rune_model::{ChannelId, ChannelKind, MessageId, Permissions};
use rune_provider::Command;
use rune_state::Store;

use crate::{editor_id, timeline, timeline_id, Intent, Ui, ViewContext};

/// Reported by the line over a conversation marked unread by hand.
pub const UNREAD_CLEAR: &str = "unread:clear";
/// How many emoji the account's recent ones keep.
const RECENT: usize = 18;

/// The state of what is done with messages, beyond what [`Ui`] had.
#[derive(Debug, Default)]
pub struct Messages {
    pub(crate) selection: Option<crate::composer_selection::ComposerSelection>,
    /// The active composer expression tab, independent of reaction pickers.
    pub expression_tab: emoji::PickerTab,
    pub gif_url: String,
    pub gif_recent: Vec<String>,
    pub gifs: crate::GifPickerState,
    /// The GIF favorites kept on this device, when the provider keeps none.
    pub gif_favorites: Vec<rune_provider::FavoriteGif>,
    pub expression_feedback: Option<String>,
    /// Selecting a sticker stages it; only the explicit send action transmits it.
    pub pending_sticker: Option<(ChannelId, rune_model::StickerId)>,
    /// The message whose «Más» menu is open.
    pub more: Option<MessageId>,
    /// The composer's «+» menu is open.
    pub plus: bool,
    /// What the open emoji picker searches.
    pub query: String,
    /// The emoji the account used lately, the most recent first.
    pub recent: Vec<String>,
    /// What the message being edited says now.
    pub edit_draft: String,
    /// The delete dialog came from the bar or the menu, so it tells how to skip it.
    pub delete_tip: bool,
    /// Shift was held on the press whose click is coming.
    pub shift: bool,
    /// The spoilers shown, by message, in their order in it.
    pub revealed: HashMap<MessageId, Vec<usize>>,
    /// The answers picked on a multiselect poll, before «Votar».
    pub poll_choice: Option<(MessageId, Vec<u32>)>,
    /// Spoiler attachments the account revealed; until then their previews
    /// are neither shown nor requested.
    pub revealed_files: std::collections::HashSet<rune_model::AttachmentId>,
    // Reads, search and remote members (P2).
    /// The last name looked up at the provider after `@`, and where.
    pub(crate) member_lookup: Option<(rune_model::ServerId, String)>,
    // limn adoption (2026-10-18).
    /// The message whose menu a secondary click opened, and where.
    pub context: Option<(MessageId, limn::Point)>,
    /// Where the secondary click being answered was, for the menu it asks.
    pub(crate) pointer: Option<limn::Point>,
    // limn-teclado-y-superficies.
    /// Shift is held: the bar over a message shows its direct actions.
    pub shift_held: bool,
}

impl Messages {
    /// Leave a conversation: what floated over its messages goes.
    pub(crate) fn leave(&mut self) {
        self.more = None;
        self.context = None;
        self.poll_choice = None;
        self.plus = false;
        self.query.clear();
        self.selection = None;
        self.pending_sticker = None;
        self.expression_feedback = None;
    }
}

/// Whether the conversation `channel` is held unread by hand.
pub(crate) fn held_unread(store: &Store, channel: ChannelId) -> bool {
    store.read(channel).unread_from.is_some()
}

/// What can complete what is being written in `channel`: people, the
/// options that notify everyone and roles after `@`, text channels after
/// `#`, emoji after `:`.
pub fn completions(ui: &Ui, store: &Store, channel: ChannelId) -> Option<Completions> {
    if ui.completions_closed {
        return None;
    }
    let draft = ui.drafts.get(&channel)?;
    let (at, sigil, query) = trigger_at_end(draft)?;
    let lower = query.to_lowercase();
    let starts = |name: &str| name.to_lowercase().starts_with(&lower);
    let found = store.channel(channel)?;
    let server = found.server;
    let look = &ui.look();
    let shout = |what: &str| {
        if query.is_empty() {
            what.to_owned()
        } else {
            tr!(
                "{what} QUE COINCIDEN CON {query}",
                what,
                query = format!("{sigil}{}", query.to_uppercase())
            )
        }
    };
    let mut groups: Vec<(String, Vec<Completion>)> = Vec::new();
    match sigil {
        '@' => {
            let people: Vec<_> = match server {
                Some(server) => store
                    .member_groups(server)
                    .into_iter()
                    .flat_map(|group| group.members)
                    .collect(),
                None => store.direct_members(channel),
            };
            let members = people
                .into_iter()
                .filter_map(|user| store.user(user))
                .filter(|user| starts(&user.name) || starts(user.shown_name()))
                .take(8)
                // As the official client's: the face, the name in the color
                // of the top role and the handle at the end of the row.
                .map(|user| Completion {
                    insert: format!("@{}", user.name),
                    label: store.display_name(user.id, server),
                    detail: String::new(),
                    icon: Icon::At,
                    section: None,
                    face: Some((
                        look.ink(user.accent),
                        ui.picture(store.avatar_in(server, user.id)),
                    )),
                    ink: server
                        .and_then(|server| store.top_role(user.id, server))
                        .and_then(|role| role.color)
                        .filter(|_| ui.prefs.role_colors)
                        .map(|color| look.ink(color)),
                    aside: user.name.clone(),
                })
                .collect();
            groups.push((t("MIEMBROS").into(), members));
            // Message actions (P4): `@silent`, at the start of a draft.
            let mut options: Vec<Completion> =
                crate::message_actions::silent_completion(store, at, query)
                    .into_iter()
                    .collect();
            if server.is_some() && found.permissions.contains(Permissions::MENTION_EVERYONE) {
                let everyone = [
                    (
                        "everyone",
                        t("Notificar a todos los que tienen permiso para ver este canal."),
                    ),
                    (
                        "here",
                        t("Notificar a todos los que están en línea y tienen permiso para ver este canal."),
                    ),
                ]
                .into_iter()
                .filter(|(name, _)| starts(name))
                .map(|(name, detail)| Completion {
                    insert: format!("@{name}"),
                    label: format!("@{name}"),
                    detail: detail.into(),
                    icon: Icon::Users,
                    section: None,
                    face: None,
                    ink: None,
                    aside: String::new(),
                });
                options.extend(everyone);
            }
            groups.push((shout(t("OPCIONES")), options));
            if let Some(server) = server.and_then(|server| store.server(server)) {
                let everyone = server.everyone_id();
                let roles = server
                    .roles
                    .iter()
                    .filter(|role| role.id != everyone && role.mentionable && starts(&role.name))
                    .map(|role| Completion {
                        insert: format!("@{}", role.name),
                        label: format!("@{}", role.name),
                        detail: t("Notificar a los usuarios con este rol que tienen permiso para ver este canal.").into(),
                        icon: Icon::Users,
                        section: None,
                        face: None,
                        ink: role
                            .color
                            .filter(|_| ui.prefs.role_colors)
                            .map(|color| look.ink(color)),
                        aside: String::new(),
                    })
                    .collect();
                groups.push((shout(t("ROLES")), roles));
            }
        }
        '#' => {
            let server = server?;
            let channels = store
                .channel_tree(server)
                .into_iter()
                .flat_map(|(category, channels)| {
                    let category = category.map(|category| category.name.clone());
                    channels
                        .into_iter()
                        .map(move |channel| (category.clone(), channel))
                })
                .filter(|(_, channel)| channel.kind.has_messages() && starts(&channel.name))
                .take(10)
                .map(|(category, channel)| Completion {
                    insert: format!("#{}", channel.name),
                    label: channel.name.clone(),
                    detail: category.unwrap_or_default(),
                    icon: Icon::Hash,
                    section: None,
                    face: None,
                    ink: None,
                    aside: String::new(),
                })
                .collect();
            groups.push((shout(t("CANALES DE TEXTO")), channels));
        }
        _ => {
            let mut found: Vec<_> = crate::expressions::custom_choices(store, channel, query)
                .into_iter()
                .take(10)
                .map(|item| Completion {
                    insert: item.insert.clone(),
                    label: format!(":{}:", item.name),
                    detail: t("Emoji del servidor").into(),
                    icon: Icon::Emoji,
                    section: None,
                    face: None,
                    ink: None,
                    aside: String::new(),
                })
                .collect();
            found.extend(
                emoji::search(query)
                    .into_iter()
                    .take(10)
                    .map(|found| Completion {
                        insert: found.glyph.to_owned(),
                        label: format!("{}  :{}:", found.glyph, found.name()),
                        detail: String::new(),
                        icon: Icon::Emoji,
                        section: None,
                        face: None,
                        ink: None,
                        aside: String::new(),
                    }),
            );
            groups.push((shout(t("EMOJIS")), found));
        }
    }
    groups.retain(|(_, items)| !items.is_empty());
    let title = groups.first()?.0.clone();
    let mut items = Vec::new();
    for (index, (heading, group)) in groups.into_iter().enumerate() {
        for (row, mut item) in group.into_iter().enumerate() {
            if index > 0 && row == 0 {
                item.section = Some(heading.clone());
            }
            items.push(item);
        }
    }
    let selected = ui.completion % items.len();
    Some(Completions {
        title,
        items,
        selected,
    })
}

/// Put `insert` in place of what is being completed.
fn complete(ui: &mut Ui, channel: ChannelId, insert: &str) -> Vec<Intent> {
    let draft = ui.drafts.get(&channel).cloned().unwrap_or_default();
    let Some((at, _, _)) = trigger_at_end(&draft) else {
        return Vec::new();
    };
    let value = format!("{}{insert} ", &draft[..at]);
    if emoji::find(insert).is_some() {
        remember(ui, insert);
    }
    ui.set_draft(channel, Some(value.clone()));
    ui.completion = 0;
    vec![
        Intent::SetInput {
            id: editor_id(channel),
            value,
        },
        Intent::Focus(editor_id(channel)),
    ]
}

/// Keep `glyph` first among the emoji used lately.
fn remember(ui: &mut Ui, glyph: &str) {
    let recent = &mut ui.messages.recent;
    recent.retain(|kept| kept != glyph);
    recent.insert(0, glyph.to_owned());
    recent.truncate(RECENT);
}

/// Leave the open conversation's composer empty, answering nothing.
fn reset_composer(ui: &mut Ui, channel: ChannelId) -> Vec<Intent> {
    ui.set_draft(channel, None);
    ui.reply_to = None;
    ui.completion = 0;
    vec![
        Intent::SetInput {
            id: editor_id(channel),
            value: String::new(),
        },
        Intent::Focus(editor_id(channel)),
    ]
}

/// Send `value` from the composer of `channel`.
fn submit(ui: &mut Ui, channel: ChannelId, value: &str) -> Vec<Intent> {
    let content = value.trim().to_owned();
    if content.is_empty() {
        return Vec::new();
    }
    let mut intents = vec![
        Intent::Send {
            channel,
            content,
            reply_to: ui.reply_to,
            mention_reply: crate::message_actions::submit_options(ui),
        },
        Intent::ScrollToEnd(timeline_id(channel)),
    ];
    intents.extend(reset_composer(ui, channel));
    intents
}

/// Start editing the account's message `message` of `channel` in its place.
pub(crate) fn edit(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    message: MessageId,
) -> Vec<Intent> {
    let Some(messages) = store.timeline(channel).map(|timeline| timeline.messages()) else {
        return Vec::new();
    };
    let Some((index, found)) = messages
        .iter()
        .enumerate()
        .find(|(_, found)| found.id == message)
    else {
        return Vec::new();
    };
    if Some(found.author) != store.me() || found.delivery.is_local() || found.is_voice() {
        return Vec::new();
    }
    ui.editing = Some(message);
    ui.messages.edit_draft = found.content.clone();
    let field = message::editor_id(message);
    vec![
        // ↑ edits a message that may be out of view: its row, and so its
        // editor, is built once it is shown.
        Intent::ScrollToIndex {
            list: timeline_id(channel),
            index,
        },
        Intent::SetInput {
            id: field.clone(),
            value: found.content.clone(),
        },
        Intent::Focus(field),
    ]
}

/// Keep what the editor says, by Enter or by «guardar», with its emoticons
/// as emoji when asked: an empty text asks to delete the message instead,
/// without the tip about Shift.
fn save_edit(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    value: &str,
    now: Instant,
) -> Vec<Intent> {
    let Some(message) = ui.editing else {
        return Vec::new();
    };
    let content = crate::emoticons::sent(ui, value.trim());
    if content.is_empty() {
        ui.confirm_delete = Some(message);
        ui.messages.delete_tip = false;
        return Vec::new();
    }
    // As the composer, an edit says why its emoji cannot go (2026-10-18).
    if crate::expressions::refuse_unusable(
        ui,
        store,
        channel,
        &content,
        t("No se puede editar el mensaje"),
        now,
    ) {
        return vec![Intent::Repaint];
    }
    ui.editing = None;
    let mut intents = vec![Intent::Focus(editor_id(channel))];
    let before = store
        .timeline(channel)
        .and_then(|timeline| timeline.get(message))
        .map(|found| found.content.clone());
    if before.is_some_and(|before| before != content) {
        store.update_local(channel, message, |message| {
            message.content = content.clone()
        });
        intents.push(Intent::Command(Command::EditMessage {
            channel,
            message,
            content,
        }));
    }
    intents
}

fn cancel_edit(ui: &mut Ui, channel: ChannelId) -> Vec<Intent> {
    ui.editing = None;
    vec![Intent::Focus(editor_id(channel))]
}

/// Add or take back the account's `emoji` on `message`, showing it at once.
fn react(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    message: MessageId,
    emoji: &str,
) -> Vec<Intent> {
    remember(ui, emoji);
    // What the person sees decides: of two quick clicks, the last one wins.
    let add = !store
        .timeline(channel)
        .and_then(|timeline| timeline.get(message))
        .is_some_and(|shown| {
            shown
                .reactions
                .iter()
                .any(|reaction| reaction.emoji == emoji && reaction.me)
        });
    store.update_local(channel, message, |message| {
        message.set_reaction(emoji, add);
    });
    vec![Intent::Command(Command::ToggleReaction {
        channel,
        message,
        emoji: emoji.to_owned(),
        add,
    })]
}

/// A choice on the poll of `message`: an answer, «Votar» or «Quitar voto».
/// A single choice votes on its click; a multiselect one picks first. The
/// vote shows at once and is put back if the provider refuses it.
fn poll(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    message: MessageId,
    what: &str,
) -> Vec<Intent> {
    let wall = crate::server_menu::wall_now();
    let Some(poll) = store
        .timeline(channel)
        .and_then(|timeline| timeline.get(message))
        .filter(|found| !found.delivery.is_local())
        .and_then(|found| found.poll.clone())
        .filter(|poll| !poll.ended(wall))
    else {
        return Vec::new();
    };
    if !store
        .abilities(channel)
        .is_some_and(|abilities| abilities.vote.allowed())
    {
        return Vec::new();
    }
    let answers = match what {
        "remove" if poll.voted() => Vec::new(),
        "vote" if poll.multiselect && !poll.voted() => match ui.messages.poll_choice.take() {
            Some((chosen, answers)) if chosen == message && !answers.is_empty() => answers,
            _ => return Vec::new(),
        },
        _ => {
            let Some(answer) = what
                .strip_prefix("answer:")
                .and_then(|answer| answer.parse::<u32>().ok())
                .filter(|answer| !poll.voted() && poll.valid_choice(&[*answer]))
            else {
                return Vec::new();
            };
            if !poll.multiselect {
                vec![answer]
            } else {
                let picked = match &mut ui.messages.poll_choice {
                    Some((chosen, answers)) if *chosen == message => answers,
                    choice => &mut choice.insert((message, Vec::new())).1,
                };
                if let Some(index) = picked.iter().position(|id| *id == answer) {
                    picked.remove(index);
                } else {
                    picked.push(answer);
                }
                return Vec::new();
            }
        }
    };
    if !poll.valid_choice(&answers) {
        return Vec::new();
    }
    store.update_local(channel, message, |message| {
        if let Some(poll) = message.poll.as_mut() {
            poll.vote(&answers);
        }
    });
    vec![Intent::Command(Command::VotePoll {
        channel,
        message,
        answers,
    })]
}

/// Delete `message` of `channel`: the provider confirms it.
fn delete(ui: &mut Ui, channel: ChannelId, message: MessageId) -> Vec<Intent> {
    ui.confirm_delete = None;
    if ui.editing == Some(message) {
        ui.editing = None;
    }
    vec![
        Intent::Command(Command::DeleteMessage { channel, message }),
        Intent::Focus(editor_id(channel)),
    ]
}

/// Reopen the picker with the input belonging to its active tab focused.
fn open_picker(ui: &mut Ui, base: &str) -> Vec<Intent> {
    ui.messages.query.clear();
    let field =
        if base == composer_ids::EMOJI && ui.messages.expression_tab == emoji::PickerTab::Gif {
            if ui.messages.gifs.available {
                crate::gifs::QUERY.into()
            } else {
                format!("{base}:gif-url")
            }
        } else {
            emoji::ids::search(base)
        };
    vec![Intent::Focus(field)]
}

/// Answer what messages and the composer report. `None` for the actions of other parts.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    if let Some(intents) = crate::uploads::update(ui, store, action) {
        return Some(intents);
    }
    if let Some(intents) = crate::gifs::update(ui, store, action, now) {
        return Some(intents);
    }
    if let Some(intents) = crate::gif_favorites::update(ui, store, action) {
        return Some(intents);
    }
    match action {
        UiAction::Clicked { id } => clicked(ui, store, id, now),
        UiAction::InputChanged { id, value } => {
            let channel = ui.channel()?;
            if *id == format!("{}:gif-url", composer_ids::EMOJI) {
                ui.messages.gif_url = value.chars().take(2048).collect();
                ui.messages.expression_feedback = None;
                return Some(Vec::new());
            }
            if *id == editor_id(channel) {
                return Some(draft_changed(ui, store, channel, value, now));
            }
            if ui.editing.map(message::editor_id).as_ref() == Some(id) {
                ui.messages.edit_draft = value.clone();
                return Some(Vec::new());
            }
            if *id == emoji::ids::search(composer_ids::EMOJI)
                || *id == emoji::ids::search(react_ids::BASE)
            {
                ui.messages.query = value.clone();
                return Some(Vec::new());
            }
            None
        }
        UiAction::Submitted { id, value } => {
            let channel = ui.channel()?;
            if *id == editor_id(channel) {
                return Some(send_draft(ui, store, channel, value, now));
            }
            if ui.editing.map(message::editor_id).as_ref() == Some(id) {
                return Some(save_edit(ui, store, channel, value, now));
            }
            None
        }
        _ => None,
    }
}

fn draft_changed(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    value: &str,
    now: Instant,
) -> Vec<Intent> {
    ui.set_draft(channel, Some(value.to_owned()));
    ui.completions_closed = false;
    ui.completion = 0;
    let lookup = lookup_members(ui, store, channel);
    if lookup.is_some() {
        return lookup
            .into_iter()
            .chain(typing(ui, store, channel, value, now))
            .collect();
    }
    typing(ui, store, channel, value, now)
}

/// Those whose name starts with what follows `@` in a server, looked up at
/// the provider beyond the members known; the same name asks once (P2).
fn lookup_members(ui: &mut Ui, store: &Store, channel: ChannelId) -> Option<Intent> {
    let (_, '@', query) = trigger_at_end(ui.drafts.get(&channel)?)? else {
        return None;
    };
    let server = store.channel(channel)?.server?;
    let query = query.to_lowercase();
    if query.is_empty()
        || !store.member_search(server).allowed()
        || ui.messages.member_lookup.as_ref() == Some(&(server, query.clone()))
    {
        return None;
    }
    ui.messages.member_lookup = Some((server, query.clone()));
    Some(Intent::Command(Command::SearchMembers { server, query }))
}

fn typing(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    value: &str,
    now: Instant,
) -> Vec<Intent> {
    if value.trim().is_empty()
        || !store
            .abilities(channel)
            .is_some_and(|abilities| abilities.typing.allowed())
    {
        return Vec::new();
    }
    // Others see the account typing; once every few seconds is enough.
    let recently = ui
        .typing_sent
        .get(&channel)
        .is_some_and(|sent| now.duration_since(*sent) < Duration::from_secs(8));
    if !recently {
        ui.typing_sent.insert(channel, now);
        return vec![Intent::Command(Command::StartTyping { channel })];
    }
    Vec::new()
}

/// What every choice does first, whichever part answers it: it closes the
/// menus of messages and of «+», and Shift counts only for the delete it
/// was held on.
pub(crate) fn before(ui: &mut Ui, action: &UiAction) {
    let UiAction::Clicked { id } = action else {
        return;
    };
    if !id.ends_with(":actions:more") {
        ui.messages.more = None;
    }
    if !id.ends_with(":context") {
        ui.messages.context = None;
    }
    if id != composer_ids::PLUS {
        ui.messages.plus = false;
    }
    if !id.ends_with(":delete") {
        ui.messages.shift = false;
    }
}

/// A message's menu of a secondary click goes with the message: deleted,
/// it is not drawn and nothing would close it.
pub(crate) fn forget_gone(ui: &mut Ui, store: &Store) {
    let Some((message, _)) = ui.messages.context else {
        return;
    };
    let there = ui
        .channel()
        .and_then(|channel| store.timeline(channel))
        .is_some_and(|timeline| timeline.messages().iter().any(|shown| shown.id == message));
    if !there {
        ui.messages.context = None;
    }
}

/// A message's menu, «Más» or the secondary click's, closes once its row
/// leaves the list: the list stops building the row, so the menu is no
/// longer drawn, and it must not come back with the row later.
pub(crate) fn forget_unshown_menus(ui: &mut Ui, tree: &limn::UiTree) {
    let shown = |dismiss: String| {
        let surface = format!("popover_surface:{dismiss}");
        tree.frame()
            .regions
            .iter()
            .any(|region| region.id == surface)
    };
    if let Some((message, _)) = ui.messages.context {
        if !shown(format!("msg:{message}:context:dismiss")) {
            ui.messages.context = None;
        }
    }
    if let Some(message) = ui.messages.more {
        if !shown(format!("msg:{message}:actions:more:dismiss")) {
            ui.messages.more = None;
        }
    }
}

fn clicked(ui: &mut Ui, store: &mut Store, id: &str, now: Instant) -> Option<Vec<Intent>> {
    if let Some(intents) = crate::expressions::clicked(ui, store, id, now) {
        return Some(intents);
    }
    let channel = ui.channel();
    if let Some(rest) = id.strip_prefix("react:") {
        let (message, emoji) = rest.split_once(':')?;
        let message = MessageId(message.parse().ok()?);
        return Some(react(ui, store, channel?, message, emoji));
    }
    if let Some(glyph) = id.strip_prefix(react_ids::PICK) {
        let target = ui.reacting.take();
        return Some(match (channel, target) {
            (Some(channel), Some(message)) => react(ui, store, channel, message, glyph),
            _ => Vec::new(),
        });
    }
    if let Some(glyph) = id.strip_prefix(composer_ids::EMOJI_PICK) {
        ui.emoji_open = false;
        return Some(insert_emoji(ui, channel?, glyph, now));
    }
    for base in [composer_ids::EMOJI, react_ids::BASE] {
        if let Some(index) = id
            .strip_prefix(&format!("{base}:cat="))
            .and_then(|index| index.parse().ok())
        {
            return Some(vec![Intent::Reveal {
                id: emoji::ids::section(base, index),
                margin: 0.0,
            }]);
        }
    }
    if let Some(insert) = id.strip_prefix(composer_ids::MENTION_PICK) {
        return Some(complete(ui, channel?, insert));
    }
    if let Some(rest) = id.strip_prefix("msg:") {
        let (message, what) = rest.split_once(':')?;
        let message = MessageId(message.parse().ok()?);
        return message_action(ui, store, channel?, message, what, now);
    }
    Some(match id {
        composer_ids::CANCEL => {
            ui.reply_to = None;
            crate::uploads::touch(ui, channel?);
            vec![Intent::Focus(editor_id(channel?))]
        }
        composer_ids::EMOJI => {
            ui.emoji_open = !ui.emoji_open;
            if ui.emoji_open {
                open_picker(ui, composer_ids::EMOJI)
            } else {
                Vec::new()
            }
        }
        composer_ids::EMOJI_DISMISS => {
            ui.emoji_open = false;
            Vec::new()
        }
        composer_ids::PLUS => {
            ui.messages.plus = !ui.messages.plus;
            Vec::new()
        }
        composer_ids::PLUS_DISMISS => Vec::new(),
        composer_ids::MENTIONS_DISMISS => {
            ui.completions_closed = true;
            Vec::new()
        }
        react_ids::DISMISS => {
            ui.reacting = None;
            Vec::new()
        }
        "delete:confirm" => {
            let message = ui.confirm_delete?;
            delete(ui, channel?, message)
        }
        "delete:cancel" => {
            ui.confirm_delete = None;
            Vec::new()
        }
        UNREAD_CLEAR => {
            let channel = channel?;
            ui.new_marker.remove(&channel);
            ui.unread_anchor = None;
            store
                .mark_read_command(channel)
                .map(|command| vec![Intent::Command(command)])
                .unwrap_or_default()
        }
        _ => return None,
    })
}

/// Replace the current composer selection with `glyph`.
fn insert_emoji(ui: &mut Ui, channel: ChannelId, glyph: &str, now: Instant) -> Vec<Intent> {
    let intents = crate::composer_selection::insert(ui, channel, glyph, false, now);
    if !intents.is_empty() {
        remember(ui, glyph);
    }
    intents
}

/// What the bar, the menus and the row of `message` report as `msg:{id}:{what}`.
fn message_action(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    message: MessageId,
    what: &str,
    now: Instant,
) -> Option<Vec<Intent>> {
    let found = store
        .timeline(channel)
        .and_then(|timeline| timeline.get(message))
        .cloned();
    if let Some((kind, glyph)) = what.split_once('=') {
        return matches!(kind, "quick" | "menu-quick")
            .then(|| react(ui, store, channel, message, glyph));
    }
    if let Some(what) = what.strip_prefix("poll:") {
        return Some(poll(ui, store, channel, message, what));
    }
    // A card's picture opens in the viewer.
    if let Some(what) = what.strip_prefix("card:") {
        return Some(crate::attachments::open_card(
            ui, store, channel, message, what,
        ));
    }
    // A spoiler of the text, or one of an app's parts after the text's.
    let part = what
        .strip_prefix("component:")
        .and_then(|n| n.strip_suffix(":reveal"));
    if let Some(index) = what.strip_prefix("spoiler:").or(part) {
        let mut index: usize = index.parse().ok()?;
        if part.is_some() {
            index = index.checked_add(rune_components::message::PARTS_SPOILER)?;
        }
        let shown = ui.messages.revealed.entry(message).or_default();
        if !shown.contains(&index) {
            shown.push(index);
        }
        return Some(Vec::new());
    }
    // A voice card joins only the channel it names, as the voice list would.
    if let Some(voice) = what
        .strip_prefix("voice:")
        .and_then(|voice| voice.parse().ok())
        .map(ChannelId)
    {
        let named = found
            .as_ref()
            .is_some_and(|found| found.embeds.iter().any(|card| card.channel == Some(voice)));
        return Some(if named {
            crate::voice::join(ui, store, voice, now)
        } else {
            Vec::new()
        });
    }
    if what == "poll-result" {
        let poll = found
            .filter(|found| found.kind == rune_model::MessageKind::PollResult)
            .and_then(|found| found.reply_to);
        return Some(poll.map_or_else(Vec::new, |poll| {
            crate::open_message(
                ui,
                store,
                channel,
                poll,
                rune_model::HistorySource::Provider,
            )
        }));
    }
    // The `index`th invitation card of the message.
    if let Some((index, what)) = what
        .strip_prefix("invitation:")
        .and_then(|rest| rest.split_once(':'))
    {
        let Ok(index) = index.parse() else {
            return Some(Vec::new());
        };
        return Some(crate::invitation_cards::act(
            ui, store, channel, message, index, what, now,
        ));
    }
    Some(match what {
        "reply" => {
            ui.reply_to = Some(message);
            ui.message_actions.mention_reply = true;
            crate::uploads::touch(ui, channel);
            vec![Intent::Focus(editor_id(channel))]
        }
        "edit" => edit(ui, store, channel, message),
        "thread" => crate::threads::start_from(ui, store, channel, message, now),
        "edit-save" => {
            let draft = ui.messages.edit_draft.clone();
            save_edit(ui, store, channel, &draft, now)
        }
        "edit-cancel" => cancel_edit(ui, channel),
        "delete" if std::mem::take(&mut ui.messages.shift) => delete(ui, channel, message),
        "delete" => {
            ui.confirm_delete = Some(message);
            ui.messages.delete_tip = true;
            Vec::new()
        }
        "react" | "react-more" => {
            ui.reacting = Some(message);
            open_picker(ui, react_ids::BASE)
        }
        "actions:more" => {
            ui.messages.more = match ui.messages.more {
                Some(open) if open == message => None,
                _ => Some(message),
            };
            Vec::new()
        }
        "actions:more:dismiss" | "context:dismiss" => Vec::new(),
        // A secondary click opens the menu where it was; asked from the
        // keyboard, the bar's «Más» opens it.
        "context" => {
            match ui.messages.pointer.take() {
                Some(at) => ui.messages.context = Some((message, at)),
                None => ui.messages.more = Some(message),
            }
            Vec::new()
        }
        "copy" => {
            ui.notify(t("Texto copiado"), None, false, now);
            found
                .map(|found| vec![Intent::Copy(found.content.clone())])
                .unwrap_or_default()
        }
        "copy-id" => vec![Intent::Copy(message.0.to_string())],
        "upload:cancel" => {
            if !found
                .as_ref()
                .is_some_and(|message| matches!(message.delivery, rune_model::Delivery::Sending))
            {
                return Some(Vec::new());
            }
            store
                .pending_attachment_request(channel, message)
                .map(|request| Intent::CancelUpload { request })
                .into_iter()
                .collect()
        }
        "unread" => {
            if !found
                .as_ref()
                .is_some_and(|message| message.channel == channel && !message.delivery.is_local())
                || !store
                    .abilities(channel)
                    .is_some_and(|a| a.read_history.allowed())
            {
                return Some(Vec::new());
            }
            let command = store.mark_unread_command(channel, message);
            ui.new_marker.insert(channel, message);
            command.into_iter().map(Intent::Command).collect()
        }
        "retry"
            if store
                .unsent(channel, message)
                .is_some_and(|unsent| !unsent.files.is_empty()) =>
        {
            crate::uploads::retry(ui, store, channel, message)
        }
        "retry" => vec![Intent::Resend { channel, message }],
        "discard" => {
            let mut intents = crate::uploads::discard(store, channel, message);
            let text = store.discard(channel, message)?;
            // What was written goes back to an empty composer, not lost.
            if ui.drafts.get(&channel).is_none_or(|draft| draft.is_empty()) {
                ui.set_draft(channel, Some(text.clone()));
                intents.extend([
                    Intent::SetInput {
                        id: editor_id(channel),
                        value: text,
                    },
                    Intent::Focus(editor_id(channel)),
                ]);
                return Some(intents);
            }
            intents
        }
        // Message actions (P4): forward, copy the link, who reacted or voted, end a poll.
        other => {
            return crate::message_actions::message_action(ui, store, channel, message, other, now)
        }
    })
}

/// Remember whether Shift was held on a press, for deleting without asking:
/// until the click its release makes. Anything else forgets it.
pub(crate) fn note_press(ui: &mut Ui, event: &UiEvent) {
    ui.messages.pointer = match event {
        UiEvent::ContextMenu { position } => Some(*position),
        _ => None,
    };
    match event {
        UiEvent::PointerDownModified { shift, .. } | UiEvent::PointerDownEditor { shift, .. } => {
            ui.messages.shift = *shift
        }
        UiEvent::PointerMove(_) | UiEvent::PointerUp(_) => {}
        _ => ui.messages.shift = false,
    }
}

/// Whether Shift is held, as limn's keys and changes of modifiers say
/// (limn-teclado-y-superficies): the bar over a message widens with it, as
/// in Serein and the official client.
pub(crate) fn note_modifiers(ui: &mut Ui, event: &UiEvent) {
    match event {
        UiEvent::ModifiersChanged(held) => ui.messages.shift_held = held.shift,
        UiEvent::Key { shift, .. } => ui.messages.shift_held = *shift,
        _ => {}
    }
}

/// Keys for messages that the window takes before the tree: the delete
/// dialog's Enter, leaving the editor, the pickers' searches, and in the
/// composer the completions, Escape and ↑ to edit the last own message.
pub(crate) fn keys(
    ui: &mut Ui,
    store: &mut Store,
    key: Key,
    ctrl: bool,
    focused: Option<&str>,
) -> Option<Vec<Intent>> {
    let channel = ui.channel()?;
    if let Some(message) = ui.confirm_delete {
        return match key {
            Key::Enter => Some(delete(ui, channel, message)),
            Key::Escape => {
                ui.confirm_delete = None;
                Some(Vec::new())
            }
            _ => None,
        };
    }
    if focused.is_some() && ui.editing.map(message::editor_id).as_deref() == focused {
        return (key == Key::Escape).then(|| cancel_edit(ui, channel));
    }
    if let Some(intents) = crate::gifs::key(ui, store, key, focused) {
        return Some(intents);
    }
    if focused == Some(format!("{}:gif-url", composer_ids::EMOJI).as_str()) {
        return match key {
            Key::Escape => {
                ui.emoji_open = false;
                Some(vec![Intent::Focus(editor_id(channel))])
            }
            Key::Enter => crate::expressions::clicked(
                ui,
                store,
                &format!("{}:gif-insert", composer_ids::EMOJI),
                Instant::now(),
            ),
            _ => None,
        };
    }
    for base in [composer_ids::EMOJI, react_ids::BASE] {
        if focused != Some(emoji::ids::search(base).as_str()) {
            continue;
        }
        if base == composer_ids::EMOJI && key == Key::Enter {
            if let Some(result) = crate::expressions::choose_first(ui, store, Instant::now()) {
                return Some(result);
            }
        }
        if base == composer_ids::EMOJI && ui.messages.expression_tab != emoji::PickerTab::Emoji {
            return (key == Key::Escape).then(|| {
                ui.emoji_open = false;
                vec![Intent::Focus(editor_id(channel))]
            });
        }
        let first = emoji::search(&ui.messages.query)
            .first()
            .map(|found| found.glyph);
        return match (key, first) {
            (Key::Escape, _) => {
                ui.emoji_open = false;
                ui.reacting = None;
                Some(vec![Intent::Focus(editor_id(channel))])
            }
            (Key::Enter, Some(glyph)) if base == composer_ids::EMOJI => {
                ui.emoji_open = false;
                Some(insert_emoji(ui, channel, glyph, Instant::now()))
            }
            (Key::Enter, Some(glyph)) => {
                let message = ui.reacting.take()?;
                Some(react(ui, store, channel, message, glyph))
            }
            _ => None,
        };
    }
    if focused != Some(editor_id(channel).as_str()) {
        return None;
    }
    if let Some(found) = completions(ui, store, channel) {
        let count = found.items.len();
        match key {
            Key::ArrowDown => ui.completion = (found.selected + 1) % count,
            Key::ArrowUp => ui.completion = (found.selected + count - 1) % count,
            Key::Enter | Key::Tab if !ctrl => {
                let insert = found.items[found.selected].insert.clone();
                return Some(complete(ui, channel, &insert));
            }
            Key::Escape => ui.completions_closed = true,
            _ => return None,
        }
        return Some(Vec::new());
    }
    match key {
        Key::Escape if ui.emoji_open || ui.messages.plus => {
            ui.emoji_open = false;
            ui.messages.plus = false;
            Some(Vec::new())
        }
        Key::Escape if ui.editing.is_some() => Some(cancel_edit(ui, channel)),
        Key::Escape if ui.reply_to.is_some() => {
            ui.reply_to = None;
            crate::uploads::touch(ui, channel);
            Some(Vec::new())
        }
        _ => None,
    }
}

/// Send what the composer of `channel` holds, as Enter does there.
pub(crate) fn send_draft(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    value: &str,
    now: Instant,
) -> Vec<Intent> {
    // What is sent, with or without its emoticons as emoji.
    let value = &crate::emoticons::sent(ui, value);
    // Emoji and states (2026-10-18): the composer says why.
    if crate::expressions::refuse_unusable(
        ui,
        store,
        channel,
        value,
        t("No se puede enviar el mensaje"),
        now,
    ) {
        return vec![Intent::Repaint];
    }
    if let Some(intents) = crate::uploads::submit(ui, store, channel, value) {
        return intents;
    }
    submit(ui, channel, value)
}

/// The account's last message of `channel` to edit, from an empty composer
/// (Up by default).
pub(crate) fn edit_last(ui: &mut Ui, store: &Store, channel: ChannelId) -> Option<Vec<Intent>> {
    if ui
        .drafts
        .get(&channel)
        .is_some_and(|draft| !draft.is_empty())
    {
        return None;
    }
    let me = store.me()?;
    let last = store
        .timeline(channel)?
        .messages()
        .iter()
        .rev()
        .find(|message| message.author == me && !message.delivery.is_local())
        .map(|message| message.id)?;
    Some(edit(ui, store, channel, last))
}

/// The composer of `channel`, `width` wide.
pub(crate) fn composer(
    ui: &Ui,
    store: &Store,
    channel: ChannelId,
    width: f32,
    context: &ViewContext,
    look: &Look,
) -> Element {
    let conversation = store.channel(channel);
    let server = conversation.and_then(|channel| channel.server);
    let title = store.channel_title(channel);
    let abilities = store.abilities(channel);
    let disabled = abilities
        .as_ref()
        .and_then(|abilities| abilities.write.reason().map(str::to_owned))
        .map(|reason| crate::profile::timeout_notice(store, server).unwrap_or(reason));
    let attach_denied = abilities
        .as_ref()
        .and_then(|abilities| abilities.attach.reason().map(str::to_owned))
        .or_else(|| {
            ui.uploads
                .limits
                .is_none()
                .then(|| t("Los archivos no están disponibles en esta conexión").into())
        })
        .or_else(|| crate::uploads::attach_denied(ui, channel));
    let reply = ui.reply_to.map(|reply| {
        match store
            .timeline(channel)
            .and_then(|timeline| timeline.get(reply))
        {
            Some(answered) => {
                let author = timeline::author(ui, store, server, answered.author, look);
                (author.name, author.ink)
            }
            None => (t("un mensaje").into(), look.text),
        }
    });
    let typing: Vec<String> = store
        .typing(channel, context.now)
        .into_iter()
        .map(|user| store.display_name(user, server))
        .collect();
    let placeholder = match conversation.map(|channel| channel.kind) {
        Some(ChannelKind::Direct) | Some(ChannelKind::Group) => {
            tr!("Enviar mensaje a @{title}", title)
        }
        _ => tr!("Enviar mensaje a #{title}", title),
    };
    let draft = ui.drafts.get(&channel).cloned().unwrap_or_default();
    let actions = crate::message_actions::composer_actions(ui, store, channel, &draft);
    let custom = crate::expressions::custom_items(ui, store, channel);
    let stickers = crate::expressions::sticker_items(ui, store, channel);
    let favorites: Vec<String> = crate::gif_favorites::shown(ui, store)
        .iter()
        .map(|gif| gif.url.clone())
        .collect();
    let favorites_scope = crate::gif_favorites::scope(store);
    let surface = composer_view(
        ComposerView {
            editor: editor_id(channel),
            highlights: text::draft_highlights(&draft, look),
            draft,
            placeholder,
            width,
            reply,
            disabled,
            attach_denied,
            attach_hint: crate::uploads::limits(ui, store, channel).map(|limits| {
                let size = rune_components::file_size(limits.max_file_bytes);
                if limits.max_files == 1 {
                    tr!("Máximo 1 archivo · {size} por archivo", size)
                } else {
                    tr!(
                        "Máximo {count} archivos · {size} por archivo",
                        count = limits.max_files,
                        size
                    )
                }
            }),
            plus_open: ui.messages.plus,
            typing: typing_sentence(&typing),
            emoji: ui.emoji_open.then_some(emoji::PickerView {
                base: composer_ids::EMOJI,
                query: &ui.messages.query,
                recent: &ui.messages.recent,
                placeholder: t("Encuentra el emoji ideal"),
                tabs: true,
                expressions: Some(emoji::ExpressionView {
                    tab: ui.messages.expression_tab,
                    custom: &custom,
                    stickers: &stickers,
                    gif_url: &ui.messages.gif_url,
                    gif_recent: &ui.messages.gif_recent,
                    gif_search: crate::gifs::view(ui, store),
                    gif_favorites: &favorites,
                    gif_favorites_scope: &favorites_scope,
                    feedback: ui.messages.expression_feedback.as_deref(),
                }),
            }),
            // The `/` of commands first (P3).
            completions: crate::commands::completions(ui, store, channel)
                .or_else(|| completions(ui, store, channel)),
            actions,
        },
        look,
    );
    let surface = crate::commands::with_form(surface, ui, store, channel, width, context.now, look);
    let surface =
        crate::uploads::with_draft(surface, ui, channel, width, context.window.height, look);
    // Rules and onboarding (2026-10-18).
    let surface = crate::onboarding::with_steps(surface, ui, store, server, look);
    crate::expressions::with_sticker_stage(surface, ui, store, channel, look)
}

/// The line over a conversation marked unread by hand: how many messages
/// since when, and a way to mark it read.
pub(crate) fn unread_bar(
    _ui: &Ui,
    store: &Store,
    channel: ChannelId,
    look: &Look,
) -> Option<Element> {
    let from = store.read(channel).unread_from?;
    let messages = store.timeline(channel)?.messages();
    let start = messages.partition_point(|message| message.id < from);
    let count = messages[start..]
        .iter()
        .filter(|message| !message.delivery.is_local())
        .count();
    let said = if let Some(first) = messages.get(start).filter(|message| message.id == from) {
        if count == 1 {
            tr!(
                "1 mensaje cargado desde las {time}",
                time = rune_components::time::clock(first.timestamp)
            )
        } else {
            tr!(
                "{count} mensajes cargados desde las {time}",
                count,
                time = rune_components::time::clock(first.timestamp)
            )
        }
    } else {
        t("Recordatorio de un mensaje que no está cargado").into()
    };
    let read_label = if store.read_sync() {
        t("Marcar como leído")
    } else {
        t("Marcar como leído (solo en RUNE)")
    };
    let clear = Element::clickable(
        UNREAD_CLEAR,
        Element::row(vec![
            rune_components::strong_line(read_label, 14.0, look.on_accent).centered_y(),
            rune_components::icon(Icon::Check, look.on_accent).centered_y(),
        ])
        .gap(6.0)
        .padding(6.0),
    )
    .radius(6.0)
    .hover_background(Look::alpha(look.on_accent, 30))
    .accessible(limn::Access::new(limn::Role::Button).name(read_label));
    Some(
        Element::row(vec![
            rune_components::gap(10.0, 0.0),
            rune_components::strong_line(said, 14.0, look.on_accent)
                .flex(1.0)
                .centered_y(),
            clear.centered_y(),
            rune_components::gap(6.0, 0.0),
        ])
        .size(None, Some(34.0))
        .corner_radii(limn::CornerRadii::bottom(8.0))
        .background(look.accent),
    )
}

/// The dialog that asks before deleting a message, over `root`: what the
/// message says, and, opened from its bar or menu, how to skip it.
pub(crate) fn overlays(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let doomed = ui
        .confirm_delete
        .zip(ui.channel())
        .and_then(|(message, channel)| {
            store
                .timeline(channel)
                .and_then(|timeline| timeline.get(message))
                .map(|found| (channel, found.clone()))
        });
    let Some((channel, doomed)) = doomed else {
        return root;
    };
    let width = Modal::fit_width(context.window.width, 510.0);
    let server = store.channel(channel).and_then(|channel| channel.server);
    let inner = Modal::body_width(width) - 2.0;
    let preview = message::message_row(
        MessageView {
            message: &doomed,
            author: timeline::author(ui, store, server, doomed.author, look),
            continued: false,
            day: None,
            first_new: false,
            start: None,
            reply: None,
            mentions_me: false,
            abilities: Default::default(),
            reacting: false,
            author_card: None,
            width: inner,
            now: context.wall,
            style: MessageStyle {
                sample: true,
                reactions: false,
                ..ui.prefs.message_style()
            },
        },
        look,
    );
    let mut body = vec![Element::panel(preview)
        .background(look.canvas)
        .border(look.border, 1.0)
        .radius(8.0)
        .clip_children()
        .size(Some(inner), None)];
    if ui.messages.delete_tip {
        body.push(
            Element::formatted_text(vec![
                limn::TextSpan::new(
                    t("CONSEJO ÚTIL: "),
                    look.online,
                    12.0,
                    limn::FontFamily::SansBold,
                ),
                limn::TextSpan::new(
                    t("Puedes mantener presionado Shift y hacer clic en "),
                    look.muted,
                    12.0,
                    limn::FontFamily::Sans,
                ),
                limn::TextSpan::new(
                    t("eliminar mensaje"),
                    look.text,
                    12.0,
                    limn::FontFamily::SansMedium,
                ),
                limn::TextSpan::new(
                    t(" para omitir esta confirmación."),
                    look.muted,
                    12.0,
                    limn::FontFamily::Sans,
                ),
            ])
            .size(Some(inner), None),
        );
    }
    AlertDialog::new(
        root,
        true,
        t("Eliminar mensaje"),
        t("¿Seguro que quieres eliminar este mensaje?"),
        "delete:confirm",
        "delete:cancel",
        look.menu_palette(),
    )
    .confirm_label(t("Eliminar"))
    .cancel_label(t("Cancelar"))
    .destructive(look.danger)
    .width(width)
    .body(Element::column(body).gap(12.0))
    .render()
}
