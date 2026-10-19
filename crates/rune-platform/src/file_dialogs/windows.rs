//! The system's file dialogs on Windows (windows-base): the shell's «Open»,
//! «Save as» and folder dialogs (`IFileOpenDialog`, `IFileSaveDialog`), modal
//! to RUNE's window, each on a thread of its own with COM. What they answer
//! goes through the same checks and the same file worker as the portal's.

use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use rune_i18n::t;
use tokio::sync::oneshot;
use windows::core::{Interface, HSTRING};
use windows::Win32::Foundation::{ERROR_CANCELLED, HWND};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IBindCtx,
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::{
    FileOpenDialog, FileSaveDialog, IFileDialog, IFileOpenDialog, IFileSaveDialog, IShellItem,
    SHCreateItemFromParsingName, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM,
    FOS_NOCHANGEDIR, FOS_OVERWRITEPROMPT, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
};

use super::files::FileWorker;
use super::{
    cancel_requested, valid_path, wait_cancel, CancelSignal, FolderRequest, ParentWindow,
    PickError, PickRequest, SaveDestination, SaveRequest, SelectedFile,
};

type Answer = Result<Vec<PathBuf>, PickError>;

/// What one dialog asks.
struct Dialog {
    kind: Kind,
    title: &'static str,
    parent: Option<usize>,
    folder: Option<PathBuf>,
}

enum Kind {
    Open { multiple: bool },
    Save { name: String },
    Folder,
}

/// One dialog at a time and one file worker, as the portal's chooser.
pub struct FileChooser {
    files: FileWorker,
    /// The answer of a dialog whose request was cancelled while it showed:
    /// a modal dialog cannot be closed from here, so the next one waits for
    /// the person to close it.
    showing: Option<oneshot::Receiver<Answer>>,
}

impl FileChooser {
    pub async fn session() -> Result<Self, PickError> {
        Ok(Self {
            files: FileWorker::new()?,
            showing: None,
        })
    }

    pub async fn choose_files(
        &mut self,
        request: &PickRequest,
        cancel: CancelSignal,
    ) -> Result<Vec<SelectedFile>, PickError> {
        request.validate()?;
        let paths = self
            .show(
                Dialog {
                    kind: Kind::Open {
                        multiple: request.max_files > 1,
                    },
                    title: if request.max_files > 1 {
                        t("Seleccionar archivos")
                    } else {
                        t("Seleccionar archivo")
                    },
                    parent: parent(request.parent.as_ref()),
                    folder: request.current_folder.clone(),
                },
                cancel.clone(),
            )
            .await?;
        let selected = self
            .files
            .open(
                paths,
                request.max_files,
                request.max_file_bytes,
                request.max_total_bytes,
                cancel.clone(),
            )
            .await?;
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        Ok(selected)
    }

    /// Choose one inert destination without opening, creating, truncating or
    /// resolving its path. The dialog does not ask about a file already
    /// there: RUNE asks itself, as on Linux.
    pub async fn save_file(
        &mut self,
        request: &SaveRequest,
        cancel: CancelSignal,
    ) -> Result<SaveDestination, PickError> {
        request.validate()?;
        let path = self
            .show(
                Dialog {
                    kind: Kind::Save {
                        name: request.suggested_name.clone(),
                    },
                    title: t("Guardar archivo como"),
                    parent: parent(request.parent.as_ref()),
                    folder: request.current_folder.clone(),
                },
                cancel,
            )
            .await?
            .pop()
            .ok_or(PickError::InvalidResponse)?;
        if !valid_path(&path) || path.file_name().is_none() {
            return Err(PickError::InvalidResponse);
        }
        Ok(SaveDestination { path })
    }

    /// Choose one folder without opening or listing it.
    pub async fn choose_folder(
        &mut self,
        request: &FolderRequest,
        cancel: CancelSignal,
    ) -> Result<PathBuf, PickError> {
        request.validate()?;
        let folder = self
            .show(
                Dialog {
                    kind: Kind::Folder,
                    title: t("Elegir una carpeta"),
                    parent: parent(request.parent.as_ref()),
                    folder: request.current_folder.clone(),
                },
                cancel,
            )
            .await?
            .pop()
            .ok_or(PickError::InvalidResponse)?;
        if !valid_path(&folder) {
            return Err(PickError::InvalidResponse);
        }
        Ok(folder)
    }

    /// Wait for a dialog still showing and the file worker's last result.
    pub async fn wait_idle(&mut self, cancel: CancelSignal) -> Result<(), PickError> {
        if let Some(showing) = self.showing.as_mut() {
            let mut waiting = cancel.clone();
            let closed = tokio::select! {
                biased;
                _ = wait_cancel(&mut waiting) => false,
                _ = showing => true,
            };
            if !closed {
                return Err(PickError::Cancelled);
            }
            self.showing = None;
        }
        self.files.wait_idle(cancel).await
    }

    async fn show(&mut self, dialog: Dialog, cancel: CancelSignal) -> Answer {
        self.wait_idle(cancel.clone()).await?;
        let (reply, answer) = oneshot::channel();
        std::thread::Builder::new()
            .name("rune-file-dialog".into())
            .spawn(move || {
                let _ = reply.send(dialog.run());
            })
            .map_err(|_| PickError::Failed)?;
        let answer = self.showing.insert(answer);
        let mut waiting = cancel;
        let paths = tokio::select! {
            biased;
            // shortcut: a cancelled dialog stays until the person closes
            // it, and the next one waits; closing it from here would need
            // its window, which only its own thread may touch.
            _ = wait_cancel(&mut waiting) => return Err(PickError::Cancelled),
            answer = answer => answer.map_err(|_| PickError::Failed)?,
        };
        self.showing = None;
        paths
    }
}

/// The window a dialog is modal to, as `ParentWindow::win32` wrote it.
fn parent(window: Option<&ParentWindow>) -> Option<usize> {
    window.and_then(ParentWindow::win32_handle)
}

impl Dialog {
    /// Show the dialog on this thread, with COM for it, and wait for it.
    fn run(self) -> Answer {
        // SAFETY: COM is started on this thread, single threaded and without
        // OLE1, as the shell's dialogs ask, and ended after the dialog's
        // objects are released.
        unsafe {
            let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
            if com.is_err() {
                return Err(PickError::Unavailable);
            }
            let answer = self.show();
            CoUninitialize();
            answer
        }
    }

    /// # Safety
    /// COM is started on this thread.
    unsafe fn show(&self) -> Answer {
        let failed = |_| PickError::Unavailable;
        let dialog: IFileDialog = unsafe {
            match self.kind {
                Kind::Save { .. } => CoCreateInstance::<_, IFileSaveDialog>(
                    &FileSaveDialog,
                    None,
                    CLSCTX_INPROC_SERVER,
                )
                .map_err(failed)?
                .cast(),
                _ => CoCreateInstance::<_, IFileOpenDialog>(
                    &FileOpenDialog,
                    None,
                    CLSCTX_INPROC_SERVER,
                )
                .map_err(failed)?
                .cast(),
            }
            .map_err(failed)?
        };
        unsafe {
            let mut options =
                dialog.GetOptions().map_err(failed)? | FOS_FORCEFILESYSTEM | FOS_NOCHANGEDIR;
            match &self.kind {
                Kind::Open { multiple } => {
                    options |= FOS_FILEMUSTEXIST;
                    if *multiple {
                        options |= FOS_ALLOWMULTISELECT;
                    }
                }
                Kind::Save { name } => {
                    options &= !FOS_OVERWRITEPROMPT;
                    dialog
                        .SetFileName(&HSTRING::from(name.as_str()))
                        .map_err(failed)?;
                }
                Kind::Folder => options |= FOS_PICKFOLDERS,
            }
            dialog.SetOptions(options).map_err(failed)?;
            dialog
                .SetTitle(&HSTRING::from(self.title))
                .map_err(failed)?;
            if let Some(folder) = &self.folder {
                // A folder the shell cannot name is skipped, not an error.
                if let Ok(item) = SHCreateItemFromParsingName::<_, _, IShellItem>(
                    &HSTRING::from(folder.as_os_str()),
                    None::<&IBindCtx>,
                ) {
                    let _ = dialog.SetFolder(&item);
                }
            }
            let owner = self.parent.map(|handle| HWND(handle as *mut _));
            if let Err(error) = dialog.Show(owner) {
                return Err(if error.code() == ERROR_CANCELLED.to_hresult() {
                    PickError::Cancelled
                } else {
                    PickError::Failed
                });
            }
            let items = match &self.kind {
                Kind::Open { .. } => {
                    let open: IFileOpenDialog = dialog.cast().map_err(failed)?;
                    let results = open.GetResults().map_err(|_| PickError::InvalidResponse)?;
                    let count = results.GetCount().map_err(|_| PickError::InvalidResponse)?;
                    if count as usize > super::MAX_FILES {
                        return Err(PickError::TooMany);
                    }
                    (0..count)
                        .map(|index| results.GetItemAt(index))
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|_| PickError::InvalidResponse)?
                }
                _ => vec![dialog.GetResult().map_err(|_| PickError::InvalidResponse)?],
            };
            items.iter().map(|item| path(item)).collect()
        }
    }
}

/// The file system path of `item`: a dialog forced to the file system
/// answers with nothing else.
///
/// # Safety
/// COM is started on this thread.
unsafe fn path(item: &IShellItem) -> Result<PathBuf, PickError> {
    unsafe {
        let name = item
            .GetDisplayName(SIGDN_FILESYSPATH)
            .map_err(|_| PickError::InvalidResponse)?;
        let path = PathBuf::from(std::ffi::OsString::from_wide(name.as_wide()));
        CoTaskMemFree(Some(name.0 as *const _));
        if valid_path(&path) {
            Ok(path)
        } else {
            Err(PickError::InvalidResponse)
        }
    }
}
