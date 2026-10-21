//! The window's own frame (barra-e-icono), as Surco draws it: where the
//! system's would take a row of its own, RUNE draws none and limn's buttons
//! sit at the end of its top bar (`rune_ui::title_bar`). The bar's free
//! stretches move the window, a double click there maximizes or restores
//! it, and the edges resize it. The system's own move and resize keep its
//! snapping. Also the window's icon.

use eframe::egui::{self, vec2, CursorIcon, Pos2, Rect, ViewportCommand};
use egui::viewport::ResizeDirection;
use rune_ui::desktop::Desktop;
use rune_ui::title_bar::{TitleBar, WindowAction};

use crate::window;

/// How far in from the window's edge a press resizes it, while undecorated
/// and not maximized.
const EDGE: f32 = 5.0;
/// The narrowest the window gets with the system's frame: the conversation's
/// header folds its tools down to this.
const MIN_SIZE: [f32; 2] = [720.0, 480.0];
/// The window's usual size, which it opens at or grows to from the splash.
pub const SIZE: [f32; 2] = [1280.0, 800.0];
/// The window while the opening screen only says the stage it is in: a
/// small tile in the middle of the screen, as the official client's splash,
/// with no frame or buttons and not resizable.
pub const SPLASH_SIZE: [f32; 2] = [300.0, 340.0];
/// The value kept for RUNE's bar: what hiding the decorations kept before
/// it existed, so that choice holds.
const RUNE: &str = window::HIDDEN;
const SYSTEM: &str = "shown";

/// Who draws the window's frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Chrome {
    /// The system, around the window.
    #[default]
    System,
    /// RUNE: no frame from the system, its buttons at the top bar's end.
    Drawn,
}

impl Chrome {
    /// The narrowest the window gets: RUNE's buttons take room of their
    /// own at the header's end, and its edges their own around the app.
    pub fn min_size(self) -> egui::Vec2 {
        let [width, height] = MIN_SIZE;
        match self {
            Self::System => vec2(width, height),
            Self::Drawn => vec2(
                width + limn::WindowControls::WIDTH + 2.0 * EDGE,
                height + 2.0 * EDGE,
            ),
        }
    }

    /// For the person's choice, here.
    pub fn current(choice: TitleBar) -> Self {
        chrome(
            choice,
            cfg!(windows),
            cfg!(target_os = "macos"),
            std::env::var_os("WAYLAND_DISPLAY").is_some_and(|display| !display.is_empty()),
        )
    }
}

/// Automatic: RUNE's on Windows and on X11; a Wayland compositor decorates
/// the window as it likes, and macOS keeps its own.
fn chrome(choice: TitleBar, windows: bool, macos: bool, wayland: bool) -> Chrome {
    match choice {
        TitleBar::Rune => Chrome::Drawn,
        TitleBar::System => Chrome::System,
        TitleBar::Automatic if windows || !macos && !wayland => Chrome::Drawn,
        TitleBar::Automatic => Chrome::System,
    }
}

/// The person's choice, kept for before the window opens.
pub fn chosen() -> TitleBar {
    from_kept(window::remembered(window::DECORATIONS).as_deref())
}

fn from_kept(value: Option<&str>) -> TitleBar {
    match value {
        Some(RUNE) => TitleBar::Rune,
        Some(SYSTEM) => TitleBar::System,
        _ => TitleBar::Automatic,
    }
}

/// Keep `choice` for the next start; the automatic one is no file.
pub fn keep(choice: TitleBar) {
    window::remember(window::DECORATIONS, kept(choice).map(str::to_owned));
}

fn kept(choice: TitleBar) -> Option<&'static str> {
    match choice {
        TitleBar::Automatic => None,
        TitleBar::Rune => Some(RUNE),
        TitleBar::System => Some(SYSTEM),
    }
}

/// The window command for what one of its buttons asked.
pub fn command(action: WindowAction) -> ViewportCommand {
    match action {
        WindowAction::Minimize => ViewportCommand::Minimized(true),
        WindowAction::Maximize(maximized) => ViewportCommand::Maximized(maximized),
        // The system's close: RUNE saves first, or goes to the tray.
        WindowAction::Close => ViewportCommand::Close,
    }
}

/// RUNE's icon for its windows and the taskbar: the white rune on its amber
/// tile, as the person's theme of icons has it once installed. Decoded once:
/// a stream's own window asks for it every frame.
pub fn icon() -> std::sync::Arc<egui::IconData> {
    static ICON: std::sync::OnceLock<std::sync::Arc<egui::IconData>> = std::sync::OnceLock::new();
    ICON.get_or_init(|| {
        std::sync::Arc::new(
            eframe::icon_data::from_png_bytes(include_bytes!(
                "../../../assets/icons/hicolor/256x256/apps/rune.png"
            ))
            .unwrap_or_default(),
        )
    })
    .clone()
}

/// The frame between frames: who draws it, whether a press on the bar
/// already moved the window, and whether it is still the splash.
#[derive(Debug, Default)]
pub struct WindowFrame {
    pub chrome: Chrome,
    moved: bool,
    /// Opened at [`SPLASH_SIZE`] and not grown yet.
    pub compact: bool,
}

impl WindowFrame {
    pub fn new(chrome: Chrome, compact: bool) -> Self {
        Self {
            chrome,
            moved: false,
            compact,
        }
    }

    /// From the splash to the usual window, once: its frame as the person
    /// chose, resizable again, and grown around the splash's middle (the
    /// screen's, unless it was dragged), no bigger than the screen.
    pub fn grow(&mut self, ctx: &egui::Context) {
        if !self.compact {
            return;
        }
        self.compact = false;
        let (outer, monitor) = ctx.input(|input| {
            let viewport = input.viewport();
            (viewport.outer_rect, viewport.monitor_size)
        });
        let size = monitor.map_or(vec2(SIZE[0], SIZE[1]), |monitor| {
            vec2(SIZE[0], SIZE[1]).min(monitor * 0.9)
        });
        ctx.send_viewport_cmd(ViewportCommand::Decorations(self.chrome == Chrome::System));
        ctx.send_viewport_cmd(ViewportCommand::Resizable(true));
        ctx.send_viewport_cmd(ViewportCommand::InnerSize(size));
        ctx.send_viewport_cmd(ViewportCommand::MinInnerSize(self.chrome.min_size()));
        if let Some(outer) = outer {
            ctx.send_viewport_cmd(ViewportCommand::OuterPosition(outer.center() - size / 2.0));
        }
    }

    /// Where the app goes in `ui`'s room, and what its view must know: the
    /// buttons show while RUNE draws the frame, not in full screen nor as
    /// the splash. The edge of a window that is not maximized is painted in
    /// `rim` and resizes it.
    pub fn content(&self, ui: &egui::Ui, desktop: &mut Desktop, rim: limn::Color) -> Rect {
        let bounds = ui.max_rect();
        let (fullscreen, maximized) = ui.input(|input| {
            let viewport = input.viewport();
            (
                viewport.fullscreen.unwrap_or(false),
                viewport.maximized.unwrap_or(false),
            )
        });
        let drawn = self.chrome == Chrome::Drawn && !fullscreen && !self.compact;
        desktop.window_buttons = drawn;
        desktop.maximized = maximized;
        if !drawn || maximized {
            return bounds;
        }
        let rim = egui::Color32::from_rgba_unmultiplied(rim.r, rim.g, rim.b, rim.a);
        ui.painter().rect_filled(bounds, 0.0, rim);
        // An undecorated window has no border of the system's to resize by.
        let edge = ui.input(|input| {
            input
                .pointer
                .hover_pos()
                .and_then(|point| resize_direction(bounds, point))
                .map(|direction| (direction, input.pointer.primary_pressed()))
        });
        if let Some((direction, pressed)) = edge {
            ui.ctx().set_cursor_icon(match direction {
                ResizeDirection::North | ResizeDirection::South => CursorIcon::ResizeVertical,
                ResizeDirection::East | ResizeDirection::West => CursorIcon::ResizeHorizontal,
                ResizeDirection::NorthEast | ResizeDirection::SouthWest => CursorIcon::ResizeNeSw,
                ResizeDirection::NorthWest | ResizeDirection::SouthEast => CursorIcon::ResizeNwSe,
            });
            if pressed {
                ui.ctx()
                    .send_viewport_cmd(ViewportCommand::BeginResize(direction));
            }
        }
        bounds.shrink(EDGE)
    }

    /// After the input: a press on a free stretch of the top bar moves the
    /// window once dragged, and a double click there maximizes or restores
    /// it. `content` is where the app is; `on_control` says whether a
    /// point, from the app's corner, presses one of its controls (the
    /// window's buttons are some).
    pub fn caption(
        &mut self,
        ctx: &egui::Context,
        content: Rect,
        on_control: impl Fn(limn::Point) -> bool,
    ) {
        let (origin, at, dragging, double, down, maximized, fullscreen) = ctx.input(|input| {
            let pointer = &input.pointer;
            (
                pointer.press_origin(),
                pointer.latest_pos(),
                pointer.is_decidedly_dragging(),
                pointer.button_double_clicked(egui::PointerButton::Primary),
                pointer.primary_down(),
                input.viewport().maximized.unwrap_or(false),
                input.viewport().fullscreen.unwrap_or(false),
            )
        });
        if !down {
            self.moved = false;
        }
        if !self.compact && (self.chrome != Chrome::Drawn || fullscreen) {
            return;
        }
        // The splash has no frame and moves from anywhere but its controls.
        let bar = if self.compact {
            content
        } else {
            Rect::from_min_size(
                content.min,
                vec2(content.width(), rune_components::sidebar::HEADER),
            )
        };
        let free = |point: Pos2| {
            bar.contains(point)
                && !on_control(limn::Point::new(
                    point.x - content.min.x,
                    point.y - content.min.y,
                ))
        };
        // A double click is told on its release, when the press has no
        // origin left.
        if double && !self.compact && at.is_some_and(free) {
            ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
        } else if dragging && down && !self.moved && origin.is_some_and(free) {
            self.moved = true;
            ctx.send_viewport_cmd(ViewportCommand::StartDrag);
        }
    }
}

/// A press at `point` of the laid-out `frame` is one of the app's controls:
/// anything but a hint's anchor or a list's empty viewport.
pub fn on_control(frame: &limn::Frame, point: limn::Point) -> bool {
    frame
        .press_region(point)
        .is_some_and(|region| !matches!(region.kind, limn::HitKind::Hover | limn::HitKind::Scroll))
}

fn resize_direction(bounds: Rect, point: Pos2) -> Option<ResizeDirection> {
    if !bounds.contains(point) {
        return None;
    }
    let left = point.x < bounds.left() + EDGE;
    let right = point.x > bounds.right() - EDGE;
    let top = point.y < bounds.top() + EDGE;
    let bottom = point.y > bounds.bottom() - EDGE;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(ResizeDirection::NorthWest),
        (_, true, true, _) => Some(ResizeDirection::NorthEast),
        (true, _, _, true) => Some(ResizeDirection::SouthWest),
        (_, true, _, true) => Some(ResizeDirection::SouthEast),
        (true, _, _, _) => Some(ResizeDirection::West),
        (_, true, _, _) => Some(ResizeDirection::East),
        (_, _, true, _) => Some(ResizeDirection::North),
        (_, _, _, true) => Some(ResizeDirection::South),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::pos2;

    const WIDTH: f32 = 800.0;
    const HEIGHT: f32 = 600.0;
    const BAR: f32 = rune_components::sidebar::HEADER;

    struct Window {
        ctx: egui::Context,
        frame: WindowFrame,
        desktop: Desktop,
        maximized: bool,
        fullscreen: bool,
    }

    impl Window {
        fn new(chrome: Chrome) -> Self {
            Self {
                ctx: egui::Context::default(),
                frame: WindowFrame::new(chrome, false),
                desktop: Desktop::default(),
                maximized: false,
                fullscreen: false,
            }
        }

        /// One frame with `events`: where the app went, and the commands.
        fn frame(&mut self, events: Vec<egui::Event>) -> (Rect, Vec<ViewportCommand>) {
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(WIDTH, HEIGHT))),
                events,
                ..Default::default()
            };
            let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
            viewport.maximized = Some(self.maximized);
            viewport.fullscreen = Some(self.fullscreen);
            let mut content = Rect::NOTHING;
            let (frame, desktop) = (&mut self.frame, &mut self.desktop);
            let mut output = self.ctx.run_ui(input, |ui| {
                content = frame.content(ui, desktop, limn::Color::rgb(0, 0, 0));
                // A control of the app's sits at the bar's start, where the
                // arrows of the history are.
                frame.caption(ui.ctx(), content, |point| point.x < 60.0);
            });
            // No renderer here to take the font atlas.
            output.textures_delta.clear();
            let commands = std::mem::take(
                &mut output
                    .viewport_output
                    .get_mut(&egui::ViewportId::ROOT)
                    .unwrap()
                    .commands,
            );
            let commands = commands
                .into_iter()
                .filter(|command| !matches!(command, ViewportCommand::SetTheme(_)))
                .collect();
            (content, commands)
        }
    }

    fn button(point: Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn rune_draws_the_bar_on_windows_and_x11_and_the_system_on_wayland_unless_chosen() {
        use TitleBar::*;
        // (windows, macos, wayland)
        for (place, automatic) in [
            ((true, false, false), Chrome::Drawn),
            ((false, false, false), Chrome::Drawn),
            ((false, false, true), Chrome::System),
            ((false, true, false), Chrome::System),
        ] {
            let (windows, macos, wayland) = place;
            assert_eq!(
                chrome(Automatic, windows, macos, wayland),
                automatic,
                "{place:?}"
            );
            assert_eq!(chrome(Rune, windows, macos, wayland), Chrome::Drawn);
            assert_eq!(chrome(System, windows, macos, wayland), Chrome::System);
        }
    }

    #[test]
    fn the_window_keeps_room_for_rune_s_buttons() {
        assert_eq!(Chrome::System.min_size(), vec2(720.0, 480.0));
        assert_eq!(Chrome::Drawn.min_size(), vec2(868.0, 490.0));
    }

    #[test]
    fn the_choice_is_kept_where_hiding_the_decorations_was() {
        for choice in [TitleBar::Automatic, TitleBar::Rune, TitleBar::System] {
            assert_eq!(from_kept(kept(choice)), choice);
        }
        // Decorations hidden before RUNE drew its own bar: RUNE's.
        assert_eq!(from_kept(Some("hidden")), TitleBar::Rune);
        assert_eq!(from_kept(Some("anything")), TitleBar::Automatic);
    }

    #[test]
    fn the_buttons_ask_for_the_window_commands() {
        assert_eq!(
            command(WindowAction::Minimize),
            ViewportCommand::Minimized(true)
        );
        assert_eq!(
            command(WindowAction::Maximize(true)),
            ViewportCommand::Maximized(true)
        );
        assert_eq!(
            command(WindowAction::Maximize(false)),
            ViewportCommand::Maximized(false)
        );
        assert_eq!(command(WindowAction::Close), ViewportCommand::Close);
    }

    #[test]
    fn the_window_s_icon_is_rune_s_at_256_pixels() {
        let icon = icon();
        assert_eq!((icon.width, icon.height), (256, 256));
        assert_eq!(icon.rgba.len(), 256 * 256 * 4);
        assert_eq!(icon.rgba[3], 0, "a rounded corner is clear");
        let middle = (128 * 256 + 200) * 4;
        assert!(icon.rgba[middle + 3] > 240, "the tile");
        assert!(
            std::sync::Arc::ptr_eq(&icon, &super::icon()),
            "decoded once"
        );
    }

    #[test]
    fn the_buttons_show_while_rune_draws_the_frame_and_not_in_full_screen() {
        let mut window = Window::new(Chrome::Drawn);
        let (content, _) = window.frame(vec![]);
        assert!(window.desktop.window_buttons);
        assert_eq!(
            content,
            Rect::from_min_max(pos2(EDGE, EDGE), pos2(WIDTH - EDGE, HEIGHT - EDGE)),
            "room for the edges"
        );
        window.maximized = true;
        let (content, _) = window.frame(vec![]);
        assert!(window.desktop.window_buttons && window.desktop.maximized);
        assert_eq!(
            content,
            Rect::from_min_size(Pos2::ZERO, vec2(WIDTH, HEIGHT))
        );
        window.fullscreen = true;
        window.frame(vec![]);
        assert!(!window.desktop.window_buttons);
        let mut system = Window::new(Chrome::System);
        let (content, _) = system.frame(vec![]);
        assert!(!system.desktop.window_buttons);
        assert_eq!(
            content,
            Rect::from_min_size(Pos2::ZERO, vec2(WIDTH, HEIGHT))
        );
    }

    #[test]
    fn dragging_a_free_stretch_moves_the_window_and_a_control_does_not() {
        let mut window = Window::new(Chrome::Drawn);
        window.frame(vec![]);
        let free = pos2(300.0, EDGE + BAR / 2.0);
        window.frame(vec![egui::Event::PointerMoved(free), button(free, true)]);
        let (_, moved) = window.frame(vec![egui::Event::PointerMoved(free + vec2(20.0, 0.0))]);
        assert_eq!(moved, [ViewportCommand::StartDrag]);
        let (_, again) = window.frame(vec![egui::Event::PointerMoved(free + vec2(40.0, 0.0))]);
        assert!(again.is_empty(), "once a press");
        window.frame(vec![button(free + vec2(40.0, 0.0), false)]);
        // On one of the bar's own controls.
        let control = pos2(30.0, EDGE + BAR / 2.0);
        window.frame(vec![
            egui::Event::PointerMoved(control),
            button(control, true),
        ]);
        let (_, moved) = window.frame(vec![egui::Event::PointerMoved(control + vec2(20.0, 0.0))]);
        assert!(moved.is_empty(), "a press on a control is the app's");
        window.frame(vec![button(control + vec2(20.0, 0.0), false)]);
        // Under the bar, the app's own.
        let below = pos2(300.0, EDGE + BAR + 40.0);
        window.frame(vec![egui::Event::PointerMoved(below), button(below, true)]);
        let (_, moved) = window.frame(vec![egui::Event::PointerMoved(below + vec2(20.0, 0.0))]);
        assert!(moved.is_empty(), "below the bar");
        window.frame(vec![button(below + vec2(20.0, 0.0), false)]);
        // With the system's frame the bar is only the app's.
        let mut system = Window::new(Chrome::System);
        system.frame(vec![]);
        system.frame(vec![egui::Event::PointerMoved(free), button(free, true)]);
        let (_, moved) = system.frame(vec![egui::Event::PointerMoved(free + vec2(20.0, 0.0))]);
        assert!(moved.is_empty());
    }

    #[test]
    fn double_clicking_a_free_stretch_maximizes_and_restores() {
        for maximized in [false, true] {
            let mut window = Window::new(Chrome::Drawn);
            window.maximized = maximized;
            let point = pos2(300.0, BAR / 2.0 + EDGE);
            window.frame(vec![]);
            window.frame(vec![egui::Event::PointerMoved(point), button(point, true)]);
            window.frame(vec![button(point, false)]);
            window.frame(vec![button(point, true)]);
            let (_, released) = window.frame(vec![button(point, false)]);
            assert_eq!(released, [ViewportCommand::Maximized(!maximized)]);
        }
        // Not on a control.
        let mut window = Window::new(Chrome::Drawn);
        let control = pos2(30.0, BAR / 2.0 + EDGE);
        window.frame(vec![]);
        window.frame(vec![
            egui::Event::PointerMoved(control),
            button(control, true),
        ]);
        window.frame(vec![button(control, false)]);
        window.frame(vec![button(control, true)]);
        let (_, released) = window.frame(vec![button(control, false)]);
        assert!(released.is_empty());
    }

    #[test]
    fn the_splash_has_no_buttons_or_edges_and_moves_from_anywhere_until_it_grows() {
        for chrome in [Chrome::Drawn, Chrome::System] {
            let mut window = Window::new(chrome);
            window.frame.compact = true;
            let corner = pos2(WIDTH - 1.0, HEIGHT - 1.0);
            let (content, commands) = window.frame(vec![
                egui::Event::PointerMoved(corner),
                button(corner, true),
            ]);
            assert!(!window.desktop.window_buttons, "{chrome:?}");
            assert_eq!(
                content,
                Rect::from_min_size(Pos2::ZERO, vec2(WIDTH, HEIGHT))
            );
            assert!(commands.is_empty(), "no edge to resize by");
            window.frame(vec![button(corner, false)]);
            // Under where the bar would be, away from the app's controls.
            let middle = pos2(WIDTH / 2.0, HEIGHT / 2.0);
            window.frame(vec![
                egui::Event::PointerMoved(middle),
                button(middle, true),
            ]);
            let (_, moved) =
                window.frame(vec![egui::Event::PointerMoved(middle + vec2(20.0, 0.0))]);
            assert_eq!(moved, [ViewportCommand::StartDrag], "{chrome:?}");
            window.frame(vec![button(middle + vec2(20.0, 0.0), false)]);
        }
        // Grown, the frame is the person's again, once.
        let mut frame = WindowFrame::new(Chrome::Drawn, true);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| frame.grow(ui.ctx()));
        let commands = &output.viewport_output[&egui::ViewportId::ROOT].commands;
        assert!(!frame.compact);
        for command in [
            ViewportCommand::Decorations(false),
            ViewportCommand::Resizable(true),
            ViewportCommand::InnerSize(vec2(SIZE[0], SIZE[1])),
            ViewportCommand::MinInnerSize(Chrome::Drawn.min_size()),
        ] {
            assert!(commands.contains(&command), "{command:?} in {commands:?}");
        }
        let again = ctx.run_ui(egui::RawInput::default(), |ui| frame.grow(ui.ctx()));
        assert!(!again.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|command| matches!(command, ViewportCommand::InnerSize(_))));
    }

    #[test]
    fn the_edges_resize_a_window_that_is_not_maximized() {
        for (point, direction) in [
            (pos2(1.0, 1.0), ResizeDirection::NorthWest),
            (pos2(WIDTH / 2.0, 1.0), ResizeDirection::North),
            (pos2(WIDTH - 1.0, 1.0), ResizeDirection::NorthEast),
            (pos2(1.0, HEIGHT / 2.0), ResizeDirection::West),
            (pos2(WIDTH - 1.0, HEIGHT / 2.0), ResizeDirection::East),
            (pos2(1.0, HEIGHT - 1.0), ResizeDirection::SouthWest),
            (pos2(WIDTH / 2.0, HEIGHT - 1.0), ResizeDirection::South),
            (pos2(WIDTH - 1.0, HEIGHT - 1.0), ResizeDirection::SouthEast),
        ] {
            let mut window = Window::new(Chrome::Drawn);
            let (_, commands) =
                window.frame(vec![egui::Event::PointerMoved(point), button(point, true)]);
            assert_eq!(commands, [ViewportCommand::BeginResize(direction)]);
        }
        let corner = pos2(WIDTH - 1.0, HEIGHT - 1.0);
        for (chrome, maximized, fullscreen) in [
            (Chrome::Drawn, true, false),
            (Chrome::Drawn, false, true),
            (Chrome::System, false, false),
        ] {
            let mut window = Window::new(chrome);
            window.maximized = maximized;
            window.fullscreen = fullscreen;
            let (_, commands) = window.frame(vec![
                egui::Event::PointerMoved(corner),
                button(corner, true),
            ]);
            assert!(commands.is_empty(), "{chrome:?} {maximized} {fullscreen}");
        }
    }
}
