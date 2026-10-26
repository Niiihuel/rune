//! Notices must leave the live draft visible, including its dynamic height.

use super::*;
use limn::{DrawCommand, HitKind, Rect, Role};
use rune_model::{PendingAttachment, UploadToken};

const UNSUPPORTED: &str = "Todavía no puedes adjuntar archivos arrastrándolos. Usa Adjuntar.";
const DRAFT: &str = "Primera línea del borrador\nSegunda línea conservada\nTercera línea sin enviar\nÚltima línea de la respuesta";

fn draft(files: u64) -> (Host, ChannelId, MessageId) {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    host.type_text(DRAFT);
    let reply = host.store.timeline(channel).unwrap().newest().unwrap().id;
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("msg:{reply}:reply"),
        },
        host.now,
    );
    host.run(intents);
    host.ui.uploads.account = host.store.me();
    host.ui.uploads.drafts.insert(
        channel,
        (1..=files)
            .map(|token| PendingAttachment {
                token: UploadToken(token),
                filename: format!("fixture-{token}.txt"),
                size_bytes: 12,
            })
            .collect(),
    );
    host.frame();
    assert_eq!(host.ui.reply_to, Some(reply));
    (host, channel, reply)
}

fn layout(host: &mut Host, window: Size) -> Frame {
    host.tree.set_root(view(
        &host.ui,
        &host.store,
        &ViewContext {
            window,
            ..host.context()
        },
    ));
    host.tree.layout(window)
}

fn region(frame: &Frame, id: &str, kind: HitKind) -> Rect {
    frame
        .regions
        .iter()
        .find(|region| region.id == id && region.kind == kind)
        .unwrap_or_else(|| panic!("{id} ({kind:?}) must remain visible"))
        .rect
}

fn alert(frame: &Frame, title: &str) -> Rect {
    let matches: Vec<_> = frame
        .access
        .iter()
        .filter(|node| node.role == Role::Alert && node.name == title)
        .collect();
    assert_eq!(matches.len(), 1, "one visible projection of {title}");
    matches[0].rect
}

#[track_caller]
fn visible(rect: Rect, window: Size) {
    assert!(rect.width > 0.0 && rect.height > 0.0, "{rect:?}");
    assert!(
        rect.x >= 0.0
            && rect.y >= 0.0
            && rect.x + rect.width <= window.width + 0.5
            && rect.y + rect.height <= window.height + 0.5,
        "{rect:?} escapes {window:?}"
    );
}

fn center(rect: Rect) -> Point {
    Point::new(rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
}

fn draft_controls(frame: &Frame, channel: ChannelId) -> Vec<Rect> {
    vec![
        region(frame, &editor_id(channel), HitKind::TextArea),
        region(frame, "composer:cancel", HitKind::Button),
        region(
            frame,
            &format!("composer:{channel}:uploads"),
            HitKind::Scroll,
        ),
    ]
}

#[test]
fn an_unsupported_drop_notice_leaves_multiline_reply_and_eight_files_visible() {
    for window in [Size::new(980.0, 800.0), WINDOW] {
        let (mut host, channel, reply) = draft(8);
        host.ui.notify(UNSUPPORTED, None, true, host.now);
        let frame = layout(&mut host, window);
        let notice = alert(&frame, UNSUPPORTED);
        visible(notice, window);
        for control in draft_controls(&frame, channel) {
            visible(control, window);
            assert!(
                notice.intersection(control).is_none(),
                "the notice {notice:?} covers a live draft control {control:?}"
            );
        }
        let editor = region(&frame, &editor_id(channel), HitKind::TextArea);
        assert_eq!(
            frame.hit_test(center(editor)),
            Some(editor_id(channel).as_str())
        );
        host.tree.handle_event(UiEvent::PointerDown(center(editor)));
        host.tree.handle_event(UiEvent::PointerUp(center(editor)));
        assert_eq!(host.tree.focused_id(), Some(editor_id(channel).as_str()));
        assert_eq!(host.tree.input_value(&editor_id(channel)), Some(DRAFT));
        assert_eq!(host.ui.drafts[&channel], DRAFT);
        assert_eq!(host.ui.reply_to, Some(reply));
        assert_eq!(host.ui.uploads.drafts[&channel].len(), 8);
        for (token, end) in [(1, false), (8, true)] {
            if end {
                host.tree
                    .scroll_to_end(&format!("composer:{channel}:uploads"));
            }
            let frame = layout(&mut host, window);
            let id = format!("composer:{channel}:upload:{token}:remove");
            let remove = region(&frame, &id, HitKind::Button);
            visible(remove, window);
            assert_eq!(frame.hit_test(center(remove)), Some(id.as_str()));
            assert!(alert(&frame, UNSUPPORTED).intersection(remove).is_none());
        }
    }
}

#[test]
fn one_short_notice_reserves_only_its_content_height_above_the_composer() {
    let (mut host, channel, _) = draft(8);
    let before = layout(&mut host, WINDOW);
    let viewport = region(&before, &timeline_id(channel), HitKind::Scroll);
    host.ui.notify("Adjunto preparado", None, false, host.now);
    let after = layout(&mut host, WINDOW);
    let notice = alert(&after, "Adjunto preparado");
    let remaining = region(&after, &timeline_id(channel), HitKind::Scroll);
    let reserved = viewport.height - remaining.height;
    assert!(
        reserved >= notice.height && reserved <= notice.height + 32.0,
        "the flow must reserve the actual card and spacing, without a large empty tray: {reserved} for {notice:?}"
    );
    assert_eq!(
        region(&before, &editor_id(channel), HitKind::TextArea),
        region(&after, &editor_id(channel), HitKind::TextArea),
        "a notice takes timeline space while the draft stays at the bottom"
    );
}

#[test]
fn three_wrapped_notices_scroll_completely_without_overflowing_into_the_draft() {
    let (mut host, channel, reply) = draft(8);
    let window = Size::new(980.0, 800.0);
    let titles: Vec<_> = (1..=3)
        .map(|index| format!("Aviso {index}: conserva el borrador y revisa el detalle completo antes de volver a intentar la preparación de estos archivos"))
        .collect();
    let details: Vec<_> = (1..=3)
        .map(|index| {
            format!(
                "{} FIN-{index}",
                "Este detalle de prueba debe poder leerse completo al desplazar los avisos. "
                    .repeat(30)
            )
        })
        .collect();
    for (title, detail) in titles.iter().zip(&details) {
        host.ui.notify(title, Some(detail.clone()), true, host.now);
    }
    let frame = layout(&mut host, window);
    let editor = region(&frame, &editor_id(channel), HitKind::TextArea);
    visible(editor, window);
    let controls = draft_controls(&frame, channel);
    let scroll = frame
        .regions
        .iter()
        .find(|region| {
            region.kind == HitKind::Scroll
                && region.id != timeline_id(channel)
                && region.id != format!("composer:{channel}:uploads")
                && frame.access.iter().any(|node| {
                    node.role == Role::Alert
                        && titles.contains(&node.name)
                        && node.rect.intersection(region.rect).is_some()
                })
        })
        .expect("oversized notices must have a scrollable viewport")
        .clone();
    visible(scroll.rect, window);
    for control in &controls {
        assert!(scroll.rect.intersection(*control).is_none());
    }
    host.tree.scroll_to_start(&scroll.id);
    let mut reached = [false; 3];
    for _ in 0..96 {
        let frame = layout(&mut host, window);
        for block in frame.text_layouts() {
            let Some(index) = details.iter().position(|detail| detail == &block.source) else {
                continue;
            };
            for run in block.runs {
                if let Some(rect) = run.visible {
                    assert!(scroll.rect.intersection(rect).is_some());
                    for control in &controls {
                        assert!(rect.intersection(*control).is_none());
                    }
                    if run.text.contains(&format!("FIN-{}", index + 1)) {
                        reached[index] = true;
                    }
                }
            }
        }
        if host.tree.is_scroll_at_end(&scroll.id, 0.5) {
            break;
        }
        host.tree.handle_event(UiEvent::ScrollDirect {
            position: center(scroll.rect),
            delta_y: -scroll.rect.height * 0.75,
        });
    }
    assert_eq!(reached, [true; 3], "all descriptions must be reachable");
    assert_eq!(host.tree.focused_id(), Some(editor_id(channel).as_str()));
    assert_eq!(host.tree.input_value(&editor_id(channel)), Some(DRAFT));
    assert_eq!(host.ui.reply_to, Some(reply));
    assert_eq!(host.ui.uploads.drafts[&channel].len(), 8);
}

#[test]
fn a_notice_is_projected_once_above_a_modal_then_returns_to_the_conversation() {
    let (mut host, channel, _) = draft(8);
    let title = "Estado del adjunto";
    host.ui.notify(title, None, true, host.now);
    host.key(Key::F, false, true);
    assert!(host.ui.search.is_some());
    let frame = layout(&mut host, WINDOW);
    visible(alert(&frame, title), WINDOW);
    let index = |needle: &str| {
        frame
            .commands
            .iter()
            .position(|command| matches!(command, DrawCommand::Text { text, .. } if text == needle))
            .unwrap_or_else(|| panic!("{needle} must be painted"))
    };
    assert!(
        index(title) > index("Buscar mensajes"),
        "the notice must be above the modal"
    );
    host.key(Key::Escape, false, false);
    assert!(host.ui.search.is_none());
    let frame = layout(&mut host, WINDOW);
    let notice = alert(&frame, title);
    for control in draft_controls(&frame, channel) {
        assert!(notice.intersection(control).is_none());
    }
    host.ui.notices.clear();
    let frame = layout(&mut host, WINDOW);
    assert!(!frame
        .access
        .iter()
        .any(|node| node.role == Role::Alert && node.name == title));
    assert_eq!(host.tree.focused_id(), Some(editor_id(channel).as_str()));
    assert_eq!(host.tree.input_value(&editor_id(channel)), Some(DRAFT));
}

#[test]
fn notices_stack_over_an_offered_update_and_never_cover_its_buttons() {
    use rune_components::updates::{ids, Stage, UpdateView};
    let (mut host, channel, _) = draft(2);
    host.ui.updates.installation = Some("C:/RUNE".into());
    host.ui.updates.offered = Some(UpdateView {
        version: "1.2.0".into(),
        installed: "1.1.0".into(),
        notes: "Notas.".into(),
        stage: Stage::Offered,
    });
    let titles = ["Primer aviso", "Segundo aviso", "Tercer aviso"];
    for title in titles {
        host.ui
            .notify(title, Some("Detalle".into()), true, host.now);
    }
    let frame = layout(&mut host, WINDOW);
    let install = region(&frame, ids::INSTALL, HitKind::Button);
    visible(install, WINDOW);
    assert_eq!(
        frame
            .press_region(center(install))
            .map(|found| found.id.as_str()),
        Some(ids::INSTALL),
        "a press on «Actualizar y reiniciar» reaches it"
    );
    for title in titles {
        let notice = alert(&frame, title);
        visible(notice, WINDOW);
        assert!(notice.intersection(install).is_none(), "{title} covers it");
    }
    // Put off, the notices go back over the composer.
    host.ui.updates.later = true;
    let frame = layout(&mut host, WINDOW);
    let notice = alert(&frame, titles[0]);
    for control in draft_controls(&frame, channel) {
        assert!(notice.intersection(control).is_none());
    }
}

#[test]
fn authentication_keeps_one_visible_notice_without_creating_a_composer() {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .authentication = true;
    host.event(Event::Authentication(
        rune_provider::AuthenticationState::SignedOut,
    ));
    let title = "No se pudo completar la selección";
    host.ui.notify(title, None, true, host.now);
    for window in [Size::new(980.0, 800.0), WINDOW] {
        let frame = layout(&mut host, window);
        visible(alert(&frame, title), window);
        assert!(frame
            .regions
            .iter()
            .any(|region| region.id == "auth:sign-in"));
        assert!(!frame
            .regions
            .iter()
            .any(|region| region.id.starts_with("composer:")));
    }
}

#[test]
fn native_selection_does_not_move_notices_back_over_the_live_composer() {
    let (mut host, channel, _) = draft(7);
    let request = begin_attachment_import(
        &mut host.ui,
        &host.store,
        channel,
        AttachmentImportSource::Picker,
    )
    .expect("one file slot remains available");
    assert!(attachment_pick_pending(&host.ui));
    host.ui.notify(UNSUPPORTED, None, true, host.now);
    let frame = layout(&mut host, WINDOW);
    let notice = alert(&frame, UNSUPPORTED);
    for control in draft_controls(&frame, channel) {
        assert!(notice.intersection(control).is_none());
    }
    let cancel = region(
        &frame,
        &format!("composer:{channel}:pick:{request}:cancel"),
        HitKind::Button,
    );
    assert!(notice.intersection(cancel).is_none());
    assert_eq!(
        frame.hit_test(center(cancel)),
        Some(format!("composer:{channel}:pick:{request}:cancel").as_str())
    );
}

#[test]
fn notices_leave_the_live_draft_reachable_at_the_minimum_desktop_size() {
    let (mut host, channel, reply) = draft(8);
    let window = Size::new(720.0, 480.0);
    for index in 1..=3 {
        host.ui.notify(
            format!("Aviso {index}: revisa la preparación de los archivos antes de continuar"),
            Some("Este detalle completo debe seguir accesible mediante scroll. ".repeat(30)),
            true,
            host.now,
        );
    }
    let frame = layout(&mut host, window);
    for control in draft_controls(&frame, channel) {
        visible(control, window);
        for notice in frame.access.iter().filter(|node| node.role == Role::Alert) {
            assert!(notice.rect.intersection(control).is_none());
        }
    }
    let layouts = frame.text_layouts();
    let title = layouts
        .iter()
        .find(|block| block.source.starts_with("Aviso 1:"))
        .expect("the initial notice title must be laid out");
    let first_line = title.runs.first().expect("a title has a line of text");
    let shown = first_line
        .visible
        .expect("the initial title line must be visible");
    let whole = first_line.rect;
    let epsilon = 0.001;
    assert!(
        shown.x <= whole.x + epsilon
            && shown.y <= whole.y + epsilon
            && shown.x + shown.width >= whole.x + whole.width - epsilon
            && shown.y + shown.height >= whole.y + whole.height - epsilon,
        "fitting the editor must leave a whole readable line of the notice, not a sliver: {shown:?} clips {whole:?}"
    );
    let editor = region(&frame, &editor_id(channel), HitKind::TextArea);
    assert_eq!(
        frame.hit_test(center(editor)),
        Some(editor_id(channel).as_str())
    );
    let files = format!("composer:{channel}:uploads");
    for (token, end) in [(1, false), (8, true)] {
        if end {
            host.tree.scroll_to_end(&files);
        } else {
            host.tree.scroll_to_start(&files);
        }
        let frame = layout(&mut host, window);
        let id = format!("composer:{channel}:upload:{token}:remove");
        let remove = region(&frame, &id, HitKind::Button);
        visible(remove, window);
        assert_eq!(frame.hit_test(center(remove)), Some(id.as_str()));
    }
    assert_eq!(host.tree.focused_id(), Some(editor_id(channel).as_str()));
    assert_eq!(host.tree.input_value(&editor_id(channel)), Some(DRAFT));
    assert_eq!(host.ui.reply_to, Some(reply));
    assert_eq!(host.ui.uploads.drafts[&channel].len(), 8);
}
