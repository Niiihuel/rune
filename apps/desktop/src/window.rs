//! The window on Linux (escritorio-linux): what is read before it opens
//! (its decorations, the GPU chosen) from small files of RUNE's
//! configuration, never a credential; and the window as the parent of the
//! portal's dialogs, so they open over it, modal to it.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::OnceLock;

use eframe::wgpu::rwh::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use rune_platform::file_dialogs::ParentWindow;

/// Where RUNE keeps the setting `name` that must be known before the window
/// opens: `~/.config/rune/{name}`, or `%APPDATA%\RUNE\{name}`.
pub fn config_path(name: &str) -> Option<PathBuf> {
    Some(rune_platform::folders::config()?.join(name))
}

/// The value kept under `name`, if any.
pub fn remembered(name: &str) -> Option<String> {
    let value = std::fs::read_to_string(config_path(name)?).ok()?;
    Some(value.trim().to_owned()).filter(|value| !value.is_empty())
}

/// Keep `value` under `name` for the next start, or forget it, off the
/// interface's thread: one writer, in the order asked, so the last change
/// is the one kept.
pub fn remember(name: &'static str, value: Option<String>) {
    static WRITER: OnceLock<Sender<(&'static str, Option<String>)>> = OnceLock::new();
    let writer = WRITER.get_or_init(|| {
        let (writer, changes) = mpsc::channel::<(&'static str, Option<String>)>();
        std::thread::spawn(move || {
            for (name, value) in changes {
                let kept = config_path(name).map_or(Ok(()), |path| keep(&path, value.as_deref()));
                if let Err(error) = kept {
                    eprintln!("rune: could not remember {name}: {error}");
                }
            }
        });
        writer
    });
    let _ = writer.send((name, value));
}

/// Write `value` to `path` whole (a temporary file renamed over it), or
/// remove it.
fn keep(path: &Path, value: Option<&str>) -> std::io::Result<()> {
    let Some(value) = value else {
        return match std::fs::remove_file(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("new");
    std::fs::write(&temporary, value)?;
    std::fs::rename(&temporary, path)
}

/// Who draws the window's title bar, as the person left it (`frame.rs`):
/// once only whether the decorations were hidden.
pub const DECORATIONS: &str = "decorations";
pub const HIDDEN: &str = "hidden";

// Own themes (temas-propios).
/// How much of the desktop shows through the window, in percent, as the
/// person left it: the window opens able to only when it is above 0.
pub fn transparency() -> u8 {
    remembered(TRANSPARENCY)
        .and_then(|value| value.parse::<u8>().ok())
        .map_or(0, |percent| percent.min(rune_ui::themes::MAX_TRANSPARENCY))
}

pub const TRANSPARENCY: &str = "transparency";

/// The window as the portal's dialogs name their parent: the X11 window, or
/// the Wayland toplevel exported for it; `None` without one, and dialogs
/// open on their own.
#[derive(Default)]
pub struct Parent(Option<ParentWindow>);

impl Parent {
    /// The window eframe just opened, which lives as long as the app. On
    /// Wayland it is exported here, while the window is set up (two round
    /// trips to the compositor, as winit's own setup makes), never at a
    /// click.
    pub fn of(window: &(impl HasWindowHandle + HasDisplayHandle)) -> Self {
        Self(parent(
            window.window_handle().ok().map(|handle| handle.as_raw()),
            window.display_handle().ok().map(|handle| handle.as_raw()),
        ))
    }

    /// The parent to give a dialog.
    pub fn get(&self) -> Option<ParentWindow> {
        self.0.clone()
    }
}

fn parent(
    window: Option<RawWindowHandle>,
    display: Option<RawDisplayHandle>,
) -> Option<ParentWindow> {
    match (window?, display?) {
        // `c_ulong`: 32 bits on some targets.
        #[allow(clippy::unnecessary_cast)]
        (RawWindowHandle::Xlib(window), _) => {
            ParentWindow::x11(std::num::NonZeroU64::new(window.window as u64)?).into()
        }
        (RawWindowHandle::Xcb(window), _) => {
            ParentWindow::x11(std::num::NonZeroU64::from(window.window)).into()
        }
        // Windows (windows-base): the dialogs are modal to the window itself.
        (RawWindowHandle::Win32(window), _) => {
            ParentWindow::win32(std::num::NonZeroUsize::new(window.hwnd.get() as usize)?).into()
        }
        #[cfg(target_os = "linux")]
        (RawWindowHandle::Wayland(window), RawDisplayHandle::Wayland(display)) => {
            // Safety: both are the window's, which outlives the app and so
            // every use of the handle.
            let parent =
                unsafe { rune_platform::wayland_parent::export(display.display, window.surface) };
            if parent.is_none() {
                eprintln!(
                    "rune: the compositor cannot export the window (xdg-foreign v2); dialogs open on their own"
                );
            }
            parent
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_x11_window_is_its_own_parent_and_none_without_a_window() {
        assert_eq!(Parent::default().get(), None);
        let window = RawWindowHandle::Xcb(eframe::wgpu::rwh::XcbWindowHandle::new(
            std::num::NonZeroU32::new(0x2a).unwrap(),
        ));
        let display = RawDisplayHandle::Xcb(eframe::wgpu::rwh::XcbDisplayHandle::new(None, 0));
        assert_eq!(
            parent(Some(window), Some(display)),
            Some(ParentWindow::x11(std::num::NonZeroU64::new(0x2a).unwrap()))
        );
        assert_eq!(parent(None, Some(display)), None);
    }

    #[test]
    fn a_setting_is_written_whole_and_forgotten() {
        let path = std::env::temp_dir()
            .join(format!("rune-window-test-{}", std::process::id()))
            .join("decorations");
        keep(&path, Some(HIDDEN)).unwrap();
        keep(&path, Some("shown")).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "shown");
        assert!(!path.with_extension("new").exists(), "no temporary left");
        keep(&path, None).unwrap();
        assert!(!path.exists());
        keep(&path, None).unwrap();
        std::fs::remove_dir(path.parent().unwrap()).unwrap();
    }
}
