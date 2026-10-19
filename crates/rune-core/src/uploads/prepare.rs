//! One fixed filesystem worker and leases retained by sealed byte owners.

use std::{
    fs::{File, Metadata},
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
    sync::{
        mpsc::{sync_channel, SyncSender},
        Arc, Mutex,
    },
};

use bytes::Bytes;
use rune_i18n::{es, t};
use rune_model::{PendingAttachment, UploadToken};
use rune_platform::file_dialogs::{CancelSignal, SelectedFile};
use rune_provider::{PreparedUpload, UploadError};
use tokio::sync::oneshot;

pub(super) const MEMORY_BYTES: usize = 32 * 1024 * 1024;
/// Files prepared at once, across drafts: a whole message's worth.
pub(super) const MAX_ENTRIES: usize = 10;

#[derive(Default)]
pub(super) struct Budget(Mutex<(usize, usize)>);

pub(super) struct Lease {
    budget: Arc<Budget>,
    bytes: usize,
}

impl Budget {
    pub(super) fn reserve_many(
        self: &Arc<Self>,
        sizes: &[usize],
    ) -> Result<Vec<Lease>, UploadError> {
        let mut used = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if sizes.is_empty() || sizes.len() > MAX_ENTRIES.saturating_sub(used.1) {
            return Err(UploadError::TooMany);
        }
        let bytes = sizes
            .iter()
            .try_fold(0usize, |sum, size| sum.checked_add(*size))
            .ok_or(UploadError::MemoryLimit)?;
        if bytes > MEMORY_BYTES.saturating_sub(used.0) {
            return Err(UploadError::MemoryLimit);
        }
        used.0 += bytes;
        used.1 += sizes.len();
        Ok(sizes
            .iter()
            .map(|bytes| Lease {
                budget: self.clone(),
                bytes: *bytes,
            })
            .collect())
    }

    #[cfg(test)]
    pub(super) fn reserve(self: &Arc<Self>, bytes: usize) -> Result<Lease, UploadError> {
        Ok(self.reserve_many(&[bytes])?.pop().unwrap())
    }

    pub(super) fn used(&self) -> (usize, usize) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let mut used = self
            .budget
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        used.0 -= self.bytes;
        used.1 -= 1;
    }
}

struct Owner {
    bytes: Box<[u8]>,
    _lease: Lease,
}

impl AsRef<[u8]> for Owner {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

pub(super) fn seal(bytes: Box<[u8]>, lease: Lease) -> Bytes {
    Bytes::from_owner(Owner {
        bytes,
        _lease: lease,
    })
}

pub(super) type Prepared = Result<Vec<PreparedUpload>, UploadError>;
/// A preparation's files, and where each was read from, in order, when
/// they have a path (a pasted picture has none): a draft keeps them.
pub(super) type Answer = (Prepared, Vec<PathBuf>);

struct Input {
    file: File,
    selected: Metadata,
    metadata: PendingAttachment,
    lease: Lease,
}

struct Work {
    source: Source,
    token: UploadToken,
    budget: Arc<Budget>,
    cancel: CancelSignal,
    result: oneshot::Sender<Answer>,
}

pub(super) enum Source {
    Selected(Vec<SelectedFile>),
    Paths {
        paths: Vec<PathBuf>,
        max_files: usize,
        max_file_bytes: u64,
        max_total_bytes: u64,
    },
    /// Read the clipboard here, on the worker: it blocks on its owner.
    Clipboard {
        max_files: usize,
        max_file_bytes: u64,
        max_total_bytes: u64,
    },
}

/// The pixels of a pasted picture read at most: 4096 × 4096 RGBA.
const CLIPBOARD_PIXELS: usize = 64 * 1024 * 1024;
/// What a pasted picture is called; it has no name of its own.
pub(super) const PASTED_IMAGE: &str = es("imagen.png");

pub(super) struct PrepareWorker {
    jobs: SyncSender<Work>,
    pending: Option<oneshot::Receiver<Answer>>,
}

impl PrepareWorker {
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub(super) async fn discard_pending(&mut self) {
        if let Some(answer) = self.pending.as_mut() {
            // This is a cancelled request's result. Await only on the actor,
            // then release its sealed buffer without requiring another pick.
            let _ = answer.await;
        }
        self.pending = None;
    }

    #[cfg(test)]
    pub(super) fn retain_cancelled(&mut self, answer: oneshot::Receiver<Answer>) {
        assert!(self.pending.is_none());
        self.pending = Some(answer);
    }

    pub(super) fn new() -> Result<Self, UploadError> {
        let (jobs, receiver) = sync_channel::<Work>(1);
        // One detached worker may finish a filesystem syscall after cancel.
        // Self retains its receiver; another preparation drains it first.
        drop(
            std::thread::Builder::new()
                .name("rune-upload-files".into())
                .spawn(move || {
                    while let Ok(work) = receiver.recv() {
                        let Work {
                            source,
                            token,
                            budget,
                            cancel,
                            result,
                        } = work;
                        let prepared = prepare_source(source, token, &budget, cancel);
                        // The result sender and any cancelled sealed buffers
                        // are released on this same filesystem service thread.
                        let _ = result.send(prepared);
                    }
                })
                .map_err(|_| UploadError::Failed)?,
        );
        Ok(Self {
            jobs,
            pending: None,
        })
    }

    pub(super) async fn wait_idle(&mut self, cancel: CancelSignal) -> Result<(), UploadError> {
        if cancelled(&cancel) {
            return Err(UploadError::Cancelled);
        }
        if self.pending.is_some() {
            let _ = self.finish(cancel.clone()).await;
            if cancelled(&cancel) {
                return Err(UploadError::Cancelled);
            }
        }
        Ok(())
    }

    pub(super) async fn prepare(
        &mut self,
        source: Source,
        token: UploadToken,
        budget: &Arc<Budget>,
        cancel: CancelSignal,
    ) -> Answer {
        if let Err(error) = self.wait_idle(cancel.clone()).await {
            return (Err(error), Vec::new());
        }
        let (result, answer) = oneshot::channel();
        if self
            .jobs
            .try_send(Work {
                source,
                token,
                budget: budget.clone(),
                cancel: cancel.clone(),
                result,
            })
            .is_err()
        {
            return (Err(UploadError::Busy), Vec::new());
        }
        self.pending = Some(answer);
        self.finish(cancel).await
    }

    async fn finish(&mut self, mut cancel: CancelSignal) -> Answer {
        let failed = |error| (Err(error), Vec::new());
        let Some(answer) = self.pending.as_mut() else {
            return failed(UploadError::Failed);
        };
        let result = tokio::select! {
            biased;
            _ = wait_cancel(&mut cancel) => return failed(UploadError::Cancelled),
            result = answer => result,
        };
        self.pending = None;
        if cancelled(&cancel) {
            return failed(UploadError::Cancelled);
        }
        result.unwrap_or_else(|_| failed(UploadError::Failed))
    }
}

fn prepare_source(
    source: Source,
    token: UploadToken,
    budget: &Arc<Budget>,
    cancel: CancelSignal,
) -> Answer {
    let mut origins = Vec::new();
    let prepared = read_source(source, token, budget, cancel, &mut origins);
    if prepared.is_err() {
        origins.clear();
    }
    (prepared, origins)
}

fn read_source(
    source: Source,
    token: UploadToken,
    budget: &Arc<Budget>,
    cancel: CancelSignal,
    origins: &mut Vec<PathBuf>,
) -> Prepared {
    if cancelled(&cancel) {
        return Err(UploadError::Cancelled);
    }
    let selected = match source {
        Source::Selected(selected) => selected,
        Source::Paths {
            paths,
            max_files,
            max_file_bytes,
            max_total_bytes,
        } => rune_platform::file_dialogs::open_local_files(
            &paths,
            max_files,
            max_file_bytes,
            max_total_bytes,
            &cancel,
        )
        .map_err(super::pick_error)?,
        Source::Clipboard {
            max_files,
            max_file_bytes,
            max_total_bytes,
        } => {
            use rune_platform::clipboard::{read_attachments, ClipboardError, ClipboardFiles};
            match read_attachments(max_files, CLIPBOARD_PIXELS) {
                Ok(ClipboardFiles::Paths(paths)) => {
                    rune_platform::file_dialogs::validate_local_paths(&paths, max_files)
                        .map_err(super::pick_error)?;
                    rune_platform::file_dialogs::open_local_files(
                        &paths,
                        max_files,
                        max_file_bytes,
                        max_total_bytes,
                        &cancel,
                    )
                    .map_err(super::pick_error)?
                }
                Ok(ClipboardFiles::Image {
                    width,
                    height,
                    rgba,
                }) => {
                    let limit = max_file_bytes.min(max_total_bytes);
                    let png = encode_png(width, height, &rgba, limit)?;
                    drop(rgba);
                    return sealed_png(png, token, budget, &cancel);
                }
                Err(ClipboardError::Empty) => return Err(UploadError::Empty),
                Err(ClipboardError::TooLarge) => return Err(UploadError::TooLarge),
                Err(ClipboardError::Unavailable) => return Err(UploadError::Unavailable),
            }
        }
    };
    *origins = selected.iter().map(|file| file.path().to_owned()).collect();
    let files = prepare_inputs(selected, token, budget)?;
    read_batch(files, cancel, |_| {})
}

/// A pasted picture as PNG, refused once it passes `limit` bytes.
fn encode_png(
    width: usize,
    height: usize,
    rgba: &[u8],
    limit: u64,
) -> Result<Vec<u8>, UploadError> {
    use image::ImageEncoder;
    let (Ok(width), Ok(height)) = (u32::try_from(width), u32::try_from(height)) else {
        return Err(UploadError::TooLarge);
    };
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|_| UploadError::InvalidFile)?;
    if png.len() as u64 > limit {
        return Err(UploadError::TooLarge);
    }
    Ok(png)
}

/// The encoded picture as one prepared file, inside the shared budget.
fn sealed_png(
    png: Vec<u8>,
    token: UploadToken,
    budget: &Arc<Budget>,
    cancel: &CancelSignal,
) -> Prepared {
    if token.0 == 0 {
        return Err(UploadError::InvalidFile);
    }
    let size = png.len();
    let lease = budget
        .reserve_many(&[size])?
        .pop()
        .ok_or(UploadError::Failed)?;
    if cancelled(cancel) {
        return Err(UploadError::Cancelled);
    }
    let metadata = PendingAttachment {
        token,
        filename: t(PASTED_IMAGE).into(),
        size_bytes: size as u64,
    };
    Ok(vec![PreparedUpload::new(
        metadata,
        seal(png.into_boxed_slice(), lease),
    )?])
}

fn prepare_inputs(
    selected: Vec<SelectedFile>,
    token: UploadToken,
    budget: &Arc<Budget>,
) -> Result<Vec<Input>, UploadError> {
    if selected.is_empty() || selected.len() > MAX_ENTRIES || token.0 == 0 {
        return Err(UploadError::InvalidFile);
    }
    let sizes = selected
        .iter()
        .map(|file| usize::try_from(file.info().size_bytes).map_err(|_| UploadError::TooLarge))
        .collect::<Result<Vec<_>, _>>()?;
    // Reserve the entire batch on the fixed filesystem worker, before any
    // content is read. A sealed buffer retains only its individual lease.
    let leases = budget.reserve_many(&sizes)?;
    selected
        .into_iter()
        .zip(leases)
        .enumerate()
        .map(|(index, (file, lease))| {
            let id = token
                .0
                .checked_add(index as u64)
                .ok_or(UploadError::Failed)?;
            let (file, selected, info) = file.into_parts();
            Ok(Input {
                file,
                selected,
                metadata: PendingAttachment {
                    token: UploadToken(id),
                    filename: info.display_name,
                    size_bytes: info.size_bytes,
                },
                lease,
            })
        })
        .collect()
}

pub(super) fn cancelled(cancel: &CancelSignal) -> bool {
    *cancel.borrow() || cancel.has_changed().is_err()
}

pub(super) async fn wait_cancel(cancel: &mut CancelSignal) {
    while !cancelled(cancel) {
        if cancel.changed().await.is_err() {
            break;
        }
    }
}

fn read_batch(
    files: Vec<Input>,
    cancel: CancelSignal,
    mut after_file: impl FnMut(usize),
) -> Prepared {
    let mut prepared = Vec::with_capacity(files.len());
    for (index, input) in files.into_iter().enumerate() {
        prepared.push(read_checked(
            input.file,
            input.selected,
            input.metadata,
            input.lease,
            cancel.clone(),
            || {},
        )?);
        after_file(index);
        if cancelled(&cancel) {
            return Err(UploadError::Cancelled);
        }
    }
    Ok(prepared)
}

#[cfg(test)]
pub(super) fn read_file(
    file: File,
    selected: Metadata,
    metadata: PendingAttachment,
    lease: Lease,
    cancel: CancelSignal,
) -> Result<PreparedUpload, UploadError> {
    read_checked(file, selected, metadata, lease, cancel, || {})
}

fn read_checked(
    mut file: File,
    selected: Metadata,
    metadata: PendingAttachment,
    lease: Lease,
    cancel: CancelSignal,
    after_read: impl FnOnce(),
) -> Result<PreparedUpload, UploadError> {
    if cancelled(&cancel) {
        return Err(UploadError::Cancelled);
    }
    let before = file.metadata().map_err(|_| UploadError::Failed)?;
    if !before.is_file() {
        return Err(UploadError::InvalidFile);
    }
    if !same_file(&selected, &before, true) || metadata.size_bytes != before.len() {
        return Err(UploadError::Changed);
    }
    if usize::try_from(before.len()).ok() != Some(lease.bytes) {
        return Err(UploadError::TooLarge);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| UploadError::Failed)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(lease.bytes)
        .map_err(|_| UploadError::MemoryLimit)?;
    bytes.resize(lease.bytes, 0);
    let mut read = 0;
    while read < bytes.len() {
        if cancelled(&cancel) {
            return Err(UploadError::Cancelled);
        }
        let end = (read + 64 * 1024).min(bytes.len());
        match file.read(&mut bytes[read..end]) {
            Ok(0) => return Err(UploadError::Changed),
            Ok(count) => read += count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return Err(UploadError::Failed),
        }
    }
    let mut extra = [0];
    if file.read(&mut extra).map_err(|_| UploadError::Failed)? != 0 {
        return Err(UploadError::Changed);
    }
    after_read();
    let after = file.metadata().map_err(|_| UploadError::Failed)?;
    if !same_file(&before, &after, true) {
        return Err(UploadError::Changed);
    }
    if cancelled(&cancel) {
        return Err(UploadError::Cancelled);
    }
    PreparedUpload::new(metadata, seal(bytes.into_boxed_slice(), lease))
}

fn same_file(before: &Metadata, after: &Metadata, changes: bool) -> bool {
    if !after.is_file()
        || before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
    {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.dev() != after.dev()
            || before.ino() != after.ino()
            || before.mode() != after.mode()
        {
            return false;
        }
        if changes && (before.ctime() != after.ctime() || before.ctime_nsec() != after.ctime_nsec())
        {
            return false;
        }
    }
    #[cfg(not(unix))]
    let _ = changes;
    true
}

#[cfg(test)]
mod tests;
