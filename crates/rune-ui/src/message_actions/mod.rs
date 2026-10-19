//! What the account does with messages beyond writing them (P4): forwarding
//! them, starting and ending polls, seeing who reacted or voted, copying a
//! message's link and opening the provider's links inside RUNE, asking
//! before opening any other, the moments written in messages, `@silent`,
//! the mention of who a reply answers, formatting shortcuts in the composer,
//! and moderating voice. Each asks [`Store`] what the account may do.

mod composer;
mod dialogs;
pub(crate) mod links;
mod voice;

use std::time::{Duration, Instant};

use limn::{Element, Key, UiAction};
use rune_components::Look;
use rune_i18n::t;
use rune_model::{ChannelId, MessageId, UserId};
use rune_provider::{Failure, RequestId};
use rune_state::{Applied, Store};

use crate::{Intent, Ui, ViewContext};

pub(crate) use composer::{composer_actions, format, silent_completion, submit_options};
pub(crate) use dialogs::{DraftPoll, Forward, People, PeopleKind};
pub(crate) use links::ExternalLink;
pub(crate) use voice::menu_group as voice_menu_group;

/// Where the domains the person trusts are kept between runs.
pub(crate) const TRUSTED_KEY: &str = "trusted_link_domains";

/// The state of message actions, beyond what [`Ui`] had.
#[derive(Debug)]
pub struct MessageActions {
    pub(crate) forward: Option<Forward>,
    pub(crate) poll: Option<DraftPoll>,
    pub(crate) people: Option<People>,
    pub(crate) external: Option<ExternalLink>,
    /// The domains whose links open without asking, as the person chose.
    pub(crate) trusted: Vec<String>,
    /// A reply notifies who is answered; back on with each new reply.
    pub(crate) mention_reply: bool,
    /// A conversation a link named, being read: its request, and where.
    pub(crate) opening: Option<(RequestId, ChannelId, Option<MessageId>)>,
    /// The poll whose early end is being asked about.
    pub(crate) ending: Option<(ChannelId, MessageId)>,
}

impl Default for MessageActions {
    fn default() -> Self {
        Self {
            forward: None,
            poll: None,
            people: None,
            external: None,
            trusted: Vec::new(),
            mention_reply: true,
            opening: None,
            ending: None,
        }
    }
}

/// What to keep of the trusted domains.
pub(crate) fn kept(ui: &Ui) -> Option<String> {
    (!ui.message_actions.trusted.is_empty()).then(|| ui.message_actions.trusted.join("\n"))
}

/// Take back what [`kept`] kept.
pub(crate) fn restore(ui: &mut Ui, value: Option<String>) {
    ui.message_actions.trusted = value
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|domain| !domain.is_empty())
        .map(str::to_owned)
        .collect();
}

/// Answer the clicks and fields of message actions. `None` for the rest.
pub(crate) fn update(
    ui: &mut Ui,
    store: &mut Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    if let Some(intents) = dialogs::update(ui, store, action, now) {
        return Some(intents);
    }
    if let Some(intents) = links::update(ui, store, action, now) {
        return Some(intents);
    }
    if let Some(intents) = composer::update(ui, store, action) {
        return Some(intents);
    }
    voice::update(ui, store, action, now)
}

/// What the message menus report as `msg:{id}:{what}` for message actions.
pub(crate) fn message_action(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    message: MessageId,
    what: &str,
    now: Instant,
) -> Option<Vec<Intent>> {
    match what {
        "forward" => Some(dialogs::open_forward(ui, store, channel, message)),
        "copy-link" => Some(match store.message_link(channel, message) {
            Some(link) => {
                ui.notify(t("Enlace copiado"), None, false, now);
                vec![Intent::Copy(link)]
            }
            None => Vec::new(),
        }),
        "reactions" => Some(dialogs::open_people(
            ui,
            store,
            channel,
            message,
            PeopleKind::Reactions,
        )),
        "voters" => Some(dialogs::open_people(
            ui,
            store,
            channel,
            message,
            PeopleKind::Voters,
        )),
        "end-poll" => Some(dialogs::end_poll(ui, store, channel, message, now)),
        _ => None,
    }
}

/// Keys the window takes before the tree for message actions: the
/// formatting shortcuts in the composer, and Escape and Enter in the dialogs.
pub(crate) fn keys(ui: &mut Ui, store: &mut Store, key: Key, now: Instant) -> Option<Vec<Intent>> {
    if let Some(intents) = dialogs::keys(ui, store, key, now) {
        return Some(intents);
    }
    links::keys(ui, key)
}

/// The dialogs of message actions over `root`.
pub(crate) fn overlays(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let root = dialogs::overlays(ui, store, root, look, context);
    links::overlay(ui, root, look, context)
}

/// What an event changed for message actions: people and conversations
/// that arrived, polls that were posted or refused.
pub(crate) fn on_applied(
    ui: &mut Ui,
    store: &mut Store,
    applied: &Applied,
    now: Instant,
) -> Vec<Intent> {
    let mut intents = dialogs::on_applied(ui, store, applied, now);
    intents.extend(links::on_applied(ui, store, applied, now));
    intents
}

/// Whether a dialog of message actions shows `failure` itself.
pub(crate) fn shows_failure(ui: &Ui, failure: &Failure) -> bool {
    match failure {
        Failure::People { request } => ui
            .message_actions
            .people
            .as_ref()
            .is_some_and(|people| people.request == Some(*request)),
        Failure::NewPoll { nonce, .. } => ui
            .message_actions
            .poll
            .as_ref()
            .is_some_and(|poll| poll.posting == Some(*nonce)),
        _ => false,
    }
}

/// When the open conversation's moments relative to now read differently:
/// every 30 s while one of its loaded messages has one.
pub fn moments_tick(ui: &Ui, store: &Store) -> Option<Duration> {
    let channel = ui.channel()?;
    let relative = store
        .timeline(channel)?
        .messages()
        .iter()
        .rev()
        .take(200)
        .any(|message| {
            message.text_references.iter().any(|reference| {
                matches!(
                    reference.target,
                    rune_model::TextTarget::Time {
                        style: rune_model::TimeStyle::Relative,
                        ..
                    }
                )
            })
        });
    relative.then_some(Duration::from_secs(30))
}

/// Now, by the state's clock when it has one, as restrictions are read.
fn wall(store: &Store) -> rune_model::Timestamp {
    store
        .clock
        .map_or_else(crate::server_menu::wall_now, |clock| clock())
}

/// Someone, as a list of people shows them.
fn person(
    ui: &Ui,
    store: &Store,
    server: Option<rune_model::ServerId>,
    user: UserId,
    look: &Look,
) -> rune_components::message_actions::Person {
    let author = crate::timeline::author(ui, store, server, user, look);
    rune_components::message_actions::Person {
        user,
        handle: store
            .user(user)
            .map(|found| found.name.clone())
            .unwrap_or_default(),
        name: author.name,
        image: author.image,
        accent: author.accent,
    }
}
