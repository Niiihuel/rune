//! The window's own shortcuts, read before limn's tree as the table of
//! `rune_ui::keybinds` binds them (atajos-y-navegacion): the history, the
//! channels above and below, the regions, the list of shortcuts and the
//! measurements. The mouse's side buttons always go back and forward.

use limn::{UiEvent, UiTree};
use rune_state::Store;
use rune_ui::keybinds::{self, Action};
use rune_ui::{Intent, Ui};

/// What the event asks of the window, when it is one of its shortcuts;
/// `None` lets the event go on to the interface and the tree.
pub fn dispatch(
    ui: &mut Ui,
    store: &mut Store,
    tree: &UiTree,
    event: &UiEvent,
) -> Option<Vec<Intent>> {
    let action = match event {
        UiEvent::PointerBack(_) => Action::Back,
        UiEvent::PointerForward(_) => Action::Forward,
        event => keybinds::pressed(&ui.prefs, event)?,
    };
    Some(match action {
        Action::Back | Action::Forward => rune_ui::travel(ui, store, action == Action::Back),
        Action::PreviousChannel | Action::NextChannel => {
            rune_ui::navigate(ui, store, action == Action::NextChannel)
        }
        // Ahead of the tree's own F6, to land on RUNE's control of a region.
        Action::NextRegion | Action::PreviousRegion => {
            rune_ui::cycle_focus(ui, tree, action == Action::PreviousRegion)
        }
        Action::Shortcuts => rune_ui::toggle_shortcuts(ui),
        Action::Metrics => {
            ui.show_metrics = !ui.show_metrics;
            Vec::new()
        }
        // The interface's own, where their field or list has the focus.
        _ => return None,
    })
}
