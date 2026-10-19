//! A fake notification daemon on a private bus; never the session bus.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;
use zbus::object_server::SignalEmitter;
use zbus::zvariant::OwnedValue;
use zbus::Connection;

use super::*;
use crate::notifications::{Notice, Notifier};
use crate::portal::tests::bus::Bus;

const WATCHDOG: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq)]
enum Seen {
    Notified {
        id: u32,
        summary: String,
        body: String,
        actions: Vec<String>,
        /// Said to be a call ringing: `call.incoming`, critical, no expiry.
        ringing: bool,
        /// The icon named, and the desktop entry hinted.
        icon: (String, Option<String>),
    },
    Closed(u32),
}

struct Daemon {
    next: AtomicU32,
    seen: mpsc::UnboundedSender<Seen>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Daemon {
    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        _app: String,
        _replaces: u32,
        icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        timeout: i32,
    ) -> u32 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let category = hints.get("category").map(|value| &**value);
        let urgency = hints.get("urgency").map(|value| &**value);
        let ringing = matches!(category, Some(zbus::zvariant::Value::Str(name)) if name.as_str() == "call.incoming")
            && matches!(urgency, Some(zbus::zvariant::Value::U8(2)))
            && timeout == 0;
        let entry = match hints.get("desktop-entry").map(|value| &**value) {
            Some(zbus::zvariant::Value::Str(entry)) => Some(entry.to_string()),
            _ => None,
        };
        let _ = self.seen.send(Seen::Notified {
            id,
            summary,
            body,
            actions,
            ringing,
            icon: (icon, entry),
        });
        id
    }

    async fn close_notification(&self, id: u32) {
        let _ = self.seen.send(Seen::Closed(id));
    }

    #[zbus(signal)]
    async fn action_invoked(emitter: &SignalEmitter<'_>, id: u32, action: &str)
        -> zbus::Result<()>;
}

async fn daemon(bus: &Bus) -> (Connection, mpsc::UnboundedReceiver<Seen>) {
    let service = bus.connect().await;
    let (seen, received) = mpsc::unbounded_channel();
    service
        .object_server()
        .at(
            PATH,
            Daemon {
                next: AtomicU32::new(0),
                seen,
            },
        )
        .await
        .unwrap();
    service.request_name(SERVICE).await.unwrap();
    (service, received)
}

async fn seen(received: &mut mpsc::UnboundedReceiver<Seen>) -> Seen {
    tokio::time::timeout(WATCHDOG, received.recv())
        .await
        .expect("the daemon saw nothing")
        .unwrap()
}

async fn click(notifier: &Notifier) -> crate::notifications::Click {
    tokio::time::timeout(WATCHDOG, async {
        loop {
            if let Some(click) = notifier.poll_click() {
                return click;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("no click arrived")
}

#[tokio::test]
async fn a_click_on_a_shown_notification_returns_only_its_key() {
    let bus = Bus::start().await;
    let (service, mut received) = daemon(&bus).await;
    let notifier = Notifier::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    assert!(notifier.show(Notice::new(77, "Ana en #general", "<b>hola</b> & chau")));
    let Seen::Notified {
        id,
        summary,
        body,
        actions,
        ringing,
        icon,
    } = seen(&mut received).await
    else {
        panic!("a notification")
    };
    assert_eq!(summary, "Ana en #general");
    assert_eq!(icon, ("rune".to_owned(), Some("rune".to_owned())));
    assert_eq!(
        body, "&lt;b&gt;hola&lt;/b&gt; &amp; chau",
        "text, never markup"
    );
    assert_eq!(actions, ["default", "Abrir"]);
    assert!(!ringing);
    let emitter = SignalEmitter::new(&service, PATH).unwrap();
    // Another action, and an ID this client never showed, are ignored.
    Daemon::action_invoked(&emitter, id, "other").await.unwrap();
    Daemon::action_invoked(&emitter, id + 100, "default")
        .await
        .unwrap();
    Daemon::action_invoked(&emitter, id, "default")
        .await
        .unwrap();
    assert_eq!(click(&notifier).await, (77, None));
    assert_eq!(notifier.poll_click(), None);
    // A stranger on the bus cannot fake a click.
    let stranger = bus.connect().await;
    let forged = SignalEmitter::new(&stranger, PATH).unwrap();
    Daemon::action_invoked(&forged, id, "default")
        .await
        .unwrap();
    notifier.withdraw_all();
    assert_eq!(seen(&mut received).await, Seen::Closed(id));
    assert_eq!(notifier.poll_click(), None);
}

#[tokio::test]
async fn a_ringing_call_offers_its_buttons_stays_and_is_withdrawn_alone() {
    let bus = Bus::start().await;
    let (service, mut received) = daemon(&bus).await;
    let notifier = Notifier::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    assert!(notifier.show(Notice::new(1, "Ana", "hola")));
    let Seen::Notified { id: message, .. } = seen(&mut received).await else {
        panic!("a notification")
    };
    let call = Notice::new(2, "Llamada entrante", "Ada te está llamando")
        .ringing(&[("accept", "Aceptar"), ("decline", "Rechazar")]);
    assert!(notifier.show(call));
    let Seen::Notified {
        id,
        actions,
        ringing,
        ..
    } = seen(&mut received).await
    else {
        panic!("a notification")
    };
    assert_eq!(
        actions,
        ["default", "Abrir", "accept", "Aceptar", "decline", "Rechazar"]
    );
    assert!(ringing, "call.incoming, critical, never expiring");
    let emitter = SignalEmitter::new(&service, PATH).unwrap();
    // Only its own buttons come back, as themselves; a message has none.
    Daemon::action_invoked(&emitter, message, "accept")
        .await
        .unwrap();
    Daemon::action_invoked(&emitter, id, "hang-up")
        .await
        .unwrap();
    Daemon::action_invoked(&emitter, id, "decline")
        .await
        .unwrap();
    assert_eq!(click(&notifier).await, (2, Some("decline")));
    Daemon::action_invoked(&emitter, id, "accept")
        .await
        .unwrap();
    assert_eq!(click(&notifier).await, (2, Some("accept")));
    assert_eq!(notifier.poll_click(), None);
    // When it stops ringing it goes, and the message stays.
    notifier.withdraw(2);
    assert_eq!(seen(&mut received).await, Seen::Closed(id));
    notifier.withdraw(2);
    notifier.withdraw_all();
    assert_eq!(seen(&mut received).await, Seen::Closed(message));
}

#[tokio::test]
async fn a_call_still_ringing_is_closed_when_rune_goes() {
    let bus = Bus::start().await;
    let (_service, mut received) = daemon(&bus).await;
    let notifier = Notifier::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    let call = Notice::new(1, "Llamada entrante", "Ada te está llamando")
        .ringing(&[("accept", "Aceptar"), ("decline", "Rechazar")]);
    assert!(notifier.show(call));
    let Seen::Notified { id, .. } = seen(&mut received).await else {
        panic!("a notification")
    };
    assert!(notifier.show(Notice::new(2, "Ana", "hola")));
    seen(&mut received).await;
    drop(notifier);
    // Only the call goes: a message's notification expires by itself.
    assert_eq!(seen(&mut received).await, Seen::Closed(id));
    let more = tokio::time::timeout(Duration::from_millis(200), received.recv()).await;
    assert!(more.is_err(), "{more:?}");
}

#[tokio::test]
async fn without_a_daemon_showing_fails_quietly() {
    let bus = Bus::start().await;
    let notifier = Notifier::spawn(Some(bus.address.clone()), Arc::new(|| {}));
    assert!(notifier.show(Notice::new(1, "a", "b")));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(notifier.poll_click(), None);
}
