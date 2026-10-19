//! Navigation to an explicit message, independent of the composer's draft.

use limn::{Button, ButtonVariant, Component, Element, Key, Spinner};
use rune_components::{line, Look};
use rune_i18n::t;
use rune_model::{ChannelId, HistorySource, MessageId};
use rune_provider::{Command, Failure};
use rune_state::{Applied, Store};

use crate::{editor_id, timeline_id, Intent, Ui};

#[derive(Debug)]
pub(crate) struct Navigation {
    channel: ChannelId,
    request: u64,
    anchor: MessageId,
    source: HistorySource,
    phase: Phase,
    reveal: bool,
    automatic: bool,
}

#[derive(Debug)]
enum Phase {
    Loading,
    Ready,
    Failed(String),
}

pub(crate) fn cancel(ui: &mut Ui) -> Vec<Intent> {
    ui.context_navigation
        .take()
        .filter(|navigation| matches!(navigation.phase, Phase::Loading))
        .map(|navigation| vec![Intent::Command(Command::Cancel(navigation.request))])
        .unwrap_or_default()
}

pub fn open_message(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    anchor: MessageId,
    source: HistorySource,
) -> Vec<Intent> {
    if ui.channel() != Some(channel) {
        return Vec::new();
    }
    ui.unread_anchor = None;
    let loaded = store
        .timeline(channel)
        .is_some_and(|timeline| timeline.get(anchor).is_some());
    let permit = if loaded {
        store
            .abilities(channel)
            .map(|abilities| abilities.read_history)
    } else {
        Some(store.context_permit(channel, source))
    };
    let mut intents = cancel(ui);
    let request = ui.fresh_request();
    let denied = permit
        .as_ref()
        .and_then(|permit| permit.reason())
        .map(str::to_owned)
        .or_else(|| {
            permit
                .is_none()
                .then(|| t("Esta conversación ya no está disponible").into())
        })
        .or_else(|| {
            (source == HistorySource::Cached && !ui.local_history_available && !loaded)
                .then(|| rune_model::LocalReadError::Unavailable.label().into())
        });
    if let Some(reason) = denied {
        ui.context_navigation = Some(Navigation {
            channel,
            request,
            anchor,
            source,
            phase: Phase::Failed(reason),
            reveal: false,
            automatic: false,
        });
        return intents;
    }
    ui.context_navigation = Some(Navigation {
        channel,
        request,
        anchor,
        source,
        phase: if loaded { Phase::Ready } else { Phase::Loading },
        reveal: loaded,
        automatic: false,
    });
    if loaded {
        ui.inbox = None;
        intents.extend(crate::search::dismiss(ui));
        if let Some(index) = store
            .timeline(channel)
            .unwrap()
            .messages()
            .iter()
            .position(|message| message.id == anchor)
        {
            intents.push(Intent::ScrollToIndex {
                list: timeline_id(channel),
                index,
            });
        }
        intents.push(Intent::Focus(editor_id(channel)));
    } else {
        intents.push(Intent::LoadContext {
            request,
            channel,
            anchor,
            source,
        });
    }
    intents
}

pub(crate) fn on_applied(ui: &mut Ui, store: &Store, applied: &Applied) -> Vec<Intent> {
    let Some(navigation) = &ui.context_navigation else {
        return Vec::new();
    };
    if matches!(applied, Applied::Ready | Applied::Authentication)
        || ui.channel() != Some(navigation.channel)
        || !store
            .abilities(navigation.channel)
            .is_some_and(|abilities| abilities.read_history.allowed())
    {
        return cancel(ui);
    }
    match applied {
        Applied::Context {
            request,
            channel,
            anchor,
        } if (*request, *channel, *anchor)
            == (navigation.request, navigation.channel, navigation.anchor)
            && matches!(navigation.phase, Phase::Loading) =>
        {
            let navigation = ui.context_navigation.as_mut().unwrap();
            navigation.phase = Phase::Ready;
            navigation.reveal = true;
            let channel = navigation.channel;
            let anchor = navigation.anchor;
            let mut intents = crate::search::dismiss(ui);
            if let Some(index) = store.timeline(channel).and_then(|timeline| {
                timeline
                    .messages()
                    .iter()
                    .position(|message| message.id == anchor)
            }) {
                intents.push(Intent::ScrollToIndex {
                    list: timeline_id(channel),
                    index,
                });
            }
            intents.push(Intent::Focus(editor_id(channel)));
            intents
        }
        Applied::Failed {
            failure: Failure::Context { request, channel },
            reason,
            ..
        } if (*request, *channel) == (navigation.request, navigation.channel)
            && matches!(navigation.phase, Phase::Loading) =>
        {
            ui.context_navigation.as_mut().unwrap().phase = Phase::Failed(reason.clone());
            Vec::new()
        }
        _ => {
            if matches!(navigation.phase, Phase::Ready)
                && store
                    .timeline(navigation.channel)
                    .is_none_or(|timeline| timeline.get(navigation.anchor).is_none())
            {
                ui.context_navigation = None;
            }
            Vec::new()
        }
    }
}

pub(crate) fn after_layout(ui: &mut Ui, store: &Store, tree: &limn::UiTree) -> Vec<Intent> {
    let Some(navigation) = ui
        .context_navigation
        .as_mut()
        .filter(|navigation| matches!(navigation.phase, Phase::Ready) && navigation.reveal)
    else {
        return Vec::new();
    };
    let Some(index) = store.timeline(navigation.channel).and_then(|timeline| {
        timeline
            .messages()
            .iter()
            .position(|message| message.id == navigation.anchor)
    }) else {
        ui.context_navigation = None;
        return Vec::new();
    };
    if tree
        .visible_rows(&timeline_id(navigation.channel))
        .is_some_and(|rows| rows.contains(&index))
    {
        navigation.reveal = false;
        if navigation.automatic {
            ui.context_navigation = None;
            return vec![Intent::Repaint];
        }
        Vec::new()
    } else {
        vec![Intent::ScrollToIndex {
            list: timeline_id(navigation.channel),
            index,
        }]
    }
}

pub(crate) fn highlighted(ui: &Ui, channel: ChannelId) -> Option<MessageId> {
    ui.context_navigation
        .as_ref()
        .filter(|navigation| {
            navigation.channel == channel && matches!(navigation.phase, Phase::Ready)
        })
        .map(|navigation| navigation.anchor)
}

pub(crate) fn automatic(ui: &mut Ui) {
    if let Some(navigation) = &mut ui.context_navigation {
        navigation.automatic = true;
    }
}

pub(crate) fn key(ui: &mut Ui, key: Key, focused: Option<&str>) -> Option<Vec<Intent>> {
    if key != Key::Escape
        || crate::reads::modal_obscured(ui)
        || ui.profile.is_some()
        || ui.profiles.author.is_some()
        || ui.channel_list.topic.is_some()
        || ui.channel_list.menu.is_some()
        || ui.channel_list.bell.is_some()
        || ui.channel_list.more.is_some()
        || ui.channel_list.pins.is_some()
        || ui.server_menu.open
        || ui.messages.more.is_some()
        || ui.messages.context.is_some()
        || ui.reacting.is_some()
    {
        return None;
    }
    let navigation = ui.context_navigation.as_ref()?;
    if matches!(navigation.phase, Phase::Ready)
        && focused.is_some_and(|focused| focused != editor_id(navigation.channel))
    {
        return None;
    }
    ui.unread_anchor = None;
    Some(cancel(ui))
}

pub(crate) fn blocks_reading(ui: &Ui) -> bool {
    ui.context_navigation
        .as_ref()
        .is_some_and(|navigation| !matches!(navigation.phase, Phase::Ready) || navigation.reveal)
}

pub(crate) fn local_unavailable(ui: &mut Ui) -> Vec<Intent> {
    let Some(navigation) = ui.context_navigation.as_mut().filter(|navigation| {
        navigation.source == HistorySource::Cached && matches!(navigation.phase, Phase::Loading)
    }) else {
        return Vec::new();
    };
    navigation.phase = Phase::Failed(rune_model::LocalReadError::Unavailable.label().into());
    vec![Intent::Command(Command::Cancel(navigation.request))]
}

pub(crate) fn update(ui: &mut Ui, store: &Store, id: &str) -> Option<Vec<Intent>> {
    match id {
        "context:cancel" => {
            ui.unread_anchor = None;
            Some(cancel(ui))
        }
        "context:retry" => {
            let navigation = ui.context_navigation.as_ref()?;
            if !matches!(navigation.phase, Phase::Failed(_)) {
                return Some(Vec::new());
            }
            let (channel, anchor, source) =
                (navigation.channel, navigation.anchor, navigation.source);
            Some(open_message(ui, store, channel, anchor, source))
        }
        _ => None,
    }
}

pub(crate) fn status(ui: &Ui, look: &Look) -> Option<Element> {
    let navigation = ui.context_navigation.as_ref()?;
    let palette = look.menu_palette();
    match &navigation.phase {
        Phase::Ready => None,
        Phase::Loading => Some(
            Element::row(vec![
                Spinner::new("context:loading", look.accent).render(),
                line(t("Cargando el contexto del mensaje…"), 13.0, look.muted).flex(1.0),
                Button::new("context:cancel", t("Cancelar"), palette)
                    .variant(ButtonVariant::Ghost)
                    .render(),
            ])
            .gap(8.0),
        ),
        Phase::Failed(reason) => Some(
            Element::column(vec![
                line(reason, 13.0, look.danger),
                Button::new("context:retry", t("Volver a intentar"), palette)
                    .variant(ButtonVariant::Secondary)
                    .render(),
                Button::new("context:cancel", t("Cerrar"), palette)
                    .variant(ButtonVariant::Ghost)
                    .render(),
            ])
            .gap(6.0),
        ),
    }
}
