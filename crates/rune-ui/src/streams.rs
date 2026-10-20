//! The streams watched on the stage (transmisiones): one focused takes the
//! stage, the rest in a strip under it, as in the official client and
//! Serein; a click on a stream focuses it, or lets go of the focus, and so
//! does «Volver a la grilla» or Escape. A secondary click opens the
//! stream's own menu: its sound's volume and silence, apart from the
//! person's, focusing it, full screen, a window of its own, and «Dejar de
//! ver». What the host and the provider do with the pictures of streams
//! lives in [`crate::video`].

use limn::{Element, Point, UiAction};
use rune_components::voice::{self as parts, ids, MenuEntry};
use rune_components::Look;
use rune_i18n::t;
use rune_model::UserId;
use rune_state::Store;

use crate::{Intent, Ui};

/// The number after `prefix` in `id`.
fn number(id: &str, prefix: &str) -> Option<u64> {
    id.strip_prefix(prefix)?.parse().ok()
}

/// `owner`'s stream is watched.
fn watched(store: &Store, owner: UserId) -> bool {
    store
        .watching()
        .iter()
        .any(|watching| watching.owner == owner)
}

/// A click: any closes the stream's menu, items of other parts included;
/// a stream's tile and «Enfocar» focus it, or let go of the focus. `None`
/// for the rest, which other parts answer.
pub(crate) fn update(ui: &mut Ui, store: &Store, action: &UiAction) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    ui.video.menu = None;
    let owner = number(id, "vz:stream:").or_else(|| number(id, "vz:spotlight:"))?;
    let owner = UserId(owner);
    ui.video.spotlight =
        (watched(store, owner) && ui.video.spotlight != Some(owner)).then_some(owner);
    Some(Vec::new())
}

/// A secondary click on a stream watched opens its menu there; on anything
/// else the menu closes and `None` lets other parts answer.
pub(crate) fn secondary_click(
    ui: &mut Ui,
    store: &Store,
    target: &str,
    at: Point,
) -> Option<Vec<Intent>> {
    ui.video.menu = None;
    let owner = UserId(number(target, "vz:stream:")?);
    if !watched(store, owner) {
        return None;
    }
    ui.video.menu = Some((owner, (at.x, at.y)));
    ui.voice.menu = None;
    ui.voice.card = None;
    ui.voice.more = false;
    ui.channel_list.menu = None;
    ui.server_menu.open = false;
    Some(Vec::new())
}

/// The entries of the menu of `owner`'s stream.
pub(crate) fn menu_entries(ui: &Ui, store: &Store, owner: UserId) -> Vec<MenuEntry> {
    let item = |id: String, label: &str, checked: Option<bool>| {
        MenuEntry::Item(id, label.to_owned(), checked)
    };
    // The stream's sound has a volume of its own, apart from the person's.
    let sound = owner.0 | rune_media::mixer::STREAM_SOUND;
    let (volume, silenced) = ui.prefs.voice_volume(sound);
    let note =
        crate::voice::simulated(store).then(|| t("Simulado: la llamada no lleva audio").to_owned());
    let mut entries = vec![
        MenuEntry::Volume {
            user: sound,
            label: t("Volumen de la transmisión"),
            volume,
            note,
        },
        item(
            ids::local_mute(sound),
            t("Silenciar la transmisión"),
            Some(silenced),
        ),
        MenuEntry::Separator,
    ];
    let focused = ui.video.spotlight == Some(owner);
    entries.push(item(
        ids::spotlight(owner.0),
        if focused {
            t("Volver a la grilla")
        } else {
            t("Enfocar")
        },
        None,
    ));
    entries.push(item(ids::fullscreen(owner.0), t("Pantalla completa"), None));
    if !ui.video.windows.contains(&owner) {
        entries.push(item(ids::window(owner.0), t("Ventana aparte"), None));
    }
    entries.push(MenuEntry::Separator);
    entries.push(item(ids::stop_watching(owner.0), t("Dejar de ver"), None));
    entries
}

/// A stream's menu, over everything, while open on a stream still watched.
pub(crate) fn overlays(ui: &Ui, store: &Store, root: Element, look: &Look) -> Element {
    let Some((owner, at)) = ui.video.menu.filter(|(owner, _)| watched(store, *owner)) else {
        return root;
    };
    Element::stack(vec![
        root,
        parts::context_menu(at, menu_entries(ui, store, owner), look),
    ])
}
