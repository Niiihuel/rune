use super::*;
use rune_components::message::attachment_ids;
use rune_model::{Attachment, AttachmentId, ImageKind, RemoteFile, RemoteImage};
use std::sync::Arc;

pub(super) fn source() -> RemoteImage {
    RemoteImage {
        url: "https://cdn.example.test/picture.png?signature=synthetic".into(),
        kind: ImageKind::AttachmentPreview,
    }
}

pub(super) fn populated() -> (Host, MessageId, AttachmentId) {
    let mut host = Host::new();
    let channel = host.open_channel("general");
    let mut message = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .last()
        .unwrap()
        .as_ref()
        .clone();
    message.id = MessageId(message.id.0 + 1);
    message.content = String::new();
    message.attachments = vec![Attachment {
        id: AttachmentId(82),
        filename: "fixture.png".into(),
        size_bytes: 12_288,
        content_type: Some("image/png".into()),
        dimensions: Some((800, 450)),
        preview: Some(source()),
        download: Some(RemoteFile { url: source().url }),
        spoiler: false,
        voice: None,
        explicit: false,
        placeholder: None,
    }];
    let id = message.id;
    host.event(Event::MessageCreated {
        message,
        nonce: None,
    });
    host.frame();
    host.tree.scroll_to_end(&timeline_id(channel));
    host.frame();
    (host, id, AttachmentId(82))
}

pub(super) fn image() -> limn::RasterImage {
    limn::RasterImage::new(
        "a".repeat(64),
        limn::RasterImageFormat::Png,
        3,
        Arc::from("AAAA"),
        "Fixture",
    )
    .unwrap()
}

pub(super) fn populated_pdf() -> (Host, MessageId, AttachmentId) {
    let (mut host, message, attachment) = populated();
    let channel = host.ui.channel().unwrap();
    let mut updated = host
        .store
        .timeline(channel)
        .unwrap()
        .get(message)
        .unwrap()
        .as_ref()
        .clone();
    updated.attachments[0].filename = "reporte.pdf".into();
    updated.attachments[0].content_type = Some("application/pdf".into());
    updated.attachments[0].preview = None;
    updated.attachments[0].dimensions = None;
    host.event(Event::MessageUpdated(updated));
    host.frame();
    (host, message, attachment)
}

#[test]
fn pdf_preview_requires_an_explicit_click_and_closing_rejects_late_pixels() {
    let (mut host, message, attachment) = populated_pdf();
    assert!(host.ui.document_viewer.is_none());
    assert!(!wanted_images(&host.ui, &host.store, &host.tree).contains(&source()));
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: attachment_ids::action(message, attachment, "pdf"),
        },
        host.now,
    );
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PreviewDocument { request, .. } => Some(*request),
            _ => None,
        })
        .expect("a click requests the first page");
    assert!(!intents
        .iter()
        .any(|intent| matches!(intent, Intent::OpenUrl(_) | Intent::Command(_))));
    host.run(intents);
    let result = document_result(
        &mut host.ui,
        &host.store,
        request,
        1,
        2,
        Vec::new(),
        Ok(image()),
    );
    host.run(result);
    let painted = host.painted().join(" ");
    assert!(painted.contains("reporte.pdf · Página 1 de 2"), "{painted}");
    host.key(Key::K, false, true);
    assert!(host.ui.switcher.is_none());
    let close = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: "document:close".into(),
        },
        host.now,
    );
    assert!(close.contains(&Intent::CancelDocument { request }));
    assert!(host.ui.document_viewer.is_none());
    document_result(
        &mut host.ui,
        &host.store,
        request,
        1,
        2,
        Vec::new(),
        Ok(image()),
    );
    assert!(host.ui.document_viewer.is_none());
}

#[test]
fn deleting_a_pdf_cancels_the_preview_and_discards_its_late_result() {
    let (mut host, message, attachment) = populated_pdf();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: attachment_ids::action(message, attachment, "pdf"),
        },
        host.now,
    );
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PreviewDocument { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap();
    host.run(intents);
    let channel = host.ui.channel().unwrap();
    let applied = host
        .store
        .apply(Event::MessageDeleted { channel, message }, host.now);
    let intents = on_applied(&mut host.ui, &mut host.store, &applied, host.now);
    assert!(intents.contains(&Intent::CancelDocument { request }));
    assert!(host.ui.document_viewer.is_none());
    document_result(
        &mut host.ui,
        &host.store,
        request,
        1,
        2,
        Vec::new(),
        Ok(image()),
    );
    assert!(host.ui.document_viewer.is_none());
}

#[test]
fn replacing_a_pdf_with_an_image_cancels_its_pending_render() {
    let (mut host, message, attachment) = populated_pdf();
    let channel = host.ui.channel().unwrap();
    let mut updated = host
        .store
        .timeline(channel)
        .unwrap()
        .get(message)
        .unwrap()
        .as_ref()
        .clone();
    let mut picture = updated.attachments[0].clone();
    picture.id = AttachmentId(83);
    picture.filename = "picture.png".into();
    picture.content_type = Some("image/png".into());
    picture.preview = Some(source());
    updated.attachments.push(picture);
    host.event(Event::MessageUpdated(updated));
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: attachment_ids::action(message, attachment, "pdf"),
        },
        host.now,
    );
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::PreviewDocument { request, .. } => Some(*request),
            _ => None,
        })
        .unwrap();
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: attachment_ids::action(message, AttachmentId(83), "preview"),
        },
        host.now,
    );
    assert!(intents.contains(&Intent::CancelDocument { request }));
    assert!(host.ui.document_viewer.is_none());
    assert!(host.ui.attachment_viewer.is_some());
    document_result(
        &mut host.ui,
        &host.store,
        request,
        1,
        2,
        Vec::new(),
        Ok(image()),
    );
    assert!(host.ui.document_viewer.is_none());
}

#[test]
fn visible_attachment_sources_are_requested_and_previews_follow_cache_status() {
    let (mut host, _, _) = populated();
    assert!(wanted_images(&host.ui, &host.store, &host.tree).contains(&source()));
    assert!(host.painted().iter().any(|text| text == "Cargando imagen…"));
    host.ui.image_failures.insert(source());
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "No se pudo cargar la imagen"));
    host.ui.image_failures.clear();
    host.ui.images.insert(source(), image());
    let ready = host.frame();
    assert!(ready.commands.iter().any(|command| matches!(command, limn::DrawCommand::RasterImage { image, .. } if image.sha256() == "a".repeat(64))));
    assert!(!host
        .painted()
        .iter()
        .any(|text| text == "Cargando imagen…" || text.contains("simulada")));
    host.ui.prefs.image_previews = false;
    host.frame();
    assert!(!wanted_images(&host.ui, &host.store, &host.tree).contains(&source()));
}

#[test]
fn image_viewer_zooms_closes_and_returns_focus_without_editing_the_conversation() {
    let (mut host, message, attachment) = populated();
    host.ui.images.insert(source(), image());
    let open = attachment_ids::action(message, attachment, "preview");
    host.click(&open);
    host.frame();
    assert!(host.ui.attachment_viewer.is_some());
    assert_eq!(host.tree.focused_id(), Some(crate::attachments::CLOSE));
    let fitted = host.ui.attachment_viewer.as_ref().unwrap().zoom;
    host.click("attachment-viewer:zoom-in");
    assert!(host.ui.attachment_viewer.as_ref().unwrap().zoom > fitted);
    host.key(Key::K, false, true);
    assert!(host.ui.switcher.is_none());
    host.key(Key::ArrowUp, false, false);
    assert!(host.ui.editing.is_none());
    host.key(Key::Escape, false, false);
    host.frame();
    assert!(host.ui.attachment_viewer.is_none());
    assert_eq!(host.tree.focused_id(), Some(open.as_str()));
}

#[test]
fn attachment_actions_resolve_current_metadata_and_never_fetch_a_file_implicitly() {
    let (mut host, message, attachment) = populated();
    let apply = |host: &mut Host, verb| {
        update(
            &mut host.ui,
            &mut host.store,
            &UiAction::Clicked {
                id: attachment_ids::action(message, attachment, verb),
            },
            host.now,
        )
    };
    assert_eq!(
        apply(&mut host, "copy"),
        vec![Intent::Copy(source().url.clone())]
    );
    assert_eq!(
        apply(&mut host, "open"),
        vec![Intent::OpenUrl(source().url.clone())]
    );
    assert_eq!(
        apply(&mut host, "download"),
        vec![Intent::DownloadFile {
            source: RemoteFile { url: source().url },
            filename: "fixture.png".into(),
            size_bytes: 12_288,
        }]
    );
    assert!(apply(&mut host, "preview")
        .iter()
        .all(|intent| matches!(intent, Intent::Focus(_))));
    let channel = host.ui.channel().unwrap();
    host.event(Event::MessageDeleted { channel, message });
    assert!(host.ui.attachment_viewer.is_none());
    assert!(apply(&mut host, "open").is_empty());
    assert!(apply(&mut host, "download").is_empty());
}

#[test]
fn modal_keeps_its_preview_wanted_and_logout_removes_all_account_image_state() {
    let (mut host, message, attachment) = populated();
    host.click(&attachment_ids::action(message, attachment, "preview"));
    host.ui.prefs.image_previews = false;
    host.frame();
    assert!(wanted_images(&host.ui, &host.store, &host.tree).contains(&source()));
    host.ui.images.insert(source(), image());
    host.ui.image_failures.insert(source());
    host.event(Event::Authentication(
        rune_provider::AuthenticationState::SignedOut,
    ));
    assert!(host.ui.images.is_empty());
    assert!(host.ui.image_failures.is_empty());
    assert!(host.ui.attachment_viewer.is_none());
    host.frame();
    assert!(!wanted_images(&host.ui, &host.store, &host.tree).contains(&source()));
}

#[test]
fn navigating_to_a_different_conversation_closes_the_viewer() {
    let (mut host, message, attachment) = populated();
    host.click(&attachment_ids::action(message, attachment, "preview"));
    let current = host.ui.channel().unwrap();
    let next = host
        .provider
        .snapshot()
        .channels
        .into_iter()
        .find(|channel| {
            channel.id != current && channel.kind.has_messages() && Store::visible(channel)
        })
        .map(|channel| (channel.id, channel.server))
        .unwrap();
    open(&mut host.ui, &mut host.store, next.1, next.0);
    assert!(host.ui.attachment_viewer.is_none());
}

#[test]
fn the_attachment_menu_is_reachable_by_keyboard_and_offers_explicit_file_actions() {
    let (mut host, message, attachment) = populated();
    let preview = attachment_ids::action(message, attachment, "preview");
    host.tree.focus(&preview);
    host.send(UiEvent::ContextMenuKeyboard);
    assert!(host.tree.context_menu_open());
    let painted = host.painted();
    assert!(painted.iter().any(|text| text == "Copiar enlace"));
    assert!(painted.iter().any(|text| text == "Guardar en Descargas"));
    host.key(Key::Escape, false, false);
    assert!(!host.tree.context_menu_open());
    assert!(host.ui.attachment_viewer.is_none());
}

#[test]
fn account_replacement_cannot_reuse_an_open_image_even_with_the_same_entity_ids() {
    let (mut host, message, attachment) = populated();
    host.click(&attachment_ids::action(message, attachment, "preview"));
    let mut snapshot = host.provider.snapshot();
    let mut other = snapshot.me.clone().unwrap();
    other.id = rune_model::UserId(other.id.0 + 90_000);
    snapshot.me = Some(other);
    host.event(Event::Ready(Box::new(snapshot)));
    assert!(host.ui.attachment_viewer.is_none());
}

#[test]
fn attachment_requests_exclude_history_outside_the_virtualized_rows() {
    let (mut host, message, _) = populated();
    let channel = host.ui.channel().unwrap();
    let template = host
        .store
        .timeline(channel)
        .unwrap()
        .get(message)
        .unwrap()
        .clone();
    for index in 1..=80 {
        let mut next = template.as_ref().clone();
        next.id = MessageId(message.0 + index);
        next.attachments[0].preview.as_mut().unwrap().url =
            format!("https://cdn.example.test/{index}.png");
        host.event(Event::MessageCreated {
            message: next,
            nonce: None,
        });
    }
    host.frame();
    host.tree.scroll_to_end(&timeline_id(channel));
    host.frame();
    let wanted = wanted_images(&host.ui, &host.store, &host.tree);
    assert!(!wanted.contains(&source()));
    assert!(wanted
        .iter()
        .any(|source| source.url == "https://cdn.example.test/80.png"));
    assert!(
        wanted
            .iter()
            .filter(|source| source.kind == ImageKind::AttachmentPreview)
            .count()
            < 12
    );
}

#[test]
fn save_as_asks_with_ids_only_shows_its_phases_and_ends_once() {
    let (mut host, message, attachment) = populated();
    let channel = host.ui.channel().unwrap();
    let save_as = attachment_ids::action(message, attachment, "save-as");
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: save_as.clone(),
        },
        host.now,
    );
    let request = host.ui.save_as.as_ref().unwrap().request;
    assert_eq!(
        intents,
        vec![Intent::SaveAttachmentAs {
            request,
            channel,
            message,
            attachment
        }]
    );
    assert_eq!(
        crate::save_as::attachment_download(&host.store, channel, message, attachment),
        Some((
            RemoteFile { url: source().url },
            "fixture.png".into(),
            12_288
        ))
    );
    host.frame();
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Eligiendo dónde guardar…"));
    // One at a time: a second one says so and asks nothing.
    let again = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked { id: save_as },
        host.now,
    );
    assert!(again.is_empty());
    assert_eq!(host.ui.save_as.as_ref().unwrap().request, request);
    crate::save_saving(&mut host.ui, request);
    crate::save_progress(&mut host.ui, request, 6_144, 12_288);
    host.frame();
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Guardando archivo…"));
    assert_eq!(
        update(
            &mut host.ui,
            &mut host.store,
            &UiAction::Clicked {
                id: crate::save_as::CANCEL.into()
            },
            host.now,
        ),
        vec![Intent::CancelSaveAttachment { request }]
    );
    // Another request's end touches nothing; its own clears it quietly.
    crate::save_finished(&mut host.ui, request + 1, Ok("otro".into()));
    assert!(host.ui.save_as.is_some());
    let notices = host.ui.notices.len();
    crate::save_finished(&mut host.ui, request, Err(None));
    assert!(host.ui.save_as.is_none());
    assert_eq!(host.ui.notices.len(), notices, "a cancellation is no error");
}

#[test]
fn a_saved_or_refused_file_says_so_and_another_account_cancels_it() {
    let (mut host, message, attachment) = populated();
    let save_as = attachment_ids::action(message, attachment, "save-as");
    let click = |host: &mut Host| {
        update(
            &mut host.ui,
            &mut host.store,
            &UiAction::Clicked {
                id: save_as.clone(),
            },
            host.now,
        );
        host.ui.save_as.as_ref().unwrap().request
    };
    let request = click(&mut host);
    crate::save_finished(&mut host.ui, request, Ok("fixture.png".into()));
    let notice = host.ui.notices.last().unwrap();
    assert_eq!(notice.title, "Archivo guardado");
    assert_eq!(notice.detail.as_deref(), Some("fixture.png"));
    let request = click(&mut host);
    crate::save_finished(&mut host.ui, request, Err(Some(rune_core_exists().into())));
    let notice = host.ui.notices.last().unwrap();
    assert!(notice.error && notice.title == "No se pudo guardar el archivo");
    let request = click(&mut host);
    host.store.apply(
        Event::Authentication(rune_provider::AuthenticationState::SignedOut),
        host.now,
    );
    assert_eq!(
        crate::save_as::refresh(&mut host.ui, &host.store),
        vec![Intent::CancelSaveAttachment { request }]
    );
    assert!(host.ui.save_as.is_none());
}

// Linux desktop (escritorio-linux): replacing with the person's word (08.3).
#[test]
fn a_file_at_the_chosen_name_is_replaced_only_once_the_person_says_so() {
    let (mut host, message, attachment) = populated();
    let save_as = attachment_ids::action(message, attachment, "save-as");
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: save_as.clone(),
        },
        host.now,
    );
    let request = host.ui.save_as.as_ref().unwrap().request;
    // Before Core asks, there is nothing to replace.
    assert!(update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: crate::save_as::REPLACE.into()
        },
        host.now,
    )
    .is_empty());
    crate::save_exists(&mut host.ui, request + 1, "otro.png".into());
    assert_eq!(host.ui.save_as.as_ref().unwrap().phase, SavePhase::Choosing);
    crate::save_exists(&mut host.ui, request, "elegido.png".into());
    host.frame();
    let painted = host.painted();
    assert!(painted
        .iter()
        .any(|text| text == "Ya hay un archivo con ese nombre"));
    assert!(painted.iter().any(|text| text == "elegido.png"));
    assert!(painted.iter().any(|text| text == "Reemplazar"));
    assert_eq!(
        update(
            &mut host.ui,
            &mut host.store,
            &UiAction::Clicked {
                id: crate::save_as::REPLACE.into()
            },
            host.now,
        ),
        vec![Intent::ReplaceSavedFile { request }]
    );
    // Once: the bar waits for the download, without the question.
    assert!(matches!(
        host.ui.save_as.as_ref().unwrap().phase,
        SavePhase::Saving { .. }
    ));
    host.frame();
    assert!(!host.painted().iter().any(|text| text == "Reemplazar"));
    // Cancelling while asked is the same cancel.
    crate::save_finished(&mut host.ui, request, Err(None));
    update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: save_as.clone(),
        },
        host.now,
    );
    let request = host.ui.save_as.as_ref().unwrap().request;
    crate::save_exists(&mut host.ui, request, "elegido.png".into());
    assert_eq!(
        update(
            &mut host.ui,
            &mut host.store,
            &UiAction::Clicked {
                id: crate::save_as::CANCEL.into()
            },
            host.now,
        ),
        vec![Intent::CancelSaveAttachment { request }]
    );
}

/// What Core says of a name already taken (the UI does not depend on Core).
fn rune_core_exists() -> &'static str {
    "Ese archivo ya existe. Elegí un nombre diferente."
}

#[test]
fn the_image_viewer_offers_save_as_with_the_same_action_as_the_menu() {
    let (mut host, message, attachment) = populated();
    host.click(&attachment_ids::action(message, attachment, "preview"));
    host.frame();
    let save_as = attachment_ids::action(message, attachment, "save-as");
    let frame = host.frame();
    let node = frame
        .access
        .iter()
        .find(|node| node.id == save_as)
        .expect("the viewer's header has it");
    assert_eq!(node.name, "Guardar como…");
    host.click(&save_as);
    assert!(host.ui.save_as.is_some(), "the viewer starts it");
    assert!(host.ui.attachment_viewer.is_some(), "and stays open");
}
