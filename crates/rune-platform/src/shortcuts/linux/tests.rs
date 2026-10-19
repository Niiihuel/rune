//! A fake GlobalShortcuts portal on a private bus; never the session bus.

use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc,
};
use std::time::Duration;

use tokio::sync::mpsc;
use zbus::message::Header;
use zbus::names::OwnedUniqueName;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::Connection;

use super::*;
use crate::portal::tests::bus::Bus;
use crate::shortcuts::GlobalShortcuts;

const WATCHDOG: Duration = Duration::from_secs(5);

#[derive(Debug)]
enum Seen {
    Created {
        session: OwnedObjectPath,
        client: OwnedUniqueName,
    },
    Bound(Described),
    Closed(OwnedObjectPath),
}

struct Fake {
    seen: mpsc::UnboundedSender<Seen>,
    bind_status: Arc<AtomicU32>,
}

fn request_path(
    header: &Header<'_>,
    options: &HashMap<String, OwnedValue>,
) -> (OwnedUniqueName, String, OwnedObjectPath) {
    let client: OwnedUniqueName = header.sender().unwrap().to_owned().into();
    let sender = client.as_str().trim_start_matches(':').replace('.', "_");
    let token = String::try_from(options["handle_token"].try_clone().unwrap()).unwrap();
    let path = OwnedObjectPath::try_from(format!("{DESKTOP}/request/{sender}/{token}")).unwrap();
    (client, sender, path)
}

/// Answer a request after its method returns, as a real portal does.
fn respond(
    connection: &Connection,
    client: OwnedUniqueName,
    path: OwnedObjectPath,
    status: u32,
    results: HashMap<&'static str, OwnedValue>,
) {
    let connection = connection.clone();
    tokio::spawn(async move {
        connection
            .emit_signal(
                Some(client.as_str()),
                path.as_str(),
                REQUEST,
                "Response",
                &(status, results),
            )
            .await
            .unwrap();
    });
}

#[zbus::interface(name = "org.freedesktop.portal.GlobalShortcuts")]
impl Fake {
    #[zbus(property, name = "version")]
    async fn version(&self) -> u32 {
        1
    }

    async fn create_session(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let (client, sender, path) = request_path(&header, &options);
        let token = String::try_from(options["session_handle_token"].try_clone().unwrap()).unwrap();
        let session =
            OwnedObjectPath::try_from(format!("{DESKTOP}/session/{sender}/{token}")).unwrap();
        connection
            .object_server()
            .at(
                session.clone(),
                FakeSession {
                    path: session.clone(),
                    seen: self.seen.clone(),
                },
            )
            .await
            .unwrap();
        respond(
            connection,
            client.clone(),
            path.clone(),
            0,
            HashMap::from([(
                "session_handle",
                OwnedValue::try_from(Value::from(session.as_str())).unwrap(),
            )]),
        );
        self.seen.send(Seen::Created { session, client }).unwrap();
        Ok(path)
    }

    async fn bind_shortcuts(
        &self,
        _session: OwnedObjectPath,
        shortcuts: Described,
        _parent: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let (client, _, path) = request_path(&header, &options);
        let bound: Vec<(String, HashMap<String, Value>)> = shortcuts
            .iter()
            .map(|(id, _)| {
                (
                    id.clone(),
                    HashMap::from([(
                        "trigger_description".to_owned(),
                        Value::from("Ctrl+Shift+T"),
                    )]),
                )
            })
            .collect();
        respond(
            connection,
            client,
            path.clone(),
            self.bind_status.load(Ordering::Acquire),
            HashMap::from([(
                "shortcuts",
                OwnedValue::try_from(Value::from(bound)).unwrap(),
            )]),
        );
        self.seen.send(Seen::Bound(shortcuts)).unwrap();
        Ok(path)
    }
}

struct FakeSession {
    path: OwnedObjectPath,
    seen: mpsc::UnboundedSender<Seen>,
}

#[zbus::interface(name = "org.freedesktop.portal.Session")]
impl FakeSession {
    async fn close(&self) {
        let _ = self.seen.send(Seen::Closed(self.path.clone()));
    }
}

struct Portal {
    bus: Bus,
    service: Connection,
    seen: mpsc::UnboundedReceiver<Seen>,
    bind_status: Arc<AtomicU32>,
}

impl Portal {
    async fn start() -> Self {
        let bus = Bus::start().await;
        let service = bus.connect().await;
        let (seen_tx, seen) = mpsc::unbounded_channel();
        let bind_status = Arc::new(AtomicU32::new(0));
        service
            .object_server()
            .at(
                DESKTOP,
                Fake {
                    seen: seen_tx,
                    bind_status: bind_status.clone(),
                },
            )
            .await
            .unwrap();
        service.request_name(DESTINATION).await.unwrap();
        Self {
            bus,
            service,
            seen,
            bind_status,
        }
    }

    async fn seen(&mut self) -> Seen {
        tokio::time::timeout(WATCHDOG, self.seen.recv())
            .await
            .expect("the portal saw nothing")
            .unwrap()
    }

    /// The created session, once bound.
    async fn bound(&mut self) -> (OwnedObjectPath, OwnedUniqueName, Described) {
        let Seen::Created { session, client } = self.seen().await else {
            panic!("no session")
        };
        let Seen::Bound(shortcuts) = self.seen().await else {
            panic!("no binding")
        };
        (session, client, shortcuts)
    }

    async fn key(&self, from: &Connection, member: &str, session: &str, id: &str) {
        from.emit_signal(
            None::<&str>,
            DESKTOP,
            GLOBAL_SHORTCUTS,
            member,
            &(
                OwnedObjectPath::try_from(session).unwrap(),
                id,
                0u64,
                HashMap::<&str, Value>::new(),
            ),
        )
        .await
        .unwrap();
    }
}

fn push_to_talk() -> Vec<Shortcut> {
    vec![Shortcut {
        id: "push-to-talk".into(),
        description: "Pulsar para hablar".into(),
        preferred_trigger: Some("CTRL+SHIFT+t".into()),
    }]
}

fn start(portal: &Bus) -> GlobalShortcuts {
    GlobalShortcuts::spawn(
        Some(portal.address.clone()),
        push_to_talk(),
        Arc::new(|| {}),
    )
}

async fn next(shortcuts: &GlobalShortcuts) -> ShortcutEvent {
    tokio::time::timeout(WATCHDOG, async {
        loop {
            if let Some(event) = shortcuts.poll() {
                return event;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("the session reported nothing")
}

#[tokio::test]
async fn presses_come_only_from_the_portal_for_this_session_and_drop_closes_it() {
    let mut portal = Portal::start().await;
    let shortcuts = start(&portal.bus);
    let (session, _, asked) = portal.bound().await;
    assert_eq!(asked.len(), 1);
    let (id, properties) = &asked[0];
    assert_eq!(id, "push-to-talk");
    assert_eq!(
        String::try_from(properties["preferred_trigger"].try_clone().unwrap()).unwrap(),
        "CTRL+SHIFT+t"
    );
    assert_eq!(
        String::try_from(properties["description"].try_clone().unwrap()).unwrap(),
        "Pulsar para hablar"
    );
    assert_eq!(
        next(&shortcuts).await,
        ShortcutEvent::Bound(vec![("push-to-talk".into(), "Ctrl+Shift+T".into())])
    );
    let stranger = portal.bus.connect().await;
    portal
        .key(&stranger, "Activated", session.as_str(), "push-to-talk")
        .await;
    portal
        .key(
            &portal.service,
            "Activated",
            "/org/freedesktop/portal/desktop/session/other/x",
            "push-to-talk",
        )
        .await;
    portal
        .key(&portal.service, "Activated", session.as_str(), "unknown")
        .await;
    portal
        .key(
            &portal.service,
            "Activated",
            session.as_str(),
            "push-to-talk",
        )
        .await;
    assert_eq!(
        next(&shortcuts).await,
        ShortcutEvent::Pressed("push-to-talk".into()),
        "only the real press counts"
    );
    portal
        .key(
            &portal.service,
            "Deactivated",
            session.as_str(),
            "push-to-talk",
        )
        .await;
    assert_eq!(
        next(&shortcuts).await,
        ShortcutEvent::Released("push-to-talk".into())
    );
    drop(shortcuts);
    let Seen::Closed(closed) = portal.seen().await else {
        panic!("dropping did not close the session")
    };
    assert_eq!(closed, session);
}

#[tokio::test]
async fn a_session_the_desktop_closes_ends_with_its_reason() {
    let mut portal = Portal::start().await;
    let shortcuts = start(&portal.bus);
    let (session, client, _) = portal.bound().await;
    assert!(matches!(next(&shortcuts).await, ShortcutEvent::Bound(_)));
    portal
        .service
        .emit_signal(
            Some(client.as_str()),
            session.as_str(),
            SESSION,
            "Closed",
            &(HashMap::<&str, Value>::new(),),
        )
        .await
        .unwrap();
    assert_eq!(
        next(&shortcuts).await,
        ShortcutEvent::Ended(ShortcutError::CLOSED)
    );
}

#[tokio::test]
async fn a_vanished_portal_ends_the_session_so_a_held_key_is_released() {
    let mut portal = Portal::start().await;
    let shortcuts = start(&portal.bus);
    let (session, _, _) = portal.bound().await;
    assert!(matches!(next(&shortcuts).await, ShortcutEvent::Bound(_)));
    portal
        .key(
            &portal.service,
            "Activated",
            session.as_str(),
            "push-to-talk",
        )
        .await;
    assert_eq!(
        next(&shortcuts).await,
        ShortcutEvent::Pressed("push-to-talk".into())
    );
    portal.service.release_name(DESTINATION).await.unwrap();
    assert_eq!(
        next(&shortcuts).await,
        ShortcutEvent::Ended(ShortcutError::CLOSED)
    );
}

#[tokio::test]
async fn a_refused_binding_ends_as_denied_and_closes_its_session() {
    let mut portal = Portal::start().await;
    portal.bind_status.store(1, Ordering::Release);
    let shortcuts = start(&portal.bus);
    let (session, _, _) = portal.bound().await;
    assert_eq!(
        next(&shortcuts).await,
        ShortcutEvent::Ended(ShortcutError::DENIED)
    );
    let Seen::Closed(closed) = portal.seen().await else {
        panic!("a refusal left the session open")
    };
    assert_eq!(closed, session);
}

#[tokio::test]
async fn a_portal_without_the_interface_reads_as_unsupported() {
    let bus = Bus::start().await;
    let service = bus.connect().await;
    // A desktop object with some other portal interface, as a compositor
    // without global shortcuts serves it.
    let (seen, _) = mpsc::unbounded_channel();
    service
        .object_server()
        .at(
            DESKTOP,
            FakeSession {
                path: OwnedObjectPath::try_from(DESKTOP).unwrap(),
                seen,
            },
        )
        .await
        .unwrap();
    service.request_name(DESTINATION).await.unwrap();
    let shortcuts = start(&bus);
    assert_eq!(
        next(&shortcuts).await,
        ShortcutEvent::Ended(ShortcutError::UNSUPPORTED)
    );
}

#[tokio::test]
async fn a_bus_without_a_portal_reads_as_no_portal() {
    let bus = Bus::start().await;
    let shortcuts = start(&bus);
    assert_eq!(
        next(&shortcuts).await,
        ShortcutEvent::Ended(ShortcutError::NO_PORTAL)
    );
}
