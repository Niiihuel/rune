//! The dialogs of message actions: forwarding, starting a poll, who reacted
//! or voted, and the question before ending a poll.

use std::time::Instant;

use limn::{AlertDialog, Component, Element, Key, Modal, UiAction};
use rune_components::composer::ids as composer_ids;
use rune_components::message_actions::{
    self as view, ids, Destination, DraftAnswer, ForwardView, PeopleTab, PeopleView, PollView,
};
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::{ChannelId, ChannelKind, MessageId, UserId};
use rune_provider::{
    Command, Failure, NewPoll, NewPollAnswer, ProviderKind, RequestId, MAX_FORWARDS, POLL_ANSWERS,
    POLL_HOURS,
};
use rune_state::{Applied, Store};

use crate::{editor_id, Intent, Ui, ViewContext};

/// How many conversations the forwarding dialog lists, chosen ones aside.
const LISTED: usize = 16;

/// A message being forwarded: where to, and the note that goes after it.
#[derive(Debug)]
pub(crate) struct Forward {
    pub channel: ChannelId,
    pub message: MessageId,
    pub query: String,
    pub chosen: Vec<ChannelId>,
    pub note: String,
}

/// A poll being written for `channel`.
#[derive(Debug)]
pub(crate) struct DraftPoll {
    pub channel: ChannelId,
    pub question: String,
    pub answers: Vec<DraftAnswer>,
    pub hours: u32,
    pub multiselect: bool,
    /// The answer whose emoji is being picked.
    pub picking: Option<usize>,
    /// Sent with this nonce: the dialog hides until the provider refuses it.
    pub posting: Option<u64>,
    pub error: Option<String>,
}

/// Whose list is shown: who reacted with each emoji, or who voted each answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PeopleKind {
    Reactions,
    Voters,
}

/// The list of who reacted to, or voted on, a message.
#[derive(Debug)]
pub(crate) struct People {
    pub channel: ChannelId,
    pub message: MessageId,
    pub kind: PeopleKind,
    /// The emoji or answer shown, by its place.
    pub tab: usize,
    /// The page being read.
    pub request: Option<RequestId>,
    pub people: Vec<UserId>,
    pub reached_end: bool,
    pub error: Option<String>,
}

pub(crate) fn open_forward(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    message: MessageId,
) -> Vec<Intent> {
    let can = store
        .abilities(channel)
        .is_some_and(|abilities| abilities.forward.allowed());
    let found = store
        .timeline(channel)
        .and_then(|timeline| timeline.get(message))
        .is_some_and(|found| !found.delivery.is_local() && found.poll.is_none());
    if !can || !found {
        return Vec::new();
    }
    ui.message_actions.forward = Some(Forward {
        channel,
        message,
        query: String::new(),
        chosen: Vec::new(),
        note: String::new(),
    });
    vec![Intent::Focus(ids::FORWARD_QUERY.into())]
}

fn forward(ui: &mut Ui, store: &Store, now: Instant) -> Vec<Intent> {
    let Some(forward) = ui.message_actions.forward.take() else {
        return Vec::new();
    };
    let note = forward.note.trim().to_owned();
    let mut intents = Vec::new();
    let mut names = Vec::new();
    for to in forward.chosen {
        if !store
            .abilities(to)
            .is_some_and(|abilities| abilities.write.allowed())
        {
            continue;
        }
        names.push(title(store, to));
        let nonce = ui.fresh_request();
        intents.push(Intent::Command(Command::ForwardMessage {
            channel: forward.channel,
            message: forward.message,
            to,
            nonce,
        }));
        // The note goes after the copy, as a message of its own.
        if !note.is_empty() {
            intents.push(Intent::Send {
                channel: to,
                content: note.clone(),
                reply_to: None,
                mention_reply: false,
            });
        }
    }
    if !names.is_empty() {
        ui.notify(
            t("Reenviando el mensaje"),
            Some(names.join(", ")),
            false,
            now,
        );
    }
    intents
}

/// «#general», «Ada», «Grupo de estudio».
fn title(store: &Store, channel: ChannelId) -> String {
    match store.channel(channel) {
        Some(found) if found.is_thread() => found.name.clone(),
        Some(found) if found.server.is_some() => format!("#{}", found.name),
        _ => store.channel_title(channel),
    }
}

fn destinations(store: &Store, forward: &Forward) -> Vec<Destination> {
    let mut listed: Vec<ChannelId> = forward.chosen.clone();
    for channel in store.forward_targets(&forward.query, LISTED + 1) {
        if channel != forward.channel && !listed.contains(&channel) && listed.len() < LISTED {
            listed.push(channel);
        }
    }
    listed
        .into_iter()
        .filter_map(|channel| {
            let found = store.channel(channel)?;
            Some(Destination {
                channel,
                title: title(store, channel),
                detail: match found.server.and_then(|server| store.server(server)) {
                    Some(server) => server.shown_name().into(),
                    None if found.kind == ChannelKind::Group => t("Grupo").into(),
                    None => t("Mensaje directo").into(),
                },
                kind: found.kind,
                chosen: forward.chosen.contains(&channel),
            })
        })
        .collect()
}

pub(crate) fn open_poll(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let Some(channel) = ui.channel() else {
        return Vec::new();
    };
    if !store
        .abilities(channel)
        .is_some_and(|abilities| abilities.create_poll.allowed())
    {
        return Vec::new();
    }
    ui.messages.plus = false;
    ui.message_actions.poll = Some(DraftPoll {
        channel,
        question: String::new(),
        answers: vec![DraftAnswer::default(), DraftAnswer::default()],
        hours: 24,
        multiselect: false,
        picking: None,
        posting: None,
        error: None,
    });
    vec![Intent::Focus(ids::POLL_QUESTION.into())]
}

fn new_poll(draft: &DraftPoll) -> NewPoll {
    NewPoll {
        question: draft.question.trim().to_owned(),
        answers: draft
            .answers
            .iter()
            .map(|answer| NewPollAnswer {
                text: answer.text.trim().to_owned(),
                emoji: answer.emoji.clone(),
            })
            .collect(),
        hours: draft.hours,
        multiselect: draft.multiselect,
    }
}

fn post_poll(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let nonce = ui.fresh_request();
    let Some(draft) = ui
        .message_actions
        .poll
        .as_mut()
        .filter(|draft| draft.posting.is_none())
    else {
        return Vec::new();
    };
    let poll = new_poll(draft);
    if poll.problem().is_some()
        || !store
            .abilities(draft.channel)
            .is_some_and(|abilities| abilities.create_poll.allowed())
    {
        return Vec::new();
    }
    draft.posting = Some(nonce);
    draft.error = None;
    draft.picking = None;
    vec![
        Intent::Command(Command::CreatePoll {
            channel: draft.channel,
            poll,
            nonce,
        }),
        Intent::ScrollToEnd(crate::timeline_id(draft.channel)),
        Intent::Focus(editor_id(draft.channel)),
    ]
}

pub(crate) fn open_people(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    message: MessageId,
    kind: PeopleKind,
) -> Vec<Intent> {
    let allowed = store
        .abilities(channel)
        .is_some_and(|abilities| match kind {
            PeopleKind::Reactions => abilities.reaction_list.allowed(),
            PeopleKind::Voters => abilities.poll_voters.allowed(),
        });
    if !allowed || tabs(store, channel, message, kind).is_empty() {
        return Vec::new();
    }
    ui.message_actions.people = Some(People {
        channel,
        message,
        kind,
        tab: 0,
        request: None,
        people: Vec::new(),
        reached_end: false,
        error: None,
    });
    load_people(ui, store)
}

/// The tabs of a list of people: each emoji with its normal reactions, or
/// each answer with its votes; the key asked for, its label and its count.
/// Super reactions are not listed, so an emoji with only those has no tab.
fn tabs(
    store: &Store,
    channel: ChannelId,
    message: MessageId,
    kind: PeopleKind,
) -> Vec<(String, String, u32)> {
    let Some(found) = store
        .timeline(channel)
        .and_then(|timeline| timeline.get(message))
    else {
        return Vec::new();
    };
    match kind {
        PeopleKind::Reactions => found
            .reactions
            .iter()
            .filter(|reaction| reaction.count > 0)
            .map(|reaction| {
                (
                    reaction.emoji.clone(),
                    reaction.emoji.clone(),
                    reaction.count,
                )
            })
            .collect(),
        PeopleKind::Voters => found
            .poll
            .iter()
            .flat_map(|poll| &poll.answers)
            .map(|answer| {
                let label = match &answer.emoji {
                    Some(emoji) if answer.text.is_empty() => emoji.clone(),
                    Some(emoji) => format!("{emoji} {}", answer.text),
                    None => answer.text.clone(),
                };
                (answer.id.to_string(), label, answer.votes)
            })
            .collect(),
    }
}

/// Ask for the next page of the list open.
fn load_people(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let request = ui.fresh_request();
    let Some(people) = ui.message_actions.people.as_mut() else {
        return Vec::new();
    };
    let Some((key, _, _)) = tabs(store, people.channel, people.message, people.kind)
        .into_iter()
        .nth(people.tab)
    else {
        return Vec::new();
    };
    people.request = Some(request);
    people.error = None;
    let after = people.people.last().copied();
    let command = match people.kind {
        PeopleKind::Reactions => Command::LoadReactions {
            request,
            channel: people.channel,
            message: people.message,
            emoji: key,
            after,
        },
        PeopleKind::Voters => Command::LoadPollVoters {
            request,
            channel: people.channel,
            message: people.message,
            answer: key.parse().unwrap_or_default(),
            after,
        },
    };
    vec![Intent::Command(command)]
}

pub(crate) fn end_poll(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    message: MessageId,
    now: Instant,
) -> Vec<Intent> {
    match store
        .end_poll(channel, message, super::wall(store))
        .reason()
    {
        Some(reason) => {
            ui.notify(
                t("No puedes terminar la encuesta"),
                Some(reason.into()),
                true,
                now,
            );
        }
        None => ui.message_actions.ending = Some((channel, message)),
    }
    Vec::new()
}

pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    match action {
        UiAction::Clicked { id } => clicked(ui, store, id, now),
        UiAction::InputChanged { id, value } => {
            let actions = &mut ui.message_actions;
            if let Some(forward) = actions.forward.as_mut() {
                match id.as_str() {
                    ids::FORWARD_QUERY => forward.query = value.chars().take(256).collect(),
                    ids::FORWARD_NOTE => forward.note = value.chars().take(2000).collect(),
                    _ => return None,
                }
                return Some(Vec::new());
            }
            let draft = actions.poll.as_mut()?;
            if id == ids::POLL_QUESTION {
                draft.question = value
                    .chars()
                    .take(rune_provider::POLL_QUESTION_MAX)
                    .collect();
            } else {
                let index: usize = id.strip_prefix(ids::POLL_ANSWER)?.parse().ok()?;
                draft.answers.get_mut(index)?.text =
                    value.chars().take(rune_provider::POLL_ANSWER_MAX).collect();
            }
            Some(Vec::new())
        }
        UiAction::Submitted { id, .. } if id == ids::FORWARD_NOTE => Some(forward(ui, store, now)),
        _ => None,
    }
}

fn clicked(ui: &mut Ui, store: &mut Store, id: &str, now: Instant) -> Option<Vec<Intent>> {
    if id == composer_ids::POLL {
        return Some(open_poll(ui, store));
    }
    if let Some(channel) = id
        .strip_prefix(ids::FORWARD_PICK)
        .and_then(|channel| channel.parse().ok())
        .map(ChannelId)
    {
        let forward = ui.message_actions.forward.as_mut()?;
        if let Some(index) = forward.chosen.iter().position(|chosen| *chosen == channel) {
            forward.chosen.remove(index);
        } else if forward.chosen.len() < MAX_FORWARDS {
            forward.chosen.push(channel);
        }
        return Some(Vec::new());
    }
    if let Some(rest) = id.strip_prefix(ids::POLL_EMOJI_PICK) {
        let (index, glyph) = rest.split_once('=')?;
        let draft = ui.message_actions.poll.as_mut()?;
        let answer = draft.answers.get_mut(index.parse::<usize>().ok()?)?;
        answer.emoji = (answer.emoji.as_deref() != Some(glyph)).then(|| glyph.to_owned());
        draft.picking = None;
        return Some(Vec::new());
    }
    if let Some(index) = id
        .strip_prefix(ids::POLL_EMOJI)
        .and_then(|index| index.parse::<usize>().ok())
    {
        let draft = ui.message_actions.poll.as_mut()?;
        draft.picking = (draft.picking != Some(index)).then_some(index);
        return Some(Vec::new());
    }
    if let Some(index) = id
        .strip_prefix(ids::POLL_REMOVE)
        .and_then(|index| index.parse::<usize>().ok())
    {
        let draft = ui.message_actions.poll.as_mut()?;
        if draft.answers.len() > *POLL_ANSWERS.start() && index < draft.answers.len() {
            draft.answers.remove(index);
            draft.picking = None;
        }
        // The fields after it now show other answers.
        return Some(answer_fields(draft));
    }
    if let Some(hours) = id
        .strip_prefix(ids::POLL_HOURS)
        .and_then(|hours| hours.parse::<u32>().ok())
        .filter(|hours| POLL_HOURS.contains(hours))
    {
        ui.message_actions.poll.as_mut()?.hours = hours;
        return Some(Vec::new());
    }
    if let Some(tab) = id
        .strip_prefix(ids::PEOPLE_TAB)
        .and_then(|tab| tab.parse::<usize>().ok())
    {
        let people = ui.message_actions.people.as_mut()?;
        if people.tab != tab {
            people.tab = tab;
            people.people.clear();
            people.reached_end = false;
            return Some(load_people(ui, store));
        }
        return Some(Vec::new());
    }
    Some(match id {
        ids::FORWARD_SEND => forward(ui, store, now),
        ids::FORWARD_CLOSE => {
            ui.message_actions.forward = None;
            Vec::new()
        }
        ids::POLL_ADD => {
            let draft = ui.message_actions.poll.as_mut()?;
            if draft.answers.len() < *POLL_ANSWERS.end() {
                draft.answers.push(DraftAnswer::default());
            }
            vec![Intent::Focus(format!(
                "{}{}",
                ids::POLL_ANSWER,
                draft.answers.len() - 1
            ))]
        }
        ids::POLL_MULTI => {
            let draft = ui.message_actions.poll.as_mut()?;
            draft.multiselect = !draft.multiselect;
            Vec::new()
        }
        ids::POLL_EMOJI_DISMISS => {
            ui.message_actions.poll.as_mut()?.picking = None;
            Vec::new()
        }
        ids::POLL_POST => post_poll(ui, store),
        ids::POLL_CLOSE => {
            ui.message_actions.poll = None;
            Vec::new()
        }
        ids::PEOPLE_MORE => load_people(ui, store),
        ids::PEOPLE_CLOSE => {
            ui.message_actions.people = None;
            Vec::new()
        }
        END_CONFIRM => {
            let (channel, message) = ui.message_actions.ending.take()?;
            if store
                .end_poll(channel, message, super::wall(store))
                .allowed()
            {
                vec![Intent::Command(Command::EndPoll { channel, message })]
            } else {
                Vec::new()
            }
        }
        END_CANCEL => {
            ui.message_actions.ending = None;
            Vec::new()
        }
        _ => return None,
    })
}

/// The answers' fields as the draft has them, after one moved.
fn answer_fields(draft: &DraftPoll) -> Vec<Intent> {
    draft
        .answers
        .iter()
        .enumerate()
        .map(|(index, answer)| Intent::SetInput {
            id: format!("{}{index}", ids::POLL_ANSWER),
            value: answer.text.clone(),
        })
        .collect()
}

const END_CONFIRM: &str = "poll:end:confirm";
const END_CANCEL: &str = "poll:end:cancel";

pub(crate) fn keys(ui: &mut Ui, store: &mut Store, key: Key, now: Instant) -> Option<Vec<Intent>> {
    let actions = &mut ui.message_actions;
    if actions.ending.is_some() {
        return match key {
            Key::Enter => clicked(ui, store, END_CONFIRM, now),
            Key::Escape => {
                ui.message_actions.ending = None;
                Some(Vec::new())
            }
            _ => None,
        };
    }
    if key != Key::Escape {
        return None;
    }
    // The emoji picker of an answer first, then the dialog under it.
    let closed = if let Some(draft) = actions
        .poll
        .as_mut()
        .filter(|draft| draft.picking.is_some())
    {
        draft.picking = None;
        true
    } else {
        actions.people.take().is_some()
            || actions.forward.take().is_some()
            || actions
                .poll
                .take_if(|draft| draft.posting.is_none())
                .is_some()
    };
    closed.then(Vec::new)
}

pub(crate) fn on_applied(
    ui: &mut Ui,
    store: &mut Store,
    applied: &Applied,
    _now: Instant,
) -> Vec<Intent> {
    let actions = &mut ui.message_actions;
    match applied {
        Applied::People {
            request,
            people,
            reached_end,
        } => {
            if let Some(open) = actions
                .people
                .as_mut()
                .filter(|open| open.request == Some(*request))
            {
                open.request = None;
                for user in people {
                    if !open.people.contains(user) {
                        open.people.push(*user);
                    }
                }
                open.reached_end = *reached_end;
            }
        }
        Applied::Failed {
            failure: Failure::People { request },
            reason,
            ..
        } => {
            if let Some(open) = actions
                .people
                .as_mut()
                .filter(|open| open.request == Some(*request))
            {
                open.request = None;
                open.error = Some(reason.clone());
            }
        }
        Applied::Failed {
            failure: Failure::NewPoll { nonce, .. },
            reason,
            ..
        } => {
            if let Some(draft) = actions
                .poll
                .as_mut()
                .filter(|draft| draft.posting == Some(*nonce))
            {
                draft.posting = None;
                draft.error = Some(tr!("No se pudo crear la encuesta: {reason}", reason));
            }
        }
        // What was open there is gone with it.
        Applied::ChannelRemoved(channel) | Applied::Channel(channel)
            if store.channel(*channel).is_none() =>
        {
            if actions
                .people
                .as_ref()
                .is_some_and(|open| open.channel == *channel)
            {
                actions.people = None;
            }
            if actions
                .forward
                .as_ref()
                .is_some_and(|open| open.channel == *channel)
            {
                actions.forward = None;
            }
        }
        Applied::Ready => {
            actions.people = None;
            actions.forward = None;
            actions.ending = None;
        }
        _ => {}
    }
    Vec::new()
}

pub(crate) fn overlays(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let actions = &ui.message_actions;
    let width = Modal::fit_width(context.window.width, 520.0);
    let mut root = root;
    if let Some(forward) = &actions.forward {
        let destinations = destinations(store, forward);
        let preview = store
            .timeline(forward.channel)
            .and_then(|timeline| timeline.get(forward.message))
            .map(|found| {
                if found.content.contains("||") {
                    t("[Spoiler oculto]").to_owned()
                } else if found.content.trim().is_empty() {
                    t("Archivo o contenido incrustado").to_owned()
                } else {
                    found.content.chars().take(240).collect()
                }
            })
            .unwrap_or_default();
        let problem = if forward.chosen.is_empty() {
            Some(t("Elige al menos una conversación"))
        } else {
            None
        };
        root = view::forward_dialog(
            root,
            ForwardView {
                query: &forward.query,
                destinations: &destinations,
                chosen: forward.chosen.len(),
                max: MAX_FORWARDS,
                note: &forward.note,
                preview: &preview,
                problem,
            },
            width,
            look,
        );
    }
    if let Some(draft) = actions
        .poll
        .as_ref()
        .filter(|draft| draft.posting.is_none())
    {
        let problem = new_poll(draft).problem();
        root = view::poll_dialog(
            root,
            PollView {
                question: &draft.question,
                answers: &draft.answers,
                hours: draft.hours,
                multiselect: draft.multiselect,
                picking: draft.picking,
                posting: false,
                problem,
                error: draft.error.as_deref(),
                lengths: &POLL_HOURS,
                max_answers: *POLL_ANSWERS.end(),
            },
            width,
            look,
        );
    }
    if let Some(people) = &actions.people {
        let server = store.channel(people.channel).and_then(|found| found.server);
        let tabs: Vec<PeopleTab> = tabs(store, people.channel, people.message, people.kind)
            .into_iter()
            .enumerate()
            .map(|(index, (_, label, count))| PeopleTab {
                label,
                count,
                selected: index == people.tab,
            })
            .collect();
        let listed: Vec<_> = people
            .people
            .iter()
            .map(|user| super::person(ui, store, server, *user, look))
            .collect();
        let simulated = store
            .provider
            .as_ref()
            .is_some_and(|info| info.kind == ProviderKind::Simulated)
            .then(|| t("Simulado: las personas salen de los números de la simulación."));
        root = view::people_dialog(
            root,
            PeopleView {
                title: match people.kind {
                    PeopleKind::Reactions => t("Reacciones"),
                    PeopleKind::Voters => t("Votos"),
                },
                tabs: &tabs,
                people: &listed,
                loading: people.request.is_some(),
                more: !people.reached_end,
                error: people.error.as_deref(),
                empty: match people.kind {
                    PeopleKind::Reactions => t("Nadie tiene esta reacción ahora."),
                    PeopleKind::Voters => t("Nadie votó esta respuesta."),
                },
                note: simulated,
            },
            width,
            look,
        );
    }
    if actions.ending.is_some() {
        root = AlertDialog::new(
            root,
            true,
            t("Terminar encuesta"),
            t("¿La terminas ahora para todos? Nadie más va a poder votar."),
            END_CONFIRM,
            END_CANCEL,
            look.menu_palette(),
        )
        .confirm_label(t("Terminar ahora"))
        .cancel_label(t("Cancelar"))
        .destructive(look.danger)
        .width(Modal::fit_width(context.window.width, 440.0))
        .render();
    }
    root
}
