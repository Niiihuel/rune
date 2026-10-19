//! Outgoing files are metadata, never local paths, descriptors or buffers.

use rune_model::{ChannelId, Delivery, MessageId, PendingAttachment, Timestamp, UploadToken};
use rune_provider::{AuthenticationState, RequestId, UploadSendScope};

#[derive(Default)]
pub(crate) struct PendingFiles {
    pub request: RequestId,
    pub files: Vec<PendingAttachment>,
    pub progress: Option<(u64, u64)>,
}

impl crate::Store {
    /// The largest file the account may attach in `channel`, when the
    /// provider said: its server's value, else the account's.
    pub fn upload_limit(&self, channel: ChannelId) -> Option<u64> {
        let server = self.channel(channel)?.server;
        self.upload_limits
            .get(&server)
            .or_else(|| self.upload_limits.get(&None))
            .copied()
    }

    pub fn add_pending_attachments(
        &mut self,
        channel: ChannelId,
        content: String,
        reply_to: Option<MessageId>,
        nonce: u64,
        request: RequestId,
        files: Vec<PendingAttachment>,
        timestamp: Timestamp,
    ) {
        if files.is_empty() || request == 0 || self.me().is_none() {
            return;
        }
        self.add_pending(channel, content, reply_to, nonce, timestamp);
        self.pending_files.insert(
            nonce,
            PendingFiles {
                request,
                files,
                progress: None,
            },
        );
    }

    pub fn resend_attachments(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        request: RequestId,
    ) {
        let Some(unsent) = self
            .unsent(channel, message)
            .filter(|u| !u.files.is_empty())
        else {
            return;
        };
        if request == 0 {
            return;
        }
        let Some(files) = self.pending_files.get_mut(&unsent.nonce) else {
            return;
        };
        files.request = request;
        files.progress = None;
        self.revision += 1;
        self.update_message(channel, message, |message| {
            message.delivery = Delivery::Sending
        });
    }

    pub fn pending_attachments(
        &self,
        channel: ChannelId,
        message: MessageId,
    ) -> &[PendingAttachment] {
        self.files_for(channel, message)
            .map_or(&[], |files| files.files.as_slice())
    }

    /// A claimed token belongs to the complete outgoing message, including retry.
    pub fn retains_attachment(&self, token: UploadToken) -> bool {
        self.pending_files
            .values()
            .any(|pending| pending.files.iter().any(|file| file.token == token))
    }

    pub fn attachment_progress(
        &self,
        channel: ChannelId,
        message: MessageId,
    ) -> Option<(u64, u64)> {
        self.files_for(channel, message)?.progress
    }

    pub fn pending_attachment_request(
        &self,
        channel: ChannelId,
        message: MessageId,
    ) -> Option<RequestId> {
        Some(self.files_for(channel, message)?.request)
    }

    fn files_for(&self, channel: ChannelId, message: MessageId) -> Option<&PendingFiles> {
        let nonce = u64::MAX - message.0;
        if self.pending.get(&nonce) != Some(&(channel, message)) {
            return None;
        }
        self.pending_files.get(&nonce)
    }

    pub(crate) fn upload_progress(
        &mut self,
        scope: UploadSendScope,
        sent: u64,
        total: u64,
    ) -> crate::Applied {
        if self.me() != Some(scope.account)
            || !self.expects_upload(scope.channel, scope.nonce, scope.request)
        {
            return crate::Applied::Nothing;
        }
        let files = self.pending_files.get_mut(&scope.nonce).unwrap();
        let expected = files
            .files
            .iter()
            .try_fold(0u64, |n, f| n.checked_add(f.size_bytes));
        if expected != Some(total)
            || sent > total
            || files.progress.is_some_and(|(previous, _)| sent < previous)
        {
            return crate::Applied::Nothing;
        }
        files.progress = Some((sent, total));
        crate::Applied::Messages {
            channel: scope.channel,
            added: 0,
        }
    }

    pub(crate) fn expects_upload(
        &self,
        channel: ChannelId,
        nonce: u64,
        request: RequestId,
    ) -> bool {
        self.pending
            .get(&nonce)
            .is_some_and(|(found, _)| *found == channel)
            && self
                .pending_files
                .get(&nonce)
                .is_some_and(|files| files.request == request)
    }

    /// Retain a draft during a temporary disconnection, but never after loss of
    /// identity, capability or channel authorization. Network admission is separate.
    pub fn can_keep_attachments(&self, channel: ChannelId) -> bool {
        self.me().is_some()
            && self.provider.as_ref().is_some_and(|provider| {
                (!provider.capabilities.authentication
                    || matches!(self.authentication, AuthenticationState::SignedIn { .. }))
                    && self.channel(channel).is_some_and(|channel| {
                        super::abilities::abilities(channel, provider.capabilities)
                            .attach
                            .allowed()
                            // A forum's files, for its new post.
                            || (channel.kind == rune_model::ChannelKind::Forum
                                && provider.capabilities.attachments
                                && channel.permissions.contains(
                                    rune_model::Permissions::VIEW | rune_model::Permissions::ATTACH,
                                ))
                    })
            })
    }
}
