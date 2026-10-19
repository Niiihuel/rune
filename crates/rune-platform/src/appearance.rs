//! The desktop's light or dark preference, followed while RUNE runs.
//!
//! On Linux this is the xdg-desktop-portal `Settings` interface, key
//! `org.freedesktop.appearance color-scheme`, which GNOME, KDE and the
//! wlroots portals all answer; its `SettingChanged` signal reports a switch
//! at once. On Windows it is the user's registry, which also gives the
//! accent color (windows-base). The watcher lives on its own thread and ends
//! when its handle drops.

use std::sync::{mpsc, Arc};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(any(windows, test))]
mod windows;

/// The desktop's accent color, as red, green and blue.
pub type Accent = [u8; 3];

/// What the desktop prefers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorScheme {
    /// No preference, or no portal to ask: the application chooses.
    #[default]
    NoPreference,
    Dark,
    Light,
}

impl ColorScheme {
    /// The portal's number: 1 is dark, 2 light, anything else none.
    pub fn from_portal(value: u32) -> Self {
        match value {
            1 => Self::Dark,
            2 => Self::Light,
            _ => Self::NoPreference,
        }
    }
}

/// Watches the preference. Dropping it ends its thread.
pub struct Appearance {
    schemes: mpsc::Receiver<ColorScheme>,
    /// Only Windows says one yet.
    accents: mpsc::Receiver<Option<Accent>>,
    // Dropping the sender is the stop signal the worker waits on.
    _stop: tokio::sync::oneshot::Sender<()>,
}

impl Appearance {
    /// Read the preference and follow it; `wake` runs after each change.
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self::spawn(None, wake)
    }

    fn spawn(address: Option<String>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, schemes) = mpsc::channel();
        let (tinted, accents) = mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let woken = wake.clone();
        let report = move |scheme: ColorScheme| {
            if sender.send(scheme).is_ok() {
                woken();
            }
        };
        let tint = move |accent: Option<Accent>| {
            if tinted.send(accent).is_ok() {
                wake();
            }
        };
        #[cfg(target_os = "linux")]
        {
            drop(tint);
            linux::spawn(address, report, stopped);
        }
        #[cfg(windows)]
        {
            drop(address);
            self::windows::spawn(report, tint, stopped);
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        drop((address, report, tint, stopped));
        Self {
            schemes,
            accents,
            _stop: stop,
        }
    }

    /// The newest preference since the last poll, if it changed.
    pub fn poll(&self) -> Option<ColorScheme> {
        self.schemes.try_iter().last()
    }

    /// The newest accent color since the last poll, if it changed:
    /// `Some(None)` when the desktop no longer says one.
    pub fn poll_accent(&self) -> Option<Option<Accent>> {
        self.accents.try_iter().last()
    }
}
