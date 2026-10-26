//! Theme files (temas-propios): one the person chooses through the system's
//! dialog, read with a bound on a service thread, or one written where the
//! person says, never over another file. The interface checks what is read;
//! nothing here parses it, and nothing is ever run from it.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rune_i18n::t;
use rune_provider::UploadError;
use tokio::{
    io::AsyncWriteExt,
    runtime::Handle,
    sync::{oneshot, watch},
    task::JoinHandle,
};

/// The largest theme file read, as `rune_components::theme::MAX_FILE`.
const MAX_FILE: u64 = 64 * 1024;

/// What came of importing or exporting the theme file of `request`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThemeFile {
    pub request: u64,
    pub exported: bool,
    /// What an import read, or the name an export was saved as; `Ok(None)`
    /// when the person cancelled.
    pub result: Result<Option<String>, &'static str>,
}

#[derive(Default)]
pub(crate) struct ThemeFiles {
    /// The one under way; dropping its sender closes its dialog.
    working: Option<(JoinHandle<()>, watch::Sender<bool>)>,
    done: Arc<Mutex<Vec<ThemeFile>>>,
}

impl ThemeFiles {
    /// Let the person choose a theme file over `parent` and read it, in
    /// place of any earlier import or export.
    pub fn import(
        &mut self,
        runtime: &Handle,
        request: u64,
        parent: Option<crate::ParentWindow>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) {
        let (cancel, cancelled) = watch::channel(false);
        self.run(runtime, cancel, wake, async move {
            ThemeFile {
                request,
                exported: false,
                result: chosen_text(cancelled, parent).await,
            }
        });
    }

    /// Write `contents` where `chosen` says, once the dialog answers.
    pub fn export(
        &mut self,
        runtime: &Handle,
        request: u64,
        chosen: oneshot::Receiver<Result<PathBuf, UploadError>>,
        cancel: watch::Sender<bool>,
        contents: String,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) {
        self.run(runtime, cancel, wake, async move {
            ThemeFile {
                request,
                exported: true,
                result: write(chosen.await, contents).await,
            }
        });
    }

    fn run(
        &mut self,
        runtime: &Handle,
        cancel: watch::Sender<bool>,
        wake: Arc<dyn Fn() + Send + Sync>,
        work: impl std::future::Future<Output = ThemeFile> + Send + 'static,
    ) {
        self.cancel();
        let done = self.done.clone();
        let task = runtime.spawn(async move {
            let finished = work.await;
            if let Ok(mut done) = done.lock() {
                done.push(finished);
            }
            wake();
        });
        self.working = Some((task, cancel));
    }

    /// Close the dialog under way: what it gives is dropped.
    pub fn cancel(&mut self) {
        if let Some((task, cancel)) = self.working.take() {
            cancel.send_replace(true);
            task.abort();
        }
    }

    /// What finished since the last call.
    pub fn poll(&mut self) -> Vec<ThemeFile> {
        self.done
            .lock()
            .map(|mut done| std::mem::take(&mut *done))
            .unwrap_or_default()
    }
}

impl Drop for ThemeFiles {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// The text of one file the person chooses over `parent`, at most
/// [`MAX_FILE`] bytes; `None` when the person cancelled.
#[cfg(any(target_os = "linux", windows))]
async fn chosen_text(
    cancel: watch::Receiver<bool>,
    parent: Option<crate::ParentWindow>,
) -> Result<Option<String>, &'static str> {
    use rune_platform::file_dialogs::{FileChooser, PickError, PickId, PickRequest};
    use std::io::Read;
    let mut chooser = FileChooser::session().await.map_err(PickError::label)?;
    let request = PickRequest {
        id: PickId(1),
        max_files: 1,
        max_file_bytes: MAX_FILE,
        max_total_bytes: MAX_FILE,
        parent,
        current_folder: None,
    };
    let mut files = match chooser.choose_files(&request, cancel).await {
        Ok(files) => files,
        Err(PickError::Cancelled) => return Ok(None),
        Err(PickError::TooLarge) => return Err(t(TOO_LARGE)),
        Err(error) => return Err(error.label()),
    };
    let file = files.pop().ok_or(t("No se eligió ningún archivo."))?;
    // Reading blocks: off the runtime's workers.
    tokio::task::spawn_blocking(move || {
        let (file, _, _) = file.into_parts();
        let mut bytes = Vec::new();
        file.take(MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| t("No se pudo leer el archivo."))?;
        text(bytes).map(Some)
    })
    .await
    .map_err(|_| t("No se pudo leer el archivo."))?
}

#[cfg(not(any(target_os = "linux", windows)))]
async fn chosen_text(
    _cancel: watch::Receiver<bool>,
    _parent: Option<crate::ParentWindow>,
) -> Result<Option<String>, &'static str> {
    Err(t(
        "El selector de archivos no está disponible en este equipo.",
    ))
}

const TOO_LARGE: &str = rune_i18n::es("El archivo pesa más de 64 KB: no es un tema de RUNE.");

/// `bytes` as a theme file's text: bounded UTF-8.
fn text(bytes: Vec<u8>) -> Result<String, &'static str> {
    if bytes.len() as u64 > MAX_FILE {
        return Err(t(TOO_LARGE));
    }
    String::from_utf8(bytes).map_err(|_| t("El archivo no es un tema de RUNE o está dañado."))
}

/// Write `contents` at the path the dialog chose, as a new file: one there
/// already, a folder or a link, stays as it was.
async fn write(
    chosen: Result<Result<PathBuf, UploadError>, oneshot::error::RecvError>,
    contents: String,
) -> Result<Option<String>, &'static str> {
    let path = match chosen {
        Ok(Ok(path)) => path,
        Ok(Err(UploadError::Cancelled)) | Err(_) => return Ok(None),
        Ok(Err(UploadError::Busy)) => {
            return Err(t("Espera a que se cierre el otro diálogo de archivos."))
        }
        Ok(Err(_)) => return Err(t("No se pudo abrir el selector de archivos.")),
    };
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(t("Elige un nombre de archivo válido."))?
        .to_owned();
    let mut file = match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .await
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return Err(t("Ya hay un archivo con ese nombre. Elige otro."))
        }
        Err(_) => return Err(t("No se pudo guardar el archivo en esa carpeta.")),
    };
    let written = async {
        file.write_all(contents.as_bytes()).await?;
        file.sync_all().await
    }
    .await;
    if written.is_err() {
        // Half a theme is no theme.
        let _ = tokio::fs::remove_file(&path).await;
        return Err(t("No se pudo guardar el archivo en esa carpeta."));
    }
    Ok(Some(filename))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_reads_as_bounded_text() {
        assert_eq!(text(b"{}".to_vec()), Ok("{}".into()));
        assert!(text(vec![0xff, 0xfe]).is_err());
        assert!(text(vec![b' '; MAX_FILE as usize + 1]).is_err());
    }

    #[tokio::test]
    async fn an_export_writes_a_new_file_and_never_over_another() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("Bosque.rune-theme.json");
        let mut files = ThemeFiles::default();
        let (reply, chosen) = oneshot::channel();
        let (cancel, _signal) = watch::channel(false);
        files.export(
            &Handle::current(),
            3,
            chosen,
            cancel,
            "{\"rune_theme\":1}".into(),
            Arc::new(|| {}),
        );
        reply.send(Ok(path.clone())).unwrap();
        let polled = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let polled = files.poll();
                if !polled.is_empty() {
                    return polled;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            polled,
            [ThemeFile {
                request: 3,
                exported: true,
                result: Ok(Some("Bosque.rune-theme.json".into()))
            }]
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"rune_theme\":1}"
        );
        // Again at the same name: refused, the first file as it was.
        let again = write(Ok(Ok(path.clone())), "otro".into()).await;
        assert!(again.is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"rune_theme\":1}"
        );
        // A cancelled dialog writes nothing.
        assert_eq!(
            write(Ok(Err(UploadError::Cancelled)), "x".into()).await,
            Ok(None)
        );
    }
}
