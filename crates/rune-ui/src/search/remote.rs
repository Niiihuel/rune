//! Searching the provider through a conversation's or a server's whole
//! history, a page of 25 at a time, beside the messages loaded here: those
//! show while typing and join the first page, each once, saying where it
//! came from. A provider still indexing is asked again once, by itself.

use std::time::Duration;

use rune_i18n::{t, tr};
use rune_model::{
    newer, RemoteOrder, RemotePage, RemoteSearch, RemoteSearchError, SearchHit, ServerId,
    Timestamp, MAX_REMOTE_OFFSET, REMOTE_PAGE,
};
use rune_provider::Command;
use rune_state::Store;

use super::{filters, hits, MessageSearch, Scope, Source};
use crate::{Intent, Ui};

#[derive(Debug)]
pub(super) enum Phase {
    Loading,
    Ready(RemotePage),
    /// The provider prepares its index; asked again at `until`.
    Indexing {
        until: Timestamp,
    },
    Failed(String),
}

/// One page asked of the provider, and what it answered.
#[derive(Debug)]
pub(super) struct RemoteRead {
    pub(super) request: u64,
    pub(super) search: RemoteSearch,
    /// The text and filters it came from: once they change, Enter asks again.
    pub(super) asked: String,
    pub(super) offset: u32,
    pub(super) phase: Phase,
    /// It was asked again after an index in progress: never a third time.
    pub(super) retried: bool,
}

/// What the text and the filters beside it make, to tell whether the page
/// shown still answers them.
pub(super) fn signature(search: &MessageSearch) -> String {
    format!(
        "{}\u{1f}{:?}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{:?}",
        search.query,
        search.author,
        search.files,
        search.pinned,
        search.from,
        search.until,
        search.order
    )
}

/// The search the text and the filters beside it make.
pub(super) fn query(search: &MessageSearch, store: &Store) -> Result<RemoteSearch, &'static str> {
    let mut query = filters::parse(store, search.channel, &search.query)?;
    if let Some(author) = search
        .author
        .filter(|author| !query.authors.contains(author))
    {
        query.authors.push(author);
    }
    if search.files && !query.has.contains(&rune_model::Has::File) {
        query.has.push(rune_model::Has::File);
    }
    if search.pinned {
        query.pinned = Some(true);
    }
    let bound = |text: &str, next| {
        rune_components::time::local_date_boundary(text, next)
            .ok_or(t("Escribí las fechas como AAAA-MM-DD."))
    };
    if !search.from.is_empty() {
        query.after = filters::narrower(query.after, bound(&search.from, false)?, true);
    }
    if !search.until.is_empty() {
        query.before = filters::narrower(query.before, bound(&search.until, true)?, false);
    }
    query.order = search.order;
    Ok(query)
}

/// Ask the provider for the page at `offset` of what the field says now.
pub(super) fn start(ui: &mut Ui, store: &Store, offset: u32, retried: bool) -> Vec<Intent> {
    let request = ui.fresh_request();
    let Some(search) = ui.search.as_mut() else {
        return Vec::new();
    };
    let mut intents = Vec::new();
    if let Some(old) = search.remote.take() {
        if matches!(old.phase, Phase::Loading) {
            intents.push(Intent::Command(Command::Cancel(old.request)));
        }
    }
    search.form_error = None;
    search.selected = 0;
    let asked = signature(search);
    let made = query(search, store).and_then(|made| made.validate().map(|()| made));
    let made = match made {
        Ok(made) => made,
        Err(error) => {
            search.form_error = Some(error);
            refresh(search, store);
            return intents;
        }
    };
    let phase = match store.remote_search(search.channel).reason() {
        Some(reason) => Phase::Failed(reason.to_owned()),
        None => {
            intents.push(Intent::Command(Command::SearchMessages {
                request,
                search: made.clone(),
                offset,
            }));
            Phase::Loading
        }
    };
    search.remote = Some(RemoteRead {
        request,
        search: made,
        asked,
        offset,
        phase,
        retried,
    });
    refresh(search, store);
    intents.push(Intent::ScrollToIndex {
        list: super::RESULTS.into(),
        index: 0,
    });
    intents
}

/// The provider's answer to `request`, if it is still the one shown.
pub(super) fn answered(
    ui: &mut Ui,
    store: &Store,
    request: u64,
    result: &Result<RemotePage, RemoteSearchError>,
    wall: Timestamp,
) {
    let Some(search) = ui
        .search
        .as_mut()
        .filter(|search| search.scope == Scope::Remote)
    else {
        return;
    };
    let Some(remote) = search
        .remote
        .as_mut()
        .filter(|remote| remote.request == request && matches!(remote.phase, Phase::Loading))
    else {
        return;
    };
    remote.phase = match result {
        Ok(page) if page.fits(&remote.search, remote.offset) => Phase::Ready(page.clone()),
        Ok(_) => Phase::Failed(t("El proveedor envió resultados incompatibles.").into()),
        Err(RemoteSearchError::Indexing { retry_after_ms }) if !remote.retried => {
            Phase::Indexing {
                until: Timestamp(wall.0.saturating_add(*retry_after_ms as i64)),
            }
        }
        Err(RemoteSearchError::Indexing { .. }) => Phase::Failed(
            t("El índice de búsqueda sigue en preparación. Volvé a intentarlo más tarde con «Actualizar».").into(),
        ),
        Err(RemoteSearchError::Failed(reason)) => Phase::Failed(reason.clone()),
    };
    refresh(search, store);
}

/// The search failed before any answer.
pub(super) fn failed(ui: &mut Ui, store: &Store, request: u64, reason: &str) {
    let Some(search) = ui.search.as_mut() else {
        return;
    };
    if let Some(remote) = search
        .remote
        .as_mut()
        .filter(|remote| remote.request == request && matches!(remote.phase, Phase::Loading))
    {
        remote.phase = Phase::Failed(reason.to_owned());
        refresh(search, store);
    }
}

/// Recompute the results shown: the page and, on the first, the loaded
/// messages that match and are not on it yet.
pub(super) fn refresh(search: &mut MessageSearch, store: &Store) {
    let remote = search.remote.as_ref();
    let made = match remote {
        Some(remote) if remote.asked == signature(search) => Some(remote.search.clone()),
        _ => query(search, store).ok().filter(|made| !made.is_empty()),
    };
    let page = remote.and_then(|remote| match &remote.phase {
        Phase::Ready(page) => Some(page),
        _ => None,
    });
    let first = remote.is_none_or(|remote| remote.offset == 0);
    // Ranked by relevance, the provider's order stays: loaded messages only
    // join a page that holds every result, after it.
    let ranked = page.is_some()
        && made
            .as_ref()
            .is_some_and(|made| made.order == RemoteOrder::Relevance);
    // More results may follow: the page is full, or the count says so (a
    // provider may answer a short page before the last one).
    let more = page.is_some_and(|page| {
        page.messages.len() == REMOTE_PAGE
            || u64::from(page.offset) + (page.messages.len() as u64) < page.total
    });
    let mut found: Vec<(SearchHit, Source)> = page
        .map(|page| {
            page.messages
                .iter()
                .map(|message| (SearchHit::from_message(message), Source::Remote))
                .collect()
        })
        .unwrap_or_default();
    let shown = found.len();
    // Loaded messages join the first page down to its oldest result, or
    // all of them when it is the last page.
    let floor = page
        .filter(|_| more)
        .and_then(|page| page.messages.last().cloned());
    let covers = |made: &RemoteSearch| made.server.is_some() || made.channel == search.channel;
    let joins = first && !(ranked && more);
    if let (true, Some(made)) = (joins, made.as_ref().filter(|made| covers(made))) {
        if let (Some(matches), Some(timeline)) =
            (made.loaded_matcher(), store.timeline(search.channel))
        {
            for message in timeline.messages().iter() {
                if !matches(message) || floor.as_ref().is_some_and(|floor| newer(floor, message)) {
                    continue;
                }
                match found.iter_mut().find(|(hit, _)| hit.id == message.id) {
                    Some((_, source)) => *source = Source::Both,
                    None => found.push((SearchHit::from_message(message), Source::Loaded)),
                }
            }
        }
    }
    let newest_first = if ranked {
        &mut found[shown..]
    } else {
        &mut found[..]
    };
    newest_first.sort_by_key(|(hit, _)| std::cmp::Reverse((hit.timestamp, hit.id)));
    search.hits = hits(found, store);
    search.selected = search.selected.min(search.hits.len().saturating_sub(1));
}

/// The page after or before the one shown, within what the provider counted.
pub(super) fn page(ui: &mut Ui, store: &Store, forward: bool) -> Vec<Intent> {
    let Some(remote) = ui.search.as_ref().and_then(|search| search.remote.as_ref()) else {
        return Vec::new();
    };
    let Phase::Ready(page) = &remote.phase else {
        return Vec::new();
    };
    let step = REMOTE_PAGE as u32;
    let offset = if forward {
        let next = remote.offset + step;
        if u64::from(next) >= page.total || next > MAX_REMOTE_OFFSET {
            return Vec::new();
        }
        next
    } else if remote.offset >= step {
        remote.offset - step
    } else {
        return Vec::new();
    };
    start(ui, store, offset, false)
}

/// Whether there is a page after the one shown, and one before it.
pub(super) fn pages(search: &MessageSearch) -> (bool, bool) {
    match search.remote.as_ref() {
        Some(RemoteRead {
            offset,
            phase: Phase::Ready(page),
            ..
        }) => (
            *offset >= REMOTE_PAGE as u32,
            u64::from(offset + REMOTE_PAGE as u32) < page.total
                && offset + (REMOTE_PAGE as u32) <= MAX_REMOTE_OFFSET,
        ),
        _ => (false, false),
    }
}

/// Ask once more a provider that was still indexing, when it said to.
pub(crate) fn tick(ui: &mut Ui, store: &Store, wall: Timestamp) -> Vec<Intent> {
    let due = ui
        .search
        .as_ref()
        .and_then(|search| search.remote.as_ref())
        .and_then(|remote| match remote.phase {
            Phase::Indexing { until } if until <= wall => Some(remote.offset),
            _ => None,
        });
    match due {
        Some(offset) => {
            // The same search as asked, even if the field changed meanwhile.
            start(ui, store, offset, true)
        }
        None => Vec::new(),
    }
}

/// How long until [`tick`] asks again.
pub(crate) fn next_tick(ui: &Ui, wall: Timestamp) -> Option<Duration> {
    match ui.search.as_ref()?.remote.as_ref()?.phase {
        Phase::Indexing { until } => Some(Duration::from_millis((until.0 - wall.0).max(0) as u64)),
        _ => None,
    }
}

/// Look up at the provider the person being written after `from:` or
/// `mentions:`, once per name.
pub(super) fn lookup(ui: &mut Ui, store: &Store) -> Option<Intent> {
    let search = ui.search.as_mut()?;
    let server: ServerId = store.channel(search.channel)?.server?;
    let query = filters::person_query(&search.query)?.to_lowercase();
    if !store.member_search(server).allowed()
        || search.lookup.as_ref() == Some(&(server, query.clone()))
    {
        return None;
    }
    search.lookup = Some((server, query.clone()));
    Some(Intent::Command(Command::SearchMembers { server, query }))
}

/// What the line above the results says of the page.
pub(super) fn status(search: &MessageSearch, store: &Store) -> String {
    let provider = store
        .provider
        .as_ref()
        .map_or(t("el proveedor"), |provider| t(&provider.name));
    let loaded = search
        .hits
        .iter()
        .filter(|hit| hit.source == Source::Loaded)
        .count();
    let Some(remote) = search
        .remote
        .as_ref()
        .filter(|remote| remote.asked == signature(search))
    else {
        return if search.hits.is_empty() {
            tr!("Enter busca en todo el historial en {provider}.", provider)
        } else {
            tr!(
                "{count} cargados coinciden. Enter busca en todo el historial en {provider}.",
                count = search.hits.len(),
                provider
            )
        };
    };
    match &remote.phase {
        Phase::Loading => tr!("Buscando en {provider}…", provider),
        Phase::Failed(reason) => reason.clone(),
        Phase::Indexing { .. } => tr!(
            "{provider} todavía está preparando el índice de búsqueda; se vuelve a intentar solo en unos segundos.",
            provider
        ),
        Phase::Ready(page) => {
            let pages = page.total.div_ceil(REMOTE_PAGE as u64).max(1);
            let mut line = tr!(
                "{total} resultados en {provider} · página {page} de {pages}",
                total = page.total,
                provider,
                page = remote.offset / REMOTE_PAGE as u32 + 1,
                pages
            );
            if loaded > 0 {
                line.push_str(&tr!(" · {loaded} cargados aún sin indexar", loaded));
            }
            if page.partial {
                line.push_str(t(" · aún indexa mensajes viejos"));
            }
            line
        }
    }
}
