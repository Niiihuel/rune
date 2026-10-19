//! «Guardar como»: one at a time, known by its request and the attachment's
//! IDs. The host resolves the attachment again and Core keeps the path; the
//! interface only learns the phase, the bytes so far and the saved name.

use std::time::Instant;

use limn::{Component, Element, Progress, UiAction};
use rune_components::Look;
use rune_i18n::{t, tr};
use rune_model::{Attachment, AttachmentId, ChannelId, MessageId, RemoteFile, UserId};
use rune_provider::RequestId;
use rune_state::Store;

use crate::{Intent, Ui};

pub(crate) const CANCEL: &str = "save-as:cancel";
/// Replace the file at the chosen name (escritorio-linux).
pub(crate) const REPLACE: &str = "save-as:replace";

/// The «Save as» running, for the account that started it.
#[derive(Clone, Debug, PartialEq)]
pub struct SaveAs {
    pub request: RequestId,
    pub account: Option<UserId>,
    pub filename: String,
    pub phase: SavePhase,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SavePhase {
    /// The system's dialog asks where.
    Choosing,
    /// Downloading into the chosen file.
    Saving { received: u64, total: u64 },
    // Linux desktop (escritorio-linux).
    /// A file already has the chosen name, shown as the save's: replace it
    /// or cancel.
    Exists,
}

/// The attachment as the conversation shows it now, with its link: what a
/// save resolves again instead of trusting an old one.
pub fn attachment_download(
    store: &Store,
    channel: ChannelId,
    message: MessageId,
    attachment: AttachmentId,
) -> Option<(RemoteFile, String, u64)> {
    let found = crate::attachments::attachment(store, channel, message, attachment)?;
    let source = found.download.clone()?;
    Some((source, found.filename.clone(), found.size_bytes))
}

/// Start saving `found`, unless another save runs.
pub(crate) fn start(
    ui: &mut Ui,
    store: &Store,
    channel: ChannelId,
    message: MessageId,
    found: &Attachment,
) -> Vec<Intent> {
    if ui.save_as.is_some() {
        ui.notify(
            t("Ya hay un archivo guardándose"),
            Some(t("Esperá a que termine o cancelalo.").into()),
            true,
            Instant::now(),
        );
        return Vec::new();
    }
    let request = ui.fresh_request();
    ui.save_as = Some(SaveAs {
        request,
        account: store.me(),
        filename: found.filename.clone(),
        phase: SavePhase::Choosing,
    });
    vec![Intent::SaveAttachmentAs {
        request,
        channel,
        message,
        attachment: found.id,
    }]
}

/// The destination of `request` was chosen: the file downloads now.
pub fn save_saving(ui: &mut Ui, request: RequestId) {
    if let Some(save) = ui.save_as.as_mut().filter(|save| save.request == request) {
        save.phase = SavePhase::Saving {
            received: 0,
            total: 0,
        };
    }
}

/// A file already has the name chosen for `request`, `filename`: the
/// person replaces it or cancels.
pub fn save_exists(ui: &mut Ui, request: RequestId, filename: String) {
    if let Some(save) = ui.save_as.as_mut().filter(|save| save.request == request) {
        save.filename = filename;
        save.phase = SavePhase::Exists;
    }
}

pub fn save_progress(ui: &mut Ui, request: RequestId, received: u64, total: u64) {
    if let Some(save) = ui.save_as.as_mut().filter(|save| save.request == request) {
        save.phase = SavePhase::Saving { received, total };
    }
}

/// `request` ended: saved under that name, cancelled (`Err(None)`), or not
/// saved for that reason. Only that request's state goes.
pub fn save_finished(ui: &mut Ui, request: RequestId, result: Result<String, Option<String>>) {
    if ui
        .save_as
        .as_ref()
        .is_none_or(|save| save.request != request)
    {
        return;
    }
    ui.save_as = None;
    let now = Instant::now();
    match result {
        Ok(filename) => ui.notify(t("Archivo guardado"), Some(filename), false, now),
        Err(None) => {}
        Err(Some(reason)) => ui.notify(t("No se pudo guardar el archivo"), Some(reason), true, now),
    }
}

/// Another account's save is not shown, and is cancelled by the host.
pub(crate) fn refresh(ui: &mut Ui, store: &Store) -> Vec<Intent> {
    match ui.save_as.as_ref() {
        Some(save) if save.account != store.me() => {
            let request = save.request;
            ui.save_as = None;
            vec![Intent::CancelSaveAttachment { request }]
        }
        _ => Vec::new(),
    }
}

pub(crate) fn update(ui: &mut Ui, action: &UiAction) -> Option<Vec<Intent>> {
    match action {
        UiAction::Clicked { id } if id == REPLACE => {
            let save = ui
                .save_as
                .as_mut()
                .filter(|save| save.phase == SavePhase::Exists)?;
            // Core says when the download starts; no second word meanwhile.
            save.phase = SavePhase::Saving {
                received: 0,
                total: 0,
            };
            Some(vec![Intent::ReplaceSavedFile {
                request: save.request,
            }])
        }
        UiAction::Clicked { id } if id == CANCEL => {
            // The host answers with the cancellation, which clears it.
            Some(
                ui.save_as
                    .as_ref()
                    .map(|save| {
                        vec![Intent::CancelSaveAttachment {
                            request: save.request,
                        }]
                    })
                    .unwrap_or_default(),
            )
        }
        _ => None,
    }
}

/// A bar over the conversation while a save runs, with its cancel.
pub(crate) fn status(ui: &Ui, look: &Look) -> Option<Element> {
    let save = ui.save_as.as_ref()?;
    if save.phase == SavePhase::Exists {
        return Some(exists(save, look));
    }
    let (title, progress) = match save.phase {
        SavePhase::Choosing | SavePhase::Exists => (t("Eligiendo dónde guardar…"), None),
        // A size the service did not give leaves the bar out.
        SavePhase::Saving { received, total } => (
            t("Guardando archivo…"),
            (total > 0).then(|| (received as f32 / total as f32).clamp(0.0, 1.0)),
        ),
    };
    let mut text = vec![
        rune_components::strong_line(title, 13.0, look.text),
        rune_components::line(&save.filename, 12.0, look.muted),
    ];
    if let Some(value) = progress {
        text.push(
            Progress::new(value, look.palette())
                .id(format!("save-as:{}:progress", save.request))
                .render()
                .accessibility_label(tr!("{pct} %", pct = (value * 100.0).round())),
        );
    }
    let cancel =
        rune_components::user_settings::secondary_button(CANCEL, t("Cancelar"), true, look);
    Some(
        Element::panel(
            Element::row(vec![Element::column(text).gap(4.0).flex(1.0), cancel])
                .gap(12.0)
                .center_content(),
        )
        .padding(12.0)
        .background(look.raised)
        .border(look.border, 1.0)
        .radius(8.0),
    )
}

/// The bar asking whether to replace the file already at the chosen name.
fn exists(save: &SaveAs, look: &Look) -> Element {
    use rune_components::user_settings::{danger_button, secondary_button};
    let text = vec![
        rune_components::strong_line(t("Ya hay un archivo con ese nombre"), 13.0, look.text),
        // Whole, wrapped: the person confirms deleting the file it names.
        rune_components::text(&save.filename, 12.0, look.muted),
        rune_components::text(
            t("Reemplazarlo borra el que está ahí. Cancelá para elegir otro nombre."),
            12.0,
            look.muted,
        ),
    ];
    // The notices' column is narrow: the buttons go under the words.
    Element::panel(
        Element::column(vec![
            Element::column(text).gap(4.0),
            Element::row(vec![
                Element::spacer().flex(1.0),
                danger_button(REPLACE, t("Reemplazar"), look),
                secondary_button(CANCEL, t("Cancelar"), true, look),
            ])
            .gap(8.0),
        ])
        .gap(12.0),
    )
    .padding(12.0)
    .background(look.raised)
    .border(look.border, 1.0)
    .radius(8.0)
}
