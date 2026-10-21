//! A server's scheduled events (eventos-programados): the entry over its
//! channels, which reads the list once a session, and the dialog with the
//! list, an event's detail, the form that plans or edits one and the
//! question before deleting it. Nothing changes before the provider
//! answers; a verification a write meets is the common one. Every action
//! starts with `ev:`.

use std::collections::HashSet;

use limn::{DatePicker, Element, Key, Modal, TimePicker, UiAction, UiEvent};
use rune_components::events::{
    self as parts, ids, EventCard, EventsPage, EventsView, FormView, InvitedEvent,
};
use rune_components::Look;
use rune_i18n::t;
use rune_model::{
    ChannelId, ChannelKind, EventDraft, EventPlace, EventStatus, RemoteImage, ScheduledEvent,
    ScheduledEventId, ServerId, Timestamp, UserId,
};
use rune_provider::{Command, Failure, ProviderKind, RequestId};
use rune_state::{Applied, Store};

use crate::{Intent, Ui, ViewContext};

/// What the interface keeps of events.
#[derive(Debug, Default)]
pub(crate) struct Events {
    dialog: Option<Dialog>,
    /// The servers whose list was asked for this session, so the entry
    /// asks once.
    asked: HashSet<ServerId>,
}

/// The dialog while it is open.
#[derive(Debug)]
struct Dialog {
    server: ServerId,
    account: Option<UserId>,
    page: Page,
    /// The list's read in flight.
    loading: Option<RequestId>,
    /// Who the event shown interests, being read.
    people: Option<RequestId>,
    /// A change in flight.
    busy: Option<RequestId>,
    problem: Option<String>,
    /// Why the form cannot go yet, once it was tried.
    invalid: Option<&'static str>,
}

#[derive(Debug)]
enum Page {
    List,
    Detail(ScheduledEventId),
    Form(Box<Form>),
    Delete(ScheduledEventId),
}

/// The form as it is being written.
#[derive(Debug)]
struct Form {
    editing: Option<ScheduledEventId>,
    channel: Option<ChannelId>,
    location: String,
    name: String,
    description: String,
    start_date: String,
    start_time: String,
    start_month: (i32, u32),
    has_end: bool,
    end_date: String,
    end_time: String,
    end_month: (i32, u32),
    /// Whether its start may still change: not once it started.
    starts: bool,
}

const HOUR: i64 = 60 * 60 * 1000;

fn wall_now() -> Timestamp {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    Timestamp(elapsed.as_millis() as i64)
}

/// The stage and voice channels of `server` an event may happen in.
fn places(store: &Store, server: ServerId) -> Vec<(ChannelId, String, bool)> {
    store
        .channel_tree(server)
        .into_iter()
        .flat_map(|(_, channels)| channels)
        .filter(|channel| matches!(channel.kind, ChannelKind::Voice | ChannelKind::Stage))
        .map(|channel| {
            (
                channel.id,
                channel.name.clone(),
                channel.kind == ChannelKind::Stage,
            )
        })
        .collect()
}

impl Form {
    /// A new event: in the first voice channel, an hour after the next
    /// whole hour, an hour long.
    fn new(store: &Store, server: ServerId) -> Self {
        let start = Timestamp((wall_now().0 / HOUR + 2) * HOUR);
        Self::from_draft(
            None,
            &EventDraft {
                name: String::new(),
                description: None,
                start,
                end: None,
                place: match places(store, server).first() {
                    Some((channel, _, true)) => EventPlace::Stage(*channel),
                    Some((channel, _, false)) => EventPlace::Voice(*channel),
                    None => EventPlace::External(String::new()),
                },
            },
            true,
        )
    }

    fn of(event: &ScheduledEvent) -> Self {
        Self::from_draft(
            Some(event.id),
            &EventDraft::of(event),
            event.status == EventStatus::Scheduled,
        )
    }

    fn from_draft(editing: Option<ScheduledEventId>, draft: &EventDraft, starts: bool) -> Self {
        let (start_date, start_time) = parts::picker_values(draft.start);
        let (end_date, end_time) =
            parts::picker_values(draft.end.unwrap_or(Timestamp(draft.start.0 + HOUR)));
        Self {
            editing,
            channel: draft.place.channel(),
            location: match &draft.place {
                EventPlace::External(location) => location.clone(),
                _ => String::new(),
            },
            name: draft.name.clone(),
            description: draft.description.clone().unwrap_or_default(),
            start_month: rune_components::time::picker_month(&start_date),
            end_month: rune_components::time::picker_month(&end_date),
            start_date,
            start_time,
            has_end: draft.end.is_some(),
            end_date,
            end_time,
            starts,
        }
    }

    /// What it says, as a draft; or why it does not say it whole yet.
    fn draft(&self, store: &Store, server: ServerId) -> Result<EventDraft, &'static str> {
        let start = parts::picker_instant(&self.start_date, &self.start_time)
            .ok_or(t("Escribí la fecha y la hora de inicio enteras."))?;
        let place = match self.channel {
            None => EventPlace::External(self.location.trim().to_owned()),
            Some(channel) => match places(store, server)
                .into_iter()
                .find(|(id, _, _)| *id == channel)
            {
                Some((_, _, true)) => EventPlace::Stage(channel),
                Some((_, _, false)) => EventPlace::Voice(channel),
                None => return Err(t("Ese canal ya no está en el servidor.")),
            },
        };
        let end = if self.has_end || self.channel.is_none() {
            Some(
                parts::picker_instant(&self.end_date, &self.end_time)
                    .ok_or(t("Escribí la fecha y la hora de fin enteras."))?,
            )
        } else {
            None
        };
        let description = self.description.trim();
        Ok(EventDraft {
            name: self.name.trim().to_owned(),
            description: (!description.is_empty()).then(|| description.to_owned()),
            start,
            end,
            place,
        })
    }
}

/// Whether the dialog covers the window.
pub(crate) fn visible(ui: &Ui) -> bool {
    ui.events.dialog.is_some()
}

/// Ask for the list of the server the column shows, once a session, so
/// its entry says how many are coming. Not while what is shown is the
/// local store's: the session's Ready forgets the lists and would ask
/// again, in the burst of reads that follows it.
pub(crate) fn after_layout(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let Some(server) = ui.server else {
        return Vec::new();
    };
    if store.from_cache
        || !store.event_abilities(server).view.allowed()
        || store.scheduled_events(server).is_some()
        || !ui.events.asked.insert(server)
    {
        return Vec::new();
    }
    let request = ui.fresh_request();
    vec![Intent::Command(Command::LoadScheduledEvents {
        request,
        server,
    })]
}

/// The entry over `server`'s channels, while the account may see its events.
pub(crate) fn entry(store: &Store, server: ServerId, look: &Look) -> Option<Element> {
    store
        .event_abilities(server)
        .view
        .allowed()
        .then(|| parts::entry(store.upcoming_events(server), look))
}

/// Open the dialog for `server` on `page`, reading its list when the
/// store does not have it.
fn open(ui: &mut Ui, store: &Store, server: ServerId, page: Page) -> Vec<Intent> {
    if !store.event_abilities(server).view.allowed() {
        return Vec::new();
    }
    let mut dialog = Dialog {
        server,
        account: store.me(),
        page,
        loading: None,
        people: None,
        busy: None,
        problem: None,
        invalid: None,
    };
    let mut intents = Vec::new();
    if store.scheduled_events(server).is_none() {
        let request = ui.fresh_request();
        dialog.loading = Some(request);
        ui.events.asked.insert(server);
        intents.push(Intent::Command(Command::LoadScheduledEvents {
            request,
            server,
        }));
    }
    if let Page::Detail(event) = dialog.page {
        intents.extend(read_people(ui, store, &mut dialog, event));
    }
    ui.events.dialog = Some(dialog);
    intents
}

/// Open `event` of `server`.
fn open_event(
    ui: &mut Ui,
    store: &Store,
    server: ServerId,
    event: ScheduledEventId,
) -> Vec<Intent> {
    open(ui, store, server, Page::Detail(event))
}

/// Open the event an accepted or known invitation names, unless what the
/// server asks of a new member is open first.
pub(crate) fn open_invited(
    ui: &mut Ui,
    store: &Store,
    event: Option<(ServerId, ScheduledEventId)>,
) -> Vec<Intent> {
    match event {
        Some((server, event)) if ui.join.is_none() => open_event(ui, store, server, event),
        _ => Vec::new(),
    }
}

/// Ask who `event` interests, when the store does not know.
fn read_people(
    ui: &mut Ui,
    store: &Store,
    dialog: &mut Dialog,
    event: ScheduledEventId,
) -> Vec<Intent> {
    if store.event_interested(event).is_some() || dialog.people.is_some() {
        return Vec::new();
    }
    let request = ui.fresh_request();
    dialog.people = Some(request);
    vec![Intent::Command(Command::LoadEventInterested {
        request,
        server: dialog.server,
        event,
    })]
}

fn shown<'a>(
    store: &'a Store,
    dialog: &Dialog,
    event: ScheduledEventId,
) -> Option<&'a ScheduledEvent> {
    store.scheduled_event(dialog.server, event)
}

pub(crate) fn update(ui: &mut Ui, store: &Store, action: &UiAction) -> Option<Vec<Intent>> {
    let (id, value) = match action {
        UiAction::Clicked { id } => (id.as_str(), None),
        UiAction::InputChanged { id, value } => (id.as_str(), Some(value)),
        _ => return None,
    };
    let rest = id.strip_prefix("ev:")?;
    if rest == "open" {
        return Some(match ui.server {
            Some(server) => open(ui, store, server, Page::List),
            None => Vec::new(),
        });
    }
    let mut dialog = ui.events.dialog.take()?;
    let intents = act(ui, store, &mut dialog, rest, value);
    if !matches!(rest, "close") || dialog.busy.is_some() {
        ui.events.dialog = Some(dialog);
    }
    Some(intents)
}

/// What an action of the dialog does.
fn act(
    ui: &mut Ui,
    store: &Store,
    dialog: &mut Dialog,
    rest: &str,
    value: Option<&String>,
) -> Vec<Intent> {
    let parts: Vec<&str> = rest.split(':').collect();
    let event = |at: usize| {
        parts
            .get(at)
            .and_then(|id| id.parse().ok())
            .map(ScheduledEventId)
    };
    let server = dialog.server;
    let busy = dialog.busy.is_some();
    let mut intents = Vec::new();
    match (parts.as_slice(), value) {
        (["close"], _) => {}
        (["back"], _) if !busy => {
            dialog.invalid = None;
            dialog.problem = None;
            dialog.page = match &dialog.page {
                Page::Form(form) => form.editing.map_or(Page::List, Page::Detail),
                Page::Delete(event) => Page::Detail(*event),
                _ => Page::List,
            };
        }
        (["retry"], _) if dialog.loading.is_none() => {
            let request = ui.fresh_request();
            dialog.loading = Some(request);
            dialog.problem = None;
            intents.push(Intent::Command(Command::LoadScheduledEvents {
                request,
                server,
            }));
        }
        (["show", _], None) => {
            if let Some(event) = event(1) {
                dialog.page = Page::Detail(event);
                dialog.problem = None;
                intents.extend(read_people(ui, store, dialog, event));
            }
        }
        _ if busy => {}
        (["interest", _], None) => {
            if let Some(found) = event(1).and_then(|event| shown(store, dialog, event)) {
                let (event, interested) = (found.id, !found.account_interested);
                let request = ui.fresh_request();
                dialog.busy = Some(request);
                dialog.problem = None;
                intents.push(Intent::Command(Command::SetEventInterest {
                    request,
                    server,
                    event,
                    interested,
                }));
            }
        }
        (["new"], None) if store.event_abilities(server).create.allowed() => {
            dialog.invalid = None;
            dialog.page = Page::Form(Box::new(Form::new(store, server)));
        }
        (["edit", _], None) => {
            if let Some(found) = event(1)
                .and_then(|event| shown(store, dialog, event))
                .filter(|found| store.may_change_event(found).allowed())
            {
                dialog.invalid = None;
                dialog.page = Page::Form(Box::new(Form::of(found)));
            }
        }
        (["delete", _], None) => {
            if let Some(found) = event(1)
                .and_then(|event| shown(store, dialog, event))
                .filter(|found| store.may_change_event(found).allowed())
            {
                dialog.page = Page::Delete(found.id);
            }
        }
        (["delete-yes"], None) => {
            if let Page::Delete(event) = dialog.page {
                let request = ui.fresh_request();
                dialog.busy = Some(request);
                dialog.problem = None;
                intents.push(Intent::Command(Command::DeleteScheduledEvent {
                    request,
                    server,
                    event,
                }));
            }
        }
        ([verb @ ("start" | "finish"), _], None) => {
            if let Some(found) = event(1)
                .and_then(|event| shown(store, dialog, event))
                .filter(|found| store.may_change_event(found).allowed())
            {
                let status = if *verb == "start" {
                    EventStatus::Active
                } else {
                    EventStatus::Completed
                };
                let request = ui.fresh_request();
                dialog.busy = Some(request);
                dialog.problem = None;
                intents.push(Intent::Command(Command::SetScheduledEventStatus {
                    request,
                    server,
                    event: found.id,
                    status,
                }));
            }
        }
        (["save"], None) => intents.extend(save(ui, store, dialog)),
        _ => {
            if let Page::Form(form) = &mut dialog.page {
                intents.extend(edit_form(form, &parts, value));
            }
        }
    }
    intents
}

/// Plan or change the event as the form says, once it says it whole.
fn save(ui: &mut Ui, store: &Store, dialog: &mut Dialog) -> Vec<Intent> {
    let Page::Form(form) = &dialog.page else {
        return Vec::new();
    };
    let server = dialog.server;
    let draft = match form.draft(store, server).and_then(|draft| {
        match draft.problem(wall_now(), form.starts) {
            Some(problem) => Err(problem),
            None => Ok(draft),
        }
    }) {
        Ok(draft) => draft,
        Err(problem) => {
            dialog.invalid = Some(problem);
            return Vec::new();
        }
    };
    dialog.invalid = None;
    dialog.problem = None;
    let request = ui.fresh_request();
    dialog.busy = Some(request);
    vec![Intent::Command(match form.editing {
        None => Command::CreateScheduledEvent {
            request,
            server,
            draft,
        },
        Some(event) => Command::EditScheduledEvent {
            request,
            server,
            event,
            draft,
        },
    })]
}

/// What a field of the form reports.
fn edit_form(form: &mut Form, parts: &[&str], value: Option<&String>) -> Vec<Intent> {
    let date = |part: &str| match part {
        "start-date" => Some(true),
        "end-date" => Some(false),
        _ => None,
    };
    match (parts, value) {
        (["name"], Some(value)) => form.name = value.clone(),
        (["description"], Some(value)) => form.description = value.clone(),
        (["location"], Some(value)) => form.location = value.clone(),
        (["start-date"], Some(value)) => form.start_date = value.clone(),
        (["start-time"], Some(value)) => form.start_time = value.clone(),
        (["end-date"], Some(value)) => form.end_date = value.clone(),
        (["end-time"], Some(value)) => form.end_time = value.clone(),
        (["has-end"], None) => form.has_end = !form.has_end,
        (["where", "elsewhere"], None) => form.channel = None,
        (["where", channel], None) => {
            if let Ok(channel) = channel.parse() {
                form.channel = Some(ChannelId(channel));
            }
        }
        ([field, "day", day], None) if date(field).is_some() => {
            let start = date(field) == Some(true);
            let (id, slot) = if start {
                (ids::START_DATE, &mut form.start_date)
            } else {
                (ids::END_DATE, &mut form.end_date)
            };
            if DatePicker::is_valid_date(day) {
                *slot = (*day).to_owned();
                return vec![Intent::SetInput {
                    id: id.into(),
                    value: (*day).to_owned(),
                }];
            }
        }
        ([field, arrow @ ("month_prev" | "month_next")], None) if date(field).is_some() => {
            let month = if date(field) == Some(true) {
                &mut form.start_month
            } else {
                &mut form.end_month
            };
            let delta = if *arrow == "month_prev" { -1 } else { 1 };
            *month = DatePicker::shift_month(month.0, month.1, delta);
        }
        ([field, _], None) if matches!(*field, "start-time" | "end-time") => {
            let id = format!("ev:{}", parts.join(":"));
            let (time_id, slot) = if *field == "start-time" {
                (ids::START_TIME, &mut form.start_time)
            } else {
                (ids::END_TIME, &mut form.end_time)
            };
            if let Some(time) = TimePicker::adjusted_value(&id, time_id, slot) {
                *slot = time.clone();
                return vec![Intent::SetInput {
                    id: time_id.into(),
                    value: time,
                }];
            }
        }
        _ => {}
    }
    Vec::new()
}

/// Esc goes back a page, or closes the list; never while a change is on
/// its way. A verification a change meets is over it and takes the key.
pub(crate) fn intercept(ui: &mut Ui, event: &UiEvent) -> Option<Vec<Intent>> {
    let dialog = ui
        .events
        .dialog
        .as_mut()
        .filter(|_| ui.challenge.is_none())?;
    if !matches!(
        event,
        UiEvent::Key {
            key: Key::Escape,
            ..
        }
    ) {
        return None;
    }
    if dialog.busy.is_none() {
        match &dialog.page {
            Page::List => ui.events.dialog = None,
            Page::Form(form) => dialog.page = form.editing.map_or(Page::List, Page::Detail),
            Page::Delete(event) => dialog.page = Page::Detail(*event),
            Page::Detail(_) => dialog.page = Page::List,
        }
    }
    Some(Vec::new())
}

/// Whether a failure is the events' own to show, not a notice's: the
/// dialog says its own beside what failed, and the list the entry reads in
/// the background only leaves the entry without its count (the dialog
/// reads it again when opened, and a new session asks again).
pub(crate) fn shows_failure(ui: &Ui, failure: &Failure) -> bool {
    match failure {
        Failure::ScheduledEvents { .. } => true,
        Failure::ScheduledEvent { request, .. } | Failure::EventInterest { request, .. } => {
            ui.events.dialog.as_ref().is_some_and(|dialog| {
                [dialog.loading, dialog.people, dialog.busy].contains(&Some(*request))
            })
        }
        _ => false,
    }
}

/// Follow what the provider answered.
pub(crate) fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied) -> Vec<Intent> {
    if matches!(applied, Applied::Ready | Applied::Authentication) {
        ui.events.asked.clear();
    }
    let Some(dialog) = ui.events.dialog.as_mut() else {
        return Vec::new();
    };
    if dialog.account != store.me() || store.server(dialog.server).is_none() {
        ui.events.dialog = None;
        return Vec::new();
    }
    match applied {
        Applied::ScheduledEvents {
            request: Some(request),
            event,
            ..
        } => {
            if dialog.loading == Some(*request) {
                dialog.loading = None;
            }
            if dialog.people == Some(*request) {
                dialog.people = None;
            }
            if dialog.busy == Some(*request) {
                dialog.busy = None;
                dialog.page = match (&dialog.page, event) {
                    // Deleted: back to the list.
                    (Page::Delete(_), _) => Page::List,
                    // Planned or changed: its detail.
                    (Page::Form(_), Some(event)) => Page::Detail(*event),
                    (Page::Detail(shown), _) => Page::Detail(*shown),
                    _ => Page::List,
                };
            }
        }
        Applied::Failed {
            failure:
                Failure::ScheduledEvents { request, .. }
                | Failure::ScheduledEvent { request, .. }
                | Failure::EventInterest { request, .. },
            reason,
            ..
        } if [dialog.loading, dialog.people, dialog.busy].contains(&Some(*request)) => {
            for slot in [&mut dialog.loading, &mut dialog.people, &mut dialog.busy] {
                if *slot == Some(*request) {
                    *slot = None;
                }
            }
            dialog.problem = Some(reason.clone());
        }
        // A new session forgot the lists: read again what is shown, and
        // a change on its way says it did not arrive.
        Applied::Ready => {
            if dialog.busy.take().is_some() {
                dialog.problem = Some(t("Se perdió la conexión: volvé a intentarlo.").into());
            }
            dialog.people = None;
            let request = ui.fresh_request();
            let dialog = ui.events.dialog.as_mut().expect("still open");
            dialog.loading = Some(request);
            ui.events.asked.insert(dialog.server);
            return vec![Intent::Command(Command::LoadScheduledEvents {
                request,
                server: dialog.server,
            })];
        }
        _ => {}
    }
    Vec::new()
}

/// Where `event` is, as a card says it.
fn place(store: &Store, event: &ScheduledEvent) -> String {
    match &event.place {
        EventPlace::External(location) => location.clone(),
        place => place
            .channel()
            .and_then(|channel| store.channel(channel))
            .map_or_else(
                || t("Un canal que ya no está").into(),
                |channel| channel.name.clone(),
            ),
    }
}

fn card<'a>(ui: &Ui, store: &Store, event: &'a ScheduledEvent) -> EventCard<'a> {
    EventCard {
        event,
        place: place(store, event),
        image: ui.image(&event.image),
        interest_denied: store
            .event_abilities(event.server)
            .view
            .reason()
            .map(str::to_owned),
    }
}

/// The dialog over `root`, while it is open.
pub(crate) fn overlay(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let Some(dialog) = ui
        .events
        .dialog
        .as_ref()
        .filter(|dialog| dialog.account == store.me())
    else {
        return root;
    };
    let Some(server) = store.server(dialog.server) else {
        return root;
    };
    let events = store.scheduled_events(dialog.server).unwrap_or_default();
    let list = || EventsPage::List {
        cards: events.iter().map(|event| card(ui, store, event)).collect(),
        create_denied: store
            .event_abilities(dialog.server)
            .create
            .reason()
            .map(str::to_owned),
    };
    let channels;
    let page = match &dialog.page {
        Page::List => list(),
        Page::Detail(id) => match events.iter().find(|event| event.id == *id) {
            Some(event) => EventsPage::Detail {
                card: card(ui, store, event),
                creator: event
                    .creator
                    .map(|creator| store.display_name(creator, Some(dialog.server))),
                interested: store.event_interested(event.id).map(|people| {
                    people
                        .iter()
                        .map(|person| store.display_name(*person, Some(dialog.server)))
                        .collect()
                }),
                change_denied: store.may_change_event(event).reason().map(str::to_owned),
            },
            // Gone, or not read yet: the list, saying so once read.
            None => list(),
        },
        Page::Form(form) => {
            channels = places(store, dialog.server);
            EventsPage::Form(FormView {
                editing: form.editing,
                channels: &channels,
                channel: form.channel,
                location: &form.location,
                name: &form.name,
                description: &form.description,
                start_date: &form.start_date,
                start_time: &form.start_time,
                start_month: form.start_month,
                has_end: form.has_end,
                end_date: &form.end_date,
                end_time: &form.end_time,
                end_month: form.end_month,
                starts: form.starts,
            })
        }
        Page::Delete(id) => EventsPage::Delete(
            events
                .iter()
                .find(|event| event.id == *id)
                .map_or("", |event| event.name.as_str()),
        ),
    };
    let gone = matches!(&dialog.page, Page::Detail(id) if !events.iter().any(|event| event.id == *id))
        && dialog.loading.is_none();
    let simulated = store
        .provider
        .as_ref()
        .is_some_and(|info| info.kind == ProviderKind::Simulated);
    parts::events_dialog(
        root,
        &EventsView {
            server: server.shown_name(),
            page,
            loading: dialog.loading.is_some(),
            busy: dialog.busy.is_some(),
            problem: dialog
                .problem
                .as_deref()
                .or(gone.then(|| t("Ese evento ya no está disponible."))),
            invalid: dialog.invalid,
            note: simulated.then(|| t("Simulación local: no se envía nada a ningún servicio.")),
        },
        Modal::fit_width(context.window.width, 520.0),
        look,
    )
}

/// The cover of the event shown, for the host to fetch.
pub(crate) fn wanted<'a>(ui: &Ui, store: &'a Store, add: &mut impl FnMut(Option<&'a RemoteImage>)) {
    if let Some(dialog) = &ui.events.dialog {
        if let Page::Detail(event) = dialog.page {
            add(store
                .scheduled_event(dialog.server, event)
                .and_then(|event| event.image.as_ref()));
        }
    }
}

/// An invitation's event as its card shows it.
pub(crate) fn invited(store: &Store, event: &ScheduledEvent) -> InvitedEvent {
    InvitedEvent {
        name: event.name.clone(),
        when: parts::when(event),
        // Its server's channels are not known before joining it.
        place: match &event.place {
            EventPlace::External(location) => location.clone(),
            EventPlace::Stage(channel) => store.channel(*channel).map_or_else(
                || t("Un escenario del servidor").into(),
                |found| found.name.clone(),
            ),
            EventPlace::Voice(channel) => store.channel(*channel).map_or_else(
                || t("Un canal de voz del servidor").into(),
                |found| found.name.clone(),
            ),
        },
    }
}
