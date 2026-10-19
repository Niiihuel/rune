//! Simulated attachment sends: no network, local file I/O or retained payloads.

use std::{sync::atomic::Ordering, time::Duration};

use rune_model::{Attachment, AttachmentId, ChannelId, Message, MessageId, Permissions, Timestamp};
use sha2::{Digest, Sha256};
use tokio::time::{sleep, sleep_until, Instant};

use super::{Session, World, ME};
use crate::{
    Event, Failure, PreparedUpload, UploadError, UploadFence, UploadLimits, UploadSendScope,
};

/// Conservative simulation policy, not a verified policy for any real account.
pub(super) const LIMITS: UploadLimits = UploadLimits {
    max_files: 10,
    max_file_bytes: 20 * 1024 * 1024,
    max_message_bytes: 25 * 1024 * 1024,
};
const MAX_ACTIVE: usize = 8;

pub(super) struct SentMessage {
    pub(super) channel: ChannelId,
    pub(super) id: MessageId,
    pub(super) upload_digest: Option<[u8; 32]>,
}

fn failed(scope: UploadSendScope, error: UploadError) -> Event {
    Event::Failed {
        failure: Failure::SendFiles {
            request: scope.request,
            channel: scope.channel,
            nonce: scope.nonce,
        },
        reason: error.label().into(),
        // Simulation publishes atomically after its delay. Cancellation before
        // that commit is definitive; withholding its ACK is separately tested.
        ambiguous: false,
    }
}

fn live(scope: UploadSendScope, fence: &UploadFence) -> Result<(), UploadError> {
    if fence.is_cancelled() {
        return Err(UploadError::Cancelled);
    }
    if Instant::now() >= Instant::from_std(scope.expires_at) {
        return Err(UploadError::Expired);
    }
    Ok(())
}

fn allowed(
    world: &World,
    scope: UploadSendScope,
    generation: u64,
    content: &str,
    reply_to: Option<MessageId>,
    files: &[PreparedUpload],
    fence: &UploadFence,
) -> Result<(), UploadError> {
    live(scope, fence)?;
    if scope.request == 0
        || scope.nonce == 0
        || scope.account != ME
        || scope.generation != generation
    {
        return Err(UploadError::Denied);
    }
    let channel = world.channel(scope.channel).ok_or(UploadError::Denied)?;
    if !channel.kind.has_messages()
        || !channel
            .permissions
            .contains(Permissions::VIEW | Permissions::SEND | Permissions::ATTACH)
    {
        return Err(UploadError::Denied);
    }
    if let Some(reply) = reply_to {
        if !channel.permissions.contains(Permissions::READ_HISTORY)
            || !world
                .message(scope.channel, reply)
                .is_some_and(|message| message.kind == rune_model::MessageKind::Default)
        {
            return Err(UploadError::Denied);
        }
    }
    if files.is_empty() {
        return Err(UploadError::InvalidFile);
    }
    if files.len() > LIMITS.max_files {
        return Err(UploadError::TooMany);
    }
    if content.chars().count() > 2000 {
        return Err(UploadError::TooLarge);
    }
    if files.iter().enumerate().any(|(index, file)| {
        files[..index]
            .iter()
            .any(|previous| previous.metadata().token == file.metadata().token)
    }) {
        return Err(UploadError::InvalidFile);
    }
    let mut total = content.len() as u64;
    for file in files {
        let bytes = file.metadata().size_bytes;
        if bytes > LIMITS.max_file_bytes {
            return Err(UploadError::TooLarge);
        }
        total = total.checked_add(bytes).ok_or(UploadError::TooLarge)?;
    }
    if total > LIMITS.max_message_bytes {
        return Err(UploadError::TooLarge);
    }
    Ok(())
}

impl Session {
    pub(super) fn send_files(
        &mut self,
        scope: UploadSendScope,
        content: String,
        reply_to: Option<MessageId>,
        files: Vec<PreparedUpload>,
        fence: UploadFence,
        mention_reply: bool,
    ) {
        // A silent message notifies nobody; here nobody is notified anyway (P4).
        let content = crate::without_silent(&content)
            .map(str::to_owned)
            .unwrap_or(content);
        self.requests.retain(|_, task| !task.is_finished());
        let admission = allowed(
            &self.world.lock().unwrap(),
            scope,
            self.events.generation(),
            &content,
            reply_to,
            &files,
            &fence,
        )
        .and_then(|()| {
            if self.requests.len() >= MAX_ACTIVE || self.requests.contains_key(&scope.request) {
                Err(UploadError::Busy)
            } else {
                Ok(())
            }
        });
        if let Err(error) = admission {
            self.later(Duration::ZERO, move |_| vec![failed(scope, error)]);
            return;
        }
        let ignore = self.control.ignore_next_send.swap(false, Ordering::Relaxed);
        let reject = self.control.fail_next_send.swap(false, Ordering::Relaxed);
        let (world, events, latency, now) = (
            self.world.clone(),
            self.events.clone(),
            self.config.latency,
            (self.config.clock)(),
        );
        let handle = tokio::spawn(async move {
            let total_bytes = files.iter().map(|file| file.metadata().size_bytes).sum();
            // Initial progress reports admission, before simulated work. It
            // does not promise cancellation can undo a later atomic commit.
            events.send_lossy(Event::UploadProgress {
                scope,
                sent_bytes: 0,
                total_bytes,
            });
            let stopped = tokio::select! {
                biased;
                _ = fence.cancelled() => Some(UploadError::Cancelled),
                _ = sleep_until(Instant::from_std(scope.expires_at)) => Some(UploadError::Expired),
                _ = sleep(latency) => None,
            };
            if let Some(error) = stopped {
                let _ = events.send(failed(scope, error)).await;
                return;
            }
            if reject {
                let _ = events.send(failed(scope, UploadError::Failed)).await;
                return;
            }
            let digest = match payload_digest(
                scope,
                &fence,
                &content,
                reply_to,
                mention_reply,
                &files,
            )
            .await
            {
                Ok(digest) => digest,
                Err(error) => {
                    let _ = events.send(failed(scope, error)).await;
                    return;
                }
            };
            // Current permission, account, generation, liveness and nonce proof
            // are checked in the same critical section as the atomic commit.
            let result = {
                let mut world = world.lock().unwrap();
                allowed(
                    &world,
                    scope,
                    events.generation(),
                    &content,
                    reply_to,
                    &files,
                    &fence,
                )
                .and_then(|()| world.post_files_once(scope, content, reply_to, &files, digest, now))
            };
            match result {
                Ok(message) => {
                    events.send_lossy(Event::UploadProgress {
                        scope,
                        sent_bytes: total_bytes,
                        total_bytes,
                    });
                    if !ignore {
                        let _ = events
                            .send(Event::MessageCreated {
                                message,
                                nonce: Some(scope.nonce),
                            })
                            .await;
                    }
                }
                Err(error) => {
                    let _ = events.send(failed(scope, error)).await;
                }
            }
        });
        self.requests.insert(scope.request, handle.abort_handle());
    }
}

async fn payload_digest(
    scope: UploadSendScope,
    fence: &UploadFence,
    content: &str,
    reply_to: Option<MessageId>,
    mention_reply: bool,
    files: &[PreparedUpload],
) -> Result<[u8; 32], UploadError> {
    let mut digest = Sha256::new();
    digest.update(b"rune.mock.upload.v1");
    digest.update(scope.account.0.to_le_bytes());
    digest.update(scope.channel.0.to_le_bytes());
    digest.update((content.len() as u64).to_le_bytes());
    digest.update(content.as_bytes());
    digest.update([u8::from(reply_to.is_some())]);
    digest.update(reply_to.map_or(0, |id| id.0).to_le_bytes());
    // Whether the reply notifies is part of the frozen body, as its text is.
    digest.update([u8::from(mention_reply)]);
    digest.update((files.len() as u64).to_le_bytes());
    for file in files {
        let metadata = file.metadata();
        digest.update((metadata.filename.len() as u64).to_le_bytes());
        digest.update(metadata.filename.as_bytes());
        digest.update(metadata.size_bytes.to_le_bytes());
        let bytes = file.bytes();
        for chunk in bytes.chunks(64 * 1024) {
            live(scope, fence)?;
            digest.update(chunk);
            tokio::task::yield_now().await;
        }
    }
    live(scope, fence)?;
    Ok(digest.finalize().into())
}

impl World {
    /// `files` as a message names them once sent, each with a new id.
    pub(super) fn attachments(&mut self, files: &[PreparedUpload]) -> Vec<Attachment> {
        files
            .iter()
            .map(|file| {
                let metadata = file.metadata();
                Attachment {
                    id: AttachmentId(self.fresh_id()),
                    filename: metadata.filename.clone(),
                    size_bytes: metadata.size_bytes,
                    content_type: None,
                    dimensions: None,
                    preview: None,
                    download: None,
                    spoiler: false,
                    voice: None,
                    explicit: false,
                    placeholder: None,
                }
            })
            .collect()
    }

    fn post_files_once(
        &mut self,
        scope: UploadSendScope,
        content: String,
        reply_to: Option<MessageId>,
        files: &[PreparedUpload],
        digest: [u8; 32],
        now: Timestamp,
    ) -> Result<Message, UploadError> {
        if let Some(sent) = self.sent.get(&scope.nonce) {
            if sent.channel != scope.channel || sent.upload_digest != Some(digest) {
                return Err(UploadError::InvalidFile);
            }
            return self
                .message(sent.channel, sent.id)
                .ok_or(UploadError::NotFound);
        }
        let mut message = self
            .post(scope.channel, scope.account, content, reply_to, now)
            .ok_or(UploadError::Denied)?;
        message.attachments = self.attachments(files);
        self.store(scope.channel, message.clone());
        self.sent.insert(
            scope.nonce,
            SentMessage {
                channel: scope.channel,
                id: message.id,
                upload_digest: Some(digest),
            },
        );
        Ok(message)
    }
}
