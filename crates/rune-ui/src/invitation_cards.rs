//! Invitation links in the timeline look up where they lead on their own, as
//! the reference client does: only for visible rows, a few at a time, cached,
//! and never joining. Each link of a message has its own card. «Unirse»
//! opens the invitation in RUNE to accept it, with its own confirmation,
//! when the connection can accept that kind; otherwise its page in the browser.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use limn::UiTree;
use rune_components::message::{invitation_links, InvitationCard, InvitedKind, InvitedServer};
use rune_model::{ChannelId, Message, MessageId, UserId};
use rune_provider::{
    InvitationDestination, InvitationError, InvitationPreview, InvitationResult, InvitationTarget,
    RequestId,
};
use rune_state::Store;

use crate::{timeline_id, Intent, Ui};

/// How long a server's preview stands before it is looked up again.
const FRESH: Duration = Duration::from_secs(5 * 60);
/// How long a failed lookup waits before another, unless told longer.
const RETRY: Duration = Duration::from_secs(60);
/// The most lookups in flight at once.
const IN_FLIGHT: usize = 8;
/// A lookup without an answer by then frees its place; the provider's own
/// limit is shorter.
const LOST: Duration = Duration::from_secs(30);
/// The most links remembered; past it the answers start over.
const LIMIT: usize = 256;

#[derive(Debug, Default)]
pub(crate) struct InvitationCards {
    account: Option<UserId>,
    /// What each link of the timeline names, as the adapter recognized it.
    links: HashMap<String, Option<InvitationTarget>>,
    /// By the invitation's canonical address.
    entries: HashMap<String, Entry>,
}

#[derive(Debug, Default)]
struct Entry {
    pending: Option<(RequestId, Instant)>,
    answer: Option<(Instant, Result<InvitationPreview, InvitationError>)>,
}

impl Entry {
    /// Whether it needs no lookup now: one is under way, or its answer stands.
    fn settled(&self, now: Instant) -> bool {
        if self
            .pending
            .is_some_and(|(_, since)| now.saturating_duration_since(since) < LOST)
        {
            return true;
        }
        let Some((at, answer)) = &self.answer else {
            return false;
        };
        let stands = match answer {
            Ok(_) | Err(InvitationError::Invalid | InvitationError::Unsupported) => FRESH,
            Err(InvitationError::RateLimited { retry_after_secs }) => {
                Duration::from_secs(*retry_after_secs).max(RETRY)
            }
            Err(InvitationError::Unavailable) => RETRY,
        };
        now.saturating_duration_since(*at) < stands
    }

    fn in_flight(&self, now: Instant) -> bool {
        self.pending
            .is_some_and(|(_, since)| now.saturating_duration_since(since) < LOST)
    }
}

/// The lookups the visible invitation cards need now, at most eight in flight;
/// `recognize` is the adapter's own reading of a link. The host starts each
/// and hands its answer to [`invitation_card_result`].
pub fn wanted_invitation_cards(
    ui: &mut Ui,
    store: &Store,
    tree: &UiTree,
    now: Instant,
    recognize: impl Fn(&str) -> Option<InvitationTarget>,
) -> Vec<(RequestId, InvitationTarget)> {
    if ui.invitation_cards.account != store.me() {
        ui.invitation_cards = InvitationCards {
            account: store.me(),
            ..InvitationCards::default()
        };
    }
    let Some(channel) = ui.channel() else {
        return Vec::new();
    };
    if !store.invitation_preview().allowed() {
        return Vec::new();
    }
    let (Some(timeline), Some(rows)) = (
        store.timeline(channel),
        tree.visible_rows(&timeline_id(channel)),
    ) else {
        return Vec::new();
    };
    let mut wanted = Vec::new();
    for message in timeline.messages().iter().take(rows.end).skip(rows.start) {
        if ui.editing == Some(message.id) {
            continue;
        }
        for source in invitation_links(message) {
            let cards = &mut ui.invitation_cards;
            if cards.links.len() >= LIMIT && !cards.links.contains_key(source) {
                cards.links.clear();
                cards.entries.retain(|_, entry| entry.in_flight(now));
            }
            let Some(target) = cards
                .links
                .entry(source.to_owned())
                .or_insert_with(|| recognize(source))
                .clone()
            else {
                continue;
            };
            if cards
                .entries
                .get(&target.url)
                .is_some_and(|entry| entry.settled(now))
            {
                continue;
            }
            if cards.entries.values().filter(|e| e.in_flight(now)).count() >= IN_FLIGHT {
                return wanted;
            }
            let request = ui.fresh_request();
            ui.invitation_cards
                .entries
                .entry(target.url.clone())
                .or_default()
                .pending = Some((request, now));
            wanted.push((request, target));
        }
    }
    wanted
}

/// Keep the answer of the latest lookup of its invitation for this account;
/// any other is stale and ignored.
pub fn invitation_card_result(ui: &mut Ui, store: &Store, result: InvitationResult, now: Instant) {
    let cards = &mut ui.invitation_cards;
    if cards.account != store.me() {
        return;
    }
    if let Some(entry) = cards.entries.get_mut(&result.target.url).filter(|entry| {
        entry
            .pending
            .is_some_and(|(request, _)| request == result.request)
    }) {
        entry.pending = None;
        entry.answer = Some((now, result.result));
    }
}

fn preview<'a>(ui: &'a Ui, store: &Store, source: &str) -> Option<&'a InvitationPreview> {
    let cards = &ui.invitation_cards;
    let target = cards.links.get(source)?.as_ref()?;
    match &cards.entries.get(&target.url)?.answer {
        Some((_, Ok(preview))) if cards.account == store.me() => Some(preview),
        _ => None,
    }
}

/// Whether the connection can accept invitations to `destination` from RUNE.
fn joins_here(store: &Store, destination: InvitationDestination) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|provider| provider.capabilities.accepts(destination))
}

/// What the cards of `message` show, one per invitation link; none while the
/// account may not look invitations up, which leaves the plain cards that
/// offer to review them.
pub(crate) fn cards(ui: &Ui, store: &Store, message: &Message) -> Vec<InvitationCard> {
    if !store.invitation_preview().allowed() {
        return Vec::new();
    }
    invitation_links(message)
        .into_iter()
        .map(|source| card(ui, store, source))
        .collect()
}

fn card(ui: &Ui, store: &Store, source: &str) -> InvitationCard {
    let cards = &ui.invitation_cards;
    if cards.account != store.me() {
        return InvitationCard::Loading;
    }
    let target = match cards.links.get(source) {
        None => return InvitationCard::Loading,
        Some(None) => return InvitationCard::Unavailable(InvitationError::Invalid.label().into()),
        Some(Some(target)) => target,
    };
    match cards
        .entries
        .get(&target.url)
        .and_then(|entry| entry.answer.as_ref())
    {
        None => InvitationCard::Loading,
        Some((_, Ok(preview))) => InvitationCard::Server(InvitedServer {
            kind: match preview.destination {
                InvitationDestination::Server(_) => InvitedKind::Server,
                InvitationDestination::Guest { .. } => InvitedKind::Guest,
                InvitationDestination::Group(_) => InvitedKind::Group,
                InvitationDestination::Friend(_) => InvitedKind::Friend,
            },
            name: preview.name.clone(),
            image: ui.image(&preview.image),
            online: preview.online,
            members: preview.members,
            member: store.invitation_joined(preview.destination),
            joins_here: joins_here(store, preview.destination),
            // Scheduled events (eventos-programados).
            event: preview
                .event
                .as_ref()
                .map(|event| crate::events::invited(store, event)),
        }),
        Some((_, Err(error))) => InvitationCard::Unavailable(error.label().into()),
    }
}

/// The pictures of where the visible cards of `message` lead.
pub(crate) fn images<'a>(
    ui: &'a Ui,
    store: &Store,
    message: &Message,
) -> Vec<&'a rune_model::RemoteImage> {
    invitation_links(message)
        .into_iter()
        .filter_map(|source| preview(ui, store, source)?.image.as_ref())
        .collect()
}

/// The actions of the `index`th card of `message`: «Revisar invitación»
/// and «Unirse» open the invitation in RUNE (or, for a kind the connection
/// cannot accept, «Unirse» opens its page in the browser); «Ir al servidor»,
/// «Ir al grupo» and «Enviar mensaje» open what the account already has.
/// None of them ever accepts the invitation by itself.
pub(crate) fn act(
    ui: &mut Ui,
    store: &mut Store,
    channel: ChannelId,
    message: MessageId,
    index: usize,
    what: &str,
    now: Instant,
) -> Vec<Intent> {
    let Some(found) = store
        .timeline(channel)
        .and_then(|timeline| timeline.get(message))
        .cloned()
    else {
        return Vec::new();
    };
    let Some(source) = invitation_links(&found)
        .get(index)
        .map(|source| (*source).to_owned())
    else {
        return Vec::new();
    };
    if ui.editing == Some(message) || !store.invitation_preview().allowed() {
        return Vec::new();
    }
    let review = |ui: &Ui, store: &Store| match store.me() {
        Some(account)
            if ui.invitation.is_none()
                && ui.invitation_input.is_none()
                && store.channel(channel).is_some_and(Store::visible) =>
        {
            vec![Intent::ReviewInvitation {
                channel,
                message,
                account,
                source: source.clone(),
            }]
        }
        _ => Vec::new(),
    };
    if what == "review" {
        return review(ui, store);
    }
    let Some(destination) = preview(ui, store, &source).map(|preview| preview.destination) else {
        return Vec::new();
    };
    let member = store.invitation_joined(destination);
    match what {
        "join" if !member && joins_here(store, destination) => review(ui, store),
        "join" if !member => ui
            .invitation_cards
            .links
            .get(&source)
            .cloned()
            .flatten()
            .map(|target| vec![Intent::OpenInBrowser(target.url)])
            .unwrap_or_default(),
        "open" if member => crate::invitations::open_destination(ui, store, destination, now),
        // Scheduled events (eventos-programados): the event's detail.
        "event" if member => {
            let event = preview(ui, store, &source)
                .and_then(|preview| preview.event.as_ref())
                .map(|event| (event.server, event.id));
            crate::events::open_invited(ui, store, event)
        }
        _ => Vec::new(),
    }
}
