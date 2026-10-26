//! Geometry regressions for the compact composer, without a window or account.

use super::*;
use limn::{DrawCommand, FontMetrics, Frame, Point, Rect, Size, UiTree};

fn metrics() -> [FontMetrics; 2] {
    [
        FontMetrics::default(),
        // Ubuntu Regular 0.83 bundled by limn-egui: 1000 units/em,
        // hhea ascent 932, descent -189, line gap 28, at its 14 px calibration.
        // This exercises the native face's line height without adding egui
        // or loading system fonts into the component tests.
        FontMetrics {
            char_width: 7.0,
            line_height: 16.086,
        },
    ]
}

fn view(width: f32, draft: &str) -> ComposerView<'static> {
    ComposerView {
        editor: "composer:fixture".into(),
        draft: draft.into(),
        placeholder: "Escribe un mensaje".into(),
        width,
        reply: None,
        disabled: None,
        attach_denied: None,
        attach_hint: None,
        plus_open: false,
        typing: None,
        emoji: None,
        completions: None,
        highlights: Vec::new(),
        actions: Default::default(),
    }
}

#[test]
fn file_drops_target_only_the_enabled_editor_with_attachment_permission() {
    for (enabled, attachments) in [(true, true), (true, false), (false, true)] {
        let mut view = view(320.0, "Conservar borrador");
        view.disabled = (!enabled).then(|| "Sin permiso para escribir".into());
        view.attach_denied = (!attachments).then(|| "Sin permiso para adjuntar".into());
        let mut tree = UiTree::new(composer(view, &Look::DARK));
        tree.set_reduced_motion(true);
        let frame = tree.layout(Size::new(320.0, 200.0));
        let position = if enabled {
            let editor = frame
                .regions
                .iter()
                .find(|region| region.id == "composer:fixture")
                .unwrap()
                .rect;
            assert!(tree.focus("composer:fixture"));
            Point::new(
                editor.x + editor.width / 2.0,
                editor.y + editor.height / 2.0,
            )
        } else {
            Point::new(150.0, 24.0)
        };
        let focused = tree.focused_id().map(str::to_owned);
        let paths = vec![
            "/tmp/rune-drop-one.txt".into(),
            "/tmp/rune-drop-two.txt".into(),
        ];
        assert_eq!(
            tree.handle_event(limn::UiEvent::FilesDropped {
                position,
                paths: paths.clone(),
            }),
            vec![limn::UiAction::FilesDropped {
                position,
                target: (enabled && attachments).then(|| "composer:fixture".into()),
                paths,
            }],
            "enabled={enabled}, attachments={attachments}"
        );
        assert_eq!(tree.focused_id(), focused.as_deref());
        if enabled {
            assert_eq!(
                tree.input_value("composer:fixture"),
                Some("Conservar borrador")
            );
        }
    }
}

fn laid(view: ComposerView<'_>, look: &Look, metrics: FontMetrics) -> Frame {
    let width = view.width;
    let mut tree = UiTree::new(Element::column(vec![
        Element::spacer().flex(1.0),
        composer(view, look),
    ]));
    tree.set_reduced_motion(true);
    tree.set_sans_metrics(metrics);
    tree.set_text_measurer(|text, _, _| text.chars().count() as f32 * 7.0);
    tree.layout(Size::new(width, 700.0))
}

fn text_position(frame: &Frame, expected: &str) -> Point {
    frame
        .commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Text { position, text, .. } if text == expected => Some(*position),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing text {expected:?}"))
}

fn text_top(frame: &Frame, expected: &str) -> f32 {
    text_position(frame, expected).y
}

fn icon_rect(frame: &Frame, expected: Icon) -> Rect {
    frame
        .commands
        .iter()
        .find_map(|command| match command {
            DrawCommand::Icon { rect, icon, .. } if *icon == expected => Some(*rect),
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing icon {expected:?}"))
}

fn assert_first_line_aligned(frame: &Frame, text: &str, metrics: FontMetrics) {
    // Use the same font metrics as this headless tree, not the font size:
    // the line box includes ascenders and descenders beyond the em square.
    let text_center = text_top(frame, text) + metrics.line_height * 16.0 / 14.0 / 2.0;
    for icon in [Icon::Add, Icon::Emoji] {
        let rect = icon_rect(frame, icon);
        let center = rect.y + rect.height / 2.0;
        assert!(
            (text_center - center).abs() <= 1.0,
            "{icon:?}: text center {text_center}, icon center {center}"
        );
    }
}

#[test]
fn one_line_draft_and_placeholder_share_the_action_center_in_a_compact_card() {
    for metrics in metrics() {
        for look in [&Look::DARK, &Look::LIGHT] {
            for width in [320.0, 600.0, 1100.0] {
                for draft in ["", "Hola"] {
                    for reply in [false, true] {
                        let mut view = view(width, draft);
                        view.reply = reply.then(|| ("Ada".into(), look.text));
                        let frame = laid(view, look, metrics);
                        let text = if draft.is_empty() {
                            "Escribe un mensaje"
                        } else {
                            draft
                        };
                        assert_first_line_aligned(&frame, text, metrics);
                        let editor = frame
                            .regions
                            .iter()
                            .find(|region| region.id == "composer:fixture")
                            .unwrap();
                        // The card is 48 tall: limn's padding around the editor.
                        assert!(
                            (32.0..=33.0).contains(&editor.rect.height),
                            "{:?}",
                            editor.rect
                        );
                        // Keep the established horizontal text inset and room for both actions.
                        assert!((text_position(&frame, text).x - 74.0).abs() <= 0.5);
                        assert!(
                            icon_rect(&frame, Icon::Emoji).x > editor.rect.x + editor.rect.width
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn disabled_reason_uses_the_same_line_center_without_an_editable_region() {
    for metrics in metrics() {
        let mut view = view(320.0, "");
        view.disabled = Some("Sin permiso para escribir".into());
        let frame = laid(view, &Look::DARK, metrics);
        assert_first_line_aligned(&frame, "Sin permiso para escribir", metrics);
        assert!(!frame
            .regions
            .iter()
            .any(|region| region.id == "composer:fixture"));
        let access = frame
            .access
            .iter()
            .find(|node| node.id == "composer:fixture")
            .unwrap();
        assert!(access.disabled && !access.actionable);
    }
}

#[test]
fn multiline_growth_keeps_actions_on_the_first_line_and_the_text_unclipped() {
    for metrics in metrics() {
        let frame = laid(
            view(320.0, "Primera\nSegunda\nTercera"),
            &Look::DARK,
            metrics,
        );
        assert_first_line_aligned(&frame, "Primera", metrics);
        let line_height = metrics.line_height * 16.0 / 14.0;
        assert!(
            (text_top(&frame, "Segunda") - text_top(&frame, "Primera") - line_height).abs() <= 0.5
        );
        assert!(
            (text_top(&frame, "Tercera") - text_top(&frame, "Segunda") - line_height).abs() <= 0.5
        );
        let editor = frame
            .regions
            .iter()
            .find(|region| region.id == "composer:fixture")
            .unwrap();
        assert!(
            editor.rect.height > 33.0 && editor.rect.height <= 100.0,
            "{:?}",
            editor.rect
        );
        assert!(text_top(&frame, "Tercera") + line_height <= editor.rect.y + editor.rect.height);
    }
}

#[test]
fn a_long_draft_still_has_a_bounded_editor_and_reachable_actions() {
    let draft = "Línea de prueba\n".repeat(100);
    let frame = laid(view(320.0, &draft), &Look::DARK, metrics()[1]);
    let editor = frame
        .regions
        .iter()
        .find(|region| region.id == "composer:fixture")
        .unwrap();
    assert!(editor.rect.height <= 300.0);
    for id in [ids::PLUS, ids::EMOJI] {
        let action = frame.regions.iter().find(|region| region.id == id).unwrap();
        assert!(action.rect.y >= editor.rect.y);
        assert!(action.rect.y + action.rect.height <= editor.rect.y + editor.rect.height);
    }
}

#[test]
fn a_long_reason_not_to_write_wraps_onto_further_lines_in_a_narrow_window() {
    let reason = "Estás aislado hasta mañana a las 10:30 · quedan 19 h 5 min";
    for metrics in metrics() {
        let mut view = view(320.0, "");
        view.disabled = Some(reason.into());
        let frame = laid(view, &Look::DARK, metrics);
        let lines: Vec<(String, f32)> = frame
            .commands
            .iter()
            .filter_map(|command| match command {
                DrawCommand::Text { text, position, .. }
                    if !text.trim().is_empty() && reason.contains(text.trim()) =>
                {
                    Some((text.clone(), position.y))
                }
                _ => None,
            })
            .collect();
        // Whole, on more than one line, and never cut short with «…».
        let said: String = lines.iter().map(|(text, _)| text.as_str()).collect();
        assert!(said.contains("quedan 19 h 5 min"), "{lines:?}");
        assert!(
            lines.windows(2).any(|pair| pair[1].1 > pair[0].1),
            "{lines:?}"
        );
        assert!(!frame.commands.iter().any(
            |command| matches!(command, DrawCommand::Text { text, .. } if text.contains('…'))
        ));
    }
}
