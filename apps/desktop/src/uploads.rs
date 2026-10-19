//! The host passes only lightweight upload intents and results across the UI.

use super::*;
use rune_model::{MessageId, UploadToken};
use rune_provider::{ConnectionState, RequestId, UploadError, UploadUpdate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeDropRejection {
    Invalid(UploadError),
    Unpositioned,
}

impl NativeDropRejection {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Invalid(error) => error.label(),
            Self::Unpositioned => {
                t("Todavía no podés adjuntar archivos arrastrándolos. Usá Adjuntar.")
            }
        }
    }
}

/// Reject legacy metadata before limn copies paths. RawInput's last pointer
/// position is not tied to its file batch, so even valid paths cannot authorize
/// a filesystem job. Reading a handle's path never reads its contents.
pub(super) fn reject_legacy_drop(input: &mut egui::RawInput) -> Option<NativeDropRejection> {
    let dropped = !input.dropped_files.is_empty();
    let error = if input.dropped_files.len() > 10 {
        Some(UploadError::TooMany)
    } else {
        input
            .dropped_files
            .iter()
            .try_fold(0usize, |total, file| {
                let path = file.path();
                let bytes = path.as_os_str().as_encoded_bytes();
                if !path.is_absolute() || bytes.is_empty() || bytes.contains(&0) {
                    return Err(UploadError::InvalidFile);
                }
                if bytes.len() > 4096 {
                    return Err(UploadError::TooLarge);
                }
                total
                    .checked_add(bytes.len())
                    .filter(|total| *total <= 10 * 4096)
                    .ok_or(UploadError::TooLarge)
            })
            .err()
    };
    input.dropped_files.clear();
    input.hovered_files.clear();
    error
        .map(NativeDropRejection::Invalid)
        .or(dropped.then_some(NativeDropRejection::Unpositioned))
}

fn permitted(store: &Store, channel: ChannelId, reply_to: Option<MessageId>) -> bool {
    store.me().is_some()
        && store.connection == ConnectionState::Connected
        && (store.abilities(channel).is_some_and(|abilities| {
            abilities.write.allowed()
                && abilities.attach.allowed()
                && (reply_to.is_none() || abilities.read_history.allowed())
        })
            // A forum's files wait for its new post.
            || (reply_to.is_none() && store.post_files(channel).allowed()))
}

impl App {
    pub(super) fn reject_native_drop(&mut self, error: NativeDropRejection, now: Instant) {
        self.ui.notify(error.label(), None, true, now);
        if let Some(capture) = self.capture.as_mut() {
            if capture.file_drop == Some(DropCapture::Waiting) {
                capture.file_drop = Some(DropCapture::Rejected);
                capture.ready = Some(now);
            }
        }
    }

    pub(super) fn pick_attachment(
        &mut self,
        ctx: &egui::Context,
        request: RequestId,
        channel: ChannelId,
    ) {
        // An app's modal open here may take files where nothing is written.
        let modal = self.store.modal().is_some_and(|(open, _)| open == channel)
            && self.store.interaction_abilities(channel).files.allowed();
        let result = if self.ui.channel() == Some(channel)
            && (permitted(&self.store, channel, None) || modal)
        {
            match self.capture {
                // A capture, run from the repository as its docs say, answers
                // with a file of it: no dialog opens.
                Some(_) => match std::env::current_dir() {
                    Ok(root) => self.client.prepare_attachment_paths(
                        request,
                        channel,
                        vec![root.join("assets/audio-sample.mp3")],
                    ),
                    Err(_) => Err(UploadError::NotFound),
                },
                // Over the window (escritorio-linux): its X11 id, or its
                // Wayland toplevel exported; unparented without either.
                None => {
                    let parent = self.parent.get();
                    self.client.pick_attachment(request, channel, parent)
                }
            }
        } else {
            Err(UploadError::Denied)
        };
        if let (Err(error), Some(account)) = (result, self.store.me()) {
            let intents = rune_ui::upload_update(
                &mut self.ui,
                &self.store,
                UploadUpdate::Prepared {
                    account,
                    channel,
                    request,
                    result: Err(error),
                },
            );
            self.run(ctx, intents);
        }
    }

    /// A forum post whose first message carries the files prepared for it.
    pub(super) fn create_post(
        &mut self,
        request: RequestId,
        forum: ChannelId,
        title: String,
        tags: Vec<rune_model::TagId>,
        content: String,
        files: Vec<UploadToken>,
    ) {
        let result = if self.store.post_files(forum).allowed() {
            self.client
                .create_post(request, forum, title, tags, content, files)
        } else {
            Err(UploadError::Denied)
        };
        if let Err(error) = result {
            rune_ui::post_refused(&mut self.ui, request, error);
        }
    }

    pub(super) fn send_attachments(
        &mut self,
        ctx: &egui::Context,
        request: RequestId,
        channel: ChannelId,
        content: String,
        reply_to: Option<MessageId>,
        mention_reply: bool,
        files: Vec<UploadToken>,
    ) {
        let result = if self.ui.channel() == Some(channel)
            && permitted(&self.store, channel, reply_to)
            && rune_ui::attachment_send_pending(&self.ui, request, channel, &files)
        {
            self.client
                .send_attachments(request, channel, content, reply_to, mention_reply, files)
        } else {
            Err(UploadError::Denied)
        };
        let intents = rune_ui::attachment_send_admitted(
            &mut self.ui,
            &mut self.store,
            request,
            result,
            wall_clock(),
        );
        self.run(ctx, intents);
    }

    pub(super) fn retry_attachment_send(
        &mut self,
        ctx: &egui::Context,
        request: RequestId,
        channel: ChannelId,
        message: MessageId,
    ) {
        let Some(unsent) = self.store.unsent(channel, message) else {
            return;
        };
        if unsent.files.is_empty() || !permitted(&self.store, channel, unsent.reply_to) {
            self.ui.notify(
                t("No se pudo reintentar"),
                Some(UploadError::Denied.label().to_owned()),
                true,
                Instant::now(),
            );
            return;
        }
        // The service owns the original immutable body. A missing grant is an
        // error; this path can never resend only its text.
        match self.client.retry_attachment_send(request, unsent.nonce) {
            Ok(()) => self.store.resend_attachments(channel, message, request),
            Err(error) => self.ui.notify(
                t("No se pudo reintentar"),
                Some(error.label().to_owned()),
                true,
                Instant::now(),
            ),
        }
        ctx.request_repaint();
    }

    pub(super) fn drain_uploads(&mut self, ctx: &egui::Context) {
        self.expire_file_paste(ctx, Instant::now());
        for _ in 0..8 {
            let Some(result) = self.client.poll_upload() else {
                return;
            };
            let (result, text) = self.take_file_paste(result);
            let intents = rune_ui::upload_update(&mut self.ui, &self.store, result);
            self.run(ctx, intents);
            if let Some(text) = text {
                self.put_paste(ctx, text);
            }
        }
        ctx.request_repaint();
    }

    /// The files the open conversation's draft kept from an earlier run come
    /// back as chosen files do, once it can take them; until then they
    /// wait (envios-y-subidas).
    pub(super) fn restore_draft_files(&mut self, ctx: &egui::Context) {
        let Some(channel) = self.ui.channel() else {
            return;
        };
        if !self.client.has_draft_files(channel)
            || rune_ui::attachment_pick_pending(&self.ui)
            || !permitted(&self.store, channel, None)
        {
            return;
        }
        let Ok(request) = rune_ui::begin_attachment_import(
            &mut self.ui,
            &self.store,
            channel,
            rune_ui::AttachmentImportSource::Restore,
        ) else {
            return;
        };
        if let (Err(error), Some(account)) = (
            self.client.restore_draft_files(request, channel),
            self.store.me(),
        ) {
            // Busy: they wait for a later frame, without a word each time.
            let error = match error {
                UploadError::Busy => UploadError::Cancelled,
                error => error,
            };
            let intents = rune_ui::upload_update(
                &mut self.ui,
                &self.store,
                UploadUpdate::Prepared {
                    account,
                    channel,
                    request,
                    result: Err(error),
                },
            );
            self.run(ctx, intents);
        }
    }

    /// «Pegar del portapapeles»: the clipboard's files or picture, prepared
    /// by Core as chosen files are.
    pub(super) fn paste_attachments(
        &mut self,
        ctx: &egui::Context,
        request: RequestId,
        channel: ChannelId,
    ) {
        let result = if self.ui.channel() == Some(channel) && permitted(&self.store, channel, None)
        {
            self.client.paste_attachments(request, channel)
        } else {
            Err(UploadError::Denied)
        };
        if let (Err(error), Some(account)) = (result, self.store.me()) {
            let intents = rune_ui::upload_update(
                &mut self.ui,
                &self.store,
                UploadUpdate::Prepared {
                    account,
                    channel,
                    request,
                    result: Err(error),
                },
            );
            self.run(ctx, intents);
        }
    }
}

#[cfg(test)]
mod drop_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::{PendingAttachment, Timestamp};
    use rune_provider::{Event, Failure};

    #[test]
    fn a_legacy_retry_cannot_post_only_the_text_when_the_file_grant_is_gone() {
        let ctx = egui::Context::default();
        let provider = Arc::new(MockProvider::new(MockConfig {
            latency: Duration::ZERO,
            ambient: false,
            replies: false,
            ..MockConfig::default()
        }));
        let mut app = App::new(
            &ctx,
            Instant::now(),
            Arguments {
                screenshot: Some("unused-headless-file-retry.png".into()),
                ..Arguments::default()
            },
            provider.clone(),
            None,
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        while app.store.me().is_none() {
            app.drain(&ctx, Instant::now());
            assert!(
                Instant::now() < deadline,
                "the simulated account did not connect"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        let snapshot = provider.snapshot();
        let channel = snapshot
            .channels
            .iter()
            .find(|channel| channel.name == "general")
            .unwrap()
            .id;
        let before = snapshot.last_messages.clone();
        let attachment = PendingAttachment {
            token: UploadToken(17),
            filename: "missing-grant.txt".into(),
            size_bytes: 50,
        };
        let nonce = 70_001;
        app.store.add_pending_attachments(
            channel,
            "Con su archivo".into(),
            None,
            nonce,
            700,
            vec![attachment.clone()],
            Timestamp(1),
        );
        app.store.apply(
            Event::Failed {
                failure: Failure::SendFiles {
                    request: 700,
                    channel,
                    nonce,
                },
                reason: "Fixture sin concesión en Core".into(),
                ambiguous: false,
            },
            Instant::now(),
        );
        let shown = app
            .store
            .timeline(channel)
            .unwrap()
            .messages()
            .iter()
            .find(|message| message.content == "Con su archivo" && message.delivery.is_local())
            .unwrap()
            .id;
        app.run(
            &ctx,
            vec![Intent::Resend {
                channel,
                message: shown,
            }],
        );
        // Cross the real runtime/provider boundary: an accidental text send
        // would post immediately and consume the local nonce on its ACK.
        let until = Instant::now() + Duration::from_millis(150);
        while Instant::now() < until {
            app.drain(&ctx, Instant::now());
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(provider.snapshot().last_messages, before);
        let unsent = app
            .store
            .unsent(channel, shown)
            .expect("the complete failed send remains available");
        assert_eq!(unsent.content, "Con su archivo");
        assert_eq!(unsent.files, vec![attachment]);
    }
}
