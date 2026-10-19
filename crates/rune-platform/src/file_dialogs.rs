//! Bounded file selection transfers ownership to a service, never to the UI.

use rune_i18n::t;
use std::{
    fs::{File, Metadata},
    num::NonZeroU64,
    path::PathBuf,
};

pub(crate) mod files;
pub use files::{open_local_files, validate_local_paths};

#[cfg(windows)]
pub use self::windows::FileChooser;
#[cfg(target_os = "linux")]
pub use crate::portal::FileChooser;

#[cfg(windows)]
mod windows;

pub(crate) const MAX_PATH_BYTES: usize = 4096;
/// Discord's attachments per message (envios-y-subidas: was 8).
pub(crate) const MAX_FILES: usize = 10;
pub(crate) const MAX_DISPLAY_BYTES: usize = 180;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PickId(pub u64);

/// A real window identifier. Wayland requires an exported xdg_foreign token;
/// raw surface pointers cannot construct one.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParentWindow(String);

impl ParentWindow {
    pub fn x11(window: NonZeroU64) -> Self {
        Self(format!("x11:{window:x}"))
    }

    /// A Win32 window (windows-base), by its handle.
    pub fn win32(window: std::num::NonZeroUsize) -> Self {
        Self(format!("win32:{window:x}"))
    }

    /// The handle [`Self::win32`] wrote, if this is a Win32 window.
    #[cfg(any(windows, test))]
    pub(crate) fn win32_handle(&self) -> Option<usize> {
        usize::from_str_radix(self.0.strip_prefix("win32:")?, 16)
            .ok()
            .filter(|handle| *handle != 0)
    }

    /// A toplevel exported with `zxdg_exporter_v2`; `None` for a handle the
    /// portal could not read back.
    pub fn wayland(handle: &str) -> Option<Self> {
        (!handle.is_empty()
            && handle.len() <= 256
            && handle.bytes().all(|byte| byte.is_ascii_graphic()))
        .then(|| Self(format!("wayland:{handle}")))
    }

    // The portal's, and its tests'.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone)]
pub struct PickRequest {
    pub id: PickId,
    pub max_files: usize,
    pub max_file_bytes: u64,
    pub max_total_bytes: u64,
    pub parent: Option<ParentWindow>,
    pub current_folder: Option<PathBuf>,
}

impl PickRequest {
    pub fn validate(&self) -> Result<(), PickError> {
        if self.id.0 == 0
            || self.max_files == 0
            || self.max_files > MAX_FILES
            || self.max_file_bytes == 0
            || self
                .current_folder
                .as_ref()
                .is_some_and(|path| !valid_path(path))
        {
            return Err(PickError::InvalidRequest);
        }
        Ok(())
    }
}

pub(crate) fn valid_path(path: &std::path::Path) -> bool {
    let bytes = path.as_os_str().as_encoded_bytes();
    path.is_absolute() && bytes.len() <= MAX_PATH_BYTES && !bytes.contains(&0)
}

impl std::fmt::Debug for PickRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PickRequest")
            .field("id", &self.id)
            .field("max_files", &self.max_files)
            .field("max_file_bytes", &self.max_file_bytes)
            .field("max_total_bytes", &self.max_total_bytes)
            .field("parent", &self.parent)
            .field(
                "current_folder",
                &self.current_folder.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

/// An inert save-dialog suggestion. Selection does not create or open a file,
/// and does not establish an overwrite policy or a filesystem permission grant.
#[derive(Clone)]
pub struct SaveRequest {
    pub id: PickId,
    pub parent: Option<ParentWindow>,
    pub suggested_name: String,
    pub current_folder: Option<PathBuf>,
}

impl SaveRequest {
    pub fn validate(&self) -> Result<(), PickError> {
        let name = &self.suggested_name;
        if self.id.0 == 0
            || name.is_empty()
            || name.len() > MAX_DISPLAY_BYTES
            || matches!(name.as_str(), "." | "..")
            || name
                .chars()
                .any(|character| character.is_control() || matches!(character, '/' | '\\'))
            || self
                .current_folder
                .as_ref()
                .is_some_and(|path| !valid_path(path))
        {
            return Err(PickError::InvalidRequest);
        }
        Ok(())
    }
}

impl std::fmt::Debug for SaveRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SaveRequest")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .field("suggested_name", &"[REDACTED]")
            .field(
                "current_folder",
                &self.current_folder.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

/// A folder to choose (escritorio-linux: the call's sounds). Choosing it
/// neither opens nor lists it; whoever uses it reads it with its own checks.
#[derive(Clone)]
pub struct FolderRequest {
    pub id: PickId,
    pub parent: Option<ParentWindow>,
    pub current_folder: Option<PathBuf>,
}

impl FolderRequest {
    pub fn validate(&self) -> Result<(), PickError> {
        if self.id.0 == 0
            || self
                .current_folder
                .as_ref()
                .is_some_and(|path| !valid_path(path))
        {
            return Err(PickError::InvalidRequest);
        }
        Ok(())
    }
}

impl std::fmt::Debug for FolderRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FolderRequest")
            .field("id", &self.id)
            .field("parent", &self.parent)
            .field(
                "current_folder",
                &self.current_folder.as_ref().map(|_| "[REDACTED]"),
            )
            .finish()
    }
}

/// A destination chosen by the user, not an open file or an overwrite grant.
/// Keep it on service threads. Consume it once to anchor and validate the
/// destination before any download; this type deliberately cannot be cloned
/// or serialized.
///
/// ```compile_fail
/// use rune_platform::file_dialogs::SaveDestination;
/// fn require_clone<T: Clone>() {}
/// require_clone::<SaveDestination>();
/// ```
pub struct SaveDestination {
    path: PathBuf,
}

impl SaveDestination {
    pub fn into_path(self) -> PathBuf {
        self.path
    }

    // The portal's, and its tests'.
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) fn from_uri(uri: &str) -> Result<Self, PickError> {
        let path = files::local_path(uri)?;
        if path.file_name().is_none() || path.as_os_str().as_encoded_bytes().ends_with(b"/") {
            return Err(PickError::InvalidResponse);
        }
        Ok(Self { path })
    }
}

impl std::fmt::Debug for SaveDestination {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SaveDestination([REDACTED])")
    }
}

pub type CancelSignal = tokio::sync::watch::Receiver<bool>;

pub(crate) fn cancel_requested(cancel: &CancelSignal) -> bool {
    *cancel.borrow() || cancel.has_changed().is_err()
}

pub(crate) async fn wait_cancel(cancel: &mut CancelSignal) {
    while !cancel_requested(cancel) {
        if cancel.changed().await.is_err() {
            break;
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileInfo {
    pub display_name: String,
    pub size_bytes: u64,
}

/// Kept and dropped on service threads. This grant (the open descriptor) must
/// never reach UI polling, a provider event or serialization, nor be reopened
/// by path. Only its path, apart from it, may be kept for a draft and opened
/// again in a later run as a new selection, with every check.
pub struct SelectedFile {
    file: File,
    metadata: Metadata,
    info: FileInfo,
    /// Where it was opened from, for a draft kept across runs: a later run
    /// opens it again by path, with every check (envios-y-subidas).
    path: PathBuf,
}

impl SelectedFile {
    pub fn info(&self) -> &FileInfo {
        &self.info
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    pub fn into_parts(self) -> (File, Metadata, FileInfo) {
        (self.file, self.metadata, self.info)
    }
}

impl std::fmt::Debug for SelectedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SelectedFile([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PickError {
    Cancelled,
    Unavailable,
    InvalidRequest,
    InvalidResponse,
    NotRegularFile,
    TooLarge,
    TooMany,
    PermissionDenied,
    TimedOut,
    Failed,
}

impl PickError {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cancelled => t("Se canceló la selección del archivo."),
            Self::Unavailable => t("El selector de archivos no está disponible en este equipo."),
            Self::InvalidRequest => t("La solicitud del selector no es válida."),
            Self::InvalidResponse => t("El selector no devolvió un archivo local válido."),
            Self::NotRegularFile => {
                t("Seleccioná un archivo normal, no una carpeta o un dispositivo.")
            }
            Self::TooMany => t("La selección tiene más archivos de los permitidos."),
            Self::TooLarge => {
                t("El archivo o la respuesta del selector supera el límite permitido.")
            }
            Self::PermissionDenied => t("No se pudo obtener permiso para abrir el archivo."),
            Self::TimedOut => t("El selector tardó demasiado. Podés volver a intentarlo."),
            Self::Failed => t("No se pudo preparar el archivo seleccionado."),
        }
    }
}

impl std::fmt::Display for PickError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

impl std::error::Error for PickError {}

#[cfg(test)]
mod save_tests;
