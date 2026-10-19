//! One fixed worker prepares descriptors without reading file contents.

use std::{
    fs::File,
    path::{Path, PathBuf},
    sync::mpsc::{sync_channel, SyncSender},
};

use tokio::sync::oneshot;

use super::{
    cancel_requested, valid_path, wait_cancel, CancelSignal, FileInfo, PickError, SelectedFile,
    MAX_DISPLAY_BYTES, MAX_FILES, MAX_PATH_BYTES,
};

type Opened = Result<Vec<SelectedFile>, PickError>;
/// Room for a whole batch of the longest paths.
const MAX_TOTAL_PATH_BYTES: usize = MAX_FILES * MAX_PATH_BYTES;

/// Validate a complete local-path batch without accessing the filesystem.
/// The same bounded admission rules apply to native drops and portal grants.
pub fn validate_local_paths(paths: &[PathBuf], max_files: usize) -> Result<(), PickError> {
    validate_count(paths.len(), max_files)?;
    let total = paths.iter().try_fold(0usize, |total, path| {
        if !valid_path(path) {
            return None;
        }
        total.checked_add(path.as_os_str().as_encoded_bytes().len())
    });
    if total.is_none_or(|bytes| bytes > MAX_TOTAL_PATH_BYTES) {
        return Err(PickError::InvalidRequest);
    }
    if paths
        .iter()
        .enumerate()
        .any(|(index, path)| paths[..index].contains(path))
    {
        return Err(PickError::InvalidResponse);
    }
    Ok(())
}

/// Open an entire local batch as read-only descriptors without reading contents.
///
/// This function is synchronous and must run on a dedicated service worker,
/// never on the UI or an async executor thread. It creates no thread or portal
/// connection. Cancellation is checked around each open; it cannot interrupt a
/// filesystem syscall already in progress. An error drops all partial grants on
/// the calling service worker. A zero total budget permits only empty files.
pub fn open_local_files(
    paths: &[PathBuf],
    max_files: usize,
    max_file_bytes: u64,
    max_total_bytes: u64,
    cancel: &CancelSignal,
) -> Result<Vec<SelectedFile>, PickError> {
    if cancel_requested(cancel) {
        return Err(PickError::Cancelled);
    }
    validate_local_paths(paths, max_files)?;
    if max_file_bytes == 0 {
        return Err(PickError::InvalidRequest);
    }
    open_batch(paths, max_file_bytes, max_total_bytes, cancel)
}

enum Job {
    Open {
        paths: Vec<PathBuf>,
        max_file_bytes: u64,
        max_total_bytes: u64,
        cancel: CancelSignal,
        reply: oneshot::Sender<Opened>,
    },
    #[cfg(test)]
    Pause {
        entered: SyncSender<()>,
        release: std::sync::mpsc::Receiver<()>,
    },
}

pub(crate) struct FileWorker {
    jobs: SyncSender<Job>,
    // Cancellation drops the waiter, not the syscall. Retain its slot until it
    // completes, so another request cannot accumulate a thread or descriptor.
    pending: Option<oneshot::Receiver<Opened>>,
}

impl FileWorker {
    pub(crate) fn new() -> Result<Self, PickError> {
        let (jobs, receiver) = sync_channel(1);
        // Dropping the handle detaches. Closing the sender stops this one
        // worker after its current syscall; shutdown never joins it on UI.
        drop(
            std::thread::Builder::new()
                .name("rune-file-picker".into())
                .spawn(move || {
                    while let Ok(job) = receiver.recv() {
                        match job {
                            Job::Open {
                                paths,
                                max_file_bytes,
                                max_total_bytes,
                                cancel,
                                reply,
                            } => {
                                let result =
                                    open_batch(&paths, max_file_bytes, max_total_bytes, &cancel);
                                let _ = reply.send(result);
                            }
                            #[cfg(test)]
                            Job::Pause { entered, release } => {
                                let _ = entered.send(());
                                let _ = release.recv();
                            }
                        }
                    }
                })
                .map_err(|_| PickError::Failed)?,
        );
        Ok(Self {
            jobs,
            pending: None,
        })
    }

    pub(crate) async fn wait_idle(&mut self, cancel: CancelSignal) -> Result<(), PickError> {
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        if self.pending.is_some() {
            // A previous cancelled request owns this result. Its error cannot
            // fail the next choice, and its descriptor is dropped here.
            let _ = self.finish(cancel.clone()).await;
            if cancel_requested(&cancel) {
                return Err(PickError::Cancelled);
            }
        }
        Ok(())
    }

    pub(crate) async fn open(
        &mut self,
        paths: Vec<PathBuf>,
        max_files: usize,
        max_file_bytes: u64,
        max_total_bytes: u64,
        cancel: CancelSignal,
    ) -> Opened {
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        validate_local_paths(&paths, max_files)?;
        if max_file_bytes == 0 {
            return Err(PickError::InvalidRequest);
        }
        self.wait_idle(cancel.clone()).await?;
        let (reply, result) = oneshot::channel();
        self.jobs
            .try_send(Job::Open {
                paths,
                max_file_bytes,
                max_total_bytes,
                cancel: cancel.clone(),
                reply,
            })
            .map_err(|_| PickError::Failed)?;
        self.pending = Some(result);
        self.finish(cancel).await
    }

    // The portal's, and its tests'.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) async fn open_uris(
        &mut self,
        uris: &[String],
        max_files: usize,
        max_file_bytes: u64,
        max_total_bytes: u64,
        cancel: CancelSignal,
    ) -> Opened {
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        validate_count(uris.len(), max_files)?;
        // Decode every URI before any path reaches the worker. Decoded paths
        // also catch duplicate grants written with different percent escapes.
        let paths = uris
            .iter()
            .map(|uri| local_path(uri))
            .collect::<Result<Vec<_>, _>>()?;
        self.open(paths, max_files, max_file_bytes, max_total_bytes, cancel)
            .await
    }

    async fn finish(&mut self, mut cancel: CancelSignal) -> Opened {
        let receiver = self.pending.as_mut().ok_or(PickError::Failed)?;
        let result = tokio::select! {
            biased;
            _ = wait_cancel(&mut cancel) => return Err(PickError::Cancelled),
            result = receiver => result,
        };
        self.pending = None;
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        result.map_err(|_| PickError::Failed)?
    }
}

pub(crate) fn validate_count(count: usize, max_files: usize) -> Result<(), PickError> {
    if max_files == 0 || max_files > MAX_FILES {
        return Err(PickError::InvalidRequest);
    }
    if count == 0 {
        return Err(PickError::InvalidResponse);
    }
    if count > max_files {
        return Err(PickError::TooMany);
    }
    Ok(())
}

fn open_batch(
    paths: &[PathBuf],
    max_file_bytes: u64,
    max_total_bytes: u64,
    cancel: &CancelSignal,
) -> Opened {
    let mut selected = Vec::with_capacity(paths.len());
    let mut total = 0u64;
    for path in paths {
        let file = open_regular(path, max_file_bytes, cancel)?;
        total = total
            .checked_add(file.info().size_bytes)
            .ok_or(PickError::TooLarge)?;
        if total > max_total_bytes {
            return Err(PickError::TooLarge);
        }
        selected.push(file);
    }
    if cancel_requested(cancel) {
        return Err(PickError::Cancelled);
    }
    Ok(selected)
}

// The portal's, and its tests'.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) fn local_path(uri: &str) -> Result<PathBuf, PickError> {
    if uri.len() > MAX_PATH_BYTES {
        return Err(PickError::TooLarge);
    }
    if !uri
        .as_bytes()
        .get(..7)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(b"file://"))
        || !uri[7..].contains('/')
    {
        return Err(PickError::InvalidResponse);
    }
    // URL parsing otherwise trims controls, converts backslashes and accepts
    // incomplete escapes, changing a selected filename before it is opened.
    let mut bytes = uri.bytes();
    while let Some(byte) = bytes.next() {
        if byte.is_ascii_control() || byte == b'\\' || byte == b' ' {
            return Err(PickError::InvalidResponse);
        }
        if byte == b'%'
            && (!bytes.next().is_some_and(|byte| byte.is_ascii_hexdigit())
                || !bytes.next().is_some_and(|byte| byte.is_ascii_hexdigit()))
        {
            return Err(PickError::InvalidResponse);
        }
    }
    // Percent decoding must not introduce separators after segment and
    // duplicate-path validation. Literal percent names still use %25.
    if uri
        .as_bytes()
        .windows(3)
        .any(|escape| escape.eq_ignore_ascii_case(b"%2f"))
    {
        return Err(PickError::InvalidResponse);
    }
    // Dot-segment normalization can choose a different file when a preceding
    // component is a symlink. Reject it before URL parsing changes the path.
    if uri[7..].split('/').any(|segment| {
        [".", "..", "%2e", ".%2e", "%2e.", "%2e%2e"]
            .iter()
            .any(|dot| segment.eq_ignore_ascii_case(dot))
    }) {
        return Err(PickError::InvalidResponse);
    }
    let uri = url::Url::parse(uri).map_err(|_| PickError::InvalidResponse)?;
    if uri.scheme() != "file"
        || !uri.username().is_empty()
        || uri.password().is_some()
        || uri.port().is_some()
        || uri.query().is_some()
        || uri.fragment().is_some()
        || uri
            .host_str()
            .is_some_and(|host| host != "localhost" && !host.is_empty())
    {
        return Err(PickError::InvalidResponse);
    }
    let path = uri.to_file_path().map_err(|_| PickError::InvalidResponse)?;
    if !valid_path(&path) {
        return Err(PickError::InvalidResponse);
    }
    Ok(path)
}

#[cfg(target_os = "linux")]
fn open_regular(
    path: &Path,
    max_bytes: u64,
    cancel: &CancelSignal,
) -> Result<SelectedFile, PickError> {
    if cancel_requested(cancel) {
        return Err(PickError::Cancelled);
    }
    let file = File::from(
        rustix::fs::openat(
            rustix::fs::CWD,
            path,
            rustix::fs::OFlags::RDONLY | rustix::fs::OFlags::NONBLOCK | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )
        .map_err(|error| file_error(std::io::Error::from(error)))?,
    );
    selected(file, path, max_bytes, cancel)
}

/// `file`, opened from `path`, as a selection: a regular file within
/// `max_bytes`, with a name to show.
#[cfg(any(target_os = "linux", windows))]
fn selected(
    file: File,
    path: &Path,
    max_bytes: u64,
    cancel: &CancelSignal,
) -> Result<SelectedFile, PickError> {
    let metadata = file.metadata().map_err(file_error)?;
    if !metadata.is_file() {
        return Err(PickError::NotRegularFile);
    }
    if metadata.len() > max_bytes {
        return Err(PickError::TooLarge);
    }
    if cancel_requested(cancel) {
        return Err(PickError::Cancelled);
    }
    let mut display_name = String::new();
    if let Some(name) = path.file_name() {
        for character in name
            .to_string_lossy()
            .chars()
            .filter(|character| !character.is_control())
        {
            if display_name.len() + character.len_utf8() > MAX_DISPLAY_BYTES {
                break;
            }
            display_name.push(character);
        }
    }
    if display_name.trim().is_empty() {
        display_name = "archivo".into();
    }
    Ok(SelectedFile {
        info: FileInfo {
            display_name,
            size_bytes: metadata.len(),
        },
        file,
        metadata,
        path: path.to_owned(),
    })
}

/// Windows (windows-base): read-only, and only a path to a drive or a
/// share; a device (`\\.\…`) is never opened.
#[cfg(windows)]
fn open_regular(
    path: &Path,
    max_bytes: u64,
    cancel: &CancelSignal,
) -> Result<SelectedFile, PickError> {
    use std::path::{Component, Prefix};
    if cancel_requested(cancel) {
        return Err(PickError::Cancelled);
    }
    let on_disk = matches!(
        path.components().next(),
        Some(Component::Prefix(prefix)) if matches!(
            prefix.kind(),
            Prefix::Disk(_) | Prefix::VerbatimDisk(_) | Prefix::UNC(..) | Prefix::VerbatimUNC(..)
        )
    );
    if !on_disk {
        return Err(PickError::NotRegularFile);
    }
    // A folder does not open without backup semantics: say what it is.
    let file = File::open(path).map_err(|error| match path.is_dir() {
        true => PickError::NotRegularFile,
        false => file_error(error),
    })?;
    selected(file, path, max_bytes, cancel)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn open_regular(
    _path: &Path,
    _max_bytes: u64,
    _cancel: &CancelSignal,
) -> Result<SelectedFile, PickError> {
    Err(PickError::Unavailable)
}

fn file_error(error: std::io::Error) -> PickError {
    if error.kind() == std::io::ErrorKind::PermissionDenied {
        PickError::PermissionDenied
    } else {
        PickError::Failed
    }
}

#[cfg(test)]
mod tests;
