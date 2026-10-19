//! A voice message's audio, or an audio or video file, fetched in memory
//! only when the account plays it. One at a time: a new request replaces
//! the one still downloading. What it is, `rune-media` finds out.

use std::{sync::Arc, time::Duration};

use rune_i18n::t;
use rune_model::RemoteFile;
use tokio::{runtime::Handle, sync::mpsc, task::JoinHandle};

use crate::documents::{fetch, DocumentError};

/// The whole file stays in memory while it plays.
pub const MAX_CLIP_BYTES: u64 = rune_model::Attachment::MAX_PLAYED_BYTES;

#[derive(Debug)]
pub struct ClipResult {
    pub request: u64,
    /// The file, or why it could not be fetched.
    pub result: Result<Arc<[u8]>, &'static str>,
}

struct Completed {
    epoch: u64,
    result: ClipResult,
}

pub(crate) struct Clips {
    runtime: Handle,
    origins: &'static [&'static str],
    client: Option<reqwest::Client>,
    wake: Arc<dyn Fn() + Send + Sync>,
    sender: mpsc::Sender<Completed>,
    receiver: mpsc::Receiver<Completed>,
    pending: Option<(u64, JoinHandle<()>)>,
    epoch: u64,
}

fn label(error: DocumentError) -> &'static str {
    match error {
        DocumentError::TooLarge => t("El archivo supera los 50 MiB que RUNE reproduce."),
        DocumentError::Invalid => t("Lo descargado no coincide con el archivo del mensaje."),
        DocumentError::Unavailable | DocumentError::Failed => t("No se pudo descargar el archivo."),
    }
}

impl Clips {
    pub fn new(
        runtime: Handle,
        origins: &'static [&'static str],
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        Self {
            runtime,
            origins,
            wake,
            sender,
            receiver,
            pending: None,
            epoch: 0,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                // Room for the largest file on a slow connection.
                .timeout(Duration::from_secs(180))
                .user_agent(concat!("RUNE/", env!("CARGO_PKG_VERSION"), " media"))
                .build()
                .ok(),
        }
    }

    pub fn start(
        &mut self,
        request: u64,
        source: RemoteFile,
        size: u64,
    ) -> Result<(), &'static str> {
        self.clear();
        if size > MAX_CLIP_BYTES {
            return Err(label(DocumentError::TooLarge));
        }
        let url = crate::validated_resource_url(&source.url, self.origins, false)
            .ok_or(t("El enlace de este archivo no está disponible."))?;
        let client = self.client.clone().ok_or(label(DocumentError::Failed))?;
        let (epoch, sender, wake) = (self.epoch, self.sender.clone(), self.wake.clone());
        self.pending = Some((
            request,
            self.runtime.spawn(async move {
                let result = fetch(&client, url, size, MAX_CLIP_BYTES)
                    .await
                    .map(Arc::from)
                    .map_err(label);
                let completed = Completed {
                    epoch,
                    result: ClipResult { request, result },
                };
                if sender.send(completed).await.is_ok() {
                    wake();
                }
            }),
        ));
        Ok(())
    }

    pub fn cancel(&mut self, request: u64) {
        if self
            .pending
            .as_ref()
            .is_some_and(|(active, _)| *active == request)
        {
            self.clear();
        }
    }

    pub fn clear(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        if let Some((_, pending)) = self.pending.take() {
            pending.abort();
        }
        while self.receiver.try_recv().is_ok() {}
    }

    pub fn poll(&mut self) -> Option<ClipResult> {
        while let Ok(completed) = self.receiver.try_recv() {
            if completed.epoch == self.epoch {
                self.pending = None;
                return Some(completed.result);
            }
        }
        None
    }
}

impl Drop for Clips {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oversized_or_foreign_clips_are_refused_before_any_request() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let origins = &["https://cdn.example.test"];
        let mut clips = Clips::new(runtime.handle().clone(), origins, Arc::new(|| {}));
        let source = |url: &str| RemoteFile { url: url.into() };
        let ours = "https://cdn.example.test/voice-message.ogg";
        assert!(clips.start(1, source(ours), MAX_CLIP_BYTES + 1).is_err());
        for foreign in [
            "https://elsewhere.test/voice-message.ogg",
            "http://cdn.example.test/voice-message.ogg",
            "https://user@cdn.example.test/voice-message.ogg",
        ] {
            assert!(clips.start(2, source(foreign), 10).is_err());
        }
        assert!(clips.pending.is_none(), "nothing was requested");
        assert!(clips.start(3, source(ours), 10).is_ok());
        clips.cancel(3);
        assert!(clips.pending.is_none() && clips.poll().is_none());
    }
}
