//! Reading follows actual visible anchors; reminders remain in the Store.

use limn::{Button, ButtonVariant, Component, Element};
use rune_components::{line, Look};
use rune_i18n::t;
use rune_model::{ChannelId, HistorySource, MessageId};
use rune_state::{Applied, Store};

use crate::{Intent, Ui};

pub(crate) fn anchor(store: &Store, channel: ChannelId) -> Option<MessageId> {
    let read = store.read(channel);
    read.unread_from.or(read.first_unread).or_else(|| {
        store
            .timeline(channel)?
            .messages()
            .iter()
            .find(|message| !message.delivery.is_local() && Some(message.id) > read.last_read)
            .map(|message| message.id)
    })
}

pub(crate) fn source(ui: &Ui, store: &Store, channel: ChannelId) -> HistorySource {
    if store
        .context_permit(channel, HistorySource::Provider)
        .allowed()
        || !ui.local_history_available
    {
        HistorySource::Provider
    } else {
        HistorySource::Cached
    }
}

pub(crate) fn opened(ui: &mut Ui, store: &Store, channel: ChannelId) {
    let read = store.read(channel);
    let anchor = read.unread().then(|| anchor(store, channel)).flatten();
    if let Some(anchor) = anchor {
        ui.new_marker.insert(channel, anchor);
    } else {
        ui.new_marker.remove(&channel);
    }
    ui.unread_anchor = anchor.map(|anchor| (channel, anchor));
}

pub(crate) fn reveal(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    let Some((channel, _)) = ui.unread_anchor else {
        return Vec::new();
    };
    if ui.channel() != Some(channel)
        || !store
            .abilities(channel)
            .is_some_and(|a| a.read_history.allowed())
    {
        ui.unread_anchor = None;
        return Vec::new();
    }
    if !store
        .timeline(channel)
        .is_some_and(|timeline| timeline.latest)
    {
        return Vec::new();
    }
    if modal_obscured(ui) {
        return Vec::new();
    }
    ui.unread_anchor = None;
    if !store.read(channel).unread() {
        ui.new_marker.remove(&channel);
        return Vec::new();
    }
    let Some(anchor) = anchor(store, channel) else {
        ui.new_marker.remove(&channel);
        return Vec::new();
    };
    ui.new_marker.insert(channel, anchor);
    let intents = crate::open_message(ui, store, channel, anchor, source(ui, store, channel));
    crate::context::automatic(ui);
    intents
}

pub(crate) fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied) -> Vec<Intent> {
    if let Applied::History { channel, .. } = applied {
        if ui.channel() == Some(*channel) {
            if ui.unread_anchor.is_none()
                && store.read(*channel).unread()
                && !ui.new_marker.contains_key(channel)
            {
                opened(ui, store, *channel);
            }
            return reveal(ui, store);
        }
    }
    if let Some(channel) = ui.channel() {
        if !store
            .abilities(channel)
            .is_some_and(|a| a.read_history.allowed())
        {
            ui.unread_anchor = None;
            ui.new_marker.remove(&channel);
        }
    }
    Vec::new()
}

pub(crate) fn obscured(ui: &Ui) -> bool {
    ui.unread_anchor.is_some() || modal_obscured(ui)
}

pub(crate) fn modal_obscured(ui: &Ui) -> bool {
    ui.inbox.is_some()
        || crate::attachment_pick_pending(ui)
        || ui.search.is_some()
        || ui.switcher.is_some()
        || ui.settings.is_some()
        || ui.server_settings.is_some()
        || ui.channel_settings.is_some()
        || ui.profiles.editor.is_some()
        || ui.confirm_delete.is_some()
        || ui.server_menu.dialog.is_some()
        || ui.channel_list.dialog.is_some()
        || ui.voice.stage.is_some()
        || ui.attachment_viewer.is_some()
        || ui.document_viewer.is_some()
        || crate::invitations::visible(ui)
        || crate::onboarding::visible(ui)
        || crate::events::visible(ui)
        || crate::threads::covering(ui)
        || ui.shortcuts
}

pub(crate) fn status(store: &Store, channel: ChannelId, look: &Look) -> Option<Element> {
    let read = store.read(channel);
    if !store.read_sync()
        || read.last_read <= read.confirmed_read
        || !store
            .abilities(channel)
            .is_some_and(|a| a.read_history.allowed())
    {
        return None;
    }
    let label = if read.sync_pending() {
        t("Confirmando la lectura…")
    } else {
        t("Lectura guardada en RUNE · sin confirmar")
    };
    let mut parts = vec![line(label, 12.0, look.muted).flex(1.0).centered_y()];
    if !read.sync_pending() {
        parts.push(
            Button::new(
                format!("reads:retry:{channel}"),
                if read.sync_failed() {
                    t("Reintentar")
                } else {
                    t("Confirmar lectura")
                },
                look.menu_palette(),
            )
            .variant(ButtonVariant::Ghost)
            .render(),
        );
    }
    Some(Element::row(parts).gap(8.0).padding(6.0))
}
