//! The window's own title bar (barra-e-icono): limn's buttons at the end of
//! the conversation's header, the room the header keeps for them, and what
//! their clicks ask of the host.

use super::*;
use crate::title_bar::WindowAction;

fn region(frame: &Frame, id: &str) -> Option<limn::Rect> {
    frame
        .regions
        .iter()
        .find(|region| region.id == id && region.kind == limn::HitKind::Button)
        .map(|region| region.rect)
}

#[test]
fn the_buttons_end_the_header_and_the_header_keeps_their_room() {
    let mut host = Host::new();
    host.open_channel("general");
    let frame = host.frame();
    assert_eq!(region(&frame, "window:close"), None, "the system's frame");
    let search = region(&frame, "message-search:open").unwrap();
    host.ui.desktop.window_buttons = true;
    let frame = host.frame();
    let width = limn::WindowControls::BUTTON_WIDTH;
    for (index, id) in ["window:minimize", "window:maximize", "window:close"]
        .into_iter()
        .enumerate()
    {
        assert_eq!(
            region(&frame, id),
            Some(limn::Rect::new(
                WINDOW.width - (3 - index) as f32 * width,
                0.0,
                width,
                rune_components::sidebar::HEADER,
            )),
            "{id}"
        );
    }
    // The search moved over by the buttons' room, whole.
    let moved = region(&frame, "message-search:open").unwrap();
    assert_eq!(moved.x, search.x - limn::WindowControls::WIDTH);
    assert_eq!(moved.width, search.width);
    // Named for a screen reader, the middle one for what it will do.
    let names = |frame: &Frame| -> Vec<String> {
        frame
            .access
            .iter()
            .filter(|node| node.id.starts_with("window:"))
            .map(|node| node.name.clone())
            .collect()
    };
    // limn's own labels, in the language the host gives it.
    let labels = limn::labels();
    assert_eq!(
        names(&frame),
        [labels.minimize, labels.maximize, labels.close]
    );
    host.ui.desktop.maximized = true;
    let frame = host.frame();
    assert_eq!(
        names(&frame),
        [labels.minimize, labels.restore, labels.close]
    );
}

#[test]
fn the_buttons_ask_the_host_for_its_window_commands() {
    let mut host = Host::new();
    host.ui.desktop.window_buttons = true;
    let click = |host: &mut Host, id: &str| {
        update(
            &mut host.ui,
            &mut host.store,
            &UiAction::Clicked { id: id.into() },
            host.now,
        )
    };
    assert_eq!(
        click(&mut host, "window:minimize"),
        [Intent::Window(WindowAction::Minimize)]
    );
    assert_eq!(
        click(&mut host, "window:maximize"),
        [Intent::Window(WindowAction::Maximize(true))]
    );
    host.ui.desktop.maximized = true;
    assert_eq!(
        click(&mut host, "window:maximize"),
        [Intent::Window(WindowAction::Maximize(false))]
    );
    assert_eq!(
        click(&mut host, "window:close"),
        [Intent::Window(WindowAction::Close)]
    );
    // Anything else of the same start is not theirs.
    assert!(!click(&mut host, "window:other")
        .iter()
        .any(|intent| matches!(intent, Intent::Window(_))));
}

/// No control of the view but the buttons is under the buttons, in a
/// window `window` wide.
fn assert_alone_in(frame: &Frame, window: f32) {
    let width = limn::WindowControls::WIDTH;
    let corner = limn::Rect::new(window - width, 0.0, width, rune_components::sidebar::HEADER);
    for region in &frame.regions {
        let control = matches!(
            region.kind,
            limn::HitKind::Button | limn::HitKind::TextInput | limn::HitKind::TextArea
        );
        let rect = region.rect;
        let overlaps = rect.x < corner.x + corner.width
            && corner.x < rect.x + rect.width
            && rect.y < corner.y + corner.height
            && corner.y < rect.y + rect.height;
        assert!(
            !control || region.id.starts_with("window:") || !overlaps,
            "{} under the buttons: {rect:?}",
            region.id
        );
    }
    assert!(region(frame, "window:close").is_some());
}

fn assert_alone(frame: &Frame) {
    assert_alone_in(frame, WINDOW.width);
}

#[test]
fn every_top_bar_keeps_the_buttons_room() {
    // The conversation's header.
    let mut host = Host::new();
    host.ui.desktop.window_buttons = true;
    host.open_channel("general");
    let frame = host.frame();
    assert_alone(&frame);
    // The narrowest window, its header's tools folded into «más»: the
    // host keeps it wider by the buttons' room.
    let narrow = Size::new(720.0 + limn::WindowControls::WIDTH, 480.0);
    let context = ViewContext {
        window: narrow,
        ..host.context()
    };
    host.tree.set_root(view(&host.ui, &host.store, &context));
    assert_alone_in(&host.tree.layout(narrow), narrow.width);
    // A voice channel's stage.
    let room = host.channel_named("Sala común");
    host.click(&format!("voice:{room}"));
    host.click(&format!("voice:{room}"));
    assert_eq!(host.ui.voice.stage, Some(room));
    let frame = host.frame();
    assert_alone(&frame);
    let chat = region(&frame, rune_components::voice::ids::CHAT).unwrap();
    assert!(chat.x + chat.width <= WINDOW.width - limn::WindowControls::WIDTH);
    // The screen to sign in.
    let mut host = Host::new();
    host.store = Store::new();
    host.store.provider = Some(rune_provider::ProviderInfo {
        name: "Servicio autorizado de prueba".into(),
        kind: rune_provider::ProviderKind::Official,
        capabilities: rune_provider::Capabilities {
            authentication: true,
            ..rune_provider::Capabilities::default()
        },
        notice: String::new(),
    });
    host.event(Event::Authentication(
        rune_provider::AuthenticationState::SignedOut,
    ));
    host.ui.desktop.window_buttons = true;
    assert_alone(&host.frame());
}

#[test]
fn the_buttons_reach_the_corner_over_the_window_s_edge() {
    let mut host = Host::new();
    host.open_channel("general");
    host.ui.desktop.window_buttons = true;
    let close = |frame: &Frame| region(frame, "window:close").unwrap();
    let width = limn::WindowControls::BUTTON_WIDTH;
    let frame = host.frame();
    assert_eq!(
        close(&frame),
        limn::Rect::new(
            WINDOW.width - width,
            0.0,
            width,
            rune_components::sidebar::HEADER
        ),
    );
    // While RUNE draws the window's edge, the app is inset by it and the
    // buttons take its rim too.
    host.ui.desktop.edge = 5.0;
    let frame = host.frame();
    assert_eq!(
        close(&frame),
        limn::Rect::new(
            WINDOW.width - width,
            0.0,
            width,
            rune_components::sidebar::HEADER + 5.0
        ),
    );
    for region in frame
        .regions
        .iter()
        .filter(|region| region.kind == limn::HitKind::Button && !region.id.starts_with("window:"))
    {
        assert!(
            region.rect.x >= 5.0 && region.rect.y >= 5.0,
            "{}: {:?}",
            region.id,
            region.rect
        );
    }
    let clicked = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "window:close".into(),
        },
        host.now,
    );
    assert_eq!(clicked, [Intent::Window(WindowAction::Close)]);
}
