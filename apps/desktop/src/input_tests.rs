//! Ordered native snapshots exercise the actual desktop consumer without a window.

use super::*;

struct InputHost {
    ctx: egui::Context,
    app: App,
    area: egui::Rect,
    a: ChannelId,
    b: ChannelId,
}

impl InputHost {
    fn new(a_draft: &str, b_draft: &str) -> Self {
        let ctx = egui::Context::default();
        let now = Instant::now();
        let provider = Arc::new(MockProvider::new(MockConfig {
            latency: Duration::ZERO,
            ambient: false,
            replies: false,
            ..MockConfig::default()
        }));
        let snapshot = provider.snapshot();
        let mut app = App::new(
            &ctx,
            now,
            Arguments {
                screenshot: Some("unused-headless-ordered-input.png".into()),
                ..Arguments::default()
            },
            provider,
            None,
        );
        // These tests exercise local routing, not a Client/provider round trip.
        app.store
            .apply(rune_provider::Event::Ready(Box::new(snapshot)), now);
        let (a, b) = app
            .store
            .servers()
            .iter()
            .find_map(|server| {
                let order = app
                    .store
                    .channel_tree(server.id)
                    .into_iter()
                    .flat_map(|(_, channels)| channels)
                    .filter(|channel| channel.kind.has_messages())
                    .collect::<Vec<_>>();
                order
                    .windows(2)
                    .find(|pair| {
                        pair.iter().all(|channel| {
                            Store::visible(channel)
                                && app
                                    .store
                                    .abilities(channel.id)
                                    .is_some_and(|abilities| abilities.write.allowed())
                        })
                    })
                    .map(|pair| (pair[0].id, pair[1].id))
            })
            .expect("two adjacent writable simulated conversations");
        let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
        let window = Size::new(area.width(), area.height());
        app.tree.set_reduced_motion(true);
        app.tree
            .set_text_measurer(|text, size, _| text.chars().count() as f32 * size * 0.5);
        for (channel, draft) in [(b, b_draft), (a, a_draft)] {
            let server = app.store.channel(channel).unwrap().server;
            let intents = rune_ui::open(&mut app.ui, &mut app.store, server, channel);
            app.run(&ctx, intents);
            let intents = rune_ui::update(
                &mut app.ui,
                &mut app.store,
                &limn::UiAction::InputChanged {
                    id: rune_ui::editor_id(channel),
                    value: draft.into(),
                },
                now,
            );
            app.run(&ctx, intents);
            let context = app.context(window, now);
            app.tree
                .set_root(rune_ui::view(&app.ui, &app.store, &context));
            app.tree.layout(window);
        }
        assert!(app.tree.focus(&rune_ui::editor_id(a)));
        app.focus = None;
        Self {
            ctx,
            app,
            area,
            a,
            b,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>, modifiers: egui::Modifiers) {
        self.frame_with_focus(events, modifiers, true);
    }

    fn frame_with_focus(
        &mut self,
        mut events: Vec<egui::Event>,
        modifiers: egui::Modifiers,
        focused: bool,
    ) {
        events.push(egui::Event::ModifiersChanged(modifiers));
        let ctx = self.ctx.clone();
        let mut output = ctx.run_ui(
            egui::RawInput {
                events,
                focused,
                screen_rect: Some(self.area),
                ..Default::default()
            },
            |_| {
                limn_egui::normalize_keyboard_input(&ctx);
                self.app
                    .handle_input(&ctx, self.area, Instant::now(), false, false);
            },
        );
        output.textures_delta.clear();
    }

    fn drafts(&self, a: &str, b: &str) {
        for (channel, expected) in [(self.a, a), (self.b, b)] {
            assert_eq!(
                self.app.ui.drafts.get(&channel).map(String::as_str),
                Some(expected)
            );
            assert_eq!(
                self.app.tree.input_value(&rune_ui::editor_id(channel)),
                Some(expected)
            );
        }
    }

    fn focused(&self, channel: ChannelId) {
        assert_eq!(self.app.ui.channel(), Some(channel));
        assert_eq!(
            self.app.tree.focused_id(),
            Some(rune_ui::editor_id(channel).as_str())
        );
    }
}

fn native_key(key: egui::Key, pressed: bool, alt: bool, ctrl: bool) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed,
        repeat: false,
        modifiers: egui::Modifiers {
            alt,
            ctrl,
            ..Default::default()
        },
    }
}

fn alt_key(key: egui::Key) -> egui::Event {
    native_key(key, true, true, false)
}

fn alt_modifiers() -> egui::Modifiers {
    egui::Modifiers {
        alt: true,
        ..Default::default()
    }
}

#[test]
fn ordered_input_keeps_text_before_and_after_channel_navigation() {
    for (events, expected_a, expected_b) in [
        (
            vec![
                egui::Event::Text(" antes".into()),
                alt_key(egui::Key::ArrowDown),
                egui::Event::Text(" después".into()),
            ],
            "alpha antes",
            "beta después",
        ),
        (
            vec![
                egui::Event::Text(" antes".into()),
                alt_key(egui::Key::ArrowDown),
            ],
            "alpha antes",
            "beta",
        ),
        (
            vec![
                alt_key(egui::Key::ArrowDown),
                egui::Event::Text(" después".into()),
            ],
            "alpha",
            "beta después",
        ),
    ] {
        let mut host = InputHost::new("alpha", "beta");
        host.frame(events, alt_modifiers());
        host.drafts(expected_a, expected_b);
        host.focused(host.b);
    }
}

#[test]
fn ordered_input_uses_each_keys_modifiers_and_preserves_channel_aba() {
    let mut host = InputHost::new("alpha", "beta");
    host.frame(
        vec![
            alt_key(egui::Key::ArrowDown),
            egui::Event::Text(" b".into()),
        ],
        egui::Modifiers::default(),
    );
    host.focused(host.b);
    host.drafts("alpha", "beta b");

    let mut host = InputHost::new("alpha", "beta");
    host.frame(
        vec![
            alt_key(egui::Key::ArrowDown),
            egui::Event::Text(" b".into()),
            alt_key(egui::Key::ArrowUp),
            egui::Event::Text(" a".into()),
        ],
        alt_modifiers(),
    );
    host.focused(host.a);
    host.drafts("alpha a", "beta b");

    let mut host = InputHost::new("alpha", "top\nbottom");
    host.frame(
        vec![
            alt_key(egui::Key::ArrowDown),
            native_key(egui::Key::ArrowUp, true, false, false),
        ],
        alt_modifiers(),
    );
    host.focused(host.b);
    host.drafts("alpha", "top\nbottom");
    assert_eq!(
        host.app.tree.input_selection(&rune_ui::editor_id(host.b)),
        Some((3, 3)),
        "the unmodified arrow reaches the new multiline composer"
    );
}

#[test]
fn ordered_input_focuses_a_new_intercept_field_before_the_next_text() {
    let mut host = InputHost::new("alpha", "beta");
    host.frame(
        vec![
            native_key(egui::Key::K, true, false, true),
            egui::Event::Text("laboratorio".into()),
        ],
        egui::Modifiers::default(),
    );
    assert_eq!(
        host.app
            .ui
            .switcher
            .as_ref()
            .map(|switcher| switcher.query.as_str()),
        Some("laboratorio")
    );
    assert_eq!(host.app.tree.focused_id(), Some("switcher:query"));
    assert_eq!(
        host.app.tree.input_value("switcher:query"),
        Some("laboratorio")
    );
    host.drafts("alpha", "beta");
}

#[test]
fn ordered_input_focuses_a_clicked_channel_before_the_next_text() {
    let mut host = InputHost::new("alpha", "beta");
    let id = format!("channel:{}", host.b.0);
    let rect = host
        .app
        .tree
        .frame()
        .regions
        .iter()
        .rev()
        .find(|region| region.id == id && region.kind == limn::HitKind::Button)
        .expect("the next conversation is visible in the channel list")
        .rect;
    let position = egui::Pos2::new(
        host.area.min.x + rect.x + rect.width / 2.0,
        host.area.min.y + rect.y + rect.height / 2.0,
    );
    host.frame(
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
            egui::Event::Text(" en B".into()),
        ],
        egui::Modifiers::default(),
    );
    host.focused(host.b);
    host.drafts("alpha", "beta en B");
}

#[test]
fn ordered_input_applies_f12_per_press_and_does_not_fall_back_from_missing_focus() {
    let mut host = InputHost::new("alpha", "beta");
    let before = host.app.ui.show_metrics;
    host.frame(
        vec![
            native_key(egui::Key::F12, true, false, false),
            native_key(egui::Key::F12, false, false, false),
            native_key(egui::Key::F12, true, false, false),
        ],
        egui::Modifiers::default(),
    );
    assert_eq!(
        host.app.ui.show_metrics, before,
        "two presses toggle twice in their source order"
    );
    host.frame(
        vec![native_key(egui::Key::F12, false, false, false)],
        egui::Modifiers::default(),
    );
    assert_eq!(host.app.ui.show_metrics, before);
    host.frame(
        vec![native_key(egui::Key::F12, true, false, false)],
        egui::Modifiers::default(),
    );
    assert_eq!(
        host.app.ui.show_metrics, !before,
        "one press toggles metrics"
    );
    host.frame(
        vec![native_key(egui::Key::F12, false, false, false)],
        egui::Modifiers::default(),
    );
    assert_eq!(host.app.ui.show_metrics, !before);
    host.drafts("alpha", "beta");

    for intent in [
        Intent::Focus("missing:input".into()),
        Intent::FocusInput {
            id: "missing:input".into(),
            cursor: 3,
        },
    ] {
        let mut host = InputHost::new("alpha", "beta");
        host.app.run(&host.ctx, vec![intent]);
        host.frame(
            vec![egui::Event::Text("must not reach the old field".into())],
            egui::Modifiers::default(),
        );
        host.drafts("alpha", "beta");
        assert!(host.app.tree.focused_id().is_none());
        assert!(host.app.focus.is_none());
        host.app.run(
            &host.ctx,
            vec![Intent::FocusInput {
                id: rune_ui::editor_id(host.a),
                cursor: 5,
            }],
        );
        host.frame(
            vec![egui::Event::Text("!".into())],
            egui::Modifiers::default(),
        );
        host.focused(host.a);
        host.drafts("alpha!", "beta");
    }
}

#[test]
fn ordered_input_preserves_window_focus_cuts_even_while_closing() {
    for closing in [false, true] {
        let mut host = InputHost::new("alpha", "beta");
        let target = host
            .app
            .tree
            .capture_paste_target()
            .expect("the laid-out focused composer");
        let metrics = host.app.ui.show_metrics;
        let mut events = vec![egui::Event::WindowFocused(false)];
        if closing {
            assert!(host.app.pending_storage.begin_close([]));
            events.extend([
                egui::Event::Text("ignored while closing".into()),
                alt_key(egui::Key::ArrowDown),
                native_key(egui::Key::F12, true, false, false),
            ]);
        }
        events.push(egui::Event::WindowFocused(true));
        host.frame(events, alt_modifiers());
        host.focused(host.a);
        host.drafts("alpha", "beta");
        assert_eq!(host.app.ui.show_metrics, metrics);
        assert!(
            host.app
                .tree
                .handle_event(UiEvent::PasteCaptured {
                    target,
                    text: "stale".into(),
                })
                .is_empty(),
            "a focus ABA invalidates the old target, including while input is frozen"
        );
        host.drafts("alpha", "beta");
        if closing {
            assert!(host.app.pending_storage.closing);
            assert!(!host.app.pending_storage.closed);
        } else {
            let fresh = host
                .app
                .tree
                .capture_paste_target()
                .expect("a fresh target after focus returns");
            assert_eq!(
                host.app.tree.handle_event(UiEvent::PasteCaptured {
                    target: fresh,
                    text: "!".into()
                }),
                vec![limn::UiAction::InputChanged {
                    id: rune_ui::editor_id(host.a),
                    value: "alpha!".into()
                }]
            );
            assert_eq!(
                host.app.tree.input_value(&rune_ui::editor_id(host.a)),
                Some("alpha!")
            );
        }
    }
}

#[test]
fn ordered_input_uses_frame_focus_when_the_snapshot_has_no_focus_events() {
    let mut host = InputHost::new("alpha", "beta");
    let target = host
        .app
        .tree
        .capture_paste_target()
        .expect("the initially focused, laid-out composer");
    host.frame_with_focus(Vec::new(), egui::Modifiers::default(), false);
    assert!(!host.ctx.input(|input| input.focused));
    assert!(host.ctx.input(|input| input
        .events
        .iter()
        .all(|event| !matches!(event, egui::Event::WindowFocused(_)))));
    assert!(
        host.app.tree.capture_paste_target().is_none(),
        "the frame's unfocused state also blocks capture without a native focus event"
    );
    host.drafts("alpha", "beta");

    host.frame_with_focus(Vec::new(), egui::Modifiers::default(), true);
    host.focused(host.a);
    assert!(
        host.app
            .tree
            .handle_event(UiEvent::PasteCaptured {
                target,
                text: "stale".into(),
            })
            .is_empty(),
        "focus restoration cannot revive a target from before the cut"
    );
    host.drafts("alpha", "beta");
    let fresh = host
        .app
        .tree
        .capture_paste_target()
        .expect("a fresh target after frame focus returns");
    assert_eq!(
        host.app.tree.handle_event(UiEvent::PasteCaptured {
            target: fresh,
            text: "!".into(),
        }),
        vec![limn::UiAction::InputChanged {
            id: rune_ui::editor_id(host.a),
            value: "alpha!".into(),
        }]
    );
    assert_eq!(
        host.app.tree.input_value(&rune_ui::editor_id(host.a)),
        Some("alpha!")
    );
}

#[test]
fn ordered_input_reopens_the_switcher_with_a_fresh_query_for_keyboard_and_click() {
    for reopen_by_click in [false, true] {
        let mut host = InputHost::new("alpha", "beta");
        host.frame(
            vec![
                native_key(egui::Key::K, true, false, true),
                egui::Event::Text("vieja".into()),
            ],
            egui::Modifiers::default(),
        );
        assert_eq!(
            host.app
                .ui
                .switcher
                .as_ref()
                .map(|switcher| switcher.query.as_str()),
            Some("vieja"),
        );
        assert_eq!(host.app.tree.input_value("switcher:query"), Some("vieja"));
        host.frame(
            vec![native_key(egui::Key::Escape, true, false, false)],
            egui::Modifiers::default(),
        );
        assert!(host.app.ui.switcher.is_none());
        host.drafts("alpha", "beta");

        if reopen_by_click {
            let intents = rune_ui::update(
                &mut host.app.ui,
                &mut host.app.store,
                &limn::UiAction::Clicked {
                    id: "switcher:open".into(),
                },
                Instant::now(),
            );
            host.app.run(&host.ctx, intents);
            host.frame(
                vec![egui::Event::Text("nueva".into())],
                egui::Modifiers::default(),
            );
        } else {
            host.frame(
                vec![
                    native_key(egui::Key::K, true, false, true),
                    egui::Event::Text("nueva".into()),
                ],
                egui::Modifiers::default(),
            );
        }
        assert_eq!(
            host.app
                .ui
                .switcher
                .as_ref()
                .map(|switcher| switcher.query.as_str()),
            Some("nueva"),
            "the reopened query is fresh; click route: {reopen_by_click}",
        );
        assert_eq!(host.app.tree.input_value("switcher:query"), Some("nueva"));
        assert_eq!(host.app.tree.focused_id(), Some("switcher:query"));
        host.drafts("alpha", "beta");
    }
}

fn side_button(button: egui::PointerButton) -> egui::Event {
    egui::Event::PointerButton {
        pos: egui::pos2(600.0, 400.0),
        button,
        pressed: true,
        modifiers: egui::Modifiers::default(),
    }
}

fn key_with(key: egui::Key, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

#[test]
fn ordered_input_goes_back_and_forward_with_alt_arrows_and_side_buttons() {
    // The host opened b, then a: b is behind.
    let mut host = InputHost::new("alpha", "beta");
    host.frame(
        vec![
            alt_key(egui::Key::ArrowLeft),
            egui::Event::Text(" b".into()),
        ],
        alt_modifiers(),
    );
    host.focused(host.b);
    host.drafts("alpha", "beta b");
    host.frame(
        vec![
            alt_key(egui::Key::ArrowRight),
            egui::Event::Text(" a".into()),
        ],
        alt_modifiers(),
    );
    host.focused(host.a);
    host.drafts("alpha a", "beta b");
    host.frame(
        vec![side_button(egui::PointerButton::Extra1)],
        egui::Modifiers::default(),
    );
    host.frame(Vec::new(), egui::Modifiers::default());
    host.focused(host.b);
    host.frame(
        vec![side_button(egui::PointerButton::Extra2)],
        egui::Modifiers::default(),
    );
    host.frame(Vec::new(), egui::Modifiers::default());
    host.focused(host.a);
}

#[test]
fn ordered_input_moves_between_regions_with_f6_and_lists_shortcuts_with_ctrl_slash() {
    let mut host = InputHost::new("alpha", "beta");
    let none = egui::Modifiers::default();
    host.frame(vec![key_with(egui::Key::F6, none)], none);
    host.frame(Vec::new(), none);
    let after = host.app.tree.focused_id().map(str::to_owned);
    assert!(
        after.is_some() && after.as_deref() != Some(rune_ui::editor_id(host.a).as_str()),
        "F6 leaves the composer for the next region: {after:?}"
    );
    let shift = egui::Modifiers::SHIFT;
    host.frame(vec![key_with(egui::Key::F6, shift)], shift);
    host.frame(Vec::new(), none);
    host.focused(host.a);
    let command = egui::Modifiers::COMMAND;
    host.frame(vec![key_with(egui::Key::Slash, command)], command);
    assert!(host.app.ui.shortcuts);
    host.frame(vec![key_with(egui::Key::Escape, none)], none);
    assert!(!host.app.ui.shortcuts, "Escape closes the list");
    host.drafts("alpha", "beta");
}

#[test]
fn ctrl_slash_opens_the_list_where_the_slash_needs_shift() {
    // Spanish and German layouts type «/» with Shift+7: egui names the key
    // it typed, Slash, with Shift held.
    let mut host = InputHost::new("alpha", "beta");
    let both = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
    host.frame(vec![key_with(egui::Key::Slash, both)], both);
    assert!(host.app.ui.shortcuts);
    host.drafts("alpha", "beta");
}

#[test]
fn the_window_answers_a_shortcut_where_it_was_rebound() {
    use rune_ui::keybinds::{Action, Chord};
    let mut host = InputHost::new("alpha", "beta");
    host.app.ui.prefs.shortcuts = vec![
        (Action::Back, Chord::ctrl(limn::Key::J)),
        (Action::Shortcuts, Chord::plain(limn::Key::F9)),
    ];
    // The old keys do nothing of theirs any more.
    host.frame(vec![alt_key(egui::Key::ArrowLeft)], alt_modifiers());
    host.focused(host.a);
    let command = egui::Modifiers::COMMAND;
    host.frame(vec![key_with(egui::Key::Slash, command)], command);
    assert!(!host.app.ui.shortcuts);
    // The new ones do.
    host.frame(
        vec![
            key_with(egui::Key::J, command),
            egui::Event::Text(" b".into()),
        ],
        command,
    );
    host.focused(host.b);
    host.drafts("alpha", "beta b");
    let none = egui::Modifiers::default();
    host.frame(vec![key_with(egui::Key::F9, none)], none);
    assert!(host.app.ui.shortcuts);
}

#[test]
fn a_held_paste_of_file_names_goes_in_only_when_the_clipboard_has_no_files() {
    use rune_provider::{UploadError, UploadUpdate};
    let mut host = InputHost::new("alpha", "beta");
    // A capture never reads the clipboard: the names paste as text at once.
    host.frame(
        vec![egui::Event::Paste("/tmp/foto.png".into())],
        egui::Modifiers::default(),
    );
    host.drafts("alpha/tmp/foto.png", "beta");
    let account = host.app.store.me().unwrap();
    let held = |host: &mut InputHost, request| {
        let target = host.app.tree.capture_paste_target().unwrap();
        host.app.file_paste = Some(paste::FilePaste {
            request,
            target,
            text: " /tmp/otra.png".into(),
            until: Instant::now() + Duration::from_secs(60),
        });
    };
    let channel = host.a;
    let answer = |request, result| UploadUpdate::Prepared {
        account,
        channel,
        request,
        result,
    };
    // No files on the clipboard: quietly cancelled, and the text goes in.
    held(&mut host, 41);
    let (update, text) = host
        .app
        .take_file_paste(answer(41, Err(UploadError::Empty)));
    assert!(matches!(
        update,
        UploadUpdate::Prepared {
            result: Err(UploadError::Cancelled),
            ..
        }
    ));
    host.app
        .put_paste(&host.ctx.clone(), text.expect("the held text"));
    host.drafts("alpha/tmp/foto.png /tmp/otra.png", "beta");
    // Files: they are the answer, and the text is dropped.
    held(&mut host, 42);
    let (update, text) = host.app.take_file_paste(answer(42, Ok(Vec::new())));
    assert!(matches!(
        update,
        UploadUpdate::Prepared { result: Ok(_), .. }
    ));
    assert!(text.is_none() && host.app.file_paste.is_none());
    // Another request's answer leaves a held paste alone.
    held(&mut host, 43);
    let (_, text) = host.app.take_file_paste(answer(7, Err(UploadError::Empty)));
    assert!(text.is_none() && host.app.file_paste.is_some());
    // Past its wait, the text goes in without the clipboard.
    host.app.file_paste.as_mut().unwrap().until = Instant::now();
    host.app
        .expire_file_paste(&host.ctx.clone(), Instant::now());
    assert!(host.app.file_paste.is_none());
    host.drafts("alpha/tmp/foto.png /tmp/otra.png /tmp/otra.png", "beta");
}

// Reading settings (imagenes-y-lectura).
#[test]
fn the_wheel_and_the_trackpad_move_at_the_person_s_speed() {
    let position = Point::new(10.0, 20.0);
    let deltas = |event| match scaled_scroll(event, 2.5) {
        UiEvent::Scroll { delta_y, .. } | UiEvent::ScrollDirect { delta_y, .. } => (0.0, delta_y),
        UiEvent::Scroll2D {
            delta_x, delta_y, ..
        } => (delta_x, delta_y),
        _ => (f32::NAN, f32::NAN),
    };
    // The wheel's lines and a trackpad's pixels, up and down or sideways.
    assert_eq!(
        deltas(UiEvent::Scroll {
            position,
            delta_y: 30.0
        }),
        (0.0, 75.0)
    );
    assert_eq!(
        deltas(UiEvent::ScrollDirect {
            position,
            delta_y: -4.0
        }),
        (0.0, -10.0)
    );
    assert_eq!(
        deltas(UiEvent::Scroll2D {
            position,
            delta_x: 2.0,
            delta_y: 4.0,
            direct: true,
            shift: false,
        }),
        (5.0, 10.0)
    );
    assert!(matches!(
        scaled_scroll(UiEvent::PointerMove(position), 2.5),
        UiEvent::PointerMove(at) if at == position
    ));
    assert_eq!(rune_ui::Prefs::default().scroll_factor(), 1.0);
    // Through the host's input path, the tree moves at that speed.
    let mut host = InputHost::new("", "");
    let ctx = host.ctx.clone();
    let rows = Element::column(
        (0..40)
            .map(|row| Element::text(format!("fila {row}")).size(None, Some(20.0)))
            .collect(),
    );
    let mut swipe = |speed: u16| {
        host.app.ui.prefs.scroll_speed = speed;
        host.app
            .tree
            .set_root(Element::scroll("lista", rows.clone()));
        host.app.tree.layout(Size::new(200.0, 100.0));
        let before = host.app.tree.scroll_offset("lista");
        host.app.handle_ui_event(
            &ctx,
            UiEvent::ScrollDirect {
                position,
                delta_y: -20.0,
            },
            Instant::now(),
            false,
            false,
        );
        host.app.tree.scroll_offset("lista") - before
    };
    let usual = swipe(100);
    assert!(usual.abs() > 0.0);
    assert_eq!(swipe(200), 2.0 * usual);
}

#[test]
fn a_side_button_recorded_as_the_push_to_talk_key_then_holds_it() {
    let mut host = InputHost::new("alpha", "beta");
    host.app.ui.settings = Some(rune_ui::SettingsPage::default());
    host.app.ui.settings_state.recording = Some(Instant::now());
    host.frame(
        vec![side_button(egui::PointerButton::Extra1)],
        egui::Modifiers::default(),
    );
    assert_eq!(
        host.app.ui.prefs.push_to_talk_key.as_deref(),
        Some("MOUSE4")
    );
    assert!(!rune_ui::recording_key(&host.app.ui));
    host.app.ui.prefs.push_to_talk = true;
    host.frame(Vec::new(), egui::Modifiers::default());
    assert!(!host.app.audio.held, "not held before the press");
    host.frame(
        vec![side_button(egui::PointerButton::Extra1)],
        egui::Modifiers::default(),
    );
    // The held state is read by `drain`, which runs before the input each frame.
    host.app.drain(&host.ctx, Instant::now());
    assert!(host.app.audio.held, "opens on the press");
    host.frame(
        vec![egui::Event::PointerButton {
            pos: egui::pos2(600.0, 400.0),
            button: egui::PointerButton::Extra1,
            pressed: false,
            modifiers: egui::Modifiers::default(),
        }],
        egui::Modifiers::default(),
    );
    host.app.drain(&host.ctx, Instant::now());
    assert!(!host.app.audio.held, "closes on the release");
}
