//! Native platform services; the application's interface remains in limn.

/// The desktop's light or dark preference.
pub mod appearance;
/// Starting with the desktop session.
pub mod autostart;
/// Files and pictures on the clipboard, for attaching and copying them.
pub mod clipboard;
pub mod file_dialogs;
/// Where RUNE keeps its files on this system.
pub mod folders;
/// The system's fonts, for interface text.
pub mod fonts;
/// Known games running, by their processes' names, and the programs open.
pub mod games;
/// Hiding the window on Hyprland, which ignores minimizing.
#[cfg(target_os = "linux")]
pub mod hyprland;
/// Hiding the window in the tray on KDE Plasma under Wayland.
#[cfg(target_os = "linux")]
pub mod kwin;
/// Desktop notifications and which one was clicked.
pub mod notifications;
/// Web addresses and folders opened with the system's handler.
pub mod open;
/// The local Rich Presence socket other programs talk to.
pub mod rich_presence;
/// Screen and camera access through the desktop portal.
#[cfg(target_os = "linux")]
pub mod screencast;
pub mod secrets;
pub mod shortcuts;
/// One RUNE per person: later starts hand their links to the first.
#[cfg(any(target_os = "linux", windows))]
pub mod single_instance;
/// The icon in the system tray, and its menu.
pub mod tray;
/// The window as the parent of the portal's dialogs on Wayland.
#[cfg(target_os = "linux")]
pub mod wayland_parent;

#[cfg(target_os = "linux")]
mod local_socket;
#[cfg(target_os = "linux")]
mod portal;
/// What the Windows services share.
#[cfg(windows)]
mod win;
