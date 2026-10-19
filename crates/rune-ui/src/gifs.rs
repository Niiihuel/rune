//! An explicitly submitted, ephemeral GIF search. A result is bound to the
//! account, conversation and picker that requested it; selecting only inserts.
//! The search is the provider's own when it offers one (P5: trending with
//! its categories, suggestions while writing), else the optional GIPHY of
//! this device; with both, the tab lets the person choose.

use std::time::Instant;

use limn::{Key, RasterImage, RasterImageFormat, UiAction};
use rune_components::emoji::{GifSearchItem, GifSearchView, PickerTab};
use rune_i18n::t;
use rune_model::{ChannelId, UserId};
use rune_provider::{Command, Failure, GifError, GifPage, GifQuery, GifResult};
use rune_state::Store;

use crate::{editor_id, Intent, Ui};

const PREFIX: &str = "composer:emoji:gif:";
pub(crate) const QUERY: &str = "composer:emoji:gif:query";

#[derive(Clone)]
struct Request {
    id: u64,
    channel: ChannelId,
    account: UserId,
    query: GifQuery,
    offset: u32,
}

/// This page belongs to the open picker, never to a persistent media cache.
#[derive(Default)]
pub struct GifPickerState {
    pub available: bool,
    draft: String,
    request: Option<Request>,
    page: Option<GifPage>,
    images: Vec<Option<RasterImage>>,
    loading: bool,
    error: Option<GifError>,
    // Presence, identity and expressions (P5).
    /// GIPHY was chosen over the provider's own search.
    giphy: bool,
    /// What the provider suggests searching for, and the request it answers.
    suggestions: Vec<String>,
    suggesting: Option<u64>,
    // Emoji and states (2026-10-18).
    /// The first GIF of the page «Más GIF» left: a next page that starts
    /// with it again is the service ignoring the offset.
    left: Option<String>,
}

impl std::fmt::Debug for GifPickerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GifPickerState")
            .field("available", &self.available)
            .field("loading", &self.loading)
            .field("items", &self.images.len())
            .finish_non_exhaustive()
    }
}

/// The provider searches GIFs itself.
fn service(store: &Store) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.gif_search)
}

/// Some search is available: the provider's or GIPHY.
pub(crate) fn searchable(ui: &Ui, store: &Store) -> bool {
    service(store) || ui.messages.gifs.available
}

/// The provider's search is the one in use.
fn uses_service(ui: &Ui, store: &Store) -> bool {
    service(store) && !(ui.messages.gifs.giphy && ui.messages.gifs.available)
}

fn is_current(ui: &Ui, store: &Store, request: &Request) -> bool {
    searchable(ui, store)
        && ui.emoji_open
        && ui.messages.expression_tab == PickerTab::Gif
        && ui.channel() == Some(request.channel)
        && store.me() == Some(request.account)
        && store
            .abilities(request.channel)
            .is_some_and(|a| a.write.allowed())
}

fn clear(ui: &mut Ui) -> Vec<Intent> {
    let state = &mut ui.messages.gifs;
    let cancelled = state.request.take().map(|request| Intent::CancelGifSearch {
        request: request.id,
    });
    state.page = None;
    state.images.clear();
    state.loading = false;
    state.error = None;
    cancelled.into_iter().collect()
}

/// Call before polling the service and after navigation. Closed pickers drop
/// their sole page and cancel in-flight media work, including on account reset.
pub fn sync_gif_search(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    if ui
        .messages
        .gifs
        .request
        .as_ref()
        .is_some_and(|request| !is_current(ui, store, request))
    {
        ui.messages.gifs.draft.clear();
        return clear(ui);
    }
    Vec::new()
}

/// Accept only the response for the still-visible request. Images stay here,
/// not in `Ui::images`, so closing the picker also releases this page's pixels.
pub fn gif_result(ui: &mut Ui, store: &Store, result: GifResult) -> Vec<Intent> {
    let current = ui.messages.gifs.request.as_ref().is_some_and(|request| {
        request.id == result.request
            && request.query == result.query
            && request.offset == result.offset
            && is_current(ui, store, request)
    });
    if !current {
        return Vec::new();
    }
    let state = &mut ui.messages.gifs;
    state.loading = false;
    match result.result {
        Ok(mut page) => {
            // The page shown again: there is no other to offer.
            if result.offset > 0 && page.items.first().map(|item| &item.id) == state.left.as_ref() {
                page.next_offset = None;
            }
            state.images = page
                .items
                .iter()
                .map(|item| {
                    item.thumbnail.as_ref().and_then(|image| {
                        RasterImage::new(
                            image.sha256.clone(),
                            RasterImageFormat::Png,
                            image.size_bytes,
                            image.png_base64.clone(),
                            "GIF",
                        )
                    })
                })
                .collect();
            state.page = Some(page);
            state.error = None;
        }
        Err(error) => {
            state.error = Some(error);
            state.page = None;
            state.images.clear();
        }
    }
    vec![Intent::Repaint]
}

fn search(ui: &mut Ui, store: &Store, query: GifQuery, offset: u32) -> Vec<Intent> {
    let Some((channel, account)) = ui.channel().zip(store.me()) else {
        return Vec::new();
    };
    if !searchable(ui, store)
        || !ui.emoji_open
        || ui.messages.expression_tab != PickerTab::Gif
        || !store.abilities(channel).is_some_and(|a| a.write.allowed())
    {
        return Vec::new();
    }
    if matches!(&query, GifQuery::Search(value) if value.trim().is_empty() || value.chars().count() > 50 || value.chars().any(char::is_control))
    {
        ui.messages.gifs.error = Some(GifError::InvalidQuery);
        return Vec::new();
    }
    let mut intents = clear(ui);
    let request = ui.fresh_request();
    ui.messages.gifs.request = Some(Request {
        id: request,
        channel,
        account,
        query: query.clone(),
        offset,
    });
    ui.messages.gifs.loading = true;
    ui.messages.gifs.suggestions.clear();
    intents.push(if uses_service(ui, store) {
        Intent::Command(Command::SearchGifs {
            request,
            query,
            offset,
        })
    } else {
        Intent::SearchGifs {
            request,
            query,
            offset,
        }
    });
    intents
}

/// What to search for, asked of the provider while writing (P5).
fn suggest(ui: &mut Ui, store: &Store, query: &str) -> Vec<Intent> {
    if !uses_service(ui, store) || query.chars().count() > 50 {
        return Vec::new();
    }
    let request = ui.fresh_request();
    ui.messages.gifs.suggesting = Some(request);
    vec![Intent::Command(Command::SuggestGifs {
        request,
        query: query.trim().to_owned(),
    })]
}

/// The tab opened: the provider's search starts on what is trending.
pub(crate) fn opened(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let state = &ui.messages.gifs;
    if !uses_service(ui, store) || state.page.is_some() || state.loading {
        return Vec::new();
    }
    let mut intents = search(ui, store, GifQuery::Trending, 0);
    intents.extend(suggest(ui, store, ""));
    intents
}

/// Answer what the provider reported about its GIFs (P5).
pub(crate) fn on_applied(ui: &mut Ui, store: &Store, applied: &rune_state::Applied) -> Vec<Intent> {
    match applied {
        rune_state::Applied::GifResults(result) => gif_result(ui, store, result.clone()),
        rune_state::Applied::GifSuggestions { request, terms, .. }
            if ui.messages.gifs.suggesting == Some(*request) =>
        {
            ui.messages.gifs.suggestions = terms.clone();
            vec![Intent::Repaint]
        }
        rune_state::Applied::Failed {
            failure: Failure::Gifs { request },
            ..
        } if ui
            .messages
            .gifs
            .request
            .as_ref()
            .is_some_and(|current| current.id == *request) =>
        {
            ui.messages.gifs.loading = false;
            ui.messages.gifs.error = Some(GifError::Unavailable);
            vec![Intent::Repaint]
        }
        _ => Vec::new(),
    }
}

/// The pictures of the page shown that load as any picture: the
/// provider's own GIFs and categories (P5).
pub(crate) fn pictures<'a>(ui: &'a Ui, store: &Store) -> Vec<&'a rune_model::RemoteImage> {
    let state = &ui.messages.gifs;
    let Some(page) = state
        .request
        .as_ref()
        .filter(|request| is_current(ui, store, request))
        .and(state.page.as_ref())
    else {
        return Vec::new();
    };
    page.items
        .iter()
        .filter(|item| item.thumbnail.is_none())
        .map(|item| &item.preview)
        .chain(
            page.categories
                .iter()
                .filter_map(|category| category.image.as_ref()),
        )
        .collect()
}

pub(crate) fn update(
    ui: &mut Ui,
    store: &Store,
    action: &UiAction,
    now: Instant,
) -> Option<Vec<Intent>> {
    match action {
        UiAction::InputChanged { id, value } if id == QUERY => {
            ui.messages.gifs.draft = value.chars().take(51).collect();
            ui.messages.gifs.error = None;
            let draft = ui.messages.gifs.draft.clone();
            Some(suggest(ui, store, &draft))
        }
        UiAction::Submitted { id, value } if id == QUERY => {
            Some(search(ui, store, GifQuery::Search(value.clone()), 0))
        }
        UiAction::Clicked { id } if id.starts_with(PREFIX) => {
            let action = id.strip_prefix(PREFIX)?;
            let result = match action {
                "search" => search(
                    ui,
                    store,
                    GifQuery::Search(ui.messages.gifs.draft.clone()),
                    0,
                ),
                "trending" => search(ui, store, GifQuery::Trending, 0),
                // The provider's or GIPHY, when both are there (P5).
                "source=service" | "source=giphy" => {
                    ui.messages.gifs.giphy = action == "source=giphy";
                    let mut intents = clear(ui);
                    ui.messages.gifs.suggestions.clear();
                    intents.extend(opened(ui, store));
                    intents.push(Intent::Repaint);
                    intents
                }
                _ if action.starts_with("category=") || action.starts_with("suggest=") => {
                    let index = action
                        .split_once('=')
                        .and_then(|(_, index)| index.parse::<usize>().ok());
                    let state = &ui.messages.gifs;
                    let term = index.and_then(|index| {
                        if action.starts_with("category=") {
                            let page = state.page.as_ref()?;
                            Some(page.categories.get(index)?.name.clone())
                        } else {
                            state.suggestions.get(index).cloned()
                        }
                    });
                    match term {
                        Some(term) => {
                            ui.messages.gifs.draft = term.clone();
                            search(ui, store, GifQuery::Search(term), 0)
                        }
                        None => Vec::new(),
                    }
                }
                "first" | "next" => {
                    let state = &ui.messages.gifs;
                    let target = state
                        .request
                        .as_ref()
                        .filter(|request| is_current(ui, store, request))
                        .zip(state.page.as_ref())
                        .filter(|_| !state.loading)
                        .and_then(|(request, page)| {
                            let offset = if action == "first" {
                                Some(0)
                            } else {
                                page.next_offset
                            }?;
                            let first = page.items.first().map(|item| item.id.clone());
                            Some((request.query.clone(), offset, first))
                        });
                    let target = target.map(|(query, offset, first)| {
                        ui.messages.gifs.left = first;
                        (query, offset)
                    });
                    target.map_or_else(Vec::new, |(query, offset)| search(ui, store, query, offset))
                }
                "attribution" => ui
                    .messages
                    .gifs
                    .page
                    .as_ref()
                    .map(|page| vec![Intent::OpenUrl(page.attribution.url.clone())])
                    .unwrap_or_default(),
                _ => {
                    let selected = action
                        .strip_prefix("pick=")
                        .and_then(|value| value.split_once(':'))
                        .and_then(|(request, index)| {
                            Some((request.parse::<u64>().ok()?, index.parse::<usize>().ok()?))
                        });
                    let selected = selected.and_then(|(id, index)| {
                        let state = &ui.messages.gifs;
                        let request = state
                            .request
                            .as_ref()
                            .filter(|request| request.id == id && is_current(ui, store, request))?;
                        Some((
                            request.channel,
                            state.page.as_ref()?.items.get(index)?.url.clone(),
                        ))
                    });
                    selected.map_or_else(Vec::new, |(channel, url)| {
                        let mut intents =
                            crate::composer_selection::insert(ui, channel, &url, true, now);
                        if !intents.is_empty() {
                            intents.extend(clear(ui));
                        }
                        intents
                    })
                }
            };
            Some(result)
        }
        _ => None,
    }
}

pub(crate) fn key(
    ui: &mut Ui,
    store: &Store,
    key: Key,
    focused: Option<&str>,
) -> Option<Vec<Intent>> {
    if focused != Some(QUERY) {
        return None;
    }
    match key {
        Key::Enter => Some(search(
            ui,
            store,
            GifQuery::Search(ui.messages.gifs.draft.clone()),
            0,
        )),
        Key::Escape => {
            ui.emoji_open = false;
            let mut intents = clear(ui);
            if let Some(channel) = ui.channel() {
                intents.push(Intent::Focus(editor_id(channel)));
            }
            Some(intents)
        }
        _ => None,
    }
}

pub(crate) fn view(ui: &Ui, store: &Store) -> GifSearchView {
    let state = &ui.messages.gifs;
    let current = state
        .request
        .as_ref()
        .filter(|request| is_current(ui, store, request));
    let page = current.and(state.page.as_ref());
    let favorites: Vec<String> = crate::gif_favorites::shown(ui, store)
        .iter()
        .map(|gif| gif.url.clone())
        .collect();
    let both = service(store) && state.available;
    let service_name = store
        .provider
        .as_ref()
        .map_or_else(|| t("Servicio").to_owned(), |info| t(&info.name).to_owned());
    GifSearchView {
        available: searchable(ui, store),
        categories: page
            .map(|page| {
                page.categories
                    .iter()
                    .map(|category| (category.name.clone(), ui.picture(category.image.as_ref())))
                    .collect()
            })
            .unwrap_or_default(),
        suggestions: if uses_service(ui, store) {
            state.suggestions.clone()
        } else {
            Vec::new()
        },
        sources: if both {
            let service = uses_service(ui, store);
            vec![
                ("service".into(), service_name, service),
                ("giphy".into(), "GIPHY".into(), !service),
            ]
        } else {
            Vec::new()
        },
        query: state.draft.clone(),
        loading: current.is_some() && state.loading,
        error: state.error.as_ref().map(|error| error.label().to_owned()),
        items: page
            .map(|page| {
                page.items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| GifSearchItem {
                        id: format!("{}:{index}", current.expect("page has a request").id),
                        title: item.title.clone(),
                        creator: item.creator.clone(),
                        image: state
                            .images
                            .get(index)
                            .cloned()
                            .flatten()
                            .or_else(|| ui.picture(Some(&item.preview))),
                        favorite: favorites.contains(&item.url),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        has_page: page.is_some(),
        has_next: page.is_some_and(|page| page.next_offset.is_some()),
        has_previous: current.is_some_and(|request| request.offset > 0),
        // The provider's own pages name no address to credit.
        attribution: page
            .filter(|page| !page.attribution.url.is_empty())
            .map(|page| page.attribution.label.clone()),
        attribution_image: page.and_then(|page| {
            page.attribution.image.as_ref().and_then(|image| {
                RasterImage::new(
                    image.sha256.clone(),
                    RasterImageFormat::Png,
                    image.size_bytes,
                    image.png_base64.clone(),
                    page.attribution.label.clone(),
                )
            })
        }),
        favorites,
        favorites_scope: crate::gif_favorites::scope(store),
    }
}

/// The result `item` (`{request}:{index}`) of the page shown, as a favorite.
pub(crate) fn favorite_of(
    ui: &Ui,
    store: &Store,
    item: &str,
) -> Option<rune_provider::FavoriteGif> {
    let (request, index) = item.split_once(':')?;
    let (request, index) = (request.parse::<u64>().ok()?, index.parse::<usize>().ok()?);
    let state = &ui.messages.gifs;
    state
        .request
        .as_ref()
        .filter(|current| current.id == request && is_current(ui, store, current))?;
    let found = state.page.as_ref()?.items.get(index)?;
    Some(rune_provider::FavoriteGif {
        url: found.url.clone(),
        preview: Some(found.preview.url.clone()),
        width: found.thumbnail.as_ref().map_or(0, |image| image.width),
        height: found.thumbnail.as_ref().map_or(0, |image| image.height),
    })
}

#[cfg(test)]
mod tests;
