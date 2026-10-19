//! Back and forward through the conversations visited, F6 between the
//! window's regions, and the list of keyboard shortcuts.

use limn::{
    Access, Component, DescriptionItem, DescriptionList, Element, FontFamily, HitKind, Kbd, Modal,
    Role, UiTree,
};
use rune_components::Look;
use rune_i18n::t;
use rune_model::{ChannelId, MessageId, ServerId, UserId};
use rune_state::Store;

use crate::keybinds::{self, Action, Group, ACTIONS, FIXED};
use crate::{editor_id, timeline_id, Intent, Ui};

/// Places kept in each direction; the oldest go first.
pub(crate) const LIMIT: usize = 50;
pub const SHORTCUTS_CLOSE: &str = "shortcuts:close";
pub const SHORTCUTS_OPEN: &str = "shortcuts:open";

/// A conversation as it was left.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Place {
    server: Option<ServerId>,
    pub(crate) channel: ChannelId,
    /// The first message in view; `None` at the end, which a return keeps.
    pub(crate) anchor: Option<MessageId>,
    /// How far that message's top was under the top of the view, negative
    /// once scrolled past: a return puts it there again, to the pixel.
    offset: f32,
    focus: Option<String>,
}

/// The places behind and ahead of the open one, for one account.
#[derive(Debug, Default)]
pub(crate) struct History {
    account: Option<UserId>,
    pub(crate) back: Vec<Place>,
    pub(crate) forward: Vec<Place>,
    /// The open conversation as of the last layout.
    here: Option<Place>,
    /// The control to focus again once the returned-to place is laid out.
    restore_focus: Option<String>,
    /// A move through the history is opening the place: do not record it.
    moving: bool,
    /// The place returned to, until its message stands where it stood:
    /// the layouts it has been waited for.
    restore_offset: Option<(Place, u8)>,
}

/// Layouts a returned-to message is waited for, and corrected in.
const RESTORE_LAYOUTS: u8 = 8;

impl History {
    /// Another account's places are not this one's. True when they were dropped.
    fn sync(&mut self, account: Option<UserId>) -> bool {
        let changed = self.account != account;
        if changed {
            *self = Self {
                account,
                ..Self::default()
            };
        }
        changed
    }
}

fn push(stack: &mut Vec<Place>, place: Place) {
    if stack.last().map(|last| last.channel) != Some(place.channel) {
        stack.push(place);
    }
    if stack.len() > LIMIT {
        stack.remove(0);
    }
}

/// Where the conversation `channel`, about to be left, was being read.
fn leaving(ui: &Ui, channel: ChannelId) -> Place {
    ui.history
        .here
        .clone()
        .filter(|here| here.channel == channel)
        .unwrap_or(Place {
            server: ui.server,
            channel,
            anchor: None,
            offset: 0.0,
            focus: None,
        })
}

/// Called by `open_at` before it changes the open conversation.
pub(crate) fn record(ui: &mut Ui, store: &Store, from: Option<ChannelId>, to: ChannelId) {
    // The conversation left after an account changed was the other account's.
    if ui.history.sync(store.me()) || ui.history.moving {
        return;
    }
    let Some(from) = from.filter(|from| *from != to) else {
        return;
    };
    let place = leaving(ui, from);
    push(&mut ui.history.back, place);
    ui.history.forward.clear();
}

fn reachable(store: &Store, place: &Place) -> bool {
    store
        .channel(place.channel)
        .is_some_and(|channel| Store::visible(channel) && channel.server == place.server)
}

/// Go back (Alt+Left, the mouse's back button) or forward. Places that are
/// gone or no longer visible are skipped and dropped.
pub fn travel(ui: &mut Ui, store: &mut Store, back: bool) -> Vec<Intent> {
    ui.history.sync(store.me());
    if crate::reads::modal_obscured(ui) {
        return Vec::new();
    }
    let current = ui.channel();
    let place = loop {
        let stack = if back {
            &mut ui.history.back
        } else {
            &mut ui.history.forward
        };
        match stack.pop() {
            Some(place) if reachable(store, &place) && Some(place.channel) != current => {
                break place
            }
            Some(_) => continue,
            None => return Vec::new(),
        }
    };
    if let Some(current) = current {
        let here = leaving(ui, current);
        let other = if back {
            &mut ui.history.forward
        } else {
            &mut ui.history.back
        };
        push(other, here);
    }
    ui.history.moving = true;
    let intents = crate::open_at(ui, store, place.server, place.channel, place.anchor);
    ui.history.moving = false;
    if place.anchor.is_some() {
        // Scrolled back to where it was, the place is not a highlighted jump.
        crate::context::automatic(ui);
    }
    ui.history.restore_focus = place.focus.clone();
    ui.history.restore_offset = place.anchor.is_some().then(|| (place.clone(), 0));
    // Until it is laid out, leaving it again keeps where it was.
    ui.history.here = Some(place);
    intents
}

/// Whether going back, or forward, would open somewhere: a place that is
/// still there, of this account.
pub(crate) fn can_travel(ui: &Ui, store: &Store, back: bool) -> bool {
    let stack = if back {
        &ui.history.back
    } else {
        &ui.history.forward
    };
    ui.history.account == store.me()
        && stack
            .iter()
            .any(|place| reachable(store, place) && Some(place.channel) != ui.channel())
}

/// What the conversation's header shows of the history: its arrows, and
/// the keys their hints and the search's name; smaller when `narrow`.
pub(crate) fn header(ui: &Ui, store: &Store, narrow: bool) -> rune_components::chrome::HeaderNav {
    rune_components::chrome::HeaderNav {
        back: can_travel(ui, store, true),
        forward: can_travel(ui, store, false),
        back_keys: keybinds::keys(&ui.prefs, Action::Back),
        forward_keys: keybinds::keys(&ui.prefs, Action::Forward),
        search_keys: keybinds::keys(&ui.prefs, Action::Search),
        compact: narrow,
        // The window's own buttons (barra-e-icono): the header is the
        // window's rightmost.
        end: crate::title_bar::room(ui),
    }
}

/// After a layout: note where the open conversation is being read, and
/// give back the focus of a place returned to.
pub(crate) fn after_layout(ui: &mut Ui, store: &Store, tree: &UiTree) -> Vec<Intent> {
    let mut intents = Vec::new();
    if let Some(id) = ui.history.restore_focus.take() {
        if focusable(tree, &id) {
            intents.push(Intent::Focus(id));
        }
    }
    let Some(channel) = ui.channel() else {
        return intents;
    };
    if crate::reads::obscured(ui) || crate::context::blocks_reading(ui) {
        return intents;
    }
    let list = timeline_id(channel);
    if let Some(correction) = restore_offset(ui, tree, channel, &list) {
        // The place is not where it was yet: it stays the one to leave.
        intents.extend(correction);
        return intents;
    }
    let anchor = match tree.visible_rows(&list) {
        Some(_) if tree.is_scroll_at_end(&list, 24.0) => None,
        Some(rows) => store.timeline(channel).and_then(|timeline| {
            timeline
                .messages()
                .get(rows.start)
                .map(|message| message.id)
        }),
        None => None,
    };
    // Going back to a control that leaves the place would mean nothing.
    let focus = tree
        .focused_id()
        .filter(|id| !id.starts_with("channel:") && !id.starts_with("server:") && *id != "home")
        .map(str::to_owned);
    let offset = anchor
        .and_then(|anchor| tree.anchor_offset(&message_anchor(anchor)))
        .unwrap_or(0.0);
    let here = Place {
        server: ui.server,
        channel,
        anchor,
        offset,
        focus,
    };
    if ui.history.here.as_ref() != Some(&here) {
        ui.history.here = Some(here);
    }
    intents
}

/// The anchor a message's row is named by in the timeline.
fn message_anchor(message: MessageId) -> String {
    format!("anchor:{message}")
}

/// Bring the message of the place returned to where it stood, once the
/// jump to its row is done: `Some` with what to do while it is not there
/// yet, `None` once it is, or when it gave up.
fn restore_offset(
    ui: &mut Ui,
    tree: &UiTree,
    channel: ChannelId,
    list: &str,
) -> Option<Vec<Intent>> {
    let (place, waited) = ui.history.restore_offset.take()?;
    let anchor = place.anchor.filter(|_| place.channel == channel)?;
    if waited >= RESTORE_LAYOUTS {
        return None;
    }
    let jumping = ui.context_navigation.is_some();
    let Some(now) = tree
        .anchor_offset(&message_anchor(anchor))
        .filter(|_| !jumping)
    else {
        ui.history.restore_offset = Some((place, waited + 1));
        return Some(Vec::new());
    };
    let off = now - place.offset;
    if off.abs() < 0.5 {
        return None;
    }
    let offset = tree.scroll_offset(list) + off;
    ui.history.restore_offset = Some((place, waited + 1));
    Some(vec![Intent::SetScrollOffset {
        list: list.to_owned(),
        offset,
    }])
}

fn is_control(kind: HitKind) -> bool {
    matches!(
        kind,
        HitKind::Button | HitKind::TextInput | HitKind::TextArea
    )
}

fn focusable(tree: &UiTree, id: &str) -> bool {
    tree.frame()
        .regions
        .iter()
        .any(|region| region.id == id && is_control(region.kind))
}

/// The landmarks of the window, in Discord's order, as F6 goes round them.
pub(crate) const RAIL: &str = "rail";
pub(crate) const CHANNELS: &str = "channels";
pub(crate) const COMPOSER: &str = "composer";
pub(crate) const MEMBERS: &str = "members";

/// Mark `element` as the landmark region `id`, named `name` for a screen reader.
pub(crate) fn region(element: Element, id: &str, name: &str) -> Element {
    element.accessible(Access::new(Role::Region).id(id).name(name))
}

/// F6 and Shift+F6: move the focus to the next or previous region of the
/// window, in Discord's order: servers, channels, messages, composer, members.
/// A region with nothing to focus is passed over. A dialog keeps the focus.
/// The host reads F6 before the tree does, so each region lands on RUNE's
/// own control (the selected server or channel, the newest message in view,
/// the composer's field) rather than on its first.
pub fn cycle_focus(ui: &Ui, tree: &UiTree, reverse: bool) -> Vec<Intent> {
    if tree.has_open_popover() || crate::reads::modal_obscured(ui) {
        return Vec::new();
    }
    let channel = ui.channel();
    let timeline = channel.map(timeline_id).unwrap_or_default();
    let messages = tree.focusables_within(&timeline);
    // The newest message in view, as Discord lands on it: a virtual list
    // only lays out the rows in view.
    let newest = messages.last().map(|id| (*id).to_owned());
    // Each region's controls, and the one to land on when it has it.
    let regions: Vec<(Vec<&str>, Option<String>)> = vec![
        (
            tree.focusables_within(RAIL),
            Some(match ui.server {
                Some(server) => format!("server:{server}"),
                None => "home".into(),
            }),
        ),
        (
            tree.focusables_within(CHANNELS),
            channel.map(|channel| format!("channel:{channel}")),
        ),
        (messages, newest),
        (tree.focusables_within(COMPOSER), channel.map(editor_id)),
        (tree.focusables_within(MEMBERS), None),
    ];
    let focused = tree.focused_id();
    let current = focused.and_then(|focused| {
        regions
            .iter()
            .position(|(inside, _)| inside.contains(&focused))
    });
    let count = regions.len();
    (1..=count)
        .map(|step| match (current, reverse) {
            (Some(current), false) => (current + step) % count,
            (Some(current), true) => (current + count - step) % count,
            (None, false) => step - 1,
            (None, true) => count - step,
        })
        .find_map(|index| {
            let (inside, preferred) = &regions[index];
            preferred
                .as_deref()
                .filter(|preferred| inside.contains(preferred))
                .or(inside.first().copied())
        })
        .map(|target| vec![Intent::Focus(target.to_owned())])
        .unwrap_or_default()
}

/// Ctrl+/ opens and closes the list of keyboard shortcuts, never over
/// another dialog.
pub fn toggle_shortcuts(ui: &mut Ui) -> Vec<Intent> {
    if ui.shortcuts || !crate::reads::modal_obscured(ui) {
        ui.shortcuts = !ui.shortcuts;
    }
    Vec::new()
}

/// The list of shortcuts (Ctrl+/): the table's, each part under its
/// title, with the keys chosen now; then the ones that do not change.
pub(crate) fn overlay(ui: &Ui, root: Element, look: &Look) -> Element {
    if !ui.shortcuts {
        return root;
    }
    let palette = look.menu_palette();
    let list = |items: Vec<DescriptionItem>| {
        DescriptionList::new(items, palette)
            .label_width(320.0)
            .render()
    };
    let title = |text: &str| {
        rune_components::strong_line(text, 14.0, look.text).font_family(FontFamily::SansBold)
    };
    let mut body = Vec::new();
    for group in Group::ALL {
        let items: Vec<DescriptionItem> = ACTIONS
            .into_iter()
            .filter(|action| action.group() == group)
            .filter_map(|action| {
                let keys = keybinds::keys(&ui.prefs, action)?;
                Some(DescriptionItem::with(
                    action.label(),
                    Kbd::new(keys, palette).render(),
                ))
            })
            .collect();
        if !items.is_empty() {
            body.extend([title(group.title()), list(items)]);
        }
    }
    let fixed = FIXED
        .iter()
        .map(|(what, keys)| {
            let mut chords = keys.split(' ').map(t);
            let mut kbd = Kbd::new(chords.next().unwrap_or_default(), palette);
            for chord in chords {
                kbd = kbd.alternative(chord);
            }
            DescriptionItem::with(t(what), kbd.render())
        })
        .collect();
    body.extend([title(t("Fijos")), list(fixed)]);
    Modal::new(
        root,
        true,
        t("Atajos de teclado"),
        t("Los botones laterales del mouse también van atrás y adelante. Se cambian en Ajustes, Sistema."),
        SHORTCUTS_CLOSE,
        palette,
    )
    .width(560.0)
    .height(640.0)
    .body(Element::column(body).gap(8.0))
    .render()
}
