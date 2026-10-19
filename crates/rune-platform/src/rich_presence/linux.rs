//! The `discord-ipc-N` socket, on its own thread and runtime.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

use super::{protocol, Account, PresenceEvent};
use crate::local_socket::{self, Bound};

pub(super) fn spawn(
    dir: Option<PathBuf>,
    account: Arc<Mutex<Option<Account>>>,
    report: impl Fn(PresenceEvent) + Send + Sync + 'static,
    stopped: oneshot::Receiver<()>,
) {
    let report = Arc::new(report);
    let failed = report.clone();
    let started = std::thread::Builder::new()
        .name("rune-rich-presence".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                report(PresenceEvent::Unavailable);
                return;
            };
            runtime.block_on(async {
                let Some((listener, path)) = dir.as_deref().and_then(bind) else {
                    report(PresenceEvent::Unavailable);
                    return;
                };
                let (programs, heard) = mpsc::channel(1);
                tokio::select! {
                    _ = accept(listener, programs) => {}
                    _ = protocol::serve(heard, account, report) => {}
                    _ = stopped => {}
                }
                let _ = std::fs::remove_file(path);
            });
        });
    if started.is_err() {
        failed(PresenceEvent::Unavailable);
    }
}

/// The first free `discord-ipc-0` to `-9`, as programs look for them.
fn bind(dir: &std::path::Path) -> Option<(UnixListener, PathBuf)> {
    if !local_socket::private_dir(dir) {
        return None;
    }
    (0..10).find_map(|slot| {
        let path = dir.join(format!("discord-ipc-{slot}"));
        match local_socket::bind(&path) {
            Ok(Bound::Listening(listener)) => {
                listener.set_nonblocking(true).ok()?;
                Some((UnixListener::from_std(listener).ok()?, path))
            }
            _ => None,
        }
    })
}

/// Hand each program of this user that connects to `programs`.
async fn accept(listener: UnixListener, programs: mpsc::Sender<UnixStream>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            // Out of descriptors, say: try again later rather than spin.
            tokio::time::sleep(Duration::from_secs(1)).await;
            continue;
        };
        let same_user = stream
            .peer_cred()
            .is_ok_and(|peer| peer.uid() == local_socket::uid());
        if same_user && programs.send(stream).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
