use super::*;

#[test]
fn dropping_files_prepares_only_metadata_without_moving_focus_or_touching_the_draft() {
    let (mut host, channel) = host();
    multiple(&mut host, 8, 1024, 1024);
    seed(&mut host, channel, 1);
    host.type_text("a🙂bc");
    let reply = host.store.timeline(channel).unwrap().messages()[0].id;
    host.ui.reply_to = Some(reply);
    host.frame();
    host.tree.set_input_cursor(&editor_id(channel), 1);
    host.key(Key::ArrowRight, true, false);
    let selection = host.tree.input_selection(&editor_id(channel));
    let focused = host.tree.focused_id().map(str::to_owned);
    let revision = host.ui.draft_revision;
    let sent = host.upload_intents.len();
    let request = begin_attachment_import(
        &mut host.ui,
        &host.store,
        channel,
        AttachmentImportSource::Drop,
    )
    .unwrap();
    assert!(attachment_pick_pending(&host.ui));
    assert_eq!(host.ui.draft_revision, revision);
    assert_eq!(
        host.upload_intents.len(),
        sent,
        "beginning a drop never opens a picker or sends files"
    );
    let text = host.painted().join(" ");
    assert!(text.contains("Preparando archivos…"));
    assert!(!text.contains("Seleccionando y preparando"));
    assert_eq!(host.tree.focused_id(), focused.as_deref());
    assert_eq!(host.tree.input_selection(&editor_id(channel)), selection);
    assert!(prepared(&mut host, channel, request, vec![file(2), file(3)]).is_empty());
    host.frame();
    assert_eq!(
        host.ui.uploads.drafts[&channel],
        vec![file(1), file(2), file(3)]
    );
    assert_eq!(host.ui.drafts[&channel], "a🙂bc");
    assert_eq!(host.ui.reply_to, Some(reply));
    assert_eq!(host.tree.input_selection(&editor_id(channel)), selection);
    assert_eq!(host.tree.focused_id(), focused.as_deref());
    assert!(!host
        .upload_intents
        .iter()
        .any(|intent| matches!(intent, Intent::SendAttachments { .. })));
}

#[test]
fn a_drop_admission_error_releases_the_preparation_slot_and_keeps_existing_inputs() {
    let (mut host, channel) = host();
    multiple(&mut host, 8, 1024, 1024);
    seed(&mut host, channel, 1);
    host.type_text("conservar");
    let request = begin_attachment_import(
        &mut host.ui,
        &host.store,
        channel,
        AttachmentImportSource::Drop,
    )
    .unwrap();
    let account = host.store.me().unwrap();
    let intents = upload_update(
        &mut host.ui,
        &host.store,
        UploadUpdate::Prepared {
            account,
            channel,
            request,
            result: Err(UploadError::Busy),
        },
    );
    assert!(intents.is_empty());
    assert!(!attachment_pick_pending(&host.ui));
    assert_eq!(host.ui.uploads.drafts[&channel], vec![file(1)]);
    assert_eq!(host.ui.drafts[&channel], "conservar");
    assert!(begin_attachment_import(
        &mut host.ui,
        &host.store,
        channel,
        AttachmentImportSource::Drop
    )
    .is_ok());
}

#[test]
fn cancelling_a_drop_keeps_the_hold_until_its_terminal_and_discards_the_whole_late_batch() {
    let (mut host, channel) = host();
    multiple(&mut host, 8, 1024, 1024);
    seed(&mut host, channel, 1);
    let request = begin_attachment_import(
        &mut host.ui,
        &host.store,
        channel,
        AttachmentImportSource::Drop,
    )
    .unwrap();
    host.frame();
    host.click(&format!("composer:{channel}:pick:{request}:cancel"));
    assert!(
        matches!(host.upload_intents.last(),Some(Intent::CancelUpload{request:found}) if *found==request)
    );
    assert!(attachment_pick_pending(&host.ui));
    assert!(host.painted().join(" ").contains("Cancelando preparación…"));
    assert_eq!(
        prepared(&mut host, channel, request, vec![file(2), file(3)]),
        vec![
            Intent::RemoveAttachment {
                channel,
                token: UploadToken(2)
            },
            Intent::RemoveAttachment {
                channel,
                token: UploadToken(3)
            }
        ]
    );
    assert!(!attachment_pick_pending(&host.ui));
    assert_eq!(host.ui.uploads.drafts[&channel], vec![file(1)]);
}

#[test]
fn drop_preparation_revalidates_the_channel_account_connection_and_editing_state() {
    for rejection in ["channel", "account", "offline", "editing", "capability"] {
        let (mut host, channel) = host();
        host.type_text("conservar");
        let target = if rejection == "channel" {
            ChannelId(999)
        } else {
            channel
        };
        match rejection {
            "account" => host.ui.uploads.account = Some(rune_model::UserId(u64::MAX)),
            "offline" => {
                host.store.connection = rune_provider::ConnectionState::Disconnected {
                    reason: "fixture".into(),
                }
            }
            "editing" => {
                host.ui.editing = Some(host.store.timeline(channel).unwrap().messages()[0].id)
            }
            "capability" => {
                host.store
                    .provider
                    .as_mut()
                    .unwrap()
                    .capabilities
                    .attachments = false
            }
            _ => {}
        }
        assert_eq!(
            begin_attachment_import(
                &mut host.ui,
                &host.store,
                target,
                AttachmentImportSource::Drop
            ),
            Err(UploadError::Denied),
            "{rejection}"
        );
        assert!(!attachment_pick_pending(&host.ui), "{rejection}");
        assert_eq!(host.ui.drafts[&channel], "conservar", "{rejection}");
    }
}

#[test]
fn drop_preparation_waits_while_a_modal_menu_or_another_preparation_owns_the_ui() {
    for owner in [
        "settings",
        "emoji",
        "plus",
        "message",
        "profile",
        "server",
        "pins",
        "preparation",
    ] {
        let (mut host, channel) = host();
        host.type_text("conservar");
        match owner {
            "settings" => host.ui.settings = Some(SettingsPage::Account),
            "emoji" => host.ui.emoji_open = true,
            "plus" => host.ui.messages.plus = true,
            "message" => {
                host.ui.messages.more = Some(host.store.timeline(channel).unwrap().messages()[0].id)
            }
            "profile" => host.ui.status_menu = true,
            "server" => host.ui.server_menu.open = true,
            "pins" => host.ui.channel_list.pins = Some(channel),
            "preparation" => {
                begin_attachment_import(
                    &mut host.ui,
                    &host.store,
                    channel,
                    AttachmentImportSource::Drop,
                )
                .unwrap();
            }
            _ => unreachable!(),
        }
        assert_eq!(
            begin_attachment_import(
                &mut host.ui,
                &host.store,
                channel,
                AttachmentImportSource::Drop
            ),
            Err(UploadError::Busy),
            "{owner}"
        );
        assert_eq!(host.ui.drafts[&channel], "conservar", "{owner}");
        assert_eq!(
            attachment_pick_pending(&host.ui),
            owner == "preparation",
            "{owner}"
        );
    }
}

#[test]
fn drop_preparation_obeys_current_capacity_and_waits_for_send_admission() {
    for rejection in ["count", "global", "bytes", "unavailable", "admission"] {
        let (mut host, channel) = host();
        let expected = match rejection {
            "count" => {
                seed(&mut host, channel, 1);
                UploadError::TooMany
            }
            "global" => {
                host.ui
                    .uploads
                    .drafts
                    .insert(ChannelId(999), (1..=10).map(file).collect());
                UploadError::MemoryLimit
            }
            "bytes" => {
                multiple(&mut host, 8, 1024, 3);
                host.type_text("áá");
                UploadError::TooLarge
            }
            "unavailable" => {
                set_upload_limits(&mut host.ui, None);
                UploadError::Unavailable
            }
            "admission" => {
                multiple(&mut host, 8, 1024, 1024);
                seed(&mut host, channel, 1);
                submit(&mut host, channel, "");
                UploadError::Busy
            }
            _ => unreachable!(),
        };
        let revision = host.ui.draft_revision;
        assert_eq!(
            begin_attachment_import(
                &mut host.ui,
                &host.store,
                channel,
                AttachmentImportSource::Drop
            ),
            Err(expected),
            "{rejection}"
        );
        assert!(!attachment_pick_pending(&host.ui), "{rejection}");
        assert_eq!(host.ui.draft_revision, revision, "{rejection}");
    }
}
