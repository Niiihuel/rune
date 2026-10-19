//! The GIFs kept as favorites, and where they are kept: by the provider for
//! the account, across its devices, when it declares
//! [`synced_gif_favorites`](rune_provider::Capabilities::synced_gif_favorites);
//! else on this device only, in the local settings. The GIF tab says which,
//! so nobody expects to find a local favorite on another device.
//!
//! Emoji and stickers have no favorites to keep: the reference keeps their
//! frequent ones per session, and so does RUNE.

use limn::UiAction;
use rune_i18n::{t, tr};
use rune_provider::{Command, FavoriteGif};
use rune_state::Store;

use rune_components::composer::ids as composer_ids;

use crate::{Intent, Ui};

/// The setting the local favorites are kept in: one per line, its address,
/// preview, width and height apart by tabs.
pub(crate) const KEY: &str = "gif_favorites";
/// The most favorites kept on this device.
const MAX: usize = 100;

/// Whether the provider keeps the favorites.
pub(crate) fn synced(store: &Store) -> bool {
    store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.synced_gif_favorites)
}

/// The favorites shown now, the first added first; empty while the
/// provider's are loading.
pub(crate) fn shown<'a>(ui: &'a Ui, store: &'a Store) -> &'a [FavoriteGif] {
    if synced(store) {
        store.gif_favorites().unwrap_or_default()
    } else {
        &ui.messages.gif_favorites
    }
}

/// Where the favorites are kept, as the GIF tab says it.
pub(crate) fn scope(store: &Store) -> String {
    match store.provider.as_ref().filter(|_| synced(store)) {
        Some(info) => tr!(
            "Sincronizados con tu cuenta de {name}",
            name = t(&info.name)
        ),
        None => t("Guardados sólo en este equipo").to_owned(),
    }
}

/// Ask the provider for its favorites the first time the GIF tab opens.
pub(crate) fn load(store: &Store) -> Vec<Intent> {
    if synced(store) && store.gif_favorites().is_none() {
        vec![Intent::Command(Command::LoadGifFavorites)]
    } else {
        Vec::new()
    }
}

pub(crate) fn kept(ui: &Ui) -> Option<String> {
    let lines: Vec<String> = ui
        .messages
        .gif_favorites
        .iter()
        .map(|gif| {
            let preview = gif.preview.as_deref().unwrap_or("-");
            format!("{}\t{preview}\t{}\t{}", gif.url, gif.width, gif.height)
        })
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

/// Take back what [`kept`] kept; a line that is not a public address goes.
pub(crate) fn restore(ui: &mut Ui, setting: Option<String>) {
    ui.messages.gif_favorites = setting
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let mut fields = line.split('\t');
            let url = crate::expressions::public_url(fields.next()?)?;
            let preview = fields
                .next()
                .filter(|preview| *preview != "-")
                .and_then(crate::expressions::public_url);
            let size = |field: Option<&str>| field.and_then(|v| v.parse().ok()).unwrap_or(0);
            Some(FavoriteGif {
                url,
                preview,
                width: size(fields.next()),
                height: size(fields.next()),
            })
        })
        .take(MAX)
        .collect();
}

/// Add `gif` to the favorites, or take it out, where they are kept.
fn set(ui: &mut Ui, store: &mut Store, gif: FavoriteGif, favorite: bool) -> Vec<Intent> {
    if synced(store) {
        if store.gif_favorites().is_none() {
            return Vec::new();
        }
        let command = Command::SetGifFavorite { gif, favorite };
        // Shown at once; a refusal puts the list back.
        store.ahead(&command);
        return vec![Intent::Command(command)];
    }
    let list = &mut ui.messages.gif_favorites;
    let had = list.iter().any(|known| known.url == gif.url);
    match (favorite, had) {
        (true, false) if list.len() < MAX => list.push(gif),
        (false, true) => list.retain(|known| known.url != gif.url),
        _ => return Vec::new(),
    }
    vec![Intent::SaveSetting {
        key: KEY.into(),
        value: kept(ui),
    }]
}

/// A click on a heart or on a favorite of the GIF tab.
pub(crate) fn update(ui: &mut Ui, store: &mut Store, action: &UiAction) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    let rest = id.strip_prefix(composer_ids::EMOJI)?;
    let index = |value: &str| value.parse::<usize>().ok();
    // Taking one out of the list of favorites.
    if let Some(at) = rest.strip_prefix(":gif-unfavorite=").and_then(index) {
        let gif = shown(ui, store).get(at).cloned();
        return Some(gif.map_or_else(Vec::new, |gif| set(ui, store, gif, false)));
    }
    // The heart of a link used in this session.
    if let Some(at) = rest.strip_prefix(":gif-fav-recent=").and_then(index) {
        let Some(url) = ui.messages.gif_recent.get(at).cloned() else {
            return Some(Vec::new());
        };
        let favorite = !shown(ui, store).iter().any(|known| known.url == url);
        let gif = FavoriteGif {
            url,
            preview: None,
            width: 0,
            height: 0,
        };
        return Some(set(ui, store, gif, favorite));
    }
    // The heart of a search result: `gif:fav={request}:{index}`.
    if let Some(item) = rest.strip_prefix(":gif:fav=") {
        let Some(gif) = crate::gifs::favorite_of(ui, store, item) else {
            return Some(Vec::new());
        };
        let favorite = !shown(ui, store).iter().any(|known| known.url == gif.url);
        return Some(set(ui, store, gif, favorite));
    }
    None
}
