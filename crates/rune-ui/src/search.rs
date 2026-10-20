//! Loaded messages, bounded, request-scoped searches of this account's
//! cache and, when the provider offers it, its whole history with filters.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use limn::{DatePicker, Key, UiAction};
use rune_components::time;
use rune_i18n::t;
use rune_model::{
    ChannelId, Has, HistorySource, LocalReadError, Message, MessageId, RemoteImage, SearchCursor,
    SearchHit, SearchPage, SearchQuery, ServerId, UserId,
};
use rune_provider::{Command, Failure};
use rune_state::{Applied, Store};

use crate::{editor_id, Intent, Ui};

mod filters;
mod remote;
mod view;
pub(crate) use remote::{next_tick, tick};
pub(crate) use view::{view, wanted};

const QUERY: &str = "message-search:query";
const RESULTS: &str = "message-search:results";
const HELD_PAGES: usize = 5;

#[derive(Debug)]
struct Hit {
    data: SearchHit,
    source: Source,
    media: Media,
}

/// What a result shows of what it carries: a page kept on this device has
/// only its text, so none.
#[derive(Debug, Default)]
struct Media {
    pictures: Vec<RemoteImage>,
    files: Vec<String>,
}

impl Media {
    /// A spoiler or an explicit picture is named, never shown.
    fn of(message: &Message) -> Self {
        let mut media = Self::default();
        for attachment in &message.attachments {
            match &attachment.preview {
                Some(preview)
                    if !attachment.spoiler && !attachment.explicit && media.pictures.len() < 4 =>
                {
                    media.pictures.push(preview.clone())
                }
                _ if media.files.len() < 3 => media.files.push(attachment.filename.clone()),
                _ => {}
            }
        }
        media
    }
}

fn hit(message: &Message, source: Source) -> Hit {
    Hit {
        data: SearchHit::from_message(message),
        source,
        media: Media::of(message),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Scope {
    Loaded,
    Device,
    /// The provider's whole history (P2).
    Remote,
}

/// Where a result came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Source {
    Loaded,
    Device,
    Remote,
    /// The provider found it, and it is loaded too.
    Both,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Menu {
    Scope,
    Author,
    From,
    Until,
    // Reactions, pins and search (2026-10-18).
    Order,
    // Search design (busqueda-diseno).
    Has,
}

#[derive(Debug)]
enum Phase {
    Loading,
    Ready,
    Dirty,
    Failed(LocalReadError),
}

#[derive(Debug)]
struct CachedSearch {
    request: u64,
    query: SearchQuery,
    phase: Phase,
    pages: VecDeque<SearchPage>,
    page: usize,
    skipped: usize,
}

/// Cached searches retain at most five pages of lightweight hits.
#[derive(Debug)]
pub struct MessageSearch {
    channel: ChannelId,
    account: Option<UserId>,
    query: String,
    author: Option<UserId>,
    author_query: String,
    author_options: Vec<UserId>,
    files: bool,
    pinned: bool,
    from: String,
    until: String,
    month: Option<(i32, u32)>,
    menu: Option<Menu>,
    scope: Scope,
    selected: usize,
    source: Arc<Vec<Arc<Message>>>,
    hits: Arc<Vec<Hit>>,
    cached: Option<CachedSearch>,
    form_error: Option<&'static str>,
    // Reads, search and remote members (P2).
    remote: Option<remote::RemoteRead>,
    /// The filter suggestion chosen, while some are offered.
    suggestion: usize,
    suggestions_closed: bool,
    /// The last person looked up at the provider for a filter.
    lookup: Option<(ServerId, String)>,
    // Reactions, pins and search (2026-10-18).
    /// How the provider orders the whole history's results.
    order: rune_model::RemoteOrder,
    // Search design (busqueda-diseno).
    /// What the messages carry besides files, which `files` says: loaded
    /// messages and the whole history only.
    has: Vec<Has>,
}

impl MessageSearch {
    fn model_query(&self) -> Result<SearchQuery, LocalReadError> {
        let bound = |text: &str, next| {
            if text.is_empty() {
                Ok(None)
            } else {
                time::local_date_boundary(text, next)
                    .map(Some)
                    .ok_or(LocalReadError::InvalidQuery)
            }
        };
        Ok(SearchQuery {
            channel: self.channel,
            text: self.query.clone(),
            author: self.author,
            files: self.files,
            pinned: self.pinned,
            from: bound(&self.from, false)?,
            until: bound(&self.until, true)?,
        })
    }
    /// Whether nothing is written and no filter is chosen.
    fn idle(&self) -> bool {
        self.query.trim().is_empty()
            && self.author.is_none()
            && !self.files
            && !self.pinned
            && self.has.is_empty()
            && self.from.is_empty()
            && self.until.is_empty()
    }
    fn actionable(&self) -> bool {
        self.scope == Scope::Loaded
            || self.scope == Scope::Remote && !self.hits.is_empty()
            || self
                .cached
                .as_ref()
                .is_some_and(|cached| matches!(cached.phase, Phase::Ready))
    }
}

fn authors(store: &Store, text: &str, selected: Option<UserId>) -> Vec<UserId> {
    let needle = text.trim().to_lowercase();
    let mut ranked = Vec::new();
    for user in store.users() {
        let name = user.name.to_lowercase();
        let display = user.display_name.as_ref().map(|name| name.to_lowercase());
        let rank = if name == needle {
            0
        } else if display.as_deref() == Some(needle.as_str()) {
            1
        } else if name.contains(&needle)
            || display.as_ref().is_some_and(|name| name.contains(&needle))
        {
            2
        } else {
            continue;
        };
        let candidate = (rank, name, user.id);
        let index = ranked
            .binary_search(&candidate)
            .unwrap_or_else(|index| index);
        if index < 64 {
            ranked.insert(index, candidate);
            ranked.truncate(64);
        }
    }
    let mut found: Vec<_> = ranked.into_iter().map(|(_, _, id)| id).collect();
    if let Some(selected) =
        selected.filter(|selected| store.user(*selected).is_some() && !found.contains(selected))
    {
        if found.len() == 64 {
            found.pop();
        }
        found.push(selected);
    }
    found
}

fn readable(search: &MessageSearch, ui: &Ui, store: &Store) -> bool {
    ui.channel() == Some(search.channel)
        && search.account == store.me()
        && store
            .abilities(search.channel)
            .is_some_and(|abilities| abilities.read_history.allowed())
}

/// A page kept on this device, as results.
fn kept(page: &[SearchHit]) -> Arc<Vec<Hit>> {
    Arc::new(
        page.iter()
            .map(|data| Hit {
                data: data.clone(),
                source: Source::Device,
                media: Media::default(),
            })
            .collect(),
    )
}

/// The filter suggestions for what the field says now, in the whole
/// history only.
fn suggestions(search: &MessageSearch, store: &Store) -> Vec<filters::Suggestion> {
    if search.scope != Scope::Remote || search.suggestions_closed {
        return Vec::new();
    }
    let mut found = filters::suggestions(
        store,
        search.channel,
        &search.query,
        crate::server_menu::wall_now(),
    );
    // The filters' keys fit, nine of them; people and channels stop at six.
    found.truncate(9);
    found
}

/// Put suggestion `index` in place of the token being written.
fn suggest(ui: &mut Ui, store: &Store, index: usize) -> Vec<Intent> {
    let Some(search) = ui.search.as_mut() else {
        return Vec::new();
    };
    let Some(chosen) = suggestions(search, store).into_iter().nth(index) else {
        return Vec::new();
    };
    let start = filters::active(&search.query).map_or(search.query.len(), |(start, _)| start);
    let spacer = if chosen.insert.ends_with(':') {
        ""
    } else {
        " "
    };
    search.query = format!("{}{}{spacer}", &search.query[..start], chosen.insert);
    search.suggestion = 0;
    remote::refresh(search, store);
    let value = search.query.clone();
    let mut intents = vec![
        Intent::SetInput {
            id: QUERY.into(),
            value: value.clone(),
        },
        Intent::FocusInput {
            id: QUERY.into(),
            cursor: value.chars().count(),
        },
    ];
    intents.extend(remote::lookup(ui, store));
    intents
}

pub(crate) fn refresh(ui: &mut Ui, store: &Store, force: bool) {
    if ui
        .search
        .as_ref()
        .is_some_and(|search| !readable(search, ui, store))
    {
        return;
    }
    let Some(search) = &mut ui.search else {
        return;
    };
    if search.scope == Scope::Remote {
        remote::refresh(search, store);
        return;
    }
    if search.scope != Scope::Loaded {
        return;
    }
    let source = store
        .timeline(search.channel)
        .map(|timeline| timeline.messages())
        .unwrap_or_default();
    if !force && Arc::ptr_eq(&source, &search.source) {
        return;
    }
    let selected = search.hits.get(search.selected).map(|hit| hit.data.id);
    let found = match search.model_query() {
        Ok(query) if query.is_empty() && search.has.is_empty() => {
            search.form_error = None;
            Vec::new()
        }
        // What the messages carry may be the only filter.
        Ok(query) if query.is_empty() || query.validate().is_ok() => {
            search.form_error = None;
            let matches = query.matcher();
            source
                .iter()
                .rev()
                .filter(|message| {
                    matches(message) && search.has.iter().all(|has| has.found(message))
                })
                .map(|message| hit(message, Source::Loaded))
                .collect()
        }
        _ => {
            search.form_error = Some(LocalReadError::InvalidQuery.label());
            Vec::new()
        }
    };
    search.selected = selected
        .and_then(|id| found.iter().position(|hit| hit.data.id == id))
        .unwrap_or(0);
    search.source = source;
    search.hits = Arc::new(found);
}

pub(crate) fn dismiss(ui: &mut Ui) -> Vec<Intent> {
    ui.search
        .take()
        .and_then(|search| search.cached)
        .map(|cached| vec![Intent::Command(Command::Cancel(cached.request))])
        .unwrap_or_default()
}

fn close(ui: &mut Ui) -> Vec<Intent> {
    if ui.search.is_none() {
        return Vec::new();
    }
    let mut intents = dismiss(ui);
    intents.extend(crate::context::cancel(ui));
    if let Some(channel) = ui.channel() {
        intents.push(Intent::Focus(editor_id(channel)));
    }
    intents
}

fn start(ui: &mut Ui, store: &Store, cursor: Option<SearchCursor>, reset: bool) -> Vec<Intent> {
    if ui
        .search
        .as_ref()
        .is_some_and(|search| !readable(search, ui, store))
    {
        return close(ui);
    }
    let mut intents = if reset {
        crate::context::cancel(ui)
    } else {
        Vec::new()
    };
    let Some(search) = &mut ui.search else {
        return intents;
    };
    if search.scope == Scope::Loaded {
        refresh(ui, store, true);
        return intents;
    }
    if search.scope == Scope::Remote {
        intents.extend(remote::start(ui, store, 0, false));
        return intents;
    }
    if let Some(cached) = &search.cached {
        // A completed request owns its continuation generation.
        if reset || matches!(cached.phase, Phase::Loading) {
            intents.push(Intent::Command(Command::Cancel(cached.request)));
        }
    }
    let query = search.model_query();
    let error = if !ui.local_history_available {
        Some(LocalReadError::Unavailable)
    } else {
        query.as_ref().err().copied().or_else(|| {
            query
                .as_ref()
                .ok()
                .filter(|query| !query.is_empty())
                .and_then(|query| query.validate().err())
        })
    };
    if error.is_some() || query.as_ref().is_ok_and(|query| query.is_empty()) {
        search.cached = None;
        search.hits = Arc::default();
        search.form_error = error.map(LocalReadError::label);
        return intents;
    }
    let query = query.unwrap();
    let request = ui.fresh_request();
    let search = ui.search.as_mut().unwrap();
    search.form_error = None;
    search.selected = 0;
    search.hits = Arc::default();
    match search.cached.as_mut() {
        Some(cached) if !reset => {
            cached.request = request;
            cached.phase = Phase::Loading;
        }
        _ => {
            search.cached = Some(CachedSearch {
                request,
                query: query.clone(),
                phase: Phase::Loading,
                pages: VecDeque::new(),
                page: 0,
                skipped: 0,
            });
        }
    }
    intents.push(Intent::SearchLocal {
        request,
        query,
        cursor,
    });
    intents
}

pub fn local_history_changed(ui: &mut Ui, _store: &Store, available: bool) -> Vec<Intent> {
    ui.local_history_available = available;
    let mut intents = if available {
        Vec::new()
    } else {
        crate::context::local_unavailable(ui)
    };
    if !available
        && ui
            .search
            .as_ref()
            .is_some_and(|search| search.scope == Scope::Device)
    {
        let search = ui.search.as_mut().unwrap();
        intents.extend(
            search
                .cached
                .take()
                .map(|cached| vec![Intent::Command(Command::Cancel(cached.request))])
                .unwrap_or_default(),
        );
        search.hits = Arc::default();
        search.form_error = Some(LocalReadError::Unavailable.label());
        return intents;
    }
    intents
}

/// A mutation invalidates cached rows/cursors without starting a query loop.
pub fn local_messages_changed(ui: &mut Ui, channel: ChannelId) -> Vec<Intent> {
    let Some(search) = ui
        .search
        .as_mut()
        .filter(|search| search.channel == channel && search.scope == Scope::Device)
    else {
        return Vec::new();
    };
    search.hits = Arc::default();
    let Some(cached) = &mut search.cached else {
        return Vec::new();
    };
    let intents = if matches!(cached.phase, Phase::Loading) {
        vec![Intent::Command(Command::Cancel(cached.request))]
    } else {
        Vec::new()
    };
    cached.phase = Phase::Dirty;
    cached.pages.clear();
    cached.page = 0;
    intents
}

pub fn local_search_result(
    ui: &mut Ui,
    store: &Store,
    request: u64,
    channel: ChannelId,
    result: Result<SearchPage, LocalReadError>,
    _now: Instant,
) -> Vec<Intent> {
    let Some(search) = ui.search.as_mut().filter(|search| {
        search.channel == channel && search.account == store.me() && search.scope == Scope::Device
    }) else {
        return Vec::new();
    };
    if !ui.local_history_available
        || !store
            .abilities(channel)
            .is_some_and(|abilities| abilities.read_history.allowed())
    {
        return dismiss(ui);
    }
    let current_query = search.model_query().ok();
    let Some(cached) = search.cached.as_mut().filter(|cached| {
        cached.request == request
            && current_query.as_ref() == Some(&cached.query)
            && matches!(cached.phase, Phase::Loading)
    }) else {
        return Vec::new();
    };
    let result = result.and_then(|page| {
        if page.hits.len() > 20
            || page.hits.iter().any(|hit| {
                hit.channel != channel || hit.id.0 == 0 || hit.snippet.chars().count() > 160
            })
            || page.hits.windows(2).any(|pair| pair[0].id <= pair[1].id)
        {
            Err(LocalReadError::Failed)
        } else {
            Ok(page)
        }
    });
    match result {
        Ok(page) => {
            search.hits = kept(&page.hits);
            cached.pages.push_back(page);
            if cached.pages.len() > HELD_PAGES {
                cached.pages.pop_front();
                cached.skipped += 1;
            }
            cached.page = cached.pages.len() - 1;
            cached.phase = Phase::Ready;
            search.selected = 0;
        }
        Err(error) => {
            search.hits = Arc::default();
            cached.phase = Phase::Failed(error);
        }
    }
    Vec::new()
}

pub(crate) fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied) -> Vec<Intent> {
    // Reads, search and remote members (P2).
    match applied {
        Applied::SearchResults { request, result } => {
            remote::answered(ui, store, *request, result, crate::server_menu::wall_now());
            // A page joins above the loaded results the list held in view:
            // show the keyboard's choice, its first.
            return ui
                .search
                .as_ref()
                .map(|search| Intent::ScrollToIndex {
                    list: RESULTS.into(),
                    index: search.selected,
                })
                .into_iter()
                .collect();
        }
        Applied::Failed {
            failure: Failure::Search { request },
            reason,
            ..
        } => {
            remote::failed(ui, store, *request, reason);
            return Vec::new();
        }
        _ => {}
    }
    let Some(search) = &ui.search else {
        return Vec::new();
    };
    if matches!(applied, Applied::Ready)
        || ui.channel() != Some(search.channel)
        || search.account != store.me()
        || !store
            .abilities(search.channel)
            .is_some_and(|abilities| abilities.read_history.allowed())
    {
        return close(ui);
    }
    refresh(ui, store, matches!(applied, Applied::User(_)));
    Vec::new()
}

pub(crate) fn open(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    if !ui
        .channel()
        .and_then(|channel| store.abilities(channel))
        .is_some_and(|abilities| abilities.read_history.allowed())
    {
        return Vec::new();
    }
    if crate::invitations::visible(ui)
        || ui.settings.is_some()
        || ui.server_settings.is_some()
        || ui.channel_settings.is_some()
        || ui.profiles.editor.is_some()
        || ui.profiles.custom.is_some()
        || ui.server_menu.dialog.is_some()
        || ui.channel_list.dialog.is_some()
        || ui.channel_list.topic.is_some()
        || ui.confirm_delete.is_some()
    {
        return Vec::new();
    }
    let Some(channel) = ui.channel().filter(|channel| {
        store
            .channel(*channel)
            .is_some_and(|channel| channel.kind.has_messages() && Store::visible(channel))
    }) else {
        return Vec::new();
    };
    ui.switcher = None;
    ui.channel_list.menu = None;
    ui.voice.menu = None;
    if ui
        .search
        .as_ref()
        .is_none_or(|search| search.channel != channel || search.account != store.me())
    {
        ui.search = Some(MessageSearch {
            channel,
            account: store.me(),
            query: String::new(),
            author: None,
            author_query: String::new(),
            author_options: Vec::new(),
            files: false,
            pinned: false,
            from: String::new(),
            until: String::new(),
            month: None,
            menu: None,
            scope: Scope::Loaded,
            selected: 0,
            source: Arc::default(),
            hits: Arc::default(),
            cached: None,
            form_error: None,
            remote: None,
            suggestion: 0,
            suggestions_closed: false,
            lookup: None,
            order: Default::default(),
            has: Vec::new(),
        });
    }
    refresh(ui, store, false);
    vec![
        Intent::SetInput {
            id: QUERY.into(),
            value: ui.search.as_ref().unwrap().query.clone(),
        },
        Intent::Focus(QUERY.into()),
    ]
}

fn jump(ui: &mut Ui, store: &mut Store, message: MessageId) -> Vec<Intent> {
    refresh(ui, store, false);
    let Some(search) = &ui.search else {
        return Vec::new();
    };
    let Some(hit) = search.hits.iter().find(|hit| hit.data.id == message) else {
        return Vec::new();
    };
    if !search.actionable() {
        return Vec::new();
    }
    // A result of another channel of the server opens it at the message.
    if hit.data.channel != search.channel {
        let channel = hit.data.channel;
        let Some(server) = store
            .channel(channel)
            .filter(|found| Store::visible(found))
            .map(|found| found.server)
        else {
            ui.notify(
                t("Ese canal no está disponible"),
                Some(t("RUNE no tiene abierto el canal de ese resultado.").into()),
                true,
                std::time::Instant::now(),
            );
            return Vec::new();
        };
        return crate::open_at(ui, store, server, channel, Some(message));
    }
    crate::context::open_message(
        ui,
        store,
        search.channel,
        message,
        if search.scope == Scope::Device {
            HistorySource::Cached
        } else {
            HistorySource::Provider
        },
    )
}

fn page(ui: &mut Ui, store: &Store, forward: bool) -> Vec<Intent> {
    if ui
        .search
        .as_ref()
        .is_some_and(|search| search.scope == Scope::Remote)
    {
        return remote::page(ui, store, forward);
    }
    let Some(search) = ui.search.as_mut() else {
        return Vec::new();
    };
    let Some(cached) = search
        .cached
        .as_mut()
        .filter(|cached| matches!(cached.phase, Phase::Ready))
    else {
        return Vec::new();
    };
    if forward && cached.page + 1 == cached.pages.len() {
        let Some(cursor) = cached.pages.back().and_then(|page| page.continuation) else {
            return Vec::new();
        };
        return start(ui, store, Some(cursor), false);
    }
    if forward {
        cached.page += 1;
    } else if cached.page > 0 {
        cached.page -= 1;
    } else {
        return Vec::new();
    }
    search.selected = 0;
    search.hits = kept(&cached.pages[cached.page].hits);
    vec![Intent::ScrollToIndex {
        list: RESULTS.into(),
        index: 0,
    }]
}

pub(crate) fn update(ui: &mut Ui, store: &mut Store, action: &UiAction) -> Option<Vec<Intent>> {
    if let UiAction::Clicked { id } = action {
        if let Some(intents) = crate::context::update(ui, store, id) {
            return Some(intents);
        }
    }
    match action {
        UiAction::Clicked { id } if id == "message-search:open" => Some(open(ui, store)),
        UiAction::Clicked { id } if id == "message-search:close" => Some(close(ui)),
        UiAction::Clicked { id } if id == "message-search:next" => Some(page(ui, store, true)),
        UiAction::Clicked { id } if id == "message-search:previous" => Some(page(ui, store, false)),
        UiAction::Clicked { id } if id == "message-search:refresh" => {
            let offset = ui
                .search
                .as_ref()
                .filter(|search| search.scope == Scope::Remote)
                .and_then(|search| search.remote.as_ref())
                .map(|remote| remote.offset);
            Some(match offset {
                Some(offset) => remote::start(ui, store, offset, false),
                None => start(ui, store, None, true),
            })
        }
        UiAction::Clicked { id } if id.starts_with("message-search:suggest:") => {
            let index = id.strip_prefix("message-search:suggest:")?.parse().ok()?;
            Some(suggest(ui, store, index))
        }
        UiAction::Clicked { id } if id == "message-search:menu-close" => {
            ui.search.as_mut()?.menu = None;
            Some(Vec::new())
        }
        UiAction::Clicked { id }
            if matches!(
                id.as_str(),
                "message-search:order:newest" | "message-search:order:relevance"
            ) =>
        {
            let search = ui.search.as_mut()?;
            search.menu = None;
            search.order = if id.ends_with("relevance") {
                rune_model::RemoteOrder::Relevance
            } else {
                rune_model::RemoteOrder::Newest
            };
            // A page already asked for is asked again in the new order.
            if search.remote.is_some() {
                return Some(remote::start(ui, store, 0, false));
            }
            remote::refresh(search, store);
            Some(Vec::new())
        }
        UiAction::Clicked { id }
            if matches!(
                id.as_str(),
                "message-search:scope"
                    | "message-search:author"
                    | "message-search:from-open"
                    | "message-search:until-open"
                    | "message-search:order"
                    | "message-search:has"
            ) =>
        {
            let search = ui.search.as_mut()?;
            let menu = match id.as_str() {
                "message-search:scope" => Menu::Scope,
                "message-search:author" => Menu::Author,
                "message-search:from-open" => Menu::From,
                "message-search:order" => Menu::Order,
                "message-search:has" => Menu::Has,
                _ => Menu::Until,
            };
            search.menu = (search.menu != Some(menu)).then_some(menu);
            search.month = None;
            if menu == Menu::Author {
                search.author_options = authors(store, &search.author_query, search.author);
                return Some(vec![
                    Intent::SetInput {
                        id: "message-search:author-query".into(),
                        value: search.author_query.clone(),
                    },
                    Intent::Focus("message-search:author-query".into()),
                ]);
            }
            if menu == Menu::From || menu == Menu::Until {
                return Some(vec![Intent::SetInput {
                    id: if menu == Menu::From {
                        "message-search:from"
                    } else {
                        "message-search:until"
                    }
                    .into(),
                    value: if menu == Menu::From {
                        search.from.clone()
                    } else {
                        search.until.clone()
                    },
                }]);
            }
            Some(Vec::new())
        }
        UiAction::Clicked { id }
            if matches!(
                id.as_str(),
                "message-search:scope:device"
                    | "message-search:scope:loaded"
                    | "message-search:scope:remote"
            ) =>
        {
            if id.ends_with("device") && !ui.local_history_available {
                return Some(Vec::new());
            }
            let channel = ui.search.as_ref()?.channel;
            if id.ends_with("remote") && !store.remote_search(channel).allowed() {
                return Some(Vec::new());
            }
            let scope = if id.ends_with("device") {
                Scope::Device
            } else if id.ends_with("remote") {
                Scope::Remote
            } else {
                Scope::Loaded
            };
            let search = ui.search.as_mut()?;
            let mut intents = search
                .cached
                .take()
                .map(|cached| vec![Intent::Command(Command::Cancel(cached.request))])
                .unwrap_or_default();
            if let Some(old) = search.remote.take() {
                intents.push(Intent::Command(Command::Cancel(old.request)));
            }
            search.scope = scope;
            if scope == Scope::Device {
                search.source = Arc::default();
                // A page kept on this device knows files only.
                search.has.clear();
            }
            search.menu = None;
            search.hits = Arc::default();
            search.suggestions_closed = false;
            if scope == Scope::Remote {
                // The whole history is asked with Enter; what is loaded shows now.
                remote::refresh(search, store);
            } else {
                intents.extend(start(ui, store, None, true));
            }
            intents.push(Intent::Focus(QUERY.into()));
            Some(intents)
        }
        UiAction::Clicked { id }
            if matches!(
                id.as_str(),
                "message-search:mine"
                    | "message-search:files"
                    | "message-search:pinned"
                    | "message-search:clear-filters"
            ) || id.starts_with("message-search:author:")
                || id.starts_with("message-search:has:") =>
        {
            let search = ui.search.as_mut()?;
            match id.as_str() {
                "message-search:clear-filters" => {
                    search.author = None;
                    search.files = false;
                    search.pinned = false;
                    search.has.clear();
                    search.from.clear();
                    search.until.clear();
                    search.menu = None;
                }
                _ if id.starts_with("message-search:has:") => {
                    let has = Has::parse(id.strip_prefix("message-search:has:")?)
                        .filter(|has| *has != Has::File && search.scope != Scope::Device)?;
                    match search.has.iter().position(|found| *found == has) {
                        Some(at) => {
                            search.has.remove(at);
                        }
                        None => search.has.push(has),
                    }
                }
                "message-search:mine" => {
                    search.author = if search.author == store.me() {
                        None
                    } else {
                        store.me()
                    }
                }
                "message-search:files" => search.files = !search.files,
                "message-search:pinned" => search.pinned = !search.pinned,
                _ => {
                    let value = id.strip_prefix("message-search:author:")?;
                    search.author = if value == "all" {
                        None
                    } else {
                        Some(UserId(value.parse().ok()?))
                    };
                    if search
                        .author
                        .is_some_and(|author| store.user(author).is_none())
                    {
                        search.author = None;
                        return Some(Vec::new());
                    }
                    search.menu = None;
                }
            }
            let mut intents = start(ui, store, None, true);
            intents.push(Intent::ScrollToIndex {
                list: RESULTS.into(),
                index: 0,
            });
            Some(intents)
        }
        UiAction::InputChanged { id, value } if id == "message-search:author-query" => {
            let search = ui.search.as_mut()?;
            search.author_query = value.chars().take(64).collect();
            search.author_options = authors(store, &search.author_query, search.author);
            Some(Vec::new())
        }
        UiAction::InputChanged { id, value }
            if id == QUERY || id == "message-search:from" || id == "message-search:until" =>
        {
            let search = ui.search.as_mut()?;
            if id == QUERY {
                search.query = value.clone();
            } else if id.ends_with("from") {
                search.from = value.clone();
            } else {
                search.until = value.clone();
            }
            search.selected = 0;
            if id == QUERY && search.scope == Scope::Remote {
                // Typing shows what is loaded and suggests filters; Enter asks.
                search.suggestion = 0;
                search.suggestions_closed = false;
                remote::refresh(search, store);
                return Some(remote::lookup(ui, store).into_iter().collect());
            }
            let mut intents = start(ui, store, None, true);
            intents.push(Intent::ScrollToIndex {
                list: RESULTS.into(),
                index: 0,
            });
            Some(intents)
        }
        UiAction::Clicked { id }
            if id.starts_with("message-search:from:")
                || id.starts_with("message-search:until:") =>
        {
            let search = ui.search.as_mut()?;
            let from = id.starts_with("message-search:from:");
            let base = if from {
                "message-search:from"
            } else {
                "message-search:until"
            };
            let value = if from {
                &mut search.from
            } else {
                &mut search.until
            };
            if id.ends_with(":clear") {
                value.clear();
                search.menu = None;
            } else if let Some(date) = DatePicker::selected_date(id, base) {
                *value = date.into();
                search.menu = None;
            } else if id.ends_with(":month_prev") || id.ends_with(":month_next") {
                let (year, month) = search.month.unwrap_or_else(|| time::picker_month(value));
                search.month = Some(DatePicker::shift_month(
                    year,
                    month,
                    if id.ends_with(":month_prev") { -1 } else { 1 },
                ));
                return Some(Vec::new());
            } else {
                return Some(Vec::new());
            }
            let value = if from {
                search.from.clone()
            } else {
                search.until.clone()
            };
            let mut intents = start(ui, store, None, true);
            intents.push(Intent::SetInput {
                id: base.into(),
                value,
            });
            Some(intents)
        }
        UiAction::Submitted { id, .. } if id == QUERY => {
            let search = ui.search.as_ref()?;
            if search.scope == Scope::Remote {
                if !suggestions(search, store).is_empty() {
                    return Some(suggest(ui, store, search.suggestion));
                }
                if search
                    .remote
                    .as_ref()
                    .is_none_or(|remote| remote.asked != remote::signature(search))
                {
                    return Some(remote::start(ui, store, 0, false));
                }
            }
            // Nothing loaded matches: Enter goes on through the whole history.
            if search.scope == Scope::Loaded
                && search.hits.is_empty()
                && !search.query.trim().is_empty()
                && store.remote_search(search.channel).allowed()
            {
                let search = ui.search.as_mut()?;
                search.scope = Scope::Remote;
                return Some(remote::start(ui, store, 0, false));
            }
            refresh(ui, store, false);
            let message = ui
                .search
                .as_ref()?
                .hits
                .get(ui.search.as_ref()?.selected)
                .map(|hit| hit.data.id);
            Some(
                message
                    .map(|message| jump(ui, store, message))
                    .unwrap_or_default(),
            )
        }
        UiAction::Clicked { id } if id.starts_with("message-search:go:") => {
            let (channel, message) = id.strip_prefix("message-search:go:")?.split_once(':')?;
            let channel = ChannelId(channel.parse().ok()?);
            let message = MessageId(message.parse().ok()?);
            Some(
                if ui
                    .search
                    .as_ref()
                    .is_some_and(|search| search.channel == channel)
                {
                    jump(ui, store, message)
                } else {
                    Vec::new()
                },
            )
        }
        _ => None,
    }
}

pub(crate) fn keys(
    ui: &mut Ui,
    store: &Store,
    key: Key,
    ctrl: bool,
    focused: Option<&str>,
) -> Option<Vec<Intent>> {
    ui.search.as_ref()?;
    if key == Key::Escape {
        if ui.search.as_mut()?.menu.take().is_some() {
            return Some(Vec::new());
        }
        let search = ui.search.as_ref()?;
        if !suggestions(search, store).is_empty() {
            ui.search.as_mut()?.suggestions_closed = true;
            return Some(Vec::new());
        }
        return Some(close(ui));
    }
    if focused != Some(QUERY) || ctrl {
        return None;
    }
    // The filter suggestions take the arrows while they show.
    let offered = suggestions(ui.search.as_ref()?, store).len();
    if offered > 0 {
        let search = ui.search.as_mut()?;
        match key {
            Key::ArrowDown => search.suggestion = (search.suggestion + 1) % offered,
            Key::ArrowUp => search.suggestion = (search.suggestion + offered - 1) % offered,
            Key::Tab => return Some(suggest(ui, store, ui.search.as_ref()?.suggestion)),
            _ => return None,
        }
        return Some(Vec::new());
    }
    refresh(ui, store, false);
    let search = ui.search.as_mut()?;
    let count = search.hits.len();
    match key {
        Key::ArrowDown if count > 0 => search.selected = (search.selected + 1) % count,
        Key::ArrowUp if count > 0 => search.selected = (search.selected + count - 1) % count,
        _ => return None,
    }
    Some(vec![Intent::ScrollToIndex {
        list: RESULTS.into(),
        index: search.selected,
    }])
}
