//! A bounded channel inbox; it does not claim a complete mention history.

use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::Arc;

use limn::{Button, ButtonVariant, Component, Element, Key, ListRow, Modal, ScrollArea, UiAction};
use rune_components::{line, Look};
use rune_i18n::{t, tr};
use rune_model::{ChannelId, UserId};
use rune_state::Store;

use crate::{editor_id, Intent, Ui, ViewContext};

const MAX_ROWS: usize = 1000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Tab {
    Unread,
    Mentions,
}

#[derive(Debug)]
struct Row {
    channel: ChannelId,
    title: String,
    subtitle: String,
}

#[derive(Debug)]
pub(crate) struct Inbox {
    account: Option<UserId>,
    tab: Tab,
    revision: u64,
    rows: Arc<Vec<Row>>,
    limited: bool,
}

/// A live same-account bootstrap updates the background conversation without
/// dismissing the inbox opened from cached startup data.
pub(crate) fn on_ready(ui: &mut Ui, store: &mut Store) -> Vec<Intent> {
    let preserved = ui
        .inbox
        .take()
        .filter(|inbox| inbox.account.is_some() && inbox.account == store.me());
    let mut intents = crate::on_ready(ui, store);
    if let Some(inbox) = preserved {
        let focus = match inbox.tab {
            Tab::Unread => "inbox:unread",
            Tab::Mentions => "inbox:mentions",
        };
        ui.inbox = Some(inbox);
        refresh(ui, store);
        intents.retain(|intent| !matches!(intent, Intent::Focus(_) | Intent::FocusInput { .. }));
        intents.push(Intent::Focus(focus.into()));
    }
    intents
}

pub(crate) fn refresh(ui: &mut Ui, store: &Store) {
    let Some(inbox) = &mut ui.inbox else { return };
    if inbox.account != store.me() {
        ui.inbox = None;
        return;
    }
    if inbox.revision == store.revision() {
        return;
    }
    let mut newest = BinaryHeap::new();
    let mut count = 0;
    for channel in store.read_channels() {
        let read = store.read(channel.id);
        if match inbox.tab {
            Tab::Unread => !read.unread(),
            Tab::Mentions => read.mentions == 0,
        } {
            continue;
        }
        count += 1;
        newest.push(Reverse((read.last_message, channel.id)));
        if newest.len() > MAX_ROWS {
            newest.pop();
        }
    }
    let mut newest = newest.into_vec();
    newest.sort_unstable_by_key(|Reverse(key)| Reverse(*key));
    inbox.rows = Arc::new(
        newest
            .into_iter()
            .map(|Reverse((_, channel))| {
                let read = store.read(channel);
                let where_ = store
                    .channel(channel)
                    .and_then(|channel| channel.server)
                    .and_then(|server| store.server(server))
                    .map(|server| server.shown_name())
                    .unwrap_or(t("Conversación directa"));
                let anchor = if read.unread_from.is_some() {
                    t("Mensaje marcado en este equipo")
                } else if read.first_unread.is_some() {
                    t("Primer no leído conocido")
                } else if crate::reads::anchor(store, channel).is_some() {
                    t("Primer no leído cargado · alcance parcial")
                } else {
                    t("Sin ancla conocida · abrir recientes")
                };
                let mentions = if read.mentions == 1 {
                    t("1 mención conocida").to_owned()
                } else {
                    tr!("{count} menciones conocidas", count = read.mentions)
                };
                Row {
                    channel,
                    title: store.channel_title(channel),
                    subtitle: format!("{where_} · {mentions} · {anchor}"),
                }
            })
            .collect(),
    );
    inbox.limited = count > MAX_ROWS;
    inbox.revision = store.revision();
}

/// Write the rows again, as in another language.
pub(crate) fn rewrite(ui: &mut Ui, store: &Store) {
    if let Some(inbox) = &mut ui.inbox {
        inbox.revision = u64::MAX;
    }
    refresh(ui, store);
}

fn allowed(store: &Store, channel: ChannelId) -> bool {
    store.channel(channel).is_some_and(Store::visible)
        && store
            .abilities(channel)
            .is_some_and(|a| a.read_history.allowed())
}

pub(crate) fn close(ui: &mut Ui) -> Vec<Intent> {
    ui.inbox = None;
    ui.channel()
        .map(|channel| vec![Intent::Focus(editor_id(channel))])
        .unwrap_or_default()
}

pub(crate) fn key(ui: &mut Ui, key: Key) -> Option<Vec<Intent>> {
    (ui.inbox.is_some() && key == Key::Escape).then(|| close(ui))
}

pub(crate) fn update(ui: &mut Ui, store: &mut Store, action: &UiAction) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    if id == "inbox:open" {
        if crate::invitations::visible(ui)
            || ui.settings.is_some()
            || ui.server_settings.is_some()
            || ui.channel_settings.is_some()
        {
            return Some(Vec::new());
        }
        let mut intents = crate::search::dismiss(ui);
        intents.extend(crate::context::cancel(ui));
        intents.extend(crate::channel_list::close_pins(ui));
        ui.channel_list.menu = None;
        ui.channel_list.bell = None;
        ui.server_menu.open = false;
        ui.profile = None;
        ui.inbox = Some(Inbox {
            account: store.me(),
            tab: Tab::Unread,
            revision: u64::MAX,
            rows: Arc::default(),
            limited: false,
        });
        refresh(ui, store);
        return Some(intents);
    }
    if id == "inbox:close" {
        return Some(close(ui));
    }
    if id == "inbox:unread" || id == "inbox:mentions" {
        if let Some(inbox) = &mut ui.inbox {
            inbox.tab = if id == "inbox:unread" {
                Tab::Unread
            } else {
                Tab::Mentions
            };
            inbox.revision = u64::MAX;
            refresh(ui, store);
        }
        return Some(Vec::new());
    }
    if let Some(channel) = id
        .strip_prefix("reads:retry:")
        .and_then(|id| id.parse::<u64>().ok())
        .map(ChannelId)
    {
        return Some(
            if ui.channel() == Some(channel) && allowed(store, channel) {
                store
                    .retry_read_command(channel)
                    .map(Intent::Command)
                    .into_iter()
                    .collect()
            } else {
                Vec::new()
            },
        );
    }
    let (go, channel) = if let Some(id) = id.strip_prefix("inbox:go:") {
        (true, id)
    } else if let Some(id) = id.strip_prefix("inbox:read:") {
        (false, id)
    } else {
        return None;
    };
    let Some(channel) = channel.parse::<u64>().ok().map(ChannelId) else {
        return Some(Vec::new());
    };
    refresh(ui, store);
    if !allowed(store, channel)
        || !ui
            .inbox
            .as_ref()
            .is_some_and(|inbox| inbox.rows.iter().any(|row| row.channel == channel))
    {
        return Some(Vec::new());
    }
    if !go {
        let result = store
            .mark_read_command(channel)
            .map(Intent::Command)
            .into_iter()
            .collect();
        refresh(ui, store);
        return Some(result);
    }
    let anchor = crate::reads::anchor(store, channel);
    let server = store.channel(channel).unwrap().server;
    ui.inbox = None;
    Some(crate::open_at(ui, store, server, channel, anchor))
}

pub(crate) fn view(
    ui: &Ui,
    store: &Store,
    root: Element,
    look: &Look,
    context: &ViewContext,
) -> Element {
    let Some(inbox) = &ui.inbox else { return root };
    if inbox.account != store.me() {
        return root;
    }
    // Paint revalidates visibility as well: an old projection never exposes a
    // revoked row while the host is applying that frame's events.
    let rows = inbox.rows.clone();
    let indices: Vec<_> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| allowed(store, row.channel))
        .map(|(index, _)| index)
        .collect();
    let count = indices.len();
    let palette = look.menu_palette();
    let list = if count == 0 {
        line(t("No hay canales en este alcance."), 14.0, look.muted)
    } else {
        ScrollArea::uniform_list("inbox:rows", count, 76.0, move |index| {
            let row = &rows[indices[index]];
            let (channel, title, subtitle) = (row.channel, &row.title, &row.subtitle);
            Element::row(vec![
                ListRow::new(format!("inbox:go:{channel}"), title, palette)
                    .subtitle(subtitle)
                    .render()
                    .flex(1.0),
                Button::new(format!("inbox:read:{channel}"), t("Marcar leído"), palette)
                    .variant(ButtonVariant::Ghost)
                    .render()
                    .centered_y(),
            ])
            .gap(8.0)
            .size(None, Some(76.0))
        })
        .appearance(look.scrollbar(true))
        .render()
        .size(
            None,
            Some(
                (context.window.height - 300.0)
                    .clamp(76.0, 456.0)
                    .min(count.max(1) as f32 * 76.0),
            ),
        )
    };
    let title = if inbox.tab == Tab::Unread {
        t("Canales no leídos")
    } else {
        t("Canales con menciones")
    };
    let mut body = vec![Element::row(vec![
        Button::new("inbox:unread", t("No leídos"), palette).variant(if inbox.tab == Tab::Unread { ButtonVariant::Default } else { ButtonVariant::Ghost }).render(),
        Button::new("inbox:mentions", t("Menciones"), palette).variant(if inbox.tab == Tab::Mentions { ButtonVariant::Default } else { ButtonVariant::Ghost }).render(),
    ]).gap(8.0), line(title, 16.0, look.text),
    line(t("Sólo canales visibles y menciones conocidas de esta cuenta; no es un historial completo de menciones."), 12.0, look.muted), list];
    if inbox.limited {
        body.push(line(
            t("Se muestran hasta 1000 canales, los más recientes primero."),
            12.0,
            look.muted,
        ));
    }
    Modal::new(
        root,
        true,
        t("Bandeja"),
        t("No leídos y menciones por canal"),
        "inbox:close",
        palette,
    )
    .width(Modal::fit_width(context.window.width, 720.0))
    .body(Element::column(body).gap(8.0))
    .render()
}
