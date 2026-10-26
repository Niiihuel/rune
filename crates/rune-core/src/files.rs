//! Explicit, bounded downloads without account headers or automatic execution.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use rune_i18n::{es, t};
use rune_model::RemoteFile;
use rune_provider::{RequestId, UploadError};
use tokio::{
    io::AsyncWriteExt,
    runtime::Handle,
    sync::{mpsc, oneshot, watch},
    task::JoinHandle,
};

pub const MAX_DOWNLOAD_BYTES: u64 = 128 * 1024 * 1024;
const CONCURRENT: usize = 2;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub struct DownloadResult {
    pub filename: String,
    pub result: Result<PathBuf, String>,
    /// Put on the clipboard instead of kept in Downloads (contenido-de-mensajes).
    pub copied: Option<Copied>,
}

// Message content (contenido-de-mensajes).
mod copy;
pub use copy::Copied;

struct Completed {
    epoch: u64,
    id: u64,
    result: DownloadResult,
}

/// What a «Save as» tells the interface: its request, a phase and the
/// chosen name at the end, never the path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveUpdate {
    /// The destination is chosen and the file downloads into it.
    Saving { request: RequestId },
    Progress {
        request: RequestId,
        received: u64,
        total: u64,
    },
    Saved {
        request: RequestId,
        filename: String,
    },
    /// The dialog was closed or the save stopped; nothing was written.
    Cancelled { request: RequestId },
    Failed {
        request: RequestId,
        reason: &'static str,
    },
    // Linux desktop (escritorio-linux).
    /// A file already has the chosen name: nothing downloads until the
    /// person replaces it ([`Downloads::replace_save`]) or cancels.
    Exists {
        request: RequestId,
        filename: String,
    },
}

pub const SAVE_EXISTS: &str = es("Ese archivo ya existe. Elige un nombre diferente.");

/// A «Save as» while it runs. Dropping it closes its dialog.
struct Save {
    task: JoinHandle<()>,
    cancel: watch::Sender<bool>,
    saving: Arc<AtomicBool>,
    received: Arc<AtomicU64>,
    total: u64,
    told: (bool, u64),
    // Linux desktop (escritorio-linux).
    /// The chosen name, once a file is found there; told once.
    exists: Arc<std::sync::OnceLock<String>>,
    asked: bool,
    /// The person's word to replace it.
    replace: Option<oneshot::Sender<()>>,
}

struct Saved {
    epoch: u64,
    request: RequestId,
    result: Result<String, Option<&'static str>>,
}

pub(crate) struct Downloads {
    runtime: Handle,
    client: Option<reqwest::Client>,
    origins: &'static [&'static str],
    wake: Arc<dyn Fn() + Send + Sync>,
    sender: mpsc::Sender<Completed>,
    receiver: mpsc::Receiver<Completed>,
    active: HashMap<u64, JoinHandle<()>>,
    epoch: u64,
    next: u64,
    saves: HashMap<RequestId, Save>,
    saved: (mpsc::Sender<Saved>, mpsc::Receiver<Saved>),
}

impl Downloads {
    pub fn new(
        runtime: Handle,
        origins: &'static [&'static str],
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(CONCURRENT);
        Self {
            runtime,
            origins,
            wake,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(10))
                .timeout(Duration::from_secs(120))
                .user_agent(concat!(
                    "RUNE/",
                    env!("CARGO_PKG_VERSION"),
                    " file downloads"
                ))
                .build()
                .ok(),
            sender,
            receiver,
            active: HashMap::new(),
            epoch: 0,
            next: 0,
            saves: HashMap::new(),
            saved: mpsc::channel(CONCURRENT + 1),
        }
    }

    /// Whether `source` may download now: a valid link, its size, and one of
    /// the two slots, which plain downloads and «Save as» share.
    pub fn admit(&self, source: &RemoteFile, size: u64) -> Result<url::Url, &'static str> {
        let url = crate::validated_resource_url(&source.url, self.origins, false)
            .ok_or(t("El enlace de este archivo no está disponible."))?;
        if size > MAX_DOWNLOAD_BYTES {
            return Err(t("El archivo supera el límite de descarga de 128 MiB."));
        }
        if self.active.len() + self.saves.len() >= CONCURRENT {
            return Err(t("Espera a que termine una de las dos descargas activas."));
        }
        Ok(url)
    }

    /// Download `url` into the destination `chosen` answers, under exactly
    /// that name. Over a file already there only once the person says so
    /// ([`SaveUpdate::Exists`], [`Self::replace_save`]), and never over a
    /// folder or a link. `cancel` closes the dialog; [`Self::cancel_save`]
    /// also stops the download.
    pub fn save(
        &mut self,
        request: RequestId,
        url: url::Url,
        size: u64,
        chosen: oneshot::Receiver<Result<PathBuf, UploadError>>,
        cancel: watch::Sender<bool>,
    ) -> Result<(), &'static str> {
        let client = self
            .client
            .clone()
            .ok_or(t("No se pudo iniciar la descarga."))?;
        let saving = Arc::new(AtomicBool::new(false));
        let received = Arc::new(AtomicU64::new(0));
        let (epoch, sender, wake) = (self.epoch, self.saved.0.clone(), self.wake.clone());
        let (flag, count) = (saving.clone(), received.clone());
        let exists = Arc::new(std::sync::OnceLock::new());
        let (replace, replaced) = oneshot::channel();
        let found = exists.clone();
        let task = self.runtime.spawn(async move {
            let asking = Ask {
                exists: &found,
                replaced,
            };
            let result = save_to(&client, url, size, chosen, &flag, &count, &wake, asking).await;
            let _ = sender
                .send(Saved {
                    epoch,
                    request,
                    result,
                })
                .await;
            wake();
        });
        self.saves.insert(
            request,
            Save {
                task,
                cancel,
                saving,
                received,
                total: size,
                told: (false, 0),
                exists,
                asked: false,
                replace: Some(replace),
            },
        );
        Ok(())
    }

    // Linux desktop (escritorio-linux).
    /// The person replaces the file at `request`'s chosen name: the
    /// download starts. False when that save does not wait for it.
    pub fn replace_save(&mut self, request: RequestId) -> bool {
        let Some(save) = self.saves.get_mut(&request) else {
            return false;
        };
        save.exists.get().is_some()
            && save
                .replace
                .take()
                .is_some_and(|replace| replace.send(()).is_ok())
    }

    /// Stop `request`: its dialog closes, or its partial file is removed.
    /// The interface learns it as [`SaveUpdate::Cancelled`].
    pub fn cancel_save(&mut self, request: RequestId) -> bool {
        let Some(save) = self.saves.remove(&request) else {
            return false;
        };
        save.cancel.send_replace(true);
        save.task.abort();
        true
    }

    /// What changed in each «Save as» since the last call.
    pub fn poll_saves(&mut self) -> Vec<SaveUpdate> {
        let mut updates = Vec::new();
        for (request, save) in &mut self.saves {
            if let Some(filename) = save.exists.get().filter(|_| !save.asked) {
                save.asked = true;
                updates.push(SaveUpdate::Exists {
                    request: *request,
                    filename: filename.clone(),
                });
            }
            let saving = save.saving.load(Ordering::Acquire);
            if saving && !save.told.0 {
                save.told.0 = true;
                updates.push(SaveUpdate::Saving { request: *request });
            }
            let received = save.received.load(Ordering::Relaxed);
            if saving && received != save.told.1 {
                save.told.1 = received;
                updates.push(SaveUpdate::Progress {
                    request: *request,
                    received,
                    total: save.total,
                });
            }
        }
        while let Ok(done) = self.saved.1.try_recv() {
            // Another account's, or one cancelled here: already settled.
            if done.epoch != self.epoch || self.saves.remove(&done.request).is_none() {
                continue;
            }
            let request = done.request;
            updates.push(match done.result {
                Ok(filename) => SaveUpdate::Saved { request, filename },
                Err(None) => SaveUpdate::Cancelled { request },
                Err(Some(reason)) => SaveUpdate::Failed { request, reason },
            });
        }
        updates
    }

    pub fn start(
        &mut self,
        source: RemoteFile,
        filename: &str,
        size: u64,
    ) -> Result<(), &'static str> {
        self.begin(source, filename, size, None)
    }

    /// Fetch `source` and put it on the system clipboard as `copied` says;
    /// a file stays in its private folder until the next copy. It takes a
    /// download's slot and reports through [`Self::poll`].
    pub fn copy(
        &mut self,
        source: RemoteFile,
        filename: &str,
        size: u64,
        copied: Copied,
    ) -> Result<(), &'static str> {
        self.begin(source, filename, size, Some(copied))
    }

    fn begin(
        &mut self,
        source: RemoteFile,
        filename: &str,
        size: u64,
        copied: Option<Copied>,
    ) -> Result<(), &'static str> {
        let url = self.admit(&source, size)?;
        let client = self
            .client
            .clone()
            .ok_or(t("No se pudo iniciar la descarga."))?;
        let filename = safe_filename(filename);
        self.next = self.next.wrapping_add(1);
        let id = self.next;
        let epoch = self.epoch;
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        let task = self.runtime.spawn(async move {
            let result = match copied {
                Some(copied) => copy::copy_to(&client, url, &filename, size, copied).await,
                None => match tokio::task::spawn_blocking(download_directory).await {
                    Ok(Ok(directory)) => {
                        download_to(&client, url, &directory, &filename, size).await
                    }
                    _ => Err(t("No se encontró la carpeta de Descargas.").into()),
                },
            };
            let _ = sender
                .send(Completed {
                    epoch,
                    id,
                    result: DownloadResult {
                        filename,
                        result,
                        copied,
                    },
                })
                .await;
            wake();
        });
        self.active.insert(id, task);
        Ok(())
    }

    pub fn poll(&mut self) -> Vec<DownloadResult> {
        let mut results = Vec::new();
        while let Ok(completed) = self.receiver.try_recv() {
            if completed.epoch == self.epoch {
                self.active.remove(&completed.id);
                results.push(completed.result);
            }
        }
        results
    }

    pub fn clear(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        for (_, task) in self.active.drain() {
            task.abort();
        }
        while self.receiver.try_recv().is_ok() {}
        for (_, save) in self.saves.drain() {
            save.cancel.send_replace(true);
            save.task.abort();
        }
        while self.saved.1.try_recv().is_ok() {}
    }
}

impl Drop for Downloads {
    fn drop(&mut self) {
        self.clear();
    }
}

/// The desktop's configured Downloads directory, resolved on a worker thread.
fn download_directory() -> Result<PathBuf, ()> {
    // Wherever the person moved it, as the shell says (windows-base).
    #[cfg(windows)]
    {
        rune_platform::folders::downloads().ok_or(())
    }
    #[cfg(not(windows))]
    {
        if let Ok(output) = std::process::Command::new("xdg-user-dir")
            .arg("DOWNLOAD")
            .output()
        {
            if output.status.success() && output.stdout.len() <= 4096 {
                if let Ok(path) = String::from_utf8(output.stdout) {
                    let path = PathBuf::from(path.trim_end_matches(['\r', '\n']));
                    if path.is_absolute() {
                        return Ok(path);
                    }
                }
            }
        }
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .map(|path| path.join("Downloads"))
            .ok_or(())
    }
}

pub(crate) fn safe_filename(filename: &str) -> String {
    let mut safe = String::new();
    for c in filename.chars().filter(|c| !c.is_control()) {
        let c = if "/\\:*?\"<>|".contains(c) { '_' } else { c };
        if safe.len() + c.len_utf8() > 180 {
            break;
        }
        safe.push(c);
    }
    let safe = safe.trim_matches([' ', '.']);
    if safe.is_empty() {
        t("archivo").into()
    } else {
        safe.into()
    }
}

/// A private incomplete file is removed on errors or task cancellation.
struct PartialFile(PathBuf);
impl Drop for PartialFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn download_to(
    client: &reqwest::Client,
    url: url::Url,
    directory: &Path,
    filename: &str,
    expected: u64,
) -> Result<PathBuf, String> {
    let partial = fetch(client, url, directory, expected, &|_| {}).await?;
    // A hard link publishes the completed file atomically, and refuses any
    // existing file or symlink. The temporary and final names share a directory.
    let name = Path::new(filename);
    let stem = name
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(t("archivo"));
    let extension = name
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{s}"))
        .unwrap_or_default();
    for number in 0..1000 {
        let target = directory.join(if number == 0 {
            filename.to_owned()
        } else {
            format!("{stem} ({number}){extension}")
        });
        match tokio::fs::hard_link(&partial.0, &target).await {
            Ok(()) => return Ok(target),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err(t("No se pudo guardar el archivo completo en Descargas.").into()),
        }
    }
    Err(t("Hay demasiados archivos con ese nombre en Descargas.").into())
}

/// How a «Save as» asks before replacing a file (escritorio-linux).
struct Ask<'a> {
    /// Set to the chosen name when a file is there.
    exists: &'a std::sync::OnceLock<String>,
    /// The person's word to replace it; dropped unanswered, nothing is.
    replaced: oneshot::Receiver<()>,
}

/// «Save as»: wait for the chosen destination, then publish there exactly.
/// A regular file already at that name is replaced, atomically, only once
/// the person says so (`ask`); a directory or link there never is.
/// `Err(None)` is a cancellation: nothing was written.
async fn save_to(
    client: &reqwest::Client,
    url: url::Url,
    size: u64,
    chosen: oneshot::Receiver<Result<PathBuf, UploadError>>,
    saving: &AtomicBool,
    received: &AtomicU64,
    wake: &Arc<dyn Fn() + Send + Sync>,
    ask: Ask<'_>,
) -> Result<String, Option<&'static str>> {
    let path = match chosen.await {
        Ok(Ok(path)) => path,
        Ok(Err(UploadError::Cancelled)) | Err(_) => return Err(None),
        Ok(Err(UploadError::Busy)) => {
            return Err(Some(t(
                "Espera a que se cierre el otro diálogo de archivos.",
            )))
        }
        Ok(Err(_)) => return Err(Some(t("No se pudo abrir el selector de archivos."))),
    };
    let (Some(directory), Some(filename)) = (
        path.parent(),
        path.file_name().and_then(|name| name.to_str()),
    ) else {
        return Err(Some(t("Elige un nombre de archivo válido.")));
    };
    // Asked before downloading (08.3). Without a file there, the hard link
    // below still refuses one created meanwhile, which stays as it was.
    let replacing = match tokio::fs::symlink_metadata(&path).await {
        Ok(found) if found.is_file() => {
            let _ = ask.exists.set(filename.to_owned());
            wake();
            ask.replaced.await.map_err(|_| None)?;
            Some(
                identity(&path)
                    .await
                    .map_err(|_| Some(t("No se pudo guardar el archivo en esa carpeta.")))?,
            )
        }
        Ok(_) => return Err(Some(t(SAVE_EXISTS))),
        Err(_) => None,
    };
    saving.store(true, Ordering::Release);
    wake();
    // Wake about a hundred times over the file, not once per chunk.
    let step = (size / 100).max(64 * 1024);
    let progress = |bytes: u64| {
        if bytes / step != received.swap(bytes, Ordering::Relaxed) / step {
            wake();
        }
    };
    // Keep fetch's reason: an expired link reads differently from a full disk.
    let partial = fetch(client, url, directory, size, &progress)
        .await
        .map_err(Some)?;
    if let Some(seen) = replacing {
        // Only the file the person was asked about: a folder or a link put
        // there since, or another file, stays. rename(2) swaps the name at
        // once: whoever opens it gets the old file or the new one, whole.
        // shortcut: a swap between this look and the rename still wins;
        // Linux has no rename that checks what it replaces.
        match tokio::fs::symlink_metadata(&path).await {
            Ok(found) if !found.is_file() => return Err(Some(t(SAVE_EXISTS))),
            // One that cannot be named is not the one asked about either.
            Ok(_) if identity(&path).await.ok() != Some(seen) => {
                return Err(Some(t(
                    "El archivo cambió mientras se descargaba. Vuelve a guardarlo.",
                )))
            }
            _ => {}
        }
        return match tokio::fs::rename(&partial.0, &path).await {
            Ok(()) => Ok(filename.to_owned()),
            Err(_) => Err(Some(t("No se pudo reemplazar el archivo en esa carpeta."))),
        };
    }
    match tokio::fs::hard_link(&partial.0, &path).await {
        Ok(()) => Ok(filename.to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            Err(Some(t(SAVE_EXISTS)))
        }
        Err(_) => Err(Some(t("No se pudo guardar el archivo en esa carpeta."))),
    }
}

/// Which file is at `path` (device and inode): replacing waits for the one
/// the person was asked about (08.3).
#[cfg(unix)]
async fn identity(path: &Path) -> std::io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let found = tokio::fs::symlink_metadata(path).await?;
    Ok((found.dev(), found.ino()))
}

/// On Windows (windows-base), its volume serial number and file index.
/// Times name no file: tunneling gives one renamed onto the name the old
/// one's creation time, and last writes are coarse.
#[cfg(windows)]
async fn identity(path: &Path) -> std::io::Result<(u64, u64)> {
    use std::os::windows::{fs::OpenOptionsExt, io::AsRawHandle};
    use windows::Win32::{
        Foundation::HANDLE,
        Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_OPEN_REPARSE_POINT,
        },
    };
    let path = path.to_owned();
    tokio::task::spawn_blocking(move || {
        // Attributes only, and never through a link put there since.
        let file = std::fs::OpenOptions::new()
            .access_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT.0)
            .open(path)?;
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: a handle this function holds open, and a struct it owns.
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }?;
        Ok((
            u64::from(info.dwVolumeSerialNumber),
            u64::from(info.nFileIndexHigh) << 32 | u64::from(info.nFileIndexLow),
        ))
    })
    .await
    .map_err(std::io::Error::other)?
}

#[cfg(not(any(unix, windows)))]
async fn identity(_: &Path) -> std::io::Result<(u64, u64)> {
    Ok((0, 0))
}

/// The whole body of `url` in a private partial file of `directory`,
/// checked against `expected`; removed on any error or cancellation.
async fn fetch(
    client: &reqwest::Client,
    url: url::Url,
    directory: &Path,
    expected: u64,
    progress: &(dyn Fn(u64) + Sync),
) -> Result<PartialFile, &'static str> {
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| t("No se pudo conectar para descargar el archivo."))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(if response.status() == reqwest::StatusCode::FORBIDDEN {
            t("El enlace venció o ya no está disponible. Vuelve a abrir la conversación.")
        } else {
            t("El servidor no pudo entregar el archivo.")
        });
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_DOWNLOAD_BYTES)
    {
        return Err(t("El archivo supera el límite de descarga de 128 MiB."));
    }
    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|_| t("No se pudo abrir la carpeta del archivo."))?;
    let path = directory.join(format!(
        ".rune-{}-{}.part",
        std::process::id(),
        TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    // Build the guard in the same blocking operation as creation. Cancellation
    // while open is running still drops its result and removes the partial file.
    let (file, partial) = tokio::task::spawn_blocking(move || {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        options.open(&path).map(|file| (file, PartialFile(path)))
    })
    .await
    .map_err(|_| t("No se pudo iniciar el archivo de descarga."))?
    .map_err(|_| t("No se pudo crear el archivo de descarga."))?;
    let mut file = tokio::fs::File::from_std(file);
    let mut received = 0u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| t("La descarga se interrumpió."))?
    {
        received = received
            .checked_add(chunk.len() as u64)
            .ok_or(t("Archivo demasiado grande."))?;
        if received > MAX_DOWNLOAD_BYTES {
            return Err(t("El archivo supera el límite de descarga de 128 MiB."));
        }
        if expected != 0 && received > expected {
            return Err(t(
                "El tamaño descargado no coincide con el archivo anunciado.",
            ));
        }
        file.write_all(&chunk)
            .await
            .map_err(|_| t("No se pudo escribir el archivo."))?;
        progress(received);
    }
    if expected != 0 && expected != received {
        return Err(t(
            "El tamaño descargado no coincide con el archivo anunciado.",
        ));
    }
    file.sync_all()
        .await
        .map_err(|_| t("No se pudo terminar de guardar el archivo."))?;
    drop(file);
    Ok(partial)
}

#[cfg(test)]
mod tests;
