//! Global shortcuts: keys the desktop reports while another window has focus.
//!
//! On Linux this asks the xdg-desktop-portal `GlobalShortcuts` interface.
//! The compositor owns the grab and may ask the person to confirm it or to
//! choose another trigger. RUNE never grabs X11 keys nor reads input devices.
//! On Windows low-level keyboard and mouse hooks compare each input with the
//! bound shortcuts and consume nothing (windows-base, docs/SECURITY.md).
//! A session lives on its own thread and ends when its handle is dropped.

use rune_i18n::{es, t};
use std::{
    fmt,
    sync::{mpsc, Arc},
};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(any(windows, test))]
mod windows;

/// One shortcut to ask the desktop for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub id: String,
    /// What the desktop's dialog says it does.
    pub description: String,
    /// The trigger in the freedesktop shortcuts format, such as `CTRL+SHIFT+t`.
    /// The desktop may ignore it or let the person choose another.
    pub preferred_trigger: Option<String>,
}

/// What a session reports, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShortcutEvent {
    /// The desktop bound these shortcuts: each id with how it describes its trigger.
    Bound(Vec<(String, String)>),
    Pressed(String),
    Released(String),
    /// The session is over; nothing follows. A held shortcut counts as released.
    Ended(ShortcutError),
}

/// A fixed, user-facing reason; never the bus's own message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShortcutError(&'static str);

impl ShortcutError {
    pub const NO_PORTAL: Self = Self(es("No hay portal del escritorio en esta sesión."));
    pub const UNSUPPORTED: Self = Self(es("El escritorio no ofrece atajos globales."));
    pub const DENIED: Self = Self(es("El escritorio no autorizó el atajo global."));
    pub const CLOSED: Self = Self(es("El escritorio cerró los atajos globales."));
    pub const FAILED: Self = Self(es("El portal de atajos globales no respondió."));
    pub const WINDOWS_KEY: Self = Self(es("Windows no puede usar esa tecla como atajo global."));
    pub const WINDOWS_HOOK: Self = Self(es(
        "Windows no dejó instalar el escucha de teclado y ratón para el atajo global.",
    ));
}

impl fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(t(self.0))
    }
}

impl std::error::Error for ShortcutError {}

/// One portal session. Dropping it closes the session and releases the keys.
pub struct GlobalShortcuts {
    events: mpsc::Receiver<ShortcutEvent>,
    // Dropping the sender is the stop signal the worker waits on.
    _stop: tokio::sync::oneshot::Sender<()>,
}

impl GlobalShortcuts {
    /// Ask the session bus's desktop portal for `shortcuts`. Never blocks:
    /// `wake` runs after each event, from the session's thread.
    pub fn start(shortcuts: Vec<Shortcut>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self::spawn(None, shortcuts, wake)
    }

    fn spawn(
        address: Option<String>,
        shortcuts: Vec<Shortcut>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (sender, events) = mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let report = move |event: ShortcutEvent| {
            let _ = sender.send(event);
            wake();
        };
        #[cfg(target_os = "linux")]
        linux::spawn(address, shortcuts, report, stopped);
        #[cfg(windows)]
        {
            drop(address);
            self::windows::spawn(shortcuts, report, stopped);
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (address, shortcuts, stopped);
            report(ShortcutEvent::Ended(ShortcutError::UNSUPPORTED));
        }
        Self {
            events,
            _stop: stop,
        }
    }

    /// The next event, if one arrived; a vanished worker reads as ended.
    pub fn poll(&self) -> Option<ShortcutEvent> {
        match self.events.try_recv() {
            Ok(event) => Some(event),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(mpsc::TryRecvError::Disconnected) => {
                Some(ShortcutEvent::Ended(ShortcutError::FAILED))
            }
        }
    }
}
