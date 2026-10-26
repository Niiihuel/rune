//! Host boundary regressions use synthetic handles without reading any file.

use super::*;

#[derive(Debug)]
struct UnreadFile(PathBuf);

struct Fixtures {
    root: PathBuf,
    paths: Vec<PathBuf>,
}

impl Fixtures {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "rune-drop-host-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        #[cfg_attr(not(unix), allow(unused_mut))]
        let mut directory = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            directory.mode(0o700);
        }
        directory.create(&root).unwrap();
        let mut fixtures = Self {
            root,
            paths: Vec::new(),
        };
        for name in ["primero.txt", "segundo.txt"] {
            let path = fixtures.root.join(name);
            std::fs::write(&path, format!("Fixture sintético propio: {name}\n")).unwrap();
            fixtures.paths.push(path);
        }
        fixtures
    }
}

impl Drop for Fixtures {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

impl egui::DroppedFile for UnreadFile {
    fn path(&self) -> &std::path::Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        panic!("native file metadata admission must never read file bytes")
    }
}

// Unix paths and file names (windows-base).
#[cfg(unix)]
#[test]
fn all_legacy_file_drops_are_rejected_before_the_bridge_can_copy_any_paths() {
    let ctx = egui::Context::default();
    let provider = Arc::new(MockProvider::new(MockConfig {
        latency: Duration::ZERO,
        ambient: false,
        replies: false,
        ..MockConfig::default()
    }));
    let mut app = App::new(
        &ctx,
        Instant::now(),
        Arguments {
            screenshot: Some("unused-headless-drop-metadata.png".into()),
            ..Arguments::default()
        },
        provider,
        None,
    );
    let maximum = (0..10)
        .map(|index| PathBuf::from(format!("/{}{index}", "a".repeat(4094))))
        .collect::<Vec<_>>();
    let cases = [
        (maximum, true),
        (
            (0..11)
                .map(|index| format!("/fixture-{index}").into())
                .collect(),
            false,
        ),
        (vec![PathBuf::from(format!("/{}", "a".repeat(4096)))], false),
        (vec!["/owned/first".into(), PathBuf::new()], false),
        (vec!["/owned/first".into(), "relative.txt".into()], false),
        (vec!["/owned/first".into(), "/nul\0name".into()], false),
    ];
    for (paths, valid) in cases {
        let handles = paths
            .iter()
            .cloned()
            .map(|path| Arc::new(UnreadFile(path)) as egui::DroppedFileHandle)
            .collect();
        let mut input = egui::RawInput {
            dropped_files: handles,
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        assert!(
            input.dropped_files.is_empty() && input.hovered_files.is_empty(),
            "every legacy batch must be removed in full before event translation"
        );
        assert_eq!(
            matches!(app.drop_rejection, Some(NativeDropRejection::Unpositioned)),
            valid,
            "metadata validation must retain its own rejection reason"
        );
        assert!(valid || matches!(app.drop_rejection, Some(NativeDropRejection::Invalid(_))));
    }
}

fn conversation() -> (
    egui::Context,
    App,
    Arc<MockProvider>,
    rune_model::Channel,
    egui::Rect,
) {
    let ctx = egui::Context::default();
    let now = Instant::now();
    let area = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 800.0));
    let window = Size::new(area.width(), area.height());
    let provider = Arc::new(MockProvider::new(MockConfig {
        latency: Duration::ZERO,
        ambient: false,
        replies: false,
        ..MockConfig::default()
    }));
    let channel = provider
        .snapshot()
        .channels
        .into_iter()
        .find(|channel| channel.name == "general")
        .unwrap();
    let mut app = App::new(
        &ctx,
        now,
        Arguments {
            screenshot: Some("unused-headless-drop-routing.png".into()),
            ..Arguments::default()
        },
        provider.clone(),
        None,
    );
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.store.me().is_none() {
        app.drain(&ctx, Instant::now());
        assert!(
            Instant::now() < deadline,
            "the simulated service did not become ready"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let intents = rune_ui::open(&mut app.ui, &mut app.store, channel.server, channel.id);
    app.run(&ctx, intents);
    let deadline = Instant::now() + Duration::from_secs(5);
    while app
        .store
        .timeline(channel.id)
        .is_none_or(|timeline| timeline.messages().is_empty())
    {
        app.drain(&ctx, Instant::now());
        assert!(
            Instant::now() < deadline,
            "the simulated channel history did not load"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    let editor = rune_ui::editor_id(channel.id);
    let intents = rune_ui::update(
        &mut app.ui,
        &mut app.store,
        &limn::UiAction::InputChanged {
            id: editor.clone(),
            value: "Conservar este borrador".into(),
        },
        now,
    );
    app.run(&ctx, intents);
    let context = app.context(window, now);
    app.tree
        .set_root(rune_ui::view(&app.ui, &app.store, &context));
    app.tree.layout(window);
    app.tree.set_input_cursor(&editor, 3);
    let channel_control = format!("channel:{}", channel.id.0);
    assert!(app.tree.focus(&channel_control));
    app.focus = None;
    (ctx, app, provider, channel, area)
}

fn native_drop_input(app: &App, area: egui::Rect, paths: &[PathBuf]) -> egui::RawInput {
    let editor = rune_ui::editor_id(app.ui.channel().unwrap());
    let rect = app
        .tree
        .frame()
        .regions
        .iter()
        .find(|region| region.id == editor)
        .unwrap()
        .rect;
    egui::RawInput {
        screen_rect: Some(area),
        events: vec![egui::Event::PointerMoved(egui::Pos2::new(
            rect.x + 5.0,
            rect.y + 5.0,
        ))],
        dropped_files: paths
            .iter()
            .cloned()
            .map(|path| Arc::new(UnreadFile(path)) as egui::DroppedFileHandle)
            .collect(),
        ..Default::default()
    }
}

#[test]
fn legacy_file_drop_actions_cannot_bypass_rejection_without_the_raw_hook() {
    let fixtures = Fixtures::new();
    let (ctx, mut app, provider, channel, area) = conversation();
    let channel_control = format!("channel:{}", channel.id.0);
    let before = provider.snapshot().last_messages;
    let input = native_drop_input(&app, area, &fixtures.paths);
    ctx.run_ui(input, |_| {
        app.handle_input(&ctx, area, Instant::now(), false, false);
    })
    .textures_delta
    .clear();
    assert!(!rune_ui::attachment_pick_pending(&app.ui));
    assert_eq!(
        app.ui.drafts.get(&channel.id).map(String::as_str),
        Some("Conservar este borrador")
    );
    assert_eq!(app.tree.focused_id(), Some(channel_control.as_str()));
    assert!(app.focus.is_none(), "a drop must not request editor focus");
    assert_eq!(
        provider.snapshot().last_messages,
        before,
        "dropping files never sends a message"
    );
    assert!(app.ui.notices.iter().any(|notice| {
        notice.title == "Todavía no puedes adjuntar archivos arrastrándolos. Usa Adjuntar."
    }));
}

#[test]
fn legacy_drop_with_a_later_editor_pointer_is_rejected_before_any_file_work() {
    let fixtures = Fixtures::new();
    let (ctx, mut app, provider, channel, area) = conversation();
    let before = provider.snapshot().last_messages;
    let focused = app.tree.focused_id().map(str::to_owned);
    let mut input = native_drop_input(&app, area, &fixtures.paths);
    // RawInput groups files apart from pointer events. A later move into the
    // editor cannot establish that the actual drop happened on that editor.
    input
        .events
        .insert(0, egui::Event::PointerMoved(egui::pos2(10.0, 10.0)));
    eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
    let metadata_discarded = input.dropped_files.is_empty();
    ctx.run_ui(input, |_| {
        app.handle_input(&ctx, area, Instant::now(), false, false);
    })
    .textures_delta
    .clear();

    assert!(
        !rune_ui::attachment_pick_pending(&app.ui),
        "a legacy drop has no gesture-bound destination and cannot admit filesystem work"
    );
    assert!(
        metadata_discarded,
        "unpositioned file handles must not reach the bridge or the preparation service"
    );
    assert_eq!(
        app.ui.drafts.get(&channel.id).map(String::as_str),
        Some("Conservar este borrador")
    );
    assert_eq!(app.tree.focused_id(), focused.as_deref());
    assert!(app.focus.is_none(), "rejection must not move editor focus");
    assert_eq!(
        provider.snapshot().last_messages,
        before,
        "rejection must not send a text-only or attachment message"
    );
    assert!(app.ui.notices.iter().any(|notice| {
        notice.title == "Todavía no puedes adjuntar archivos arrastrándolos. Usa Adjuntar."
    }));
}

#[test]
fn rejecting_a_legacy_drop_preserves_the_existing_picker_and_its_terminal_callback() {
    let fixtures = Fixtures::new();
    let (ctx, mut app, provider, channel, area) = conversation();
    let now = Instant::now();
    let intents = rune_ui::update(
        &mut app.ui,
        &mut app.store,
        &limn::UiAction::Clicked {
            id: rune_components::composer::ids::PLUS.into(),
        },
        now,
    );
    app.run(&ctx, intents);
    let intents = rune_ui::update(
        &mut app.ui,
        &mut app.store,
        &limn::UiAction::Clicked {
            id: rune_components::composer::ids::ATTACH.into(),
        },
        now,
    );
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PickAttachment {
                request,
                channel: found,
            } if *found == channel.id => Some(*request),
            _ => None,
        })
        .expect("the explicit attachment action must still create a picker request");
    // Do not run this intent: the fixture exercises the UI callback without
    // opening a native dialog or reading the real filesystem through Picker.
    assert!(rune_ui::attachment_pick_pending(&app.ui));
    let before = provider.snapshot().last_messages;
    let mut input = native_drop_input(&app, area, &fixtures.paths);
    eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
    ctx.run_ui(input, |_| {
        app.handle_input(&ctx, area, now, false, false);
    })
    .textures_delta
    .clear();
    assert!(rune_ui::attachment_pick_pending(&app.ui));
    let intents = rune_ui::upload_update(
        &mut app.ui,
        &app.store,
        UploadUpdate::Prepared {
            account: app.store.me().unwrap(),
            channel: channel.id,
            request,
            result: Err(UploadError::Cancelled),
        },
    );
    assert!(intents.is_empty());
    assert!(!rune_ui::attachment_pick_pending(&app.ui));
    assert_eq!(
        app.ui.drafts.get(&channel.id).map(String::as_str),
        Some("Conservar este borrador")
    );
    assert_eq!(provider.snapshot().last_messages, before);
}

#[test]
fn legacy_hover_cannot_offer_a_drop_destination_or_show_a_rejection() {
    let fixtures = Fixtures::new();
    let (ctx, mut app, _, _, area) = conversation();
    let notices = app.ui.notices.len();
    let mut input = native_drop_input(&app, area, &[]);
    input.hovered_files = fixtures
        .paths
        .iter()
        .map(|path| egui::HoveredFile {
            path: Some(path.clone()),
            ..Default::default()
        })
        .collect();
    eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
    ctx.run_ui(input, |_| {
        assert!(!limn_egui::events(&ctx, area.min).iter().any(|event| {
            matches!(
                event,
                UiEvent::FilesHovered { .. } | UiEvent::FilesDropped { .. }
            )
        }));
        app.handle_input(&ctx, area, Instant::now(), false, false);
    })
    .textures_delta
    .clear();
    assert!(!rune_ui::attachment_pick_pending(&app.ui));
    assert_eq!(app.ui.notices.len(), notices);
}

#[test]
fn capture_drop_waits_for_input_then_records_rejection_without_a_preparation_timeout() {
    let fixtures = Fixtures::new();
    let (ctx, mut app, _, _, area) = conversation();
    let now = Instant::now();
    app.capture.as_mut().unwrap().file_drop = Some(DropCapture::Waiting);
    app.capture_step(&ctx, now);
    app.capture_step(&ctx, now + Duration::from_secs(2));
    assert!(!app.capture.as_ref().unwrap().requested);

    let rejected = now + Duration::from_secs(3);
    let mut input = native_drop_input(&app, area, &fixtures.paths);
    eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
    ctx.run_ui(input, |_| {
        app.handle_input(&ctx, area, rejected, false, false);
    })
    .textures_delta
    .clear();
    assert!(matches!(
        app.capture.as_ref().unwrap().file_drop,
        Some(DropCapture::Rejected)
    ));
    assert!(!rune_ui::attachment_pick_pending(&app.ui));
    app.capture_step(&ctx, rejected + Duration::from_millis(1_499));
    assert!(!app.capture.as_ref().unwrap().requested);
    app.capture_step(&ctx, rejected + Duration::from_millis(1_501));
    assert!(app.capture.as_ref().unwrap().requested);
}

#[test]
fn a_native_close_or_a_menu_opened_in_the_same_frame_prevents_file_import() {
    let fixtures = Fixtures::new();
    for close in [true, false] {
        let (ctx, mut app, _, _, area) = conversation();
        let mut input = native_drop_input(&app, area, &fixtures.paths);
        if !close {
            let rect = app
                .tree
                .frame()
                .regions
                .iter()
                .find(|region| region.id == "me:settings")
                .unwrap()
                .rect;
            let position = egui::Pos2::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0);
            input.events.splice(
                0..0,
                [
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
            );
        }
        ctx.run_ui(input, |_| {
            app.handle_input(&ctx, area, Instant::now(), close, false);
        })
        .textures_delta
        .clear();
        assert!(!rune_ui::attachment_pick_pending(&app.ui), "close={close}");
        if close {
            assert!(app.pending_storage.closing);
        } else {
            assert!(
                app.ui.settings.is_some(),
                "the earlier native click must have opened settings"
            );
        }
    }
}

#[test]
fn enter_in_a_frame_with_new_files_cannot_send_an_incomplete_message() {
    let fixtures = Fixtures::new();
    for valid in [true, false] {
        let (ctx, mut app, provider, channel, area) = conversation();
        assert!(app.tree.focus(&rune_ui::editor_id(channel.id)));
        let before = provider.snapshot().last_messages;
        let paths = if valid {
            fixtures.paths.clone()
        } else {
            vec![fixtures.paths[0].clone(); 11]
        };
        let mut input = native_drop_input(&app, area, &paths);
        input.events.push(egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        ctx.run_ui(input, |_| {
            app.handle_input(&ctx, area, Instant::now(), false, false);
        })
        .textures_delta
        .clear();
        assert_eq!(
            app.ui.drafts.get(&channel.id).map(String::as_str),
            Some("Conservar este borrador"),
            "valid={valid}; sending requires a later submission after file preparation"
        );
        assert_eq!(provider.snapshot().last_messages, before, "valid={valid}");
        assert!(!rune_ui::attachment_pick_pending(&app.ui));
        // The rejection flag must expire with this frame, rather than leave
        // the ordinary editor permanently unable to send an explicit message.
        let mut input = egui::RawInput {
            screen_rect: Some(area),
            events: vec![egui::Event::Key {
                key: egui::Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        ctx.run_ui(input, |_| {
            app.handle_input(&ctx, area, Instant::now(), false, false);
        })
        .textures_delta
        .clear();
        assert!(app.ui.drafts.get(&channel.id).is_none_or(String::is_empty));
        assert!(app
            .store
            .timeline(channel.id)
            .unwrap()
            .messages()
            .iter()
            .any(|message| {
                message.content == "Conservar este borrador" && message.delivery.is_local()
            }));
    }
}
