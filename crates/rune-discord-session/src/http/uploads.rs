//! Outgoing attachments by the route Serein uses (its `discord-api` crate,
//! `upload.rs`, read 2026-10-17): Discord names a storage target for each
//! file (`POST /channels/{id}/attachments`), the bytes go there in a plain
//! `PUT` that carries no account header, and the message names the stored
//! files. Nothing reaches the conversation until that last request, which is
//! one attempt, never repeated. Fixtures only: never tried against Discord.

use rune_i18n::es;
use std::{
    convert::Infallible,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
};

use bytes::Bytes;
use http_body::{Frame, SizeHint};
use reqwest::{
    header::{CONTENT_LENGTH, CONTENT_TYPE, USER_AGENT},
    Method,
};
use rune_model::MessageId;
use rune_provider::{Event, EventSink, PreparedUpload, UploadError, UploadFence, UploadSendScope};
use serde_json::{json, Value};
use tokio::time::Instant;

use super::{mapped_message, read_bounded, Http, RestError};
use crate::SessionSecret;

/// The only storage that may receive an upload's bytes.
const STORAGE_HOST: &str = "discord-attachments-uploads-prd.storage.googleapis.com";
/// What one progress report covers; reports are dropped when the queue is full.
const CHUNK: usize = 256 * 1024;
const ANSWER: usize = 64 * 1024;

/// A file Discord stored, as the message names it.
struct Staged {
    filename: String,
    uploaded: String,
}

fn stopped(error: UploadError) -> RestError {
    RestError::rejected(error.label())
}

impl Http {
    /// Stage every file of `files`, then send the message naming them, with
    /// `nonce` so the Gateway's copy confirms it too, before `deadline`
    /// (the scope's, unless a verification moved it). `posting` turns true
    /// when the message request starts: past it, a failure is ambiguous. A
    /// reply notifies its author only with `mention_reply`, as a text one.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_files(
        &self,
        secret: &SessionSecret,
        events: &EventSink,
        scope: UploadSendScope,
        deadline: Instant,
        content: &str,
        reply_to: Option<MessageId>,
        mention_reply: bool,
        files: &[PreparedUpload],
        fence: &UploadFence,
        nonce: &str,
        posting: &AtomicBool,
    ) -> Result<Vec<Event>, RestError> {
        if content.encode_utf16().count() > 2000 {
            return Err(stopped(UploadError::TooLarge));
        }
        // A timer may first fire a tick late: a past deadline sends nothing.
        if Instant::now() >= deadline {
            return Err(stopped(UploadError::Expired));
        }
        let total = files.iter().map(|file| file.metadata().size_bytes).sum();
        events.send_lossy(progress(scope, 0, total));
        let staged = tokio::select! {
            biased;
            _ = fence.cancelled() => return Err(stopped(UploadError::Cancelled)),
            _ = tokio::time::sleep_until(deadline) =>
                return Err(stopped(UploadError::Expired)),
            staged = self.stage_files(secret, scope.channel, Some((events, Report::Send(scope))), files, total) => staged?,
        };
        if fence.is_cancelled() {
            return Err(stopped(UploadError::Cancelled));
        }
        let channel = scope.channel;
        let attachments = named(&staged);
        let mut body = json!({"nonce": nonce, "attachments": attachments,
            "allowed_mentions": {"parse": ["users"],
                "replied_user": mention_reply && reply_to.is_some()}});
        // `@silent` first: nobody is notified (P4); with files it may be all.
        let content = match rune_provider::without_silent(content) {
            Some(rest) => {
                body["flags"] = json!(super::SUPPRESS_NOTIFICATIONS);
                rest
            }
            None => content,
        };
        if !content.trim().is_empty() {
            body["content"] = json!(content);
        }
        if let Some(message) = reply_to {
            body["message_reference"] = json!({"message_id": message.to_string(),
                "channel_id": channel.to_string(), "fail_if_not_exists": true});
        }
        // Cancelling from here on cannot take the message back: it finishes
        // and the answer says what happened.
        posting.store(true, Ordering::Release);
        let value = self
            .request(
                secret,
                Method::POST,
                &format!("/channels/{channel}/messages"),
                Some(body),
                1024 * 1024,
            )
            .await?;
        mapped_message(&value, channel, None, Some(scope.nonce), true)
    }

    /// Stage `files` for a new post's first message in `channel`, reporting
    /// the bytes handed over as the post `request`'s when `events` is given;
    /// what the message names them by.
    pub(crate) async fn post_files(
        &self,
        secret: &SessionSecret,
        channel: rune_model::ChannelId,
        files: &[PreparedUpload],
        progress: Option<(&EventSink, rune_provider::RequestId)>,
    ) -> Result<Vec<Value>, RestError> {
        let total = files.iter().map(|file| file.metadata().size_bytes).sum();
        let progress = progress.map(|(events, request)| (events, Report::Post(request)));
        if let Some((events, report)) = progress {
            events.send_lossy(report.event(0, total));
        }
        let staged = self
            .stage_files(secret, channel, progress, files, total)
            .await?;
        Ok(named(&staged))
    }

    /// Each of `files` staged for a message in `channel`, reporting the
    /// bytes handed over to `progress`'s send or post when there is one.
    async fn stage_files(
        &self,
        secret: &SessionSecret,
        channel: rune_model::ChannelId,
        progress: Option<(&EventSink, Report)>,
        files: &[PreparedUpload],
        total: u64,
    ) -> Result<Vec<Staged>, RestError> {
        let mut staged = Vec::with_capacity(files.len());
        let mut sent = 0;
        for (index, file) in files.iter().enumerate() {
            let metadata = file.metadata();
            let body = json!({"files": [{"id": index.to_string(), "filename": metadata.filename,
                "file_size": metadata.size_bytes, "is_clip": false}]});
            // No message exists yet: whatever happens here, nothing was sent.
            let answer = self
                .request(
                    secret,
                    Method::POST,
                    &format!("/channels/{channel}/attachments"),
                    Some(body),
                    ANSWER,
                )
                .await
                .map_err(|error| RestError {
                    ambiguous: false,
                    ..error
                })?;
            let uploaded = self
                .store(&answer, index, file, progress, sent, total)
                .await?;
            sent += metadata.size_bytes;
            staged.push(Staged {
                filename: metadata.filename.clone(),
                uploaded,
            });
        }
        Ok(staged)
    }

    /// PUT `file` to the target `answer` names; its stored name.
    #[allow(clippy::too_many_arguments)]
    async fn store(
        &self,
        answer: &Value,
        index: usize,
        file: &PreparedUpload,
        progress: Option<(&EventSink, Report)>,
        before: u64,
        total: u64,
    ) -> Result<String, RestError> {
        let unsupported = || {
            RestError::rejected(es(
                "Discord no indicó dónde guardar el archivo; no se envió nada.",
            ))
        };
        let target = answer["attachments"]
            .as_array()
            .filter(|targets| targets.len() == 1)
            .map(|targets| &targets[0])
            .ok_or_else(unsupported)?;
        let id = &target["id"];
        if !(id.is_null() || *id == json!(index) || *id == json!(index.to_string())) {
            return Err(unsupported());
        }
        let uploaded = target["upload_filename"]
            .as_str()
            .filter(|name| (1..=1024).contains(&name.len()) && !name.chars().any(char::is_control))
            .ok_or_else(unsupported)?;
        let url = target["upload_url"]
            .as_str()
            .and_then(|url| self.storage_url(url))
            .ok_or_else(unsupported)?;
        let bytes = file.bytes();
        let size = bytes.len() as u64;
        let response = self
            .storage
            .put(url)
            .header(USER_AGENT, crate::browser::user_agent())
            .header(CONTENT_TYPE, content_type(&file.metadata().filename))
            .header(CONTENT_LENGTH, size)
            .body(match progress {
                Some((events, report)) => reqwest::Body::wrap(Progressive {
                    bytes,
                    sent: 0,
                    before,
                    total,
                    report,
                    events: events.clone(),
                }),
                None => reqwest::Body::from(bytes),
            })
            .send()
            .await
            .map_err(|_| {
                RestError::rejected(es("No se pudo subir el archivo; no se envió nada."))
            })?;
        if !response.status().is_success() {
            return Err(RestError::rejected(es(
                "El almacenamiento rechazó el archivo; no se envió nada.",
            )));
        }
        read_bounded(response, ANSWER)
            .await
            .map_err(|reason| RestError::response(reason, false))?;
        Ok(uploaded.to_owned())
    }

    /// Only Discord's upload storage, over HTTPS, without credentials in it.
    fn storage_url(&self, value: &str) -> Option<url::Url> {
        if value.len() > 4096 {
            return None;
        }
        let url = url::Url::parse(value).ok()?;
        if !url.username().is_empty() || url.password().is_some() || url.fragment().is_some() {
            return None;
        }
        let allowed = url.scheme() == "https"
            && url.host_str() == Some(STORAGE_HOST)
            && url.port_or_known_default() == Some(443);
        // Fixtures: the loopback origin the session's own requests go to.
        #[cfg(test)]
        let allowed = allowed
            || (url.scheme() == "http"
                && self
                    .base
                    .starts_with(&format!("{}/", url.origin().ascii_serialization())));
        allowed.then_some(url)
    }
}

/// The stored files as a message names them, in order.
fn named(staged: &[Staged]) -> Vec<Value> {
    staged
        .iter()
        .enumerate()
        .map(|(index, file)| {
            json!({"id": index.to_string(), "filename": file.filename,
                "uploaded_filename": file.uploaded})
        })
        .collect()
}

fn progress(scope: UploadSendScope, sent_bytes: u64, total_bytes: u64) -> Event {
    Report::Send(scope).event(sent_bytes, total_bytes)
}

/// Whose bytes a progress report counts: a message's send, or a new post's.
#[derive(Clone, Copy)]
enum Report {
    Send(UploadSendScope),
    Post(rune_provider::RequestId),
}

impl Report {
    fn event(self, sent_bytes: u64, total_bytes: u64) -> Event {
        match self {
            Self::Send(scope) => Event::UploadProgress {
                scope,
                sent_bytes,
                total_bytes,
            },
            Self::Post(request) => Event::PostProgress {
                request,
                sent_bytes,
                total_bytes,
            },
        }
    }
}

/// A file's bytes handed to HTTP a chunk at a time, each one reported. The
/// count is of bytes handed over, not a receipt from the storage.
struct Progressive {
    bytes: Bytes,
    sent: usize,
    before: u64,
    total: u64,
    report: Report,
    events: EventSink,
}

impl http_body::Body for Progressive {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        let start = self.sent;
        if start >= self.bytes.len() {
            return Poll::Ready(None);
        }
        let end = (start + CHUNK).min(self.bytes.len());
        let chunk = self.bytes.slice(start..end);
        self.sent = end;
        self.events
            .send_lossy(self.report.event(self.before + end as u64, self.total));
        Poll::Ready(Some(Ok(Frame::data(chunk))))
    }

    fn is_end_stream(&self) -> bool {
        self.sent >= self.bytes.len()
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::with_exact((self.bytes.len() - self.sent) as u64)
    }
}

/// The media type the storage is told for each extension (any case);
/// anything else goes as plain bytes.
const MEDIA_TYPES: [(&str, &str); 19] = [
    ("csv", "text/plain"),
    ("gif", "image/gif"),
    ("jpeg", "image/jpeg"),
    ("jpg", "image/jpeg"),
    ("json", "application/json"),
    ("log", "text/plain"),
    ("md", "text/plain"),
    ("mov", "video/quicktime"),
    ("mp3", "audio/mpeg"),
    ("mp4", "video/mp4"),
    ("ogg", "audio/ogg"),
    ("opus", "audio/ogg"),
    ("pdf", "application/pdf"),
    ("png", "image/png"),
    ("txt", "text/plain"),
    ("wav", "audio/wav"),
    ("webm", "video/webm"),
    ("webp", "image/webp"),
    ("zip", "application/zip"),
];

fn content_type(filename: &str) -> &'static str {
    let (_, extension) = filename.rsplit_once('.').unwrap_or_default();
    MEDIA_TYPES
        .iter()
        .find(|(known, _)| extension.eq_ignore_ascii_case(known))
        .map_or("application/octet-stream", |(_, kind)| kind)
}

/// One attempt in flight: its command for the failure, when it started,
/// and whether the message request started.
pub(crate) struct Upload {
    pub command: rune_provider::Command,
    pub started: Instant,
    pub posting: Arc<AtomicBool>,
    pub future: rune_provider::BoxFuture<'static, Result<Vec<Event>, RestError>>,
}

impl Upload {
    /// Start sending `command`, a `SendFiles` already admitted, before `deadline`.
    pub fn start(
        http: Http,
        secret: Arc<SessionSecret>,
        events: EventSink,
        command: rune_provider::Command,
        nonce: String,
        deadline: Instant,
    ) -> Self {
        let posting = Arc::new(AtomicBool::new(false));
        let rune_provider::Command::SendFiles {
            scope,
            content,
            reply_to,
            files,
            fence,
            mention_reply,
        } = command.clone()
        else {
            unreachable!("only file sends are uploads")
        };
        let flag = posting.clone();
        Self {
            command,
            started: Instant::now(),
            posting,
            future: Box::pin(async move {
                http.send_files(
                    &secret,
                    &events,
                    scope,
                    deadline,
                    &content,
                    reply_to,
                    mention_reply,
                    &files,
                    &fence,
                    &nonce,
                    &flag,
                )
                .await
            }),
        }
    }

    /// Whether a failure now leaves the message's fate unknown.
    pub fn ambiguous(&self) -> bool {
        self.posting.load(Ordering::Acquire)
    }
}

#[cfg(test)]
#[path = "uploads_tests.rs"]
mod tests;
