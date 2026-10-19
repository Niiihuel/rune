use super::*;
use rune_model::{PendingAttachment, UploadToken};
use rune_provider::{UploadError, UploadUpdate};

fn file(token: u64) -> PendingAttachment {
    PendingAttachment {
        token: UploadToken(token),
        filename: format!("fixture-{token}.txt"),
        size_bytes: 12,
    }
}

fn host() -> (Host, ChannelId) {
    let mut host = Host::new();
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .attachments = true;
    set_upload_limits(
        &mut host.ui,
        Some(rune_provider::UploadLimits {
            max_files: 1,
            max_file_bytes: 1024,
            max_message_bytes: 1024,
        }),
    );
    let channel = host.open_channel("general");
    (host, channel)
}

fn seed(host: &mut Host, channel: ChannelId, token: u64) {
    host.ui.uploads.account = host.store.me();
    host.ui
        .uploads
        .drafts
        .entry(channel)
        .or_default()
        .push(file(token));
}

fn pick(host: &mut Host) -> u64 {
    host.click(rune_components::composer::ids::PLUS);
    host.click(rune_components::composer::ids::ATTACH);
    match host
        .upload_intents
        .last()
        .expect("the attachment action must emit a typed pick intent")
    {
        Intent::PickAttachment { request, .. } => *request,
        other => panic!("expected pick, got {other:?}"),
    }
}

fn submit(host: &mut Host, channel: ChannelId, text: &str) -> u64 {
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Submitted {
            id: editor_id(channel),
            value: text.into(),
        },
        host.now,
    );
    let request = intents
        .iter()
        .find_map(|intent| match intent {
            Intent::SendAttachments { request, .. } => Some(*request),
            _ => None,
        })
        .expect("files require a typed send intent");
    host.run(intents);
    request
}

#[test]
fn selecting_a_file_stages_metadata_without_sending_or_consuming_text() {
    let (mut host, channel) = host();
    host.type_text("borrador");
    let request = pick(&mut host);
    let account = host.store.me().unwrap();
    let intents = upload_update(
        &mut host.ui,
        &host.store,
        UploadUpdate::Prepared {
            account,
            channel,
            request,
            result: Ok(vec![file(1)]),
        },
    );
    assert!(intents.is_empty());
    assert_eq!(host.ui.uploads.drafts[&channel], vec![file(1)]);
    assert_eq!(host.ui.drafts[&channel], "borrador");
    assert!(host
        .sent
        .iter()
        .all(|command| !matches!(command, Command::SendMessage { .. })));
}

#[test]
fn a_file_only_submission_is_admitted_before_consuming_the_draft() {
    let (mut host, channel) = host();
    seed(&mut host, channel, 1);
    let request = submit(&mut host, channel, "");
    assert!(attachment_send_pending(
        &host.ui,
        request,
        channel,
        &[UploadToken(1)]
    ));
    assert_eq!(host.ui.uploads.drafts[&channel].len(), 1);
    let intents =
        attachment_send_admitted(&mut host.ui, &mut host.store, request, Ok(8), Timestamp(10));
    host.run(intents);
    assert!(host
        .ui
        .uploads
        .drafts
        .get(&channel)
        .is_none_or(Vec::is_empty));
    let shown = host
        .store
        .timeline(channel)
        .unwrap()
        .get(MessageId(u64::MAX - 8))
        .unwrap();
    assert!(shown.content.is_empty() && shown.attachments.is_empty());
    assert_eq!(
        host.store.pending_attachments(channel, shown.id),
        &[file(1)]
    );
}

#[test]
fn old_admission_preserves_new_text_and_new_attachment_tokens() {
    let (mut host, channel) = host();
    seed(&mut host, channel, 1);
    host.type_text("anterior");
    let request = submit(&mut host, channel, "anterior");
    host.key(Key::A, false, true);
    host.type_text("nuevo borrador");
    host.ui.uploads.drafts.insert(channel, vec![file(2)]);
    let intents =
        attachment_send_admitted(&mut host.ui, &mut host.store, request, Ok(9), Timestamp(10));
    host.run(intents);
    assert_eq!(host.ui.drafts[&channel], "nuevo borrador");
    assert_eq!(host.ui.uploads.drafts[&channel], vec![file(2)]);
    assert_eq!(
        host.store
            .pending_attachments(channel, MessageId(u64::MAX - 9)),
        &[file(1)]
    );
}

#[test]
fn a_rejected_admission_keeps_the_whole_draft_for_explicit_retry() {
    let (mut host, channel) = host();
    seed(&mut host, channel, 1);
    host.type_text("con archivo");
    let request = submit(&mut host, channel, "con archivo");
    let intents = attachment_send_admitted(
        &mut host.ui,
        &mut host.store,
        request,
        Err(UploadError::Busy),
        Timestamp(10),
    );
    host.run(intents);
    assert_eq!(host.ui.drafts[&channel], "con archivo");
    assert_eq!(host.ui.uploads.drafts[&channel], vec![file(1)]);
    assert!(!attachment_send_pending(
        &host.ui,
        request,
        channel,
        &[UploadToken(1)]
    ));
}

#[test]
fn a_late_selection_updates_only_its_original_channel_without_focus() {
    let (mut host, channel) = host();
    let request = pick(&mut host);
    let other = host.open_channel("rendimiento");
    assert_ne!(channel, other);
    host.type_text("otro canal");
    let account = host.store.me().unwrap();
    let intents = upload_update(
        &mut host.ui,
        &host.store,
        UploadUpdate::Prepared {
            account,
            channel,
            request,
            result: Ok(vec![file(3)]),
        },
    );
    assert!(intents.is_empty());
    assert_eq!(host.ui.uploads.drafts[&channel], vec![file(3)]);
    assert_eq!(host.ui.drafts[&other], "otro canal");
    assert_eq!(host.ui.channel(), Some(other));
}

#[test]
fn retrying_a_failed_file_never_reconstructs_a_text_only_send() {
    let (mut host, channel) = host();
    host.store.add_pending_attachments(
        channel,
        "archivo".into(),
        None,
        15,
        100,
        vec![file(1)],
        Timestamp(10),
    );
    host.event(Event::Failed {
        failure: rune_provider::Failure::SendFiles {
            channel,
            nonce: 15,
            request: 100,
        },
        reason: "fixture".into(),
        ambiguous: false,
    });
    host.click(&format!("msg:{}:retry", u64::MAX - 15));
    assert!(
        matches!(host.upload_intents.last(), Some(Intent::RetryAttachmentSend { channel: found, message, .. }) if *found==channel && message.0==u64::MAX-15)
    );
    assert!(
        !matches!(host.sent.last(), Some(Command::SendMessage { content, .. }) if content=="archivo")
    );
}

#[test]
fn cancellation_and_revocation_release_tokens_without_losing_text() {
    let (mut host, channel) = host();
    seed(&mut host, channel, 1);
    host.type_text("conservar");
    host.open_channel("rendimiento");
    let request = pick(&mut host);
    host.store
        .provider
        .as_mut()
        .unwrap()
        .capabilities
        .attachments = false;
    let intents = on_applied(
        &mut host.ui,
        &mut host.store,
        &rune_state::Applied::Channel(channel),
        host.now,
    );
    assert!(intents.iter().any(
        |intent| matches!(intent, Intent::CancelUpload { request: found } if *found==request)
    ));
    assert!(intents.iter().any(|intent| matches!(intent, Intent::RemoveAttachment { channel: found, token } if *found==channel && *token==UploadToken(1))));
    assert_eq!(host.ui.drafts[&channel], "conservar");
    assert!(host
        .ui
        .uploads
        .drafts
        .get(&channel)
        .is_none_or(Vec::is_empty));
}

#[test]
fn cancelling_a_native_selection_keeps_the_reading_hold_until_its_terminal_callback() {
    let (mut host, channel) = host();
    host.type_text("conservar");
    let request = pick(&mut host);
    host.click(&format!("composer:{channel}:pick:{request}:cancel"));
    assert!(
        attachment_pick_pending(&host.ui),
        "the native dialog may still be closing"
    );
    assert!(crate::reads::modal_obscured(&host.ui));
    let account = host.store.me().unwrap();
    let intents = upload_update(
        &mut host.ui,
        &host.store,
        UploadUpdate::Prepared {
            account,
            channel,
            request,
            result: Err(UploadError::Cancelled),
        },
    );
    assert!(intents.is_empty());
    assert!(!attachment_pick_pending(&host.ui));
    assert_eq!(host.ui.drafts[&channel], "conservar");
}

#[test]
fn message_limit_counts_utf8_text_in_addition_to_file_bytes() {
    let (mut host, channel) = host();
    seed(&mut host, channel, 1);
    set_upload_limits(
        &mut host.ui,
        Some(rune_provider::UploadLimits {
            max_files: 1,
            max_file_bytes: 20,
            max_message_bytes: 15,
        }),
    );
    host.type_text("áá");
    let intents = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Submitted {
            id: editor_id(channel),
            value: "áá".into(),
        },
        host.now,
    );
    assert!(!intents
        .iter()
        .any(|intent| matches!(intent, Intent::SendAttachments { .. } | Intent::Send { .. })));
    assert_eq!(host.ui.drafts[&channel], "áá");
    assert_eq!(host.ui.uploads.drafts[&channel], vec![file(1)]);
}

#[test]
fn enter_in_an_empty_real_editor_emits_a_file_only_send() {
    let (mut host, channel) = host();
    seed(&mut host, channel, 1);
    host.frame();
    host.key(Key::Enter, false, false);
    assert!(
        matches!(host.upload_intents.last(),Some(Intent::SendAttachments {channel:found,content,files,..}) if *found==channel && content.is_empty() && files==&[UploadToken(1)])
    );
}

#[test]
fn a_sending_file_has_keyboard_cancellation_and_keeps_its_ambiguous_body() {
    let (mut host, channel) = host();
    host.store.add_pending_attachments(
        channel,
        "archivo".into(),
        None,
        71,
        701,
        vec![file(1)],
        Timestamp(10),
    );
    host.frame();
    host.run(vec![Intent::ScrollToEnd(timeline_id(channel))]);
    let id = format!("msg:{}:upload:cancel", u64::MAX - 71);
    let frame = host.frame();
    assert!(
        frame.regions.iter().any(|region| region.id == id),
        "an in-flight file requires its own cancellation control"
    );
    assert!(host.tree.focus(&id));
    host.key(Key::Enter, false, false);
    assert!(matches!(
        host.upload_intents.last(),
        Some(Intent::CancelUpload { request: 701 })
    ));
    host.event(Event::Failed {
        failure: rune_provider::Failure::SendFiles {
            request: 701,
            channel,
            nonce: 71,
        },
        reason: "cancelled after starting".into(),
        ambiguous: true,
    });
    assert!(
        host.store
            .unsent(channel, MessageId(u64::MAX - 71))
            .unwrap()
            .ambiguous
    );
    assert_eq!(
        host.store
            .pending_attachments(channel, MessageId(u64::MAX - 71)),
        &[file(1)]
    );
    assert!(!host.frame().regions.iter().any(|region| region.id == id));
    assert!(!host
        .sent
        .iter()
        .any(|command| matches!(command, Command::SendMessage { .. })));
}

#[test]
fn an_old_admission_cannot_clear_a_new_reply_to_the_same_message() {
    let (mut host, channel) = host();
    let reply = host
        .store
        .timeline(channel)
        .unwrap()
        .messages()
        .iter()
        .find(|message| !message.delivery.is_local())
        .unwrap()
        .id;
    seed(&mut host, channel, 1);
    host.ui.reply_to = Some(reply);
    host.type_text("anterior");
    let request = submit(&mut host, channel, "anterior");
    let cancelled = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: rune_components::composer::ids::CANCEL.into(),
        },
        host.now,
    );
    host.run(cancelled);
    assert_eq!(host.ui.reply_to, None);
    let replied = update(
        &mut host.ui,
        &mut host.store,
        &UiAction::Clicked {
            id: format!("msg:{reply}:reply"),
        },
        host.now,
    );
    host.run(replied);
    assert_eq!(host.ui.reply_to, Some(reply));
    let intents = attachment_send_admitted(
        &mut host.ui,
        &mut host.store,
        request,
        Ok(90),
        Timestamp(10),
    );
    host.run(intents);
    assert_eq!(
        host.ui.reply_to,
        Some(reply),
        "a new reply action belongs to the new draft even when its ID repeats"
    );
}

#[test]
fn preparation_limit_includes_current_utf8_text_and_previously_staged_files() {
    let (mut host, channel) = host();
    set_upload_limits(
        &mut host.ui,
        Some(rune_provider::UploadLimits {
            max_files: 8,
            max_file_bytes: 12,
            max_message_bytes: 25,
        }),
    );
    seed(&mut host, channel, 1);
    host.type_text("áá");
    let reply = host.store.timeline(channel).unwrap().messages()[0].id;
    host.ui.reply_to = Some(reply);
    let request = pick(&mut host);
    let account = host.store.me().unwrap();
    let intents = upload_update(
        &mut host.ui,
        &host.store,
        UploadUpdate::Prepared {
            account,
            channel,
            request,
            result: Ok(vec![file(2)]),
        },
    );
    assert_eq!(
        intents,
        vec![Intent::RemoveAttachment {
            channel,
            token: UploadToken(2)
        }]
    );
    assert_eq!(host.ui.uploads.drafts[&channel], vec![file(1)]);
    assert_eq!(host.ui.drafts[&channel], "áá");
    assert_eq!(host.ui.reply_to, Some(reply));
}

fn multiple(host: &mut Host, max_files: usize, max_file_bytes: u64, max_message_bytes: u64) {
    set_upload_limits(
        &mut host.ui,
        Some(rune_provider::UploadLimits {
            max_files,
            max_file_bytes,
            max_message_bytes,
        }),
    );
}

fn prepared(
    host: &mut Host,
    channel: ChannelId,
    request: u64,
    files: Vec<PendingAttachment>,
) -> Vec<Intent> {
    let account = host.store.me().unwrap();
    upload_update(
        &mut host.ui,
        &host.store,
        UploadUpdate::Prepared {
            account,
            channel,
            request,
            result: Ok(files),
        },
    )
}

#[test]
fn a_complete_batch_appends_in_order_with_one_revision_and_preserves_the_draft() {
    let (mut host, channel) = host();
    multiple(&mut host, 8, 1024, 1024);
    seed(&mut host, channel, 1);
    host.type_text("conservar");
    let reply = host.store.timeline(channel).unwrap().messages()[0].id;
    host.ui.reply_to = Some(reply);
    let request = pick(&mut host);
    let revision = host.ui.draft_revision;
    assert!(prepared(&mut host, channel, request, vec![file(2), file(3)]).is_empty());
    assert_eq!(
        host.ui.uploads.drafts[&channel],
        vec![file(1), file(2), file(3)]
    );
    assert_eq!(host.ui.draft_revision, revision + 1);
    assert_eq!(host.ui.drafts[&channel], "conservar");
    assert_eq!(host.ui.reply_to, Some(reply));
    assert!(!host
        .upload_intents
        .iter()
        .any(|intent| matches!(intent, Intent::SendAttachments { .. })));
}

#[test]
fn a_batch_rejected_by_count_or_its_final_file_preserves_every_previous_input() {
    for constraint in ["count", "individual", "total"] {
        let (mut host, channel) = host();
        multiple(
            &mut host,
            if constraint == "count" { 2 } else { 8 },
            12,
            if constraint == "total" { 39 } else { 1024 },
        );
        seed(&mut host, channel, 1);
        host.type_text("áá");
        let reply = host.store.timeline(channel).unwrap().messages()[0].id;
        host.ui.reply_to = Some(reply);
        let request = pick(&mut host);
        let revision = host.ui.draft_revision;
        let mut last = file(3);
        if constraint == "individual" {
            last.size_bytes = 13;
        }
        let intents = prepared(&mut host, channel, request, vec![file(2), last]);
        assert_eq!(
            intents,
            vec![
                Intent::RemoveAttachment {
                    channel,
                    token: UploadToken(2)
                },
                Intent::RemoveAttachment {
                    channel,
                    token: UploadToken(3)
                }
            ],
            "{constraint}"
        );
        assert_eq!(
            host.ui.uploads.drafts[&channel],
            vec![file(1)],
            "{constraint}"
        );
        assert_eq!(host.ui.draft_revision, revision, "{constraint}");
        assert_eq!(host.ui.drafts[&channel], "áá", "{constraint}");
        assert_eq!(host.ui.reply_to, Some(reply), "{constraint}");
    }
}

#[test]
fn an_obsolete_cancelled_or_revoked_batch_releases_all_new_tokens() {
    for terminal in ["obsolete", "cancelled", "revoked"] {
        let (mut host, channel) = host();
        multiple(&mut host, 8, 1024, 1024);
        seed(&mut host, channel, 1);
        host.type_text("conservar");
        let request = pick(&mut host);
        let callback = if terminal == "obsolete" {
            request + 1
        } else {
            request
        };
        if terminal == "cancelled" {
            host.click(&format!("composer:{channel}:pick:{request}:cancel"));
        }
        if terminal == "revoked" {
            host.store
                .provider
                .as_mut()
                .unwrap()
                .capabilities
                .attachments = false;
        }
        let revision = host.ui.draft_revision;
        let intents = prepared(&mut host, channel, callback, vec![file(2), file(3)]);
        assert_eq!(
            intents,
            vec![
                Intent::RemoveAttachment {
                    channel,
                    token: UploadToken(2)
                },
                Intent::RemoveAttachment {
                    channel,
                    token: UploadToken(3)
                }
            ],
            "{terminal}"
        );
        assert_eq!(
            host.ui.uploads.drafts[&channel],
            vec![file(1)],
            "{terminal}"
        );
        assert_eq!(host.ui.draft_revision, revision, "{terminal}");
        assert_eq!(host.ui.drafts[&channel], "conservar", "{terminal}");
    }
}

#[test]
fn a_repeated_prepared_callback_cannot_release_the_tokens_already_staged() {
    let (mut host, channel) = host();
    multiple(&mut host, 8, 1024, 1024);
    let request = pick(&mut host);
    assert!(prepared(&mut host, channel, request, vec![file(1), file(2)]).is_empty());
    let revision = host.ui.draft_revision;
    assert!(prepared(&mut host, channel, request, vec![file(1), file(2)]).is_empty());
    assert_eq!(host.ui.uploads.drafts[&channel], vec![file(1), file(2)]);
    assert_eq!(host.ui.draft_revision, revision);
}

#[test]
fn empty_or_repeated_tokens_never_partially_stage_a_batch() {
    for batch in [vec![], vec![file(2), file(2)], vec![file(1), file(2)]] {
        let (mut host, channel) = host();
        multiple(&mut host, 8, 1024, 1024);
        seed(&mut host, channel, 1);
        let request = pick(&mut host);
        let revision = host.ui.draft_revision;
        let has_new = batch.iter().any(|file| file.token == UploadToken(2));
        let intents = prepared(&mut host, channel, request, batch);
        assert_eq!(
            intents,
            if has_new {
                vec![Intent::RemoveAttachment {
                    channel,
                    token: UploadToken(2),
                }]
            } else {
                vec![]
            }
        );
        assert_eq!(host.ui.uploads.drafts[&channel], vec![file(1)]);
        assert_eq!(host.ui.draft_revision, revision);
        assert!(!attachment_pick_pending(&host.ui));
    }
}

#[test]
fn a_two_file_only_send_keeps_its_whole_batch_and_a_newer_draft_on_admission() {
    let (mut host, channel) = host();
    multiple(&mut host, 8, 1024, 1024);
    let pick_request = pick(&mut host);
    assert!(prepared(&mut host, channel, pick_request, vec![file(1), file(2)]).is_empty());
    host.frame();
    host.key(Key::Enter, false, false);
    let request = match host.upload_intents.last().unwrap() {
        Intent::SendAttachments {
            request,
            channel: found,
            content,
            files,
            ..
        } if *found == channel
            && content.is_empty()
            && files == &[UploadToken(1), UploadToken(2)] =>
        {
            *request
        }
        other => panic!("expected the complete file-only batch, got {other:?}"),
    };
    host.type_text("nuevo borrador");
    let intents = attachment_send_admitted(
        &mut host.ui,
        &mut host.store,
        request,
        Ok(88),
        Timestamp(10),
    );
    host.run(intents);
    assert_eq!(
        host.store
            .pending_attachments(channel, MessageId(u64::MAX - 88)),
        &[file(1), file(2)]
    );
    assert_eq!(host.ui.drafts[&channel], "nuevo borrador");
    assert!(!host
        .sent
        .iter()
        .any(|command| matches!(command, Command::SendMessage { .. })));
}

#[test]
fn a_repeated_prepared_callback_after_admission_cannot_discard_a_pending_send() {
    let (mut host, channel) = host();
    multiple(&mut host, 8, 1024, 1024);
    let pick_request = pick(&mut host);
    assert!(prepared(&mut host, channel, pick_request, vec![file(1), file(2)]).is_empty());
    let request = submit(&mut host, channel, "");
    let intents = attachment_send_admitted(
        &mut host.ui,
        &mut host.store,
        request,
        Ok(89),
        Timestamp(10),
    );
    host.run(intents);
    assert_eq!(
        host.store
            .pending_attachments(channel, MessageId(u64::MAX - 89)),
        &[file(1), file(2)]
    );
    assert!(
        prepared(&mut host, channel, pick_request, vec![file(1), file(2)]).is_empty(),
        "discarding a token already claimed by a pending send cancels its frozen retry"
    );
}

#[test]
fn a_file_over_what_the_account_may_attach_here_is_refused_when_prepared() {
    let (mut host, channel) = host();
    // The server's own value; outside servers the account's.
    let server = host.store.channel(channel).unwrap().server;
    host.event(Event::UploadLimit {
        server: None,
        max_file_bytes: 1000,
    });
    host.event(Event::UploadLimit {
        server,
        max_file_bytes: 10,
    });
    assert_eq!(host.store.upload_limit(channel), Some(10));
    let request = pick(&mut host);
    let account = host.store.me().unwrap();
    let intents = upload_update(
        &mut host.ui,
        &host.store,
        UploadUpdate::Prepared {
            account,
            channel,
            request,
            result: Ok(vec![file(1)]),
        },
    );
    assert_eq!(
        intents,
        vec![Intent::RemoveAttachment {
            channel,
            token: UploadToken(1)
        }],
        "the prepared bytes are released at once"
    );
    assert!(host
        .ui
        .uploads
        .drafts
        .get(&channel)
        .is_none_or(Vec::is_empty));
    let notice = host.ui.notices.last().unwrap();
    assert_eq!(notice.title, UploadError::TooLarge.label());
    assert_eq!(
        notice.detail.as_deref(),
        Some("Cada archivo puede pesar hasta 10 B en esta conversación.")
    );
}

#[test]
fn pasting_from_the_menu_asks_for_the_clipboard_and_an_empty_one_says_so() {
    let (mut host, channel) = host();
    host.type_text("borrador");
    host.click(rune_components::composer::ids::PLUS);
    host.click(rune_components::composer::ids::PASTE);
    let request = match host.upload_intents.last() {
        Some(Intent::PasteAttachments {
            request,
            channel: asked,
        }) if *asked == channel => *request,
        other => panic!("expected a clipboard read, got {other:?}"),
    };
    let account = host.store.me().unwrap();
    upload_update(
        &mut host.ui,
        &host.store,
        UploadUpdate::Prepared {
            account,
            channel,
            request,
            result: Err(UploadError::Empty),
        },
    );
    assert_eq!(
        host.ui.notices.last().unwrap().title,
        UploadError::Empty.label()
    );
    assert_eq!(host.ui.drafts[&channel], "borrador", "the draft stays");
    assert!(!attachment_pick_pending(&host.ui));
}

mod drop;

#[test]
fn draft_files_are_kept_as_they_change_and_what_did_not_come_back_is_said() {
    use crate::{dirty_draft_files, draft_files_dirty, draft_files_kept};
    let (mut host, channel) = host();
    multiple(&mut host, 10, 1024, 1024);
    assert!(!draft_files_dirty(&host.ui));
    let request = pick(&mut host);
    assert!(prepared(&mut host, channel, request, vec![file(1)]).is_empty());
    assert_eq!(
        dirty_draft_files(&host.ui, &host.store),
        vec![(channel, vec![UploadToken(1)])]
    );
    draft_files_kept(&mut host.ui, channel);
    assert!(!draft_files_dirty(&host.ui));
    // Removed: kept as none, which forgets them.
    host.click(&format!("composer:{channel}:upload:1:remove"));
    assert_eq!(
        dirty_draft_files(&host.ui, &host.store),
        vec![(channel, vec![])]
    );
    draft_files_kept(&mut host.ui, channel);
    // Brought back from an earlier run, without one that changed: said.
    let request = begin_attachment_import(
        &mut host.ui,
        &host.store,
        channel,
        AttachmentImportSource::Restore,
    )
    .unwrap();
    let account = host.store.me().unwrap();
    let skipped = |request| UploadUpdate::Skipped {
        account,
        channel,
        request,
        files: vec!["vieja.png".into(), "notas.txt".into()],
    };
    let said = |host: &Host| {
        host.ui.notices.iter().any(|notice| {
            notice
                .detail
                .as_deref()
                .is_some_and(|detail| detail.ends_with(": vieja.png, notas.txt."))
        })
    };
    // Another request's says nothing.
    assert!(upload_update(&mut host.ui, &host.store, skipped(request + 1)).is_empty());
    assert!(!said(&host));
    assert!(upload_update(&mut host.ui, &host.store, skipped(request)).is_empty());
    assert!(said(&host));
    assert!(draft_files_dirty(&host.ui), "kept again as it is now");
    assert!(prepared(&mut host, channel, request, vec![file(2)]).is_empty());
    assert_eq!(host.ui.uploads.drafts[&channel], vec![file(2)]);
}
