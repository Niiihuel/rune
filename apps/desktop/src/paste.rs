//! Ctrl+V of copied files in the composer.
//!
//! A file manager copies files as their list (`text/uri-list`) and also as
//! text naming them, which is all the window's paste event carries. When
//! that text names files, its paste waits: Core reads the clipboard on its
//! files worker and prepares the files as the selector's are. With files,
//! they are attached and the text is dropped; without, or after a short
//! wait, the text goes into the same editing state it was pasted into
//! (limn's captured paste target), or nowhere if that state changed.
//! A picture alone puts no text on the clipboard: its Ctrl+V arrives as
//! `UiAction::PasteImage` and takes the same path as «Pegar del portapapeles»
//! in the composer's menu, which reads the clipboard's picture (rune-ui).

use super::*;
use rune_provider::{RequestId, UploadError, UploadUpdate};

/// How long a held paste waits for the clipboard before the text goes in.
const WAIT: Duration = Duration::from_millis(500);

pub(super) struct FilePaste {
    pub(super) request: RequestId,
    pub(super) target: limn::PasteTarget,
    pub(super) text: String,
    pub(super) until: Instant,
}

/// Whether pasted `text` is how a file manager names copied files: one to
/// ten lines (a message's files), each an absolute path or a `file://`
/// address.
pub(super) fn names_files(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    (1..=10).contains(&lines.len())
        && lines.iter().all(|line| {
            line.len() <= 4096 && (line.starts_with("file:///") || line.starts_with('/'))
        })
}

impl App {
    /// Hold the paste of `text` into the open composer if it names files,
    /// and ask Core for them. `false` leaves the paste to the editor.
    pub(super) fn defer_file_paste(
        &mut self,
        ctx: &egui::Context,
        text: &str,
        focused: Option<&str>,
        now: Instant,
    ) -> bool {
        let Some(channel) = self.ui.channel() else {
            return false;
        };
        // A capture never reads the clipboard.
        if self.file_paste.is_some()
            || self.capture.is_some()
            || focused != Some(rune_ui::editor_id(channel).as_str())
            || !names_files(text)
        {
            return false;
        }
        let Some(target) = self.tree.capture_paste_target() else {
            return false;
        };
        let Ok(request) = rune_ui::begin_attachment_import(
            &mut self.ui,
            &self.store,
            channel,
            rune_ui::AttachmentImportSource::Clipboard,
        ) else {
            return false;
        };
        if self.client.paste_attachments(request, channel).is_err() {
            // The pick the interface opened ends quietly; the text goes in.
            self.settle_pick(ctx, request, channel);
            return false;
        }
        self.file_paste = Some(FilePaste {
            request,
            target,
            text: text.to_owned(),
            until: now + WAIT,
        });
        ctx.request_repaint_after(WAIT);
        true
    }

    /// The answer for `update`'s request: what the interface gets, and the
    /// held text to put in after it, when the clipboard had no files.
    pub(super) fn take_file_paste(
        &mut self,
        update: UploadUpdate,
    ) -> (UploadUpdate, Option<FilePaste>) {
        let held = matches!(&update, UploadUpdate::Prepared { request, .. }
            if self.file_paste.as_ref().is_some_and(|paste| paste.request == *request));
        if !held {
            return (update, None);
        }
        let paste = self.file_paste.take().expect("held");
        match update {
            UploadUpdate::Prepared {
                account,
                channel,
                request,
                result: Err(_),
            } => (
                // Not an error to show: the text is what was pasted.
                UploadUpdate::Prepared {
                    account,
                    channel,
                    request,
                    result: Err(UploadError::Cancelled),
                },
                Some(paste),
            ),
            files => (files, None),
        }
    }

    /// Past its wait, a held paste gives up on the clipboard.
    pub(super) fn expire_file_paste(&mut self, ctx: &egui::Context, now: Instant) {
        if self
            .file_paste
            .as_ref()
            .is_some_and(|paste| paste.until <= now)
        {
            let paste = self.file_paste.take().expect("held");
            self.client.cancel_upload(paste.request);
            self.put_paste(ctx, paste);
        }
    }

    /// Paste the held text into the editing state it was meant for.
    pub(super) fn put_paste(&mut self, ctx: &egui::Context, paste: FilePaste) {
        let now = Instant::now();
        for action in self.tree.handle_event(UiEvent::PasteCaptured {
            target: paste.target,
            text: paste.text,
        }) {
            let intents = rune_ui::update(&mut self.ui, &mut self.store, &action, now);
            self.run(ctx, intents);
        }
    }

    fn settle_pick(&mut self, ctx: &egui::Context, request: RequestId, channel: ChannelId) {
        if let Some(account) = self.store.me() {
            let intents = rune_ui::upload_update(
                &mut self.ui,
                &self.store,
                UploadUpdate::Prepared {
                    account,
                    channel,
                    request,
                    result: Err(UploadError::Cancelled),
                },
            );
            self.run(ctx, intents);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::names_files;

    #[test]
    fn only_paths_and_file_addresses_name_copied_files() {
        assert!(names_files("/home/ana/foto.png"));
        assert!(names_files(
            "file:///home/ana/a.png\nfile:///home/ana/b%20c.txt\n"
        ));
        assert!(names_files("/a\r\n/b"));
        assert!(names_files(&"/a\n".repeat(10)), "a whole message's worth");
        for text in [
            "",
            "hola",
            "mira /etc/hosts",
            "https://example.com/a.png",
            "file://host/a.png",
            "relativo/a.png",
            &"/a\n".repeat(11),
        ] {
            assert!(!names_files(text), "{text:?}");
        }
    }
}
