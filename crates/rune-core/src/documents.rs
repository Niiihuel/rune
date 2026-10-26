//! Explicit page previews; document bytes never enter the UI or account client.
//! The downloaded PDF stays in memory only while its viewer is open, so later
//! pages render without downloading it again.

use std::{sync::Arc, time::Duration};

use rune_i18n::t;
use rune_model::{PdfMark, PdfWord, RemoteFile};
use tokio::{runtime::Handle, sync::mpsc, task::JoinHandle};

use crate::ImageData;

mod render;

pub const MAX_DOCUMENT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentError {
    Unavailable,
    Invalid,
    TooLarge,
    Failed,
}

impl DocumentError {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unavailable => t("La vista previa de PDF no está disponible en este equipo."),
            Self::Invalid => t("Este archivo no tiene un PDF válido para mostrar."),
            Self::TooLarge => t("La vista previa admite PDF de hasta 16 MiB."),
            Self::Failed => t("No se pudo mostrar este PDF. Puedes guardarlo en Descargas."),
        }
    }
}

#[derive(Debug)]
pub struct DocumentResult {
    pub request: u64,
    /// The page these pixels are, counting from 1.
    pub page: u32,
    /// How many pages the document has; 1 when it could not be counted.
    pub pages: u32,
    pub result: Result<Arc<ImageData>, DocumentError>,
    /// What is highlighted on it, as it was asked for.
    pub marks: Vec<PdfMark>,
}

/// The text of the open document's pages, for its search.
#[derive(Debug)]
pub struct DocumentText {
    pub request: u64,
    /// Each page's words, in order, with where each sits on it; a page
    /// without text has none.
    pub result: Result<Arc<[Vec<PdfWord>]>, DocumentError>,
}

struct Completed {
    epoch: u64,
    result: DocumentResult,
    /// The downloaded document, with the first page only.
    document: Option<Arc<[u8]>>,
}

/// One replaceable preview, tied to the current UI request and account epoch.
pub(crate) struct Documents {
    runtime: Handle,
    origins: &'static [&'static str],
    client: Option<reqwest::Client>,
    wake: Arc<dyn Fn() + Send + Sync>,
    sender: mpsc::Sender<Completed>,
    receiver: mpsc::Receiver<Completed>,
    pending: Option<(u64, JoinHandle<()>)>,
    /// The open request's PDF and page count, at most 16 MiB.
    document: Option<(u64, Arc<[u8]>, u32)>,
    epoch: u64,
    /// Texts read, tagged with the epoch they were asked in.
    text_sender: mpsc::Sender<(u64, DocumentText)>,
    text_receiver: mpsc::Receiver<(u64, DocumentText)>,
    /// Its text being read, apart from its pages.
    reading: Option<JoinHandle<()>>,
}

impl Documents {
    pub fn new(
        runtime: Handle,
        origins: &'static [&'static str],
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let (text_sender, text_receiver) = mpsc::channel(1);
        Self {
            runtime,
            origins,
            wake,
            sender,
            receiver,
            pending: None,
            document: None,
            epoch: 0,
            text_sender,
            text_receiver,
            reading: None,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(20))
                .user_agent(concat!("RUNE/", env!("CARGO_PKG_VERSION"), " PDF preview"))
                .build()
                .ok(),
        }
    }

    pub fn start(
        &mut self,
        request: u64,
        source: RemoteFile,
        size: u64,
    ) -> Result<(), DocumentError> {
        self.clear();
        let tools = render::Tools::configured().ok_or(DocumentError::Unavailable)?;
        if size > MAX_DOCUMENT_BYTES {
            return Err(DocumentError::TooLarge);
        }
        let url = crate::validated_resource_url(&source.url, self.origins, false)
            .ok_or(DocumentError::Invalid)?;
        let client = self.client.clone().ok_or(DocumentError::Unavailable)?;
        let epoch = self.epoch;
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        self.pending = Some((
            request,
            self.runtime.spawn(async move {
                let (pages, document, result) = match download(&client, url, size).await {
                    Ok(bytes) => {
                        let bytes: Arc<[u8]> = bytes.into();
                        let pages = render::page_count(bytes.clone(), tools).await.unwrap_or(1);
                        let result = render::page(bytes.clone(), 1, tools, Vec::new())
                            .await
                            .map(Arc::new);
                        (pages, result.is_ok().then_some(bytes), result)
                    }
                    Err(error) => (1, None, Err(error)),
                };
                let result = DocumentResult {
                    request,
                    page: 1,
                    pages,
                    result,
                    marks: Vec::new(),
                };
                let completed = Completed {
                    epoch,
                    result,
                    document,
                };
                if sender.send(completed).await.is_ok() {
                    wake();
                }
            }),
        ));
        Ok(())
    }

    /// Render another page of the open document, with `marks` highlighted on
    /// it. A page asked for before the last one finished replaces it: its
    /// sandboxed renderer is killed.
    pub fn page(
        &mut self,
        request: u64,
        page: u32,
        marks: Vec<PdfMark>,
    ) -> Result<(), DocumentError> {
        let Some((open, bytes, pages)) = self.document.clone() else {
            return Err(DocumentError::Failed);
        };
        if open != request || page == 0 || page > pages {
            return Err(DocumentError::Invalid);
        }
        let tools = render::Tools::configured().ok_or(DocumentError::Unavailable)?;
        if let Some((_, pending)) = self.pending.take() {
            pending.abort();
        }
        let (epoch, sender, wake) = (self.epoch, self.sender.clone(), self.wake.clone());
        self.pending = Some((
            request,
            self.runtime.spawn(async move {
                let result = render::page(bytes, page, tools, marks.clone())
                    .await
                    .map(Arc::new);
                let completed = Completed {
                    epoch,
                    result: DocumentResult {
                        request,
                        page,
                        pages,
                        result,
                        marks,
                    },
                    document: None,
                };
                if sender.send(completed).await.is_ok() {
                    wake();
                }
            }),
        ));
        Ok(())
    }

    /// Read the text of the open document's pages, in the same sandbox as
    /// its pages; it arrives through [`Documents::poll_text`].
    pub fn text(&mut self, request: u64) -> Result<(), DocumentError> {
        let Some((open, bytes, _)) = self.document.clone() else {
            return Err(DocumentError::Failed);
        };
        if open != request {
            return Err(DocumentError::Invalid);
        }
        let tools = render::Tools::configured().ok_or(DocumentError::Unavailable)?;
        if let Some(reading) = self.reading.take() {
            reading.abort();
        }
        let (epoch, sender, wake) = (self.epoch, self.text_sender.clone(), self.wake.clone());
        self.reading = Some(self.runtime.spawn(async move {
            let result = render::text(bytes, tools).await.map(Arc::from);
            if sender
                .send((epoch, DocumentText { request, result }))
                .await
                .is_ok()
            {
                wake();
            }
        }));
        Ok(())
    }

    pub fn poll_text(&mut self) -> Option<DocumentText> {
        while let Ok((epoch, text)) = self.text_receiver.try_recv() {
            if epoch == self.epoch {
                self.reading = None;
                return Some(text);
            }
        }
        None
    }

    pub fn cancel(&mut self, request: u64) {
        let pending = self
            .pending
            .as_ref()
            .is_some_and(|(active, _)| *active == request);
        let open = self
            .document
            .as_ref()
            .is_some_and(|(open, ..)| *open == request);
        if pending || open {
            self.clear();
        }
    }

    pub fn clear(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        if let Some((_, pending)) = self.pending.take() {
            pending.abort();
        }
        if let Some(reading) = self.reading.take() {
            reading.abort();
        }
        self.document = None;
        while self.receiver.try_recv().is_ok() {}
        while self.text_receiver.try_recv().is_ok() {}
    }

    pub fn poll(&mut self) -> Option<DocumentResult> {
        while let Ok(completed) = self.receiver.try_recv() {
            if completed.epoch == self.epoch {
                let request = completed.result.request;
                if self
                    .pending
                    .as_ref()
                    .is_some_and(|(active, _)| *active == request)
                {
                    self.pending = None;
                }
                if let Some(bytes) = completed.document {
                    self.document = Some((request, bytes, completed.result.pages));
                }
                return Some(completed.result);
            }
        }
        None
    }
}

impl Drop for Documents {
    fn drop(&mut self) {
        self.clear();
    }
}

/// A page already rendered as `png`, with `marks` highlighted as on a page
/// the sandbox renders: for a capture, which renders nothing.
pub fn highlight_page(png: Vec<u8>, marks: &[PdfMark]) -> Option<ImageData> {
    render::normalize(png, marks).ok()
}

async fn download(
    client: &reqwest::Client,
    url: url::Url,
    expected: u64,
) -> Result<Vec<u8>, DocumentError> {
    let bytes = fetch(client, url, expected, MAX_DOCUMENT_BYTES).await?;
    if !bytes.starts_with(b"%PDF-") {
        return Err(DocumentError::Invalid);
    }
    Ok(bytes)
}

/// The whole file at `url` in memory, at most `limit` bytes and exactly the
/// `expected` size when that is known (not 0).
pub(crate) async fn fetch(
    client: &reqwest::Client,
    url: url::Url,
    expected: u64,
    limit: u64,
) -> Result<Vec<u8>, DocumentError> {
    if expected > limit {
        return Err(DocumentError::TooLarge);
    }
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| DocumentError::Failed)?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(DocumentError::Failed);
    }
    if response.content_length().is_some_and(|size| size > limit) {
        return Err(DocumentError::TooLarge);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| DocumentError::Failed)? {
        if bytes.len().saturating_add(chunk.len()) as u64 > limit {
            return Err(DocumentError::TooLarge);
        }
        bytes.extend_from_slice(&chunk);
    }
    if expected != 0 && expected != bytes.len() as u64 {
        return Err(DocumentError::Invalid);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests;
