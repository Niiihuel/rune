//! Outgoing file controls contain only tokens and display metadata.

use crate::{Intent, Ui};
use limn::{Element, UiAction};
use rune_i18n::{t, tr};
use rune_model::{ChannelId, MessageId, PendingAttachment, Timestamp, UploadToken, UserId};
use rune_provider::{ConnectionState, RequestId, UploadError, UploadLimits, UploadUpdate};
use rune_state::{Applied, Store};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

#[derive(Debug, Default)]
pub(crate) struct Uploads {
    pub account: Option<UserId>,
    pub drafts: HashMap<ChannelId, Vec<PendingAttachment>>,
    pub limits: Option<UploadLimits>,
    pick: Option<Pick>,
    submissions: HashMap<RequestId, Submission>,
    /// Drafts whose files changed since the host kept them for the next run.
    files_dirty: HashSet<ChannelId>,
}

pub fn set_upload_limits(ui: &mut Ui, limits: Option<UploadLimits>) {
    ui.uploads.limits = limits;
}

/// The provider's limits, narrowed to the largest file the account may
/// attach in `channel` when the provider says.
pub(crate) fn limits(ui: &Ui, store: &Store, channel: ChannelId) -> Option<UploadLimits> {
    let limits = ui.uploads.limits?;
    Some(UploadLimits {
        max_file_bytes: store
            .upload_limit(channel)
            .map_or(limits.max_file_bytes, |bytes| {
                bytes.min(limits.max_file_bytes)
            }),
        ..limits
    })
}

/// Too large: with the size that fits per file, where the provider said.
fn too_large(ui: &mut Ui, store: &Store, channel: ChannelId) {
    let detail = store.upload_limit(channel).map(|bytes| {
        tr!(
            "Cada archivo puede pesar hasta {size} en esta conversación.",
            size = rune_components::file_size(bytes)
        )
    });
    ui.notify(UploadError::TooLarge.label(), detail, true, Instant::now());
}

/// Only the origin is retained here; native paths and file access belong to Core.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachmentImportSource {
    Picker,
    Drop,
    /// The files or the picture on the clipboard.
    Clipboard,
    /// The files a draft kept from an earlier run (envios-y-subidas).
    Restore,
}

#[derive(Debug)]
struct Pick {
    request: RequestId,
    account: UserId,
    channel: ChannelId,
    source: AttachmentImportSource,
    cancelling: bool,
    /// The field of the modal open the files are for (comandos-de-apps);
    /// `None`, the channel's draft or its forum's post.
    field: Option<String>,
}

#[derive(Debug)]
struct Submission {
    account: UserId,
    channel: ChannelId,
    revision: u64,
    content: String,
    reply_to: Option<MessageId>,
    files: Vec<PendingAttachment>,
}

fn revision(ui: &Ui, channel: ChannelId) -> u64 {
    ui.draft_revisions.get(&channel).copied().unwrap_or(0)
}

pub(crate) fn touch(ui: &mut Ui, channel: ChannelId) {
    ui.draft_revision += 1;
    ui.draft_revisions.insert(channel, ui.draft_revision);
    ui.uploads.files_dirty.insert(channel);
}

/// The drafts whose files changed since the host last kept them, with the
/// files each has now (none forgets them). A forum's, which wait for a
/// post being written, are never kept.
pub fn dirty_draft_files(ui: &Ui, store: &Store) -> Vec<(ChannelId, Vec<UploadToken>)> {
    ui.uploads
        .files_dirty
        .iter()
        .map(|channel| {
            let forum = store
                .channel(*channel)
                .is_some_and(|found| found.kind == rune_model::ChannelKind::Forum);
            let files = ui
                .uploads
                .drafts
                .get(channel)
                .filter(|_| !forum)
                .map(|files| files.iter().map(|file| file.token).collect())
                .unwrap_or_default();
            (*channel, files)
        })
        .collect()
}

/// The host kept `channel`'s draft files as [`dirty_draft_files`] gave them.
pub fn draft_files_kept(ui: &mut Ui, channel: ChannelId) {
    ui.uploads.files_dirty.remove(&channel);
}

/// Whether some draft's files wait to be kept.
pub fn draft_files_dirty(ui: &Ui) -> bool {
    !ui.uploads.files_dirty.is_empty()
}

/// Forgetting local data also cancels the draft files not kept yet.
pub(crate) fn forget_draft_files(ui: &mut Ui) {
    ui.uploads.files_dirty.clear();
}

pub(crate) fn error(ui: &mut Ui, error: UploadError) {
    if error != UploadError::Cancelled {
        ui.notify(error.label(), None, true, Instant::now());
    }
}

/// Whether files may be chosen in `channel`, for its draft or post, or for
/// a modal's `field`.
fn may_choose(store: &Store, channel: ChannelId, field: Option<&str>) -> bool {
    match field {
        Some(_) => store.interaction_abilities(channel).files.allowed(),
        None => allowed(store, channel),
    }
}

fn allowed(store: &Store, channel: ChannelId) -> bool {
    store.me().is_some()
        && store.connection == ConnectionState::Connected
        && (store
            .abilities(channel)
            .is_some_and(|a| a.write.allowed() && a.attach.allowed())
            // A forum's files wait for its new post.
            || store.post_files(channel).allowed())
}

/// Start a metadata-only preparation. The host validates the native drop target
/// and owns paths; an admission error must complete this request via upload_update.
pub fn begin_attachment_import(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    source: AttachmentImportSource,
) -> Result<RequestId, UploadError> {
    let obscured = import_obscured(ui);
    admit_import(ui, store, channel, source, obscured, None)
}

/// Choose files for the field `custom_id` of the modal open over
/// `channel`: they wait with the modal, apart from the draft.
pub(crate) fn begin_modal_import(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    custom_id: &str,
) -> Result<RequestId, UploadError> {
    admit_import(
        ui,
        store,
        channel,
        AttachmentImportSource::Picker,
        false,
        Some(custom_id),
    )
}

/// The modal field whose files are being chosen, while that runs.
pub(crate) fn modal_picking(ui: &Ui) -> Option<&str> {
    ui.uploads.pick.as_ref()?.field.as_deref()
}

/// The files being chosen for `channel`'s draft or post, not a modal's.
fn draft_pick(ui: &Ui, channel: ChannelId) -> Option<&Pick> {
    ui.uploads
        .pick
        .as_ref()
        .filter(|pick| pick.channel == channel && pick.field.is_none())
}

/// Stop choosing files for a modal that closed.
pub(crate) fn cancel_modal_pick(ui: &mut Ui) -> Option<Intent> {
    let pick = ui
        .uploads
        .pick
        .as_mut()
        .filter(|pick| pick.field.is_some() && !pick.cancelling)?;
    pick.cancelling = true;
    Some(Intent::CancelUpload {
        request: pick.request,
    })
}

/// Choose files for the post being written in `forum`, from its dialog: they
/// wait with the forum, which has no composer of its own.
pub(crate) fn begin_post_import(
    ui: &mut Ui,
    store: &Store,
    forum: ChannelId,
) -> Result<RequestId, UploadError> {
    admit_import(
        ui,
        store,
        forum,
        AttachmentImportSource::Picker,
        false,
        None,
    )
}

/// The files chosen for the post being written in `forum`.
pub(crate) fn post_files(ui: &Ui, forum: ChannelId) -> &[PendingAttachment] {
    ui.uploads.drafts.get(&forum).map_or(&[], Vec::as_slice)
}

/// The request choosing files for `forum`'s post, while it runs.
pub(crate) fn post_picking(ui: &Ui, forum: ChannelId) -> Option<RequestId> {
    draft_pick(ui, forum).map(|pick| pick.request)
}

/// Let go of the files of a post no longer written in `forum`.
pub(crate) fn release_post(ui: &mut Ui, forum: ChannelId) -> Vec<Intent> {
    let mut intents = Vec::new();
    if let Some(pick) = ui
        .uploads
        .pick
        .as_mut()
        .filter(|pick| pick.channel == forum && !pick.cancelling)
    {
        pick.cancelling = true;
        intents.push(Intent::CancelUpload {
            request: pick.request,
        });
    }
    let files = ui.uploads.drafts.remove(&forum).unwrap_or_default();
    intents.extend(files.into_iter().map(|file| Intent::RemoveAttachment {
        channel: forum,
        token: file.token,
    }));
    intents
}

fn admit_import(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    source: AttachmentImportSource,
    obscured: bool,
    field: Option<&str>,
) -> Result<RequestId, UploadError> {
    let account = store.me().ok_or(UploadError::Denied)?;
    if ui.channel() != Some(channel)
        || ui.editing.is_some()
        || !may_choose(store, channel, field)
        || ui.uploads.account.is_some_and(|held| held != account)
    {
        return Err(UploadError::Denied);
    }
    if ui.uploads.pick.is_some()
        || ui
            .uploads
            .submissions
            .values()
            .any(|pending| pending.channel == channel)
        || obscured
    {
        return Err(UploadError::Busy);
    }
    let limits = limits(ui, store, channel).ok_or(UploadError::Unavailable)?;
    let (files, text) = match field {
        Some(_) => (crate::interactions::modal_files(ui, store), 0),
        None => (
            ui.uploads
                .drafts
                .get(&channel)
                .map_or(&[][..], Vec::as_slice),
            ui.drafts.get(&channel).map_or(0, |text| text.trim().len()) as u64,
        ),
    };
    if files.len() >= limits.max_files {
        return Err(UploadError::TooMany);
    }
    // As many as Core holds prepared at once, across drafts and the modal.
    let kept = ui.uploads.drafts.values().map(Vec::len).sum::<usize>()
        + crate::interactions::modal_files(ui, store).len();
    if kept >= 10 {
        return Err(UploadError::MemoryLimit);
    }
    if files
        .iter()
        .any(|file| file.size_bytes > limits.max_file_bytes)
        || files
            .iter()
            .try_fold(text, |size, file| size.checked_add(file.size_bytes))
            .is_none_or(|size| size > limits.max_message_bytes)
    {
        return Err(UploadError::TooLarge);
    }
    let request = ui.fresh_request();
    ui.uploads.account = Some(account);
    ui.uploads.pick = Some(Pick {
        request,
        account,
        channel,
        source,
        cancelling: false,
        field: field.map(str::to_owned),
    });
    Ok(request)
}

fn import_obscured(ui: &Ui) -> bool {
    crate::reads::modal_obscured(ui)
        || ui.emoji_open
        || ui.reacting.is_some()
        || ui.messages.more.is_some()
        || ui.messages.context.is_some()
        || ui.messages.plus
        || ui.profile.is_some()
        || ui.status_menu
        || ui.profiles.author.is_some()
        || ui.profiles.status_open
        || ui.profiles.devices.is_some()
        || ui.profiles.custom.is_some()
        || ui.server_menu.open
        || ui.channel_list.menu.is_some()
        || ui.channel_list.bell.is_some()
        || ui.channel_list.more.is_some()
        || ui.channel_list.pins.is_some()
        || ui.channel_list.topic.is_some()
        || ui.channel_list.drag.is_some()
        || ui.voice.details
        || ui.voice.more
        || ui.voice.menu.is_some()
        || ui.voice.card.is_some()
}

pub fn upload_update(ui: &mut Ui, store: &Store, update: UploadUpdate) -> Vec<Intent> {
    // Kept files that did not come back: said, for the pick still current.
    if let UploadUpdate::Skipped {
        account,
        channel,
        request,
        files,
    } = &update
    {
        let current = ui.uploads.pick.as_ref().is_some_and(|pick| {
            (pick.account, pick.channel, pick.request) == (*account, *channel, *request)
        });
        if current && !files.is_empty() {
            // Core names one without a name as nothing: it is named here.
            let files: Vec<&str> = files
                .iter()
                .map(|name| {
                    if name.is_empty() {
                        t("archivo")
                    } else {
                        name.as_str()
                    }
                })
                .collect();
            ui.notify(
                t("Algunos archivos del borrador no volvieron"),
                Some(tr!(
                    "Ya no están, cambiaron desde que los elegiste o no se pueden leer: {files}.",
                    files = files.join(", ")
                )),
                true,
                Instant::now(),
            );
            // Kept again as they are now, without the ones left out.
            ui.uploads.files_dirty.insert(*channel);
        }
        return Vec::new();
    }
    // Progress has one projection: Client emits Event::UploadProgress to Store.
    let UploadUpdate::Prepared {
        account,
        channel,
        request,
        result,
    } = update
    else {
        return Vec::new();
    };
    let matching = ui.uploads.pick.as_ref().is_some_and(|pick| {
        (pick.account, pick.channel, pick.request) == (account, channel, request)
    });
    if !matching {
        return result
            .ok()
            .map_or_else(Vec::new, |files| release(ui, store, channel, files));
    }
    let pick = ui.uploads.pick.take().unwrap();
    if pick.cancelling || store.me() != Some(account) || ui.uploads.account != Some(account) {
        return result
            .ok()
            .map_or_else(Vec::new, |files| release(ui, store, channel, files));
    }
    match result {
        Ok(files) => {
            let field = pick.field.as_deref();
            let (current, text) = match field {
                Some(_) => (crate::interactions::modal_files(ui, store), 0),
                None => (
                    ui.uploads
                        .drafts
                        .get(&channel)
                        .map_or(&[][..], Vec::as_slice),
                    ui.drafts.get(&channel).map_or(0, |text| text.trim().len()) as u64,
                ),
            };
            let mut tokens = HashSet::new();
            let failure = if !may_choose(store, channel, field) {
                Some(UploadError::Denied)
            } else if files.is_empty()
                || files
                    .iter()
                    .any(|file| !tokens.insert(file.token) || held(ui, store, file.token))
            {
                Some(UploadError::InvalidFile)
            } else if let Some(limits) = limits(ui, store, channel) {
                if current
                    .len()
                    .checked_add(files.len())
                    .is_none_or(|count| count > limits.max_files)
                {
                    Some(UploadError::TooMany)
                } else if files
                    .iter()
                    .any(|file| file.size_bytes > limits.max_file_bytes)
                    || current
                        .iter()
                        .chain(&files)
                        .try_fold(text, |size, file| size.checked_add(file.size_bytes))
                        .is_none_or(|size| size > limits.max_message_bytes)
                {
                    Some(UploadError::TooLarge)
                } else {
                    None
                }
            } else {
                Some(UploadError::Unavailable)
            };
            if let Some(failure) = failure {
                if failure == UploadError::TooLarge {
                    too_large(ui, store, channel);
                } else {
                    error(ui, failure);
                }
                return release(ui, store, channel, files);
            }
            if let Some(field) = pick.field {
                // The modal may have closed, or its field filled, meanwhile.
                return match crate::interactions::files_chosen(ui, store, &field, files.clone()) {
                    Ok(()) => Vec::new(),
                    Err(failure) => {
                        error(ui, failure);
                        release(ui, store, channel, files)
                    }
                };
            }
            ui.uploads.drafts.entry(channel).or_default().extend(files);
            touch(ui, channel);
        }
        Err(failure) => error(ui, failure),
    }
    Vec::new()
}

/// Whether `token` is held by a draft, a post or the modal open.
fn held(ui: &Ui, store: &Store, token: UploadToken) -> bool {
    ui.uploads
        .drafts
        .values()
        .flatten()
        .chain(crate::interactions::modal_files(ui, store))
        .any(|held| held.token == token)
}

fn release(
    ui: &Ui,
    store: &Store,
    channel: ChannelId,
    files: Vec<PendingAttachment>,
) -> Vec<Intent> {
    let mut released = HashSet::new();
    files
        .into_iter()
        .filter(|file| {
            released.insert(file.token)
                && !store.retains_attachment(file.token)
                && !held(ui, store, file.token)
        })
        .map(|file| Intent::RemoveAttachment {
            channel,
            token: file.token,
        })
        .collect()
}

pub fn attachment_pick_pending(ui: &Ui) -> bool {
    ui.uploads.pick.is_some()
}

pub(crate) fn attach_denied(ui: &Ui, channel: ChannelId) -> Option<String> {
    if ui.uploads.pick.is_some() {
        return Some(t("Espera a que termine la selección del archivo").into());
    }
    let limits = ui.uploads.limits?;
    (ui.uploads.drafts.get(&channel).map_or(0, Vec::len) >= limits.max_files).then(|| {
        if limits.max_files == 1 {
            t("Este mensaje admite hasta 1 archivo. Quita uno para elegir otro.").into()
        } else {
            tr!(
                "Este mensaje admite hasta {count} archivos. Quita uno para elegir otro.",
                count = limits.max_files
            )
        }
    })
}

pub fn attachment_send_pending(
    ui: &Ui,
    request: RequestId,
    channel: ChannelId,
    files: &[UploadToken],
) -> bool {
    ui.uploads.submissions.get(&request).is_some_and(|pending| {
        pending.channel == channel
            && ui.uploads.account == Some(pending.account)
            && pending
                .files
                .iter()
                .map(|file| file.token)
                .eq(files.iter().copied())
            && files.iter().all(|token| {
                ui.uploads
                    .drafts
                    .get(&channel)
                    .is_some_and(|draft| draft.iter().any(|file| file.token == *token))
            })
    })
}

pub fn attachment_send_admitted(
    ui: &mut Ui,
    store: &mut Store,
    request: RequestId,
    result: Result<u64, UploadError>,
    timestamp: Timestamp,
) -> Vec<Intent> {
    let Some(pending) = ui.uploads.submissions.remove(&request) else {
        return Vec::new();
    };
    if store.me() != Some(pending.account) || ui.uploads.account != Some(pending.account) {
        return Vec::new();
    }
    let nonce = match result {
        Ok(nonce) => nonce,
        Err(failure) => {
            error(ui, failure);
            return Vec::new();
        }
    };
    let same_revision = revision(ui, pending.channel) == pending.revision;
    store.add_pending_attachments(
        pending.channel,
        pending.content,
        pending.reply_to,
        nonce,
        request,
        pending.files.clone(),
        timestamp,
    );
    if let Some(draft) = ui.uploads.drafts.get_mut(&pending.channel) {
        draft.retain(|file| !pending.files.iter().any(|sent| sent.token == file.token));
    }
    touch(ui, pending.channel);
    let mut intents = Vec::new();
    if same_revision {
        ui.set_draft(pending.channel, None);
        if ui.channel() == Some(pending.channel) && ui.reply_to == pending.reply_to {
            ui.reply_to = None;
        }
        intents.push(Intent::SetInput {
            id: crate::editor_id(pending.channel),
            value: String::new(),
        });
    }
    if ui.channel() == Some(pending.channel) {
        intents.push(Intent::ScrollToEnd(crate::timeline_id(pending.channel)));
    }
    intents
}

pub(crate) fn update(ui: &mut Ui, store: &Store, action: &UiAction) -> Option<Vec<Intent>> {
    let UiAction::Clicked { id } = action else {
        return None;
    };
    if id == rune_components::composer::ids::PASTE {
        let channel = ui.channel()?;
        return Some(
            match begin_attachment_import(ui, store, channel, AttachmentImportSource::Clipboard) {
                Ok(request) => vec![
                    Intent::Focus(crate::editor_id(channel)),
                    Intent::PasteAttachments { request, channel },
                ],
                Err(failure) => {
                    error(ui, failure);
                    Vec::new()
                }
            },
        );
    }
    if id == rune_components::composer::ids::ATTACH {
        let channel = ui.channel()?;
        let request =
            match begin_attachment_import(ui, store, channel, AttachmentImportSource::Picker) {
                Ok(request) => request,
                Err(failure) => {
                    error(ui, failure);
                    return Some(Vec::new());
                }
            };
        return Some(vec![
            Intent::Focus(crate::editor_id(channel)),
            Intent::PickAttachment { request, channel },
        ]);
    }
    let mut parts = id.strip_prefix("composer:")?.split(':');
    let channel = ChannelId(parts.next()?.parse().ok()?);
    let kind = parts.next()?;
    let identifier = parts.next()?.parse::<u64>().ok()?;
    let action = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    if kind == "pick" && action == "cancel" {
        if draft_pick(ui, channel)
            .is_some_and(|pick| pick.request == identifier && !pick.cancelling)
        {
            ui.uploads.pick.as_mut().unwrap().cancelling = true;
            return Some(vec![Intent::CancelUpload {
                request: identifier,
            }]);
        }
        return Some(Vec::new());
    }
    if kind == "upload" && action == "remove" {
        let token = UploadToken(identifier);
        if let Some(files) = ui.uploads.drafts.get_mut(&channel) {
            let previous = files.len();
            files.retain(|file| file.token != token);
            if previous != files.len() {
                touch(ui, channel);
                return Some(vec![Intent::RemoveAttachment { channel, token }]);
            }
        }
        return Some(Vec::new());
    }
    None
}

/// None leaves the existing text-only submit unchanged. A pending file blocks
/// the whole submission rather than silently dropping an attachment.
pub(crate) fn submit(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    value: &str,
) -> Option<Vec<Intent>> {
    if draft_pick(ui, channel).is_some() {
        error(ui, UploadError::Busy);
        return Some(Vec::new());
    }
    let files = ui
        .uploads
        .drafts
        .get(&channel)
        .filter(|files| !files.is_empty())?
        .clone();
    if !allowed(store, channel) {
        error(ui, UploadError::Denied);
        return Some(Vec::new());
    }
    let Some(account) = store.me() else {
        return Some(Vec::new());
    };
    let Some(limits) = limits(ui, store, channel) else {
        error(ui, UploadError::Unavailable);
        return Some(Vec::new());
    };
    if files.len() > limits.max_files {
        error(ui, UploadError::TooMany);
        return Some(Vec::new());
    }
    if files.iter().any(|f| f.size_bytes > limits.max_file_bytes)
        || files
            .iter()
            .try_fold(value.trim().len() as u64, |n, f| {
                n.checked_add(f.size_bytes)
            })
            .is_none_or(|n| n > limits.max_message_bytes)
    {
        too_large(ui, store, channel);
        return Some(Vec::new());
    }
    if ui.uploads.submissions.len() >= 8
        || ui
            .uploads
            .submissions
            .values()
            .any(|pending| pending.channel == channel)
    {
        error(ui, UploadError::Busy);
        return Some(Vec::new());
    }
    let request = ui.fresh_request();
    let content = value.trim().to_owned();
    let reply_to = ui.reply_to;
    let tokens = files.iter().map(|file| file.token).collect();
    ui.uploads.submissions.insert(
        request,
        Submission {
            account,
            channel,
            revision: revision(ui, channel),
            content: content.clone(),
            reply_to,
            files,
        },
    );
    Some(vec![Intent::SendAttachments {
        request,
        channel,
        content,
        reply_to,
        files: tokens,
        mention_reply: crate::message_actions::submit_options(ui),
    }])
}

pub(crate) fn retry(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    message: MessageId,
) -> Vec<Intent> {
    let Some(unsent) = store.unsent(channel, message) else {
        return Vec::new();
    };
    if !allowed(store, channel) {
        error(ui, UploadError::Denied);
        return Vec::new();
    }
    if unsent.ambiguous
        && !store
            .provider
            .as_ref()
            .is_some_and(|p| p.capabilities.idempotent_send)
    {
        ui.notify(
            t("Este envío puede haber llegado. Esta conexión no permite reintentarlo sin duplicarlo."),
            None,
            true,
            Instant::now(),
        );
        return Vec::new();
    }
    vec![Intent::RetryAttachmentSend {
        request: ui.fresh_request(),
        channel,
        message,
    }]
}

pub(crate) fn discard(store: &Store, channel: ChannelId, message: MessageId) -> Vec<Intent> {
    let Some(unsent) = store.unsent(channel, message) else {
        return Vec::new();
    };
    let mut intents: Vec<_> = unsent
        .upload_request
        .map(|request| Intent::CancelUpload { request })
        .into_iter()
        .collect();
    intents.extend(
        unsent
            .files
            .into_iter()
            .map(|file| Intent::RemoveAttachment {
                channel,
                token: file.token,
            }),
    );
    intents
}

pub(crate) fn on_applied(ui: &mut Ui, store: &Store, _applied: &Applied) -> Vec<Intent> {
    let account_changed = ui.uploads.account.is_some() && ui.uploads.account != store.me();
    let mut intents = Vec::new();
    let invalid: Vec<_> = ui
        .uploads
        .drafts
        .keys()
        .copied()
        .filter(|channel| account_changed || !store.can_keep_attachments(*channel))
        .collect();
    for channel in invalid {
        for file in ui.uploads.drafts.remove(&channel).unwrap_or_default() {
            intents.push(Intent::RemoveAttachment {
                channel,
                token: file.token,
            });
        }
        touch(ui, channel);
    }
    // A modal's choice is kept while its files may still be attached.
    if ui.uploads.pick.as_ref().is_some_and(|pick| {
        !pick.cancelling
            && (account_changed
                || !match pick.field {
                    Some(_) => store.interaction_abilities(pick.channel).files.allowed(),
                    None => store.can_keep_attachments(pick.channel),
                })
    }) {
        let pick = ui.uploads.pick.as_mut().unwrap();
        pick.cancelling = true;
        intents.push(Intent::CancelUpload {
            request: pick.request,
        });
    }
    let invalid: Vec<_> = ui
        .uploads
        .submissions
        .iter()
        .filter(|(_, pending)| account_changed || !store.can_keep_attachments(pending.channel))
        .map(|(request, _)| *request)
        .collect();
    for request in invalid {
        ui.uploads.submissions.remove(&request);
        intents.push(Intent::CancelUpload { request });
    }
    if account_changed {
        ui.uploads.account = None;
    }
    intents
}

pub(crate) fn with_draft(
    surface: Element,
    ui: &Ui,
    channel: ChannelId,
    width: f32,
    window_height: f32,
    look: &rune_components::Look,
) -> Element {
    let files = ui
        .uploads
        .drafts
        .get(&channel)
        .map_or(&[][..], Vec::as_slice);
    let pick = draft_pick(ui, channel);
    let picking = pick.map(|pick| pick.request);
    if files.is_empty() && picking.is_none() {
        return surface;
    }
    let stage = rune_components::uploads::draft_uploads(
        rune_components::uploads::DraftUploads {
            channel,
            files,
            picking,
            selecting: pick.is_some_and(|pick| pick.source == AttachmentImportSource::Picker),
            cancelling: pick.is_some_and(|pick| pick.cancelling),
            submitting: ui
                .uploads
                .submissions
                .values()
                .any(|pending| pending.channel == channel),
            width,
            max_list_height: (window_height / 5.0).min(152.0),
        },
        look,
    );
    Element::column(vec![stage, surface]).gap(4.0)
}
