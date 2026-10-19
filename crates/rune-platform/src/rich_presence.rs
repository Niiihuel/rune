//! The local Rich Presence socket other programs on this computer talk to
//! (`$XDG_RUNTIME_DIR/discord-ipc-N`), so a game or a player can say what
//! the person is doing. Off unless the person turns it on.
//!
//! Only the local protocol is spoken: a frame is an opcode and a length,
//! both 32-bit little endian, then that many bytes of JSON. A program
//! greets (opcode 0, `{"v":1,"client_id":"…"}`), is told `READY`, and may
//! then `SET_ACTIVITY` (cleared when it leaves) or `INVITE_BROWSER`; every
//! other command is refused. Pings (3) are answered (4). There is no TCP or
//! WebSocket port, and only this user's programs are heard.

use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};

#[cfg(target_os = "linux")]
mod linux;

/// What a program says it is doing. Text is bounded and without control
/// characters; anything else it sent is dropped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Activity {
    /// The program's application ID, from its greeting.
    pub application: u64,
    /// 0 playing, 2 listening, 3 watching, 5 competing.
    pub kind: u8,
    pub details: Option<String>,
    pub state: Option<String>,
    /// Unix milliseconds.
    pub start: Option<i64>,
    pub end: Option<i64>,
    /// The application's asset keys, `mp:` paths or `https` addresses, as
    /// the program gave them; the provider decides what it can publish.
    pub large_image: Option<String>,
    pub large_text: Option<String>,
    pub small_image: Option<String>,
    pub small_text: Option<String>,
}

/// What the programs did, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresenceEvent {
    /// The program on connection `client` set or changed its activity.
    Set { client: u64, activity: Activity },
    /// It cleared its activity, or went away.
    Cleared { client: u64 },
    /// It asked to show an invitation by its code; nothing joins by itself.
    Invite { code: String },
    /// The socket could not be opened; nothing else follows.
    Unavailable,
}

/// Who the programs are told is signed in, in `READY`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub id: u64,
    pub username: String,
}

/// The socket and its thread. Dropping it closes them and removes the socket.
pub struct RichPresence {
    events: mpsc::Receiver<PresenceEvent>,
    account: Arc<Mutex<Option<Account>>>,
    // Dropping the sender is the stop signal the worker waits on.
    _stop: tokio::sync::oneshot::Sender<()>,
}

impl RichPresence {
    /// Open the first free `discord-ipc-N` in the runtime directory and
    /// listen; `wake` runs after each event.
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        #[cfg(target_os = "linux")]
        let dir = crate::local_socket::runtime_dir();
        #[cfg(not(target_os = "linux"))]
        let dir = None;
        Self::spawn(dir, wake)
    }

    fn spawn(dir: Option<PathBuf>, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, events) = mpsc::channel();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let account = Arc::new(Mutex::new(None));
        let report = move |event: PresenceEvent| {
            if sender.send(event).is_ok() {
                wake();
            }
        };
        #[cfg(target_os = "linux")]
        linux::spawn(dir, account.clone(), report, stopped);
        #[cfg(not(target_os = "linux"))]
        {
            let _ = (dir, stopped);
            report(PresenceEvent::Unavailable);
        }
        Self {
            events,
            account,
            _stop: stop,
        }
    }

    /// Tell programs that greet from now on who is signed in.
    pub fn set_account(&self, account: Option<Account>) {
        if let Ok(mut current) = self.account.lock() {
            *current = account;
        }
    }

    pub fn poll(&self) -> Option<PresenceEvent> {
        self.events.try_recv().ok()
    }
}
