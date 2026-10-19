//! A folder the person chooses through the system's dialog (escritorio-linux:
//! the call's sounds). It takes the attachments' dialog slot, one dialog at
//! a time; the interface gets the folder's path as text, nothing opened.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rune_i18n::t;
use rune_provider::UploadError;
use tokio::{
    runtime::Handle,
    sync::{oneshot, watch},
    task::JoinHandle,
};

/// What came of choosing a folder asked for with `request`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChosenFolder {
    pub request: u64,
    /// The folder; `Ok(None)` when the person cancelled.
    pub result: Result<Option<String>, &'static str>,
}

#[derive(Default)]
pub(crate) struct Folders {
    /// The choice under way; dropping its sender closes the dialog.
    choosing: Option<(JoinHandle<()>, watch::Sender<bool>)>,
    done: Arc<Mutex<Vec<ChosenFolder>>>,
}

impl Folders {
    /// Wait for the folder `chosen` answers, in place of any earlier choice.
    pub fn wait(
        &mut self,
        runtime: &Handle,
        request: u64,
        chosen: oneshot::Receiver<Result<PathBuf, UploadError>>,
        cancel: watch::Sender<bool>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) {
        self.cancel();
        let done = self.done.clone();
        let task = runtime.spawn(async move {
            let result = answer(chosen.await);
            if let Ok(mut done) = done.lock() {
                done.push(ChosenFolder { request, result });
            }
            wake();
        });
        self.choosing = Some((task, cancel));
    }

    /// Close the dialog under way: what it finds is dropped.
    pub fn cancel(&mut self) {
        if let Some((task, cancel)) = self.choosing.take() {
            cancel.send_replace(true);
            task.abort();
        }
    }

    /// What was chosen since the last call.
    pub fn poll(&mut self) -> Vec<ChosenFolder> {
        self.done
            .lock()
            .map(|mut done| std::mem::take(&mut *done))
            .unwrap_or_default()
    }
}

impl Drop for Folders {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// The dialog's answer as the interface keeps a folder: printable text.
fn answer(
    chosen: Result<Result<PathBuf, UploadError>, oneshot::error::RecvError>,
) -> Result<Option<String>, &'static str> {
    match chosen {
        Ok(Ok(path)) => path
            .into_os_string()
            .into_string()
            .ok()
            .filter(|folder| !folder.chars().any(char::is_control))
            .map(Some)
            .ok_or(t(
                "RUNE no puede guardar el nombre de esa carpeta. Elegí otra.",
            )),
        Ok(Err(UploadError::Cancelled)) | Err(_) => Ok(None),
        Ok(Err(UploadError::Busy)) => Err(t("Esperá a que se cierre el otro diálogo de archivos.")),
        Ok(Err(UploadError::Unavailable)) => Err(t(
            "El selector de archivos no está disponible en este equipo.",
        )),
        Ok(Err(_)) => Err(t("No se pudo abrir el selector de archivos.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_comes_back_as_text_and_a_cancellation_as_nothing() {
        assert_eq!(
            answer(Ok(Ok(PathBuf::from("/home/vos/sonidos ü")))),
            Ok(Some("/home/vos/sonidos ü".into()))
        );
        assert_eq!(answer(Ok(Err(UploadError::Cancelled))), Ok(None));
        assert!(answer(Ok(Err(UploadError::Busy))).is_err());
        assert!(
            answer(Ok(Ok(PathBuf::from("/tmp/a\nb")))).is_err(),
            "a name the settings cannot keep"
        );
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            let odd = PathBuf::from(std::ffi::OsString::from_vec(b"/tmp/\xff".to_vec()));
            assert!(answer(Ok(Ok(odd))).is_err());
        }
    }

    #[tokio::test]
    async fn the_answer_is_polled_once_and_a_cancel_drops_it() {
        let mut folders = Folders::default();
        let (reply, chosen) = oneshot::channel();
        let (cancel, signal) = watch::channel(false);
        folders.wait(&Handle::current(), 7, chosen, cancel, Arc::new(|| {}));
        reply.send(Ok(PathBuf::from("/tmp/sonidos"))).unwrap();
        let polled = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let polled = folders.poll();
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
            [ChosenFolder {
                request: 7,
                result: Ok(Some("/tmp/sonidos".into()))
            }]
        );
        assert!(folders.poll().is_empty());
        // Another choice closes the first's dialog.
        let (reply, chosen) = oneshot::channel();
        let (cancel, again) = watch::channel(false);
        folders.wait(&Handle::current(), 8, chosen, cancel, Arc::new(|| {}));
        folders.cancel();
        assert!(*again.borrow());
        // An answer after the cancel, if it still gets through, is dropped.
        let _ = reply.send(Ok(PathBuf::from("/tmp/tarde")));
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(folders.poll().is_empty());
        drop(signal);
    }
}
