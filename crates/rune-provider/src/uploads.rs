//! Ephemeral upload contracts. Payloads stay in service tasks, never UI results.

use std::{fmt, time::Instant};

use bytes::Bytes;
use rune_i18n::t;
use rune_model::{ChannelId, PendingAttachment, UserId};
use tokio::sync::watch;

use crate::RequestId;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadLimits {
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_message_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UploadSendScope {
    pub request: RequestId,
    pub account: UserId,
    pub channel: ChannelId,
    pub generation: u64,
    pub nonce: u64,
    pub expires_at: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UploadError {
    Cancelled,
    Unavailable,
    Denied,
    Busy,
    TooLarge,
    TooMany,
    Changed,
    InvalidFile,
    Expired,
    NotFound,
    MemoryLimit,
    Failed,
    /// The clipboard holds no files and no picture to attach.
    Empty,
}

impl UploadError {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cancelled => t("Se canceló el archivo."),
            Self::Unavailable => t("Los archivos no están disponibles en esta conexión."),
            Self::Denied => t("No tienes permiso para enviar archivos en esta conversación."),
            Self::Busy => t("Espera a que termine la preparación o el envío anterior."),
            Self::TooLarge => t("El archivo o el mensaje supera el límite permitido."),
            Self::TooMany => t("Este envío tiene más archivos de los permitidos."),
            Self::Changed => t("El archivo cambió mientras se preparaba. Vuelve a seleccionarlo."),
            Self::InvalidFile => t("El archivo seleccionado no es válido."),
            Self::Expired => t("Venció el intento de envío. Puedes volver a intentarlo."),
            Self::NotFound => t("El archivo ya no está preparado. Vuelve a seleccionarlo."),
            Self::MemoryLimit => t("Quita un archivo preparado antes de agregar otro."),
            Self::Failed => t("No se pudo completar el archivo."),
            Self::Empty => t("El portapapeles no tiene archivos ni imágenes para adjuntar."),
        }
    }
}

/// Sealed service-owned bytes. Clones and slices retain the original owner.
#[derive(Clone, PartialEq)]
pub struct PreparedUpload {
    metadata: PendingAttachment,
    bytes: Bytes,
}

impl PreparedUpload {
    pub fn new(metadata: PendingAttachment, bytes: Bytes) -> Result<Self, UploadError> {
        if metadata.token.0 == 0
            || metadata.size_bytes != bytes.len() as u64
            || metadata.filename.trim().is_empty()
            || metadata.filename.len() > 180
            || metadata.filename.chars().any(char::is_control)
            || metadata.filename.contains('/')
        {
            return Err(UploadError::InvalidFile);
        }
        Ok(Self { metadata, bytes })
    }

    pub fn metadata(&self) -> &PendingAttachment {
        &self.metadata
    }

    pub fn bytes(&self) -> Bytes {
        self.bytes.clone()
    }
}

impl fmt::Debug for PreparedUpload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("PreparedUpload([REDACTED])")
    }
}

/// The control sender stays in Core. Dropping it cancels every queued clone.
#[derive(Clone)]
pub struct UploadFence(watch::Receiver<bool>);

impl UploadFence {
    pub fn new() -> (watch::Sender<bool>, Self) {
        let (control, signal) = watch::channel(false);
        (control, Self(signal))
    }

    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow() || self.0.has_changed().is_err()
    }

    pub async fn cancelled(&self) {
        let mut signal = self.0.clone();
        while !*signal.borrow() {
            if signal.changed().await.is_err() {
                return;
            }
        }
    }
}

impl PartialEq for UploadFence {
    fn eq(&self, other: &Self) -> bool {
        self.0.same_channel(&other.0)
    }
}

impl fmt::Debug for UploadFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("UploadFence([REDACTED])")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum UploadUpdate {
    Prepared {
        account: UserId,
        channel: ChannelId,
        request: RequestId,
        result: Result<Vec<PendingAttachment>, UploadError>,
    },
    Progress {
        scope: UploadSendScope,
        sent_bytes: u64,
        total_bytes: u64,
    },
    // Sends and uploads (envios-y-subidas).
    /// Files a draft kept from an earlier run that did not come back, by
    /// name: gone, unreadable or changed since they were chosen. Sent
    /// before the `Prepared` of the same request.
    Skipped {
        account: UserId,
        channel: ChannelId,
        request: RequestId,
        files: Vec<String>,
    },
}

#[cfg(test)]
mod tests;
