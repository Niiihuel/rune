//! The window's own title bar (barra-e-icono): who draws it, as the person
//! chose, and limn's minimize, maximize and close at the end of the top bar
//! while RUNE draws it. The host owns the window: it says whether the
//! buttons show ([`Desktop::window_buttons`](crate::desktop::Desktop)) and
//! whether the window is maximized, and answers [`Intent::Window`] with its
//! window commands.

use limn::{Component, Element, UiAction, WindowControls};
use rune_components::Look;

use crate::{Intent, Ui};

/// Who draws the window's title bar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TitleBar {
    /// RUNE's on Windows and X11, the system's on Wayland, where the
    /// compositor decorates as it likes.
    #[default]
    Automatic,
    /// RUNE's: no frame from the system, its buttons at the top bar's end.
    Rune,
    /// The system's frame around the window.
    System,
}

/// What the window's buttons ask of the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowAction {
    Minimize,
    /// Maximize (`true`) or restore.
    Maximize(bool),
    Close,
}

/// What the buttons' IDs are made from: `window:minimize`, `window:maximize`
/// and `window:close`.
pub const CONTROLS: &str = "window";

/// The room the buttons take at the end of the top bar; none while they do
/// not show.
pub fn room(ui: &Ui) -> f32 {
    if ui.desktop.window_buttons {
        WindowControls::WIDTH
    } else {
        0.0
    }
}

/// A click of one of the buttons.
pub(crate) fn update(ui: &Ui, action: &UiAction) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    let action = if *id == WindowControls::minimize_id(CONTROLS) {
        WindowAction::Minimize
    } else if *id == WindowControls::maximize_id(CONTROLS) {
        WindowAction::Maximize(!ui.desktop.maximized)
    } else if *id == WindowControls::close_id(CONTROLS) {
        WindowAction::Close
    } else {
        return None;
    };
    Some(vec![Intent::Window(action)])
}

/// `root` with the buttons at its top right, as tall as the headers and the
/// window's edge, over everything, as a system's title bar stays over the
/// app's dialogs. With an edge, `root` is inset by it and the buttons reach
/// the window's corner, as Windows draws them.
pub(crate) fn overlay(ui: &Ui, root: Element, look: &Look) -> Element {
    if !ui.desktop.window_buttons {
        return root;
    }
    let edge = ui.desktop.edge;
    let root = if edge > 0.0 {
        Element::stack(vec![root]).padding(edge)
    } else {
        root
    };
    let controls = WindowControls::new(CONTROLS, look.palette())
        .maximized(ui.desktop.maximized)
        .height(rune_components::sidebar::HEADER + edge)
        .render();
    Element::stack(vec![
        root,
        Element::column(vec![
            Element::row(vec![Element::spacer().flex(1.0), controls]),
            Element::spacer().flex(1.0),
        ]),
    ])
}
