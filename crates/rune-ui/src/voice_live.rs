//! The card of someone live beside their row under a voice channel in the
//! column (voz-filas-y-arrastre): as the official client's preview of a
//! stream, the pointer on the row shows who streams, what, the latest
//! picture of it (asked for as [`crate::video::stream_previews`] does) and
//! «Ver transmisión».

use std::time::Instant;

use rune_components::voice::{ids, LiveCard, Thumbnail};
use rune_i18n::t;
use rune_model::{ActivityKind, ChannelId, UserId};
use rune_state::Store;

use crate::{Intent, Ui};

/// The card of `user`, live in `channel` under the name `name`; `None`
/// while someone is carried to another channel.
pub(crate) fn card(
    ui: &Ui,
    store: &Store,
    channel: ChannelId,
    user: UserId,
    name: &str,
) -> Option<LiveCard> {
    if ui.voice_drag.is_some() {
        return None;
    }
    let simulated = crate::voice::simulated(store);
    let offered = store
        .provider
        .as_ref()
        .is_some_and(|info| info.capabilities.stream_previews);
    let picture = match store.stream_preview(channel, user) {
        _ if simulated => Ok(Thumbnail::Simulated),
        _ if !offered => Err(t("Vista previa no disponible").to_owned()),
        Some(Some(image)) => match ui.images.get(image) {
            Some(loaded) => Ok(Thumbnail::Image(loaded.clone())),
            // Its address did not load: asked for again on its own time.
            None if ui.image_failures.contains(image) => {
                Err(t("Vista previa no disponible").to_owned())
            }
            None => Err(t("Cargando la vista previa…").to_owned()),
        },
        Some(None) => Err(t("Vista previa no disponible").to_owned()),
        None => Err(t("Cargando la vista previa…").to_owned()),
    };
    let denied = store
        .abilities(channel)
        .and_then(|abilities| abilities.watch_stream.reason().map(str::to_owned));
    let watch = match denied {
        Some(reason) => Err(reason),
        None if store.call().is_some_and(|call| call.channel == channel) => Ok(()),
        None => Err(t("Entrá al canal de voz para ver la transmisión").to_owned()),
    };
    let activity = store
        .presence(user)
        .activities
        .iter()
        .find(|activity| activity.kind == ActivityKind::Playing)
        .map(|activity| activity.name.clone());
    Some(LiveCard {
        user: user.0,
        name: name.to_owned(),
        activity,
        picture,
        watch,
    })
}

/// «Ver transmisión» on the card: the stream is watched and the call's
/// stage shows it, as the official client's does; the focus goes to it, so
/// the card, held while a control of it has the focus, closes once the
/// pointer leaves.
pub(crate) fn watch(ui: &mut Ui, store: &mut Store, owner: UserId, now: Instant) -> Vec<Intent> {
    let mut intents = crate::video::watch(ui, store, owner, now);
    if store
        .watching()
        .iter()
        .any(|watching| watching.owner == owner)
    {
        ui.voice.stage = store.call().map(|call| call.channel);
        intents.push(Intent::Focus(ids::stream_tile(owner.0)));
    }
    intents
}
