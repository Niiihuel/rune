//! RUNE's icon in the system tray, with its menu: open the window, mute,
//! deafen, choose the status, quit. A red dot marks unread mentions and the
//! tooltip counts them.
//!
//! On Linux this is a StatusNotifierItem with a `com.canonical.dbusmenu`
//! menu, spoken over D-Bus as KDE, GNOME's AppIndicator extension, waybar
//! and most panels expect, without GTK. On Windows it is an icon of the
//! notification area (windows-base). The item lives on its own thread and
//! goes when its handle drops. Without a tray host on the desktop it says
//! so, and the window must not hide in it.

use std::sync::{mpsc, Arc};

use rune_i18n::{t, tr, Language};

#[cfg(target_os = "linux")]
mod linux;
mod mark;
#[cfg(any(windows, test))]
mod windows;

/// The account's status, as the menu offers it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrayStatus {
    #[default]
    Online,
    Idle,
    DoNotDisturb,
    Invisible,
}

impl TrayStatus {
    pub const ALL: [Self; 4] = [
        Self::Online,
        Self::Idle,
        Self::DoNotDisturb,
        Self::Invisible,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Online => t("En línea"),
            Self::Idle => t("Ausente"),
            Self::DoNotDisturb => t("No molestar"),
            Self::Invisible => t("Invisible"),
        }
    }
}

/// What the icon and its menu show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrayState {
    /// Unread mentions and direct messages, for the dot and the tooltip.
    pub mentions: u32,
    /// Conversations with something unread.
    pub unread: u32,
    pub muted: bool,
    pub deafened: bool,
    /// `None` while no account is signed in: the status menu is off.
    pub status: Option<TrayStatus>,
    /// What the menu and the tooltip are written in; a change rewrites both.
    pub language: Language,
}

impl TrayState {
    /// The tooltip's line under the name.
    fn summary(&self) -> String {
        let _language = rune_i18n::scoped(self.language);
        let mentions = match self.mentions {
            0 => None,
            1 => Some(t("1 mención").to_owned()),
            many => Some(tr!("{many} menciones", many)),
        };
        let unread = match self.unread {
            0 => None,
            1 => Some(t("1 conversación sin leer").to_owned()),
            many => Some(tr!("{many} conversaciones sin leer", many)),
        };
        match (mentions, unread) {
            (None, None) => t("Todo leído").to_owned(),
            (Some(one), None) | (None, Some(one)) => one,
            (Some(mentions), Some(unread)) => format!("{mentions} · {unread}"),
        }
    }
}

/// What the person did with the icon or its menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayEvent {
    /// The desktop shows the icon (again).
    Shown,
    /// There is no tray host, or it went away.
    Unavailable,
    /// Bring the window to the front.
    Open,
    ToggleMute,
    ToggleDeafen,
    Status(TrayStatus),
    Quit,
}

/// The icon. Dropping it removes it.
pub struct Tray {
    state: tokio::sync::watch::Sender<TrayState>,
    events: mpsc::Receiver<TrayEvent>,
}

impl Tray {
    /// Put the icon in the session's tray; `wake` runs after each event.
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self::spawn(None, wake)
    }

    fn spawn(address: Option<String>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (state, watched) = tokio::sync::watch::channel(TrayState::default());
        let (sender, events) = mpsc::channel();
        let report = move |event: TrayEvent| {
            if sender.send(event).is_ok() {
                wake();
            }
        };
        #[cfg(target_os = "linux")]
        linux::spawn(address, watched, report);
        #[cfg(windows)]
        {
            drop(address);
            self::windows::spawn(watched, report);
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        {
            let _ = (address, watched);
            report(TrayEvent::Unavailable);
        }
        Self { state, events }
    }

    /// Show `state`; nothing is sent when it did not change. Never blocks.
    pub fn update(&self, state: TrayState) {
        self.state.send_if_modified(|current| {
            let changed = *current != state;
            *current = state;
            changed
        });
    }

    pub fn poll(&self) -> Option<TrayEvent> {
        self.events.try_recv().ok()
    }
}
