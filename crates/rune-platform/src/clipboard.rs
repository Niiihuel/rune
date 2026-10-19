//! Files and pictures on the system clipboard, read for attaching them and
//! written when the person copies a picture or a video of a message.
//!
//! On Linux this asks the X11 clipboard through `arboard`, the library the
//! window's text clipboard already uses: copied files as `text/uri-list`,
//! a picture as `image/png`. A Wayland session reaches it through XWayland,
//! which carries every clipboard type across; without XWayland nothing is
//! read. Wayland's own clipboard belongs to limn's data owner, whose
//! integration with the window is still pending. On Windows `arboard` speaks
//! the system's clipboard itself (windows-base): copied files as a file
//! drop, a picture as a bitmap. Reading blocks: call it only from a service
//! thread, never the interface's.

use std::path::PathBuf;

/// What the clipboard holds that can be attached.
#[derive(Debug, PartialEq, Eq)]
pub enum ClipboardFiles {
    /// Copied files, as local absolute paths, in their order.
    Paths(Vec<PathBuf>),
    /// A picture, as 8-bit RGBA rows of `width` pixels.
    Image {
        width: usize,
        height: usize,
        rgba: Vec<u8>,
    },
}

/// Why nothing was read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    /// The clipboard holds no files and no picture.
    Empty,
    /// More files than `max_files`, or a picture over `max_image_bytes`.
    TooLarge,
    /// No clipboard to ask in this session.
    Unavailable,
}

/// Read the files or the picture on the clipboard, files first. At most
/// `max_files` paths, each absolute; a picture of at most `max_image_bytes`
/// of pixels. Blocks while the clipboard's owner answers.
pub fn read_attachments(
    max_files: usize,
    max_image_bytes: usize,
) -> Result<ClipboardFiles, ClipboardError> {
    #[cfg(any(target_os = "linux", windows))]
    {
        let mut clipboard = arboard::Clipboard::new().map_err(|_| ClipboardError::Unavailable)?;
        if let Ok(paths) = clipboard.get().file_list() {
            return files(paths, max_files);
        }
        match clipboard.get().image() {
            Ok(image) => picture(
                image.width,
                image.height,
                image.bytes.into_owned(),
                max_image_bytes,
            ),
            Err(arboard::Error::ContentNotAvailable) => Err(ClipboardError::Empty),
            Err(_) => Err(ClipboardError::Unavailable),
        }
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = (max_files, max_image_bytes);
        Err(ClipboardError::Unavailable)
    }
}

// Message content (contenido-de-mensajes): copying a picture or a video.

/// Why a copy could not reach the clipboard, as text for the person; it
/// goes through `rune_i18n::t` where it is shown.
const NO_CLIPBOARD: &str = rune_i18n::es("No hay un portapapeles disponible en esta sesión.");
#[cfg(target_os = "linux")]
const NO_WINDOW: &str = rune_i18n::es(
    "En Wayland, RUNE copia imágenes y videos con su ventana propia del portapapeles de XWayland, y XWayland no está activo en esta sesión. Guardá el archivo y copialo desde ahí.",
);
const NOT_COPIED: &str = rune_i18n::es("El portapapeles no aceptó la copia.");

/// Whether this is a Wayland session without XWayland: there RUNE has no
/// window of its own to hold the clipboard (`arboard` speaks X11 only).
#[cfg(target_os = "linux")]
fn wayland_alone() -> bool {
    std::env::var_os("WAYLAND_DISPLAY").is_some() && std::env::var_os("DISPLAY").is_none()
}

/// The clipboard RUNE writes to, kept for the whole run: on X11 what was
/// copied lasts only while its owner lives.
#[cfg(any(target_os = "linux", windows))]
fn writer() -> Result<std::sync::MutexGuard<'static, Option<arboard::Clipboard>>, &'static str> {
    static WRITER: std::sync::Mutex<Option<arboard::Clipboard>> = std::sync::Mutex::new(None);
    let mut writer = WRITER.lock().map_err(|_| NO_CLIPBOARD)?;
    if writer.is_none() {
        #[cfg(target_os = "linux")]
        if wayland_alone() {
            return Err(NO_WINDOW);
        }
        *writer = Some(arboard::Clipboard::new().map_err(|_| NO_CLIPBOARD)?);
    }
    Ok(writer)
}

/// Put a picture on the clipboard, 8-bit RGBA rows of `width` pixels, for
/// the programs that paste images. Blocks briefly: call it off the
/// interface's thread.
pub fn write_image(width: usize, height: usize, rgba: &[u8]) -> Result<(), &'static str> {
    if width == 0
        || height == 0
        || width.checked_mul(height).and_then(|n| n.checked_mul(4)) != Some(rgba.len())
    {
        return Err(NOT_COPIED);
    }
    #[cfg(any(target_os = "linux", windows))]
    {
        writer()?
            .as_mut()
            .ok_or(NO_CLIPBOARD)?
            .set_image(arboard::ImageData {
                width,
                height,
                bytes: std::borrow::Cow::Borrowed(rgba),
            })
            .map_err(|_| NOT_COPIED)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Err(NO_CLIPBOARD)
    }
}

/// Put the file at `path` on the clipboard, as file managers and chat
/// programs paste files (`text/uri-list`). The file must stay while it is
/// there: see [`copies_dir`].
pub fn write_file(path: &std::path::Path) -> Result<(), &'static str> {
    if !path.is_absolute() {
        return Err(NOT_COPIED);
    }
    #[cfg(any(target_os = "linux", windows))]
    {
        writer()?
            .as_mut()
            .ok_or(NO_CLIPBOARD)?
            .set()
            .file_list(&[path])
            .map_err(|_| NOT_COPIED)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        Err(NO_CLIPBOARD)
    }
}

/// Where a copied file waits while it is on the clipboard: a folder of
/// `$XDG_RUNTIME_DIR` only this user may enter, made on first use. RUNE
/// never falls back to a shared directory such as `/tmp`. On Windows, one
/// in RUNE's folder of the user's profile, which only they may enter.
pub fn copies_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let dir = crate::folders::data()?.join("copied");
        std::fs::create_dir_all(&dir).ok()?;
        Some(dir)
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::DirBuilderExt;
        let dir = crate::local_socket::runtime_dir()?.join("rune-copied");
        if let Err(error) = std::fs::DirBuilder::new().mode(0o700).create(&dir) {
            if error.kind() != std::io::ErrorKind::AlreadyExists {
                return None;
            }
        }
        crate::local_socket::private_dir(&dir).then_some(dir)
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        None
    }
}

/// Copied files: some, not too many, each a local absolute path.
fn files(paths: Vec<PathBuf>, max_files: usize) -> Result<ClipboardFiles, ClipboardError> {
    if paths.is_empty() || !paths.iter().all(|path| path.is_absolute()) {
        return Err(ClipboardError::Empty);
    }
    if paths.len() > max_files {
        return Err(ClipboardError::TooLarge);
    }
    Ok(ClipboardFiles::Paths(paths))
}

/// A picture whose pixels are what its size says, within the bound.
fn picture(
    width: usize,
    height: usize,
    rgba: Vec<u8>,
    max_image_bytes: usize,
) -> Result<ClipboardFiles, ClipboardError> {
    let expected = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4));
    if width == 0 || height == 0 || expected != Some(rgba.len()) {
        return Err(ClipboardError::Empty);
    }
    if rgba.len() > max_image_bytes {
        return Err(ClipboardError::TooLarge);
    }
    Ok(ClipboardFiles::Image {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_files_must_be_absolute_and_within_the_count() {
        let absolute = || {
            let folder = std::env::temp_dir();
            vec![folder.join("a.png"), folder.join("b.txt")]
        };
        assert_eq!(files(absolute(), 2), Ok(ClipboardFiles::Paths(absolute())));
        assert_eq!(files(absolute(), 1), Err(ClipboardError::TooLarge));
        assert_eq!(files(vec![], 8), Err(ClipboardError::Empty));
        assert_eq!(
            files(vec![PathBuf::from("relative.txt")], 8),
            Err(ClipboardError::Empty)
        );
    }

    #[test]
    fn a_picture_must_have_the_pixels_its_size_says_within_the_bound() {
        assert!(picture(2, 1, vec![0; 8], 8).is_ok());
        assert_eq!(picture(2, 1, vec![0; 7], 8), Err(ClipboardError::Empty));
        assert_eq!(picture(0, 0, vec![], 8), Err(ClipboardError::Empty));
        assert_eq!(picture(2, 2, vec![0; 16], 8), Err(ClipboardError::TooLarge));
        assert_eq!(
            picture(usize::MAX, 2, vec![], usize::MAX),
            Err(ClipboardError::Empty)
        );
    }
}
