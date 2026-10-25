//! Network changes the system reports, so the Gateway tries again at once
//! instead of waiting out its backoff: NetworkManager reaching a connected
//! state, and the machine resuming from sleep (logind). Without the bus or
//! the services, nothing is watched.

use tokio::task::JoinHandle;

/// The listener; dropping it stops listening.
pub struct NetworkWatch(JoinHandle<()>);

impl Drop for NetworkWatch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Calls `on_change` on each network change; `None` where nothing is watched.
/// Needs a Tokio runtime.
#[cfg(target_os = "linux")]
pub fn watch(on_change: impl Fn() + Send + 'static) -> Option<NetworkWatch> {
    Some(NetworkWatch(tokio::spawn(linux::listen(on_change))))
}

// shortcut: Windows (NotifyIpInterfaceChange) is not wired; its windows crate
// features do not include the IP Helper API. The Gateway's backoff still covers it.
#[cfg(not(target_os = "linux"))]
pub fn watch(_on_change: impl Fn() + Send + 'static) -> Option<NetworkWatch> {
    None
}

/// NetworkManager's CONNECTED_LOCAL, CONNECTED_SITE and CONNECTED_GLOBAL,
/// reached from a state that is not one of them.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn reached_connection(previous: Option<u32>, state: u32) -> bool {
    let connected = |state: u32| (50..=70).contains(&state);
    connected(state) && !previous.is_some_and(connected)
}

#[cfg(target_os = "linux")]
mod linux {
    use std::sync::atomic::{AtomicBool, Ordering};

    use futures_util::StreamExt;

    use super::reached_connection;

    const NETWORK_MANAGER: &str = "org.freedesktop.NetworkManager";
    const LOGIN: &str = "org.freedesktop.login1";
    static LOGGED: AtomicBool = AtomicBool::new(false);

    pub(super) async fn listen(on_change: impl Fn() + Send + 'static) {
        let Ok(system) = zbus::Connection::system().await else {
            // Logged once: a session without a system bus is normal.
            if !LOGGED.swap(true, Ordering::Relaxed) {
                eprintln!("rune: no system bus, so network changes are not watched");
            }
            return;
        };
        let (Ok(network), Ok(login)) = (
            zbus::Proxy::new(
                &system,
                NETWORK_MANAGER,
                "/org/freedesktop/NetworkManager",
                NETWORK_MANAGER,
            )
            .await,
            zbus::Proxy::new(
                &system,
                LOGIN,
                "/org/freedesktop/login1",
                "org.freedesktop.login1.Manager",
            )
            .await,
        ) else {
            return;
        };
        let (Ok(mut states), Ok(mut sleeps)) = (
            network.receive_signal("StateChanged").await,
            login.receive_signal("PrepareForSleep").await,
        ) else {
            return;
        };
        let mut state = None;
        loop {
            tokio::select! {
                Some(message) = states.next() => {
                    let Ok(next) = message.body().deserialize::<u32>() else { continue };
                    if reached_connection(state, next) {
                        on_change();
                    }
                    state = Some(next);
                }
                // PrepareForSleep(true) goes to sleep; (false) is the resume.
                Some(message) = sleeps.next() => {
                    if message.body().deserialize::<bool>().is_ok_and(|sleeping| !sleeping) {
                        on_change();
                    }
                }
                else => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::reached_connection;

    #[test]
    fn a_connection_is_reported_when_reached_not_while_it_lasts() {
        // Unknown before, then connected: reported.
        assert!(reached_connection(None, 70));
        assert!(reached_connection(Some(20), 50));
        assert!(reached_connection(Some(40), 60));
        // Connected to connected (local to global) is the same connection.
        assert!(!reached_connection(Some(50), 70));
        // Not connected states are never reported.
        for state in [0, 10, 20, 30, 40] {
            assert!(!reached_connection(Some(70), state));
        }
    }
}
