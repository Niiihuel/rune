//! The `org.freedesktop.Notifications` client, on its own thread.
//!
//! It connects when first asked to show something, and again after the
//! service goes away. Clicks are the service's `ActionInvoked` signals for
//! the default action, and presses of the buttons the notification offered
//! (llamadas-y-panel-de-voz), accepted only from the service's current
//! owner and only for a notification this client showed.

use rune_i18n::t;
use std::{collections::HashMap, collections::VecDeque, time::Duration};

use futures_util::StreamExt;
use tokio::time::timeout;
use zbus::{message::Type, zvariant::Value, Connection, MatchRule, MessageStream};

use super::{Click, Notice, Request};

const SERVICE: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const RPC_TIMEOUT: Duration = Duration::from_secs(2);
/// Notifications remembered to route a click; the oldest are forgotten,
/// a call's last: it must be found again to close it.
const REMEMBERED: usize = 32;

/// A notification shown: the service's ID, the caller's key, the actions
/// of the buttons it offered, and whether it is a call ringing.
struct Shown {
    id: u32,
    key: u64,
    buttons: Vec<&'static str>,
    ringing: bool,
}

pub(super) fn spawn(
    address: Option<String>,
    mut requests: tokio::sync::mpsc::Receiver<Request>,
    report: impl Fn(Click) + Send + 'static,
) {
    // A thread that cannot start drops the receiver: showing then fails.
    let _ = std::thread::Builder::new()
        .name("rune-notifications".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(run(address, &mut requests, &report));
        });
}

struct Service {
    connection: Connection,
    signals: MessageStream,
}

async fn run(
    address: Option<String>,
    requests: &mut tokio::sync::mpsc::Receiver<Request>,
    report: &impl Fn(Click),
) {
    let mut service: Option<Service> = None;
    // What is shown, by the service's IDs.
    let mut shown: VecDeque<Shown> = VecDeque::new();
    loop {
        tokio::select! {
            request = requests.recv() => {
                let Some(request) = request else {
                    // RUNE is going: a call's notification, which never
                    // expires, does not outlive it.
                    if let Some(active) = &service {
                        for known in shown.iter().filter(|known| known.ringing) {
                            close(&active.connection, known.id).await;
                        }
                    }
                    break;
                };
                if service.is_none() {
                    service = connect(address.as_deref()).await;
                }
                let Some(active) = &service else { continue };
                let close = |id: u32| close(&active.connection, id);
                match request {
                    Request::Show(notice) => match notify(&active.connection, &notice).await {
                        Some(id) => {
                            shown.retain(|known| known.id != id);
                            shown.push_back(Shown {
                                id,
                                key: notice.key,
                                buttons: notice.buttons.iter().map(|(action, _)| *action).collect(),
                                ringing: notice.ringing,
                            });
                            if shown.len() > REMEMBERED {
                                let oldest = shown.iter().position(|known| !known.ringing);
                                shown.remove(oldest.unwrap_or(0));
                            }
                        }
                        // Try a fresh connection next time.
                        None => service = None,
                    },
                    Request::Withdraw => {
                        for known in shown.drain(..) {
                            close(known.id).await;
                        }
                    }
                    Request::Close(key) => {
                        let ids: Vec<u32> = shown.iter().filter(|known| known.key == key).map(|known| known.id).collect();
                        shown.retain(|known| known.key != key);
                        for id in ids {
                            close(id).await;
                        }
                    }
                }
            }
            message = async {
                match service.as_mut() {
                    Some(active) => active.signals.next().await,
                    None => std::future::pending().await,
                }
            } => {
                let Some(Ok(message)) = message else {
                    service = None;
                    continue;
                };
                let header = message.header();
                match header.member().map(|member| member.as_str()) {
                    Some("ActionInvoked") => {
                        let Ok((id, action)) = message.body().deserialize::<(u32, String)>() else {
                            continue;
                        };
                        let Some(known) = shown.iter().find(|known| known.id == id) else {
                            continue;
                        };
                        if action == "default" {
                            report((known.key, None));
                        } else if let Some(button) = known.buttons.iter().find(|button| **button == action) {
                            report((known.key, Some(*button)));
                        }
                    }
                    Some("NotificationClosed") => {
                        if let Ok((id, _)) = message.body().deserialize::<(u32, u32)>() {
                            shown.retain(|known| known.id != id);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// The session bus (or `address`), subscribed to the service's signals.
async fn connect(address: Option<&str>) -> Option<Service> {
    let builder = match address {
        Some(address) => zbus::connection::Builder::address(address),
        None => zbus::connection::Builder::session(),
    }
    .ok()?;
    let connection = timeout(
        RPC_TIMEOUT,
        builder.method_timeout(RPC_TIMEOUT).max_queued(16).build(),
    )
    .await
    .ok()?
    .ok()?;
    let owner = timeout(RPC_TIMEOUT, async {
        zbus::fdo::DBusProxy::new(&connection)
            .await?
            .get_name_owner(SERVICE.try_into().unwrap())
            .await
    })
    .await
    .ok()?
    .ok()?;
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(owner.as_str())
        .ok()?
        .path(PATH)
        .ok()?
        .interface(SERVICE)
        .ok()?
        .build();
    let signals = timeout(
        RPC_TIMEOUT,
        MessageStream::for_match_rule(rule, &connection, Some(16)),
    )
    .await
    .ok()?
    .ok()?;
    Some(Service {
        connection,
        signals,
    })
}

/// Show `notice` with a default action, so a click is reported, and its
/// buttons; its ID. A call ringing is a critical `call.incoming`, which
/// daemons keep on screen until it is closed.
async fn notify(connection: &Connection, notice: &Notice) -> Option<u32> {
    let mut hints: HashMap<&str, Value<'_>> = HashMap::new();
    if notice.ringing {
        hints.insert("category", Value::from("call.incoming"));
        hints.insert("urgency", Value::from(2u8));
    } else {
        hints.insert("category", Value::from("im.received"));
    }
    // RUNE's icon (barra-e-icono): its entry, as `rune --install` leaves it.
    hints.insert("desktop-entry", Value::from("rune"));
    let mut actions = vec!["default", t("Abrir")];
    for (action, label) in &notice.buttons {
        actions.extend([*action, label.as_str()]);
    }
    let reply = timeout(
        RPC_TIMEOUT,
        connection.call_method(
            Some(SERVICE),
            PATH,
            Some(SERVICE),
            "Notify",
            &(
                "RUNE",
                0u32,
                // The icon of the theme, installed with the entry.
                "rune",
                notice.title.as_str(),
                escape(&notice.body),
                actions,
                hints,
                // A ring never expires by itself: it is closed when it stops.
                if notice.ringing { 0i32 } else { -1i32 },
            ),
        ),
    )
    .await
    .ok()?
    .ok()?;
    reply.body().deserialize::<u32>().ok()
}

/// Close the notification `id`; one already gone is no matter.
async fn close(connection: &Connection, id: u32) {
    let _ = timeout(
        RPC_TIMEOUT,
        connection.call_method(
            Some(SERVICE),
            PATH,
            Some(SERVICE),
            "CloseNotification",
            &(id,),
        ),
    )
    .await;
}

/// Servers may read the body as markup: what someone wrote stays text.
fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
