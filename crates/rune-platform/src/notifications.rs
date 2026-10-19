//! Desktop notifications: a title and a line, and which one the person
//! clicked, or which of its buttons they pressed.
//!
//! On Linux this is the freedesktop `org.freedesktop.Notifications` service,
//! which every desktop's notification daemon offers to unsandboxed programs;
//! the desktop portal's own interface needs an installed application ID that
//! RUNE does not have. On Windows they are the system's toasts
//! (windows-base). The service lives on its own thread, so nothing here
//! blocks the caller; it ends when the handle drops. A click comes back only
//! as the caller's own key, never as the service's ID, and only for a
//! notification this handle showed; a button, only as one of those it
//! offered.

use std::sync::{mpsc, Arc};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(any(windows, test))]
mod windows;

/// What one notification says, and the caller's key to know it again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub key: u64,
    pub title: String,
    pub body: String,
    /// Buttons besides the click, by the caller's own action and its
    /// label; at most [`MAX_BUTTONS`]. A daemon without buttons shows none.
    pub buttons: Vec<(&'static str, String)>,
    /// A call ringing (llamadas-y-panel-de-voz): the desktop is told so, and
    /// it stays until [`Notifier::withdraw`] takes it away.
    pub ringing: bool,
}

/// Bounds of what a notification may carry; longer text is cut.
const MAX_TITLE: usize = 128;
const MAX_BODY: usize = 512;
const MAX_LABEL: usize = 32;
pub const MAX_BUTTONS: usize = 3;

impl Notice {
    /// `title` and `body` cut to their bounds, on a character boundary.
    pub fn new(key: u64, title: &str, body: &str) -> Self {
        Self {
            key,
            title: cut(title, MAX_TITLE),
            body: cut(body, MAX_BODY),
            buttons: Vec::new(),
            ringing: false,
        }
    }

    /// A call coming in, with its `buttons` (answering, declining): it
    /// stays while it rings.
    pub fn ringing(mut self, buttons: &[(&'static str, &str)]) -> Self {
        self.ringing = true;
        self.buttons = buttons
            .iter()
            // "default" is the click on the notification itself.
            .filter(|(action, _)| *action != "default" && !action.is_empty())
            .take(MAX_BUTTONS)
            .map(|(action, label)| (*action, cut(label, MAX_LABEL)))
            .collect();
        self
    }
}

fn cut(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_owned();
    }
    let mut end = max.saturating_sub('…'.len_utf8());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &text[..end])
}

enum Request {
    Show(Notice),
    /// Close every notification still shown, as when the account changes.
    Withdraw,
    /// Close the one shown with this key, as when a call stops ringing.
    Close(u64),
}

/// What the person did with a notification: its key, and the button they
/// pressed (one of its own actions), or `None` for a click on it.
pub type Click = (u64, Option<&'static str>);

/// Windows: give this process the application ID its toasts and the
/// installer's Start menu shortcut carry, so its windows group with a
/// pinned RUNE however it started (the shortcut, the session's autostart,
/// an update). Before any window opens; elsewhere nothing.
pub fn claim_application_id() {
    #[cfg(windows)]
    self::windows::claim_application_id();
}

/// The notification service. Dropping it ends its thread.
pub struct Notifier {
    requests: tokio::sync::mpsc::Sender<Request>,
    clicks: mpsc::Receiver<Click>,
}

impl Notifier {
    /// Connect on a thread of its own; `wake` runs after each click.
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        Self::spawn(None, wake)
    }

    fn spawn(address: Option<String>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (requests, received) = tokio::sync::mpsc::channel(8);
        let (clicked, clicks) = mpsc::channel();
        let report = move |click: Click| {
            if clicked.send(click).is_ok() {
                wake();
            }
        };
        #[cfg(target_os = "linux")]
        linux::spawn(address, received, report);
        #[cfg(windows)]
        {
            drop(address);
            self::windows::spawn(received, report);
        }
        #[cfg(not(any(target_os = "linux", windows)))]
        drop((address, received, report));
        Self { requests, clicks }
    }

    /// Show `notice`; `false` when the service is busy or gone. Never blocks.
    pub fn show(&self, notice: Notice) -> bool {
        self.requests.try_send(Request::Show(notice)).is_ok()
    }

    /// Close every notification this handle still shows.
    pub fn withdraw_all(&self) {
        let _ = self.requests.try_send(Request::Withdraw);
    }

    /// Close the notification shown with `key`, if it still is; `false`
    /// only while the service is busy, to be asked again. Never blocks.
    pub fn withdraw(&self, key: u64) -> bool {
        !matches!(
            self.requests.try_send(Request::Close(key)),
            Err(tokio::sync::mpsc::error::TrySendError::Full(_))
        )
    }

    /// What the person did with a notification, if anything.
    pub fn poll_click(&self) -> Option<Click> {
        self.clicks.try_recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_text_is_cut_on_a_character_boundary() {
        let notice = Notice::new(1, &"ñ".repeat(100), "corto");
        assert!(notice.title.len() <= MAX_TITLE && notice.title.ends_with('…'));
        assert!(notice.title.trim_end_matches('…').chars().all(|c| c == 'ñ'));
        assert_eq!(notice.body, "corto");
        assert!(notice.buttons.is_empty() && !notice.ringing);
    }

    #[test]
    fn a_ringing_notice_keeps_its_own_buttons_but_never_the_click() {
        let notice = Notice::new(1, "Llamada", "Ada").ringing(&[
            ("default", "Abrir"),
            ("accept", "Aceptar"),
            ("decline", &"x".repeat(80)),
            ("", "Nada"),
            ("a", "1"),
            ("b", "2"),
        ]);
        assert!(notice.ringing);
        let actions: Vec<_> = notice.buttons.iter().map(|(action, _)| *action).collect();
        assert_eq!(actions, ["accept", "decline", "a"]);
        assert!(notice.buttons[1].1.len() <= MAX_LABEL);
    }
}
