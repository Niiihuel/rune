//! A fake ScreenCast/Camera portal on a private bus: each test answers the
//! requests itself, so permission, denial, cancellation and the session's
//! end are exercised without the real desktop, screen or camera.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use zbus::message::Header;
use zbus::names::OwnedUniqueName;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::Connection;

use super::*;
use crate::portal::tests::bus::Bus;

const WATCHDOG: Duration = Duration::from_secs(5);

/// What the fake portal was asked.
#[derive(Debug)]
enum Seen {
    Method(Call),
    ClosedRequest(OwnedObjectPath),
    ClosedSession(OwnedObjectPath),
}

#[derive(Debug)]
struct Call {
    method: &'static str,
    client: OwnedUniqueName,
    request: Option<OwnedObjectPath>,
    session: Option<OwnedObjectPath>,
    options: HashMap<String, OwnedValue>,
}

struct Shared {
    seen: mpsc::UnboundedSender<Seen>,
    camera: bool,
    /// The far ends of the remotes handed out, to prove they work.
    peers: Mutex<Vec<UnixStream>>,
}

impl Shared {
    async fn call(
        &self,
        method: &'static str,
        session: Option<OwnedObjectPath>,
        mut options: HashMap<String, OwnedValue>,
        connection: &Connection,
        header: &Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let client: OwnedUniqueName = header.sender().unwrap().to_owned().into();
        let sender = client.as_str().trim_start_matches(':').replace('.', "_");
        let token = options
            .remove("handle_token")
            .and_then(|value| String::try_from(value).ok())
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("missing handle token".into()))?;
        let request =
            OwnedObjectPath::try_from(format!("{DESKTOP}/request/{sender}/{token}")).unwrap();
        connection
            .object_server()
            .at(
                request.clone(),
                Request {
                    path: request.clone(),
                    seen: self.seen.clone(),
                },
            )
            .await
            .unwrap();
        if let Some(token) = options
            .get("session_handle_token")
            .and_then(|value| <&str>::try_from(value).ok())
        {
            let session =
                OwnedObjectPath::try_from(format!("{DESKTOP}/session/{sender}/{token}")).unwrap();
            connection
                .object_server()
                .at(
                    session.clone(),
                    Session {
                        path: session,
                        seen: self.seen.clone(),
                    },
                )
                .await
                .unwrap();
        }
        let _ = self.seen.send(Seen::Method(Call {
            method,
            client,
            request: Some(request.clone()),
            session,
            options,
        }));
        Ok(request)
    }

    fn remote(&self, method: &'static str, header: &Header<'_>) -> zbus::zvariant::OwnedFd {
        let (ours, theirs) = UnixStream::pair().unwrap();
        self.peers.lock().unwrap().push(theirs);
        let _ = self.seen.send(Seen::Method(Call {
            method,
            client: header.sender().unwrap().to_owned().into(),
            request: None,
            session: None,
            options: HashMap::new(),
        }));
        std::os::fd::OwnedFd::from(ours).into()
    }
}

struct Cast(Arc<Shared>);

#[zbus::interface(name = "org.freedesktop.portal.ScreenCast")]
impl Cast {
    async fn create_session(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.0
            .call("CreateSession", None, options, connection, &header)
            .await
    }

    async fn select_sources(
        &self,
        session: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.0
            .call("SelectSources", Some(session), options, connection, &header)
            .await
    }

    async fn start(
        &self,
        session: OwnedObjectPath,
        _parent: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.0
            .call("Start", Some(session), options, connection, &header)
            .await
    }

    #[zbus(name = "OpenPipeWireRemote")]
    async fn open_remote(
        &self,
        _session: OwnedObjectPath,
        _options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::zvariant::OwnedFd {
        self.0.remote("OpenPipeWireRemote", &header)
    }

    #[zbus(property)]
    async fn available_source_types(&self) -> u32 {
        MONITOR | WINDOW
    }

    #[zbus(property)]
    async fn available_cursor_modes(&self) -> u32 {
        CURSOR_HIDDEN | CURSOR_EMBEDDED
    }

    // Lowercase on the bus, as the portal spells it.
    #[zbus(property, name = "version")]
    async fn version(&self) -> u32 {
        PERSISTS
    }
}

struct Camera(Arc<Shared>);

#[zbus::interface(name = "org.freedesktop.portal.Camera")]
impl Camera {
    async fn access_camera(
        &self,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.0
            .call("AccessCamera", None, options, connection, &header)
            .await
    }

    #[zbus(name = "OpenPipeWireRemote")]
    async fn open_remote(
        &self,
        _options: HashMap<String, OwnedValue>,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::zvariant::OwnedFd {
        self.0.remote("OpenCameraRemote", &header)
    }

    #[zbus(property)]
    async fn is_camera_present(&self) -> bool {
        self.0.camera
    }
}

struct Request {
    path: OwnedObjectPath,
    seen: mpsc::UnboundedSender<Seen>,
}

#[zbus::interface(name = "org.freedesktop.portal.Request")]
impl Request {
    async fn close(&self) {
        let _ = self.seen.send(Seen::ClosedRequest(self.path.clone()));
    }
}

struct Session {
    path: OwnedObjectPath,
    seen: mpsc::UnboundedSender<Seen>,
}

#[zbus::interface(name = "org.freedesktop.portal.Session")]
impl Session {
    async fn close(&self) {
        let _ = self.seen.send(Seen::ClosedSession(self.path.clone()));
    }
}

struct Fixture {
    bus: Bus,
    service: Connection,
    shared: Arc<Shared>,
    seen: mpsc::UnboundedReceiver<Seen>,
}

impl Fixture {
    async fn start(camera: bool) -> Self {
        let bus = Bus::start().await;
        let service = bus.connect().await;
        let (seen_tx, seen) = mpsc::unbounded_channel();
        let shared = Arc::new(Shared {
            seen: seen_tx,
            camera,
            peers: Mutex::new(Vec::new()),
        });
        let server = service.object_server();
        server.at(DESKTOP, Cast(shared.clone())).await.unwrap();
        server.at(DESKTOP, Camera(shared.clone())).await.unwrap();
        service.request_name(DESTINATION).await.unwrap();
        Self {
            bus,
            service,
            shared,
            seen,
        }
    }

    async fn next(&mut self) -> Seen {
        tokio::time::timeout(WATCHDOG, self.seen.recv())
            .await
            .expect("the portal was asked nothing more")
            .unwrap()
    }

    async fn method(&mut self, name: &str) -> Call {
        match self.next().await {
            Seen::Method(call) if call.method == name => call,
            other => panic!("expected {name}, the portal saw {other:?}"),
        }
    }

    /// Nothing else reached the portal, after anything in flight arrived.
    async fn quiet(&mut self) {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(seen) = self.seen.try_recv() {
            panic!("the portal was still asked {seen:?}");
        }
    }

    async fn respond(&self, call: &Call, status: u32, results: HashMap<&str, Value<'_>>) {
        respond_from(&self.service, call, status, results).await;
    }

    async fn end_session(&self, call: &Call, session: &OwnedObjectPath) {
        self.service
            .emit_signal(
                Some(call.client.as_str()),
                session.as_str(),
                SESSION,
                "Closed",
                &(HashMap::<&str, Value>::new(),),
            )
            .await
            .unwrap();
    }
}

async fn respond_from(
    connection: &Connection,
    call: &Call,
    status: u32,
    results: HashMap<&str, Value<'_>>,
) {
    connection
        .emit_signal(
            Some(call.client.as_str()),
            call.request.as_ref().unwrap().as_str(),
            REQUEST,
            "Response",
            &(status, results),
        )
        .await
        .unwrap();
}

async fn result<T>(task: tokio::task::JoinHandle<T>) -> T {
    tokio::time::timeout(WATCHDOG, task)
        .await
        .expect("the request never finished")
        .unwrap()
}

fn session_of(call: &Call) -> OwnedObjectPath {
    let token = call
        .options
        .get("session_handle_token")
        .and_then(|value| <&str>::try_from(value).ok())
        .unwrap();
    let sender = call
        .client
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    OwnedObjectPath::try_from(format!("{DESKTOP}/session/{sender}/{token}")).unwrap()
}

/// Answer CreateSession and SelectSources as the person would by picking.
async fn up_to_start(portal: &mut Fixture) -> (Call, OwnedObjectPath) {
    let create = portal.method("CreateSession").await;
    let session = session_of(&create);
    portal
        .respond(
            &create,
            0,
            HashMap::from([("session_handle", Value::from(session.as_str()))]),
        )
        .await;
    let select = portal.method("SelectSources").await;
    assert_eq!(select.session.as_ref(), Some(&session));
    portal.respond(&select, 0, HashMap::new()).await;
    (portal.method("Start").await, session)
}

fn picked() -> HashMap<&'static str, Value<'static>> {
    let properties = HashMap::from([
        ("size", Value::from((1920i32, 1080i32))),
        ("source_type", Value::from(MONITOR)),
    ]);
    HashMap::from([("streams", Value::from(vec![(42u32, properties)]))])
}

#[tokio::test]
async fn a_picked_screen_gives_its_node_and_a_working_remote_until_closed() {
    let mut portal = Fixture::start(true).await;
    let (_cancel, signal) = watch::channel(false);
    let client = portal.bus.connect().await;
    let starting = tokio::spawn(ScreenCast::start_on(client, None, None, signal));
    let create = portal.method("CreateSession").await;
    let session = session_of(&create);
    portal
        .respond(
            &create,
            0,
            HashMap::from([("session_handle", Value::from(session.as_str()))]),
        )
        .await;
    let mut select = portal.method("SelectSources").await;
    let mut option = |name: &str| u32::try_from(select.options.remove(name).unwrap()).unwrap();
    assert_eq!(option("types"), MONITOR | WINDOW, "a monitor or a window");
    assert_eq!(
        option("cursor_mode"),
        CURSOR_EMBEDDED,
        "the pointer is drawn in"
    );
    assert_eq!(option("persist_mode"), 2, "remembered until revoked");
    assert!(!bool::try_from(select.options.remove("multiple").unwrap()).unwrap());
    assert!(!select.options.contains_key("restore_token"), "nothing yet");
    portal.respond(&select, 0, HashMap::new()).await;
    let start = portal.method("Start").await;
    let mut chosen = picked();
    chosen.insert("restore_token", Value::from("first-choice"));
    portal.respond(&start, 0, chosen).await;
    portal.method("OpenPipeWireRemote").await;
    let (mut cast, remote) = result(starting).await.unwrap();
    assert_eq!(cast.restore_token(), Some("first-choice"));
    assert_eq!(
        cast.streams(),
        &[ScreenStream {
            node: 42,
            size: Some((1920, 1080)),
            window: false
        }]
    );
    UnixStream::from(remote).write_all(b"pw").unwrap();
    let mut heard = [0; 2];
    portal.shared.peers.lock().unwrap()[0]
        .read_exact(&mut heard)
        .unwrap();
    assert_eq!(&heard, b"pw");
    cast.close().await;
    assert!(matches!(portal.next().await, Seen::ClosedSession(path) if path == session));
}

#[tokio::test]
async fn a_remembered_choice_is_asked_for_again_and_a_malformed_one_is_never_sent() {
    for (given, sent, kept) in [
        (
            Some("first-choice"),
            Some("first-choice"),
            Some("first-choice"),
        ),
        (Some("not a token"), None, None),
    ] {
        let mut portal = Fixture::start(true).await;
        let (_cancel, signal) = watch::channel(false);
        let client = portal.bus.connect().await;
        let starting =
            tokio::spawn(async move { ScreenCast::start_on(client, None, given, signal).await });
        let create = portal.method("CreateSession").await;
        let session = session_of(&create);
        portal
            .respond(
                &create,
                0,
                HashMap::from([("session_handle", Value::from(session.as_str()))]),
            )
            .await;
        let select = portal.method("SelectSources").await;
        let token = select
            .options
            .get("restore_token")
            .and_then(|value| <&str>::try_from(value).ok());
        assert_eq!(token, sent);
        portal.respond(&select, 0, HashMap::new()).await;
        // The portal shares the same screen without a new token: the one
        // given still names it.
        let start = portal.method("Start").await;
        portal.respond(&start, 0, picked()).await;
        portal.method("OpenPipeWireRemote").await;
        let (cast, _remote) = result(starting).await.unwrap();
        assert_eq!(cast.restore_token(), kept);
    }
}

#[tokio::test]
async fn dismissing_or_denying_the_picker_closes_the_session_without_a_remote() {
    for (status, expected) in [(1, CaptureError::Cancelled), (2, CaptureError::Denied)] {
        let mut portal = Fixture::start(true).await;
        let (_cancel, signal) = watch::channel(false);
        let client = portal.bus.connect().await;
        let starting = tokio::spawn(ScreenCast::start_on(client, None, None, signal));
        let (start, session) = up_to_start(&mut portal).await;
        portal.respond(&start, status, HashMap::new()).await;
        assert_eq!(result(starting).await.unwrap_err(), expected);
        assert!(matches!(portal.next().await, Seen::ClosedSession(path) if path == session));
        portal.quiet().await;
        assert!(portal.shared.peers.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn stopping_while_the_picker_is_open_closes_its_request_and_the_session() {
    let mut portal = Fixture::start(true).await;
    let (cancel, signal) = watch::channel(false);
    let client = portal.bus.connect().await;
    let starting = tokio::spawn(ScreenCast::start_on(client, None, None, signal));
    let create = portal.method("CreateSession").await;
    let session = session_of(&create);
    portal
        .respond(
            &create,
            0,
            HashMap::from([("session_handle", Value::from(session.as_str()))]),
        )
        .await;
    let select = portal.method("SelectSources").await;
    cancel.send_replace(true);
    assert!(
        matches!(portal.next().await, Seen::ClosedRequest(path) if Some(&path) == select.request.as_ref())
    );
    assert!(matches!(portal.next().await, Seen::ClosedSession(path) if path == session));
    assert_eq!(result(starting).await.unwrap_err(), CaptureError::Cancelled);
    portal.quiet().await;
}

#[tokio::test]
async fn the_system_ending_the_session_is_reported_and_not_closed_again() {
    let mut portal = Fixture::start(true).await;
    let (_cancel, signal) = watch::channel(false);
    let client = portal.bus.connect().await;
    let starting = tokio::spawn(ScreenCast::start_on(client, None, None, signal));
    let (start, session) = up_to_start(&mut portal).await;
    portal.respond(&start, 0, picked()).await;
    portal.method("OpenPipeWireRemote").await;
    let (mut cast, _remote) = result(starting).await.unwrap();
    portal.end_session(&start, &session).await;
    let ended = tokio::time::timeout(WATCHDOG, cast.ended()).await.unwrap();
    assert_eq!(ended, CaptureError::Closed);
    cast.close().await;
    drop(cast);
    portal.quiet().await;
}

#[tokio::test]
async fn answers_from_anyone_but_the_portal_are_ignored() {
    let mut portal = Fixture::start(true).await;
    let stranger = portal.bus.connect().await;
    let (_cancel, signal) = watch::channel(false);
    let client = portal.bus.connect().await;
    let starting = tokio::spawn(ScreenCast::start_on(client, None, None, signal));
    let (start, _) = up_to_start(&mut portal).await;
    respond_from(&stranger, &start, 0, picked()).await;
    portal.respond(&start, 1, HashMap::new()).await;
    assert_eq!(result(starting).await.unwrap_err(), CaptureError::Cancelled);
}

#[tokio::test]
async fn a_screen_without_a_portal_on_the_bus_is_unavailable() {
    let bus = Bus::start().await;
    let (_cancel, signal) = watch::channel(false);
    let error = ScreenCast::start_on(bus.connect().await, None, None, signal)
        .await
        .unwrap_err();
    assert_eq!(error, CaptureError::Unavailable(NO_PORTAL));
}

#[tokio::test]
async fn no_camera_is_unavailable_without_asking_for_access() {
    let mut portal = Fixture::start(false).await;
    let (_cancel, signal) = watch::channel(false);
    let client = portal.bus.connect().await;
    assert!(!camera_present_on(client.clone(), signal.clone())
        .await
        .unwrap());
    let error = open_camera_on(client, signal).await.unwrap_err();
    assert_eq!(error, CaptureError::Unavailable(NO_DEVICE));
    portal.quiet().await;
}

#[tokio::test]
async fn a_denied_or_dismissed_camera_opens_no_remote() {
    for (status, expected) in [(2, CaptureError::Denied), (1, CaptureError::Cancelled)] {
        let mut portal = Fixture::start(true).await;
        let (_cancel, signal) = watch::channel(false);
        let client = portal.bus.connect().await;
        let opening = tokio::spawn(open_camera_on(client, signal));
        let access = portal.method("AccessCamera").await;
        portal.respond(&access, status, HashMap::new()).await;
        assert_eq!(result(opening).await.unwrap_err(), expected);
        portal.quiet().await;
        assert!(portal.shared.peers.lock().unwrap().is_empty());
    }
}

#[tokio::test]
async fn a_granted_camera_gives_a_working_remote() {
    let mut portal = Fixture::start(true).await;
    let (_cancel, signal) = watch::channel(false);
    let client = portal.bus.connect().await;
    let opening = tokio::spawn(open_camera_on(client, signal));
    let access = portal.method("AccessCamera").await;
    portal.respond(&access, 0, HashMap::new()).await;
    portal.method("OpenCameraRemote").await;
    let remote = result(opening).await.unwrap();
    UnixStream::from(remote).write_all(b"cam").unwrap();
    let mut heard = [0; 3];
    portal.shared.peers.lock().unwrap()[0]
        .read_exact(&mut heard)
        .unwrap();
    assert_eq!(&heard, b"cam");
}

#[tokio::test]
async fn cancelling_the_camera_prompt_closes_its_request() {
    let mut portal = Fixture::start(true).await;
    let (cancel, signal) = watch::channel(false);
    let client = portal.bus.connect().await;
    let opening = tokio::spawn(open_camera_on(client, signal));
    let access = portal.method("AccessCamera").await;
    cancel.send_replace(true);
    assert!(
        matches!(portal.next().await, Seen::ClosedRequest(path) if Some(&path) == access.request.as_ref())
    );
    assert_eq!(result(opening).await.unwrap_err(), CaptureError::Cancelled);
    portal.quiet().await;
}

#[tokio::test]
async fn dropping_a_shared_screen_still_releases_its_session() {
    let mut portal = Fixture::start(true).await;
    let (_cancel, signal) = watch::channel(false);
    let client = portal.bus.connect().await;
    let starting = tokio::spawn(ScreenCast::start_on(client, None, None, signal));
    let (start, session) = up_to_start(&mut portal).await;
    portal.respond(&start, 0, picked()).await;
    portal.method("OpenPipeWireRemote").await;
    let (cast, _remote) = result(starting).await.unwrap();
    drop(cast);
    assert!(matches!(portal.next().await, Seen::ClosedSession(path) if path == session));
}
