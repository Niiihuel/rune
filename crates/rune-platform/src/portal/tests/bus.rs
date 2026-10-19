//! An isolated bus with no activation directories or access to the desktop portal.

use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use zbus::message::Header;
use zbus::names::OwnedUniqueName;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::Connection;

use super::super::{DESTINATION, REQUEST};

const WATCHDOG: Duration = Duration::from_secs(5);

pub(crate) struct Bus {
    _directory: tempfile::TempDir,
    child: Child,
    pub address: String,
}

impl Bus {
    pub async fn start() -> Self {
        Self::start_with(|_| String::new()).await
    }

    /// A bus that starts `name` on demand by running `exec`, as a session
    /// bus starts the portal.
    pub async fn activating(name: &str, exec: &str) -> Self {
        Self::start_with(|directory| {
            std::fs::write(
                directory.join(format!("{name}.service")),
                format!("[D-BUS Service]\nName={name}\nExec={exec}\n"),
            )
            .unwrap();
            format!("<servicedir>{}</servicedir>", directory.display())
        })
        .await
    }

    async fn start_with(extra: impl FnOnce(&std::path::Path) -> String) -> Self {
        let directory = tempfile::Builder::new()
            .prefix("rune-filechooser-test-")
            .tempdir()
            .unwrap();
        let config = directory.path().join("bus.conf");
        let extra = extra(directory.path());
        std::fs::write(&config, format!("<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><auth>EXTERNAL</auth>{extra}<policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>")).unwrap();
        let mut child = Command::new(option_env!("RUNE_TEST_DBUS_DAEMON").unwrap_or("dbus-daemon"))
            .env(
                "RUNE_FILE_DIALOG_TEST_BUS",
                "rune-platform-portal-tests-20261015",
            )
            .arg("--config-file")
            .arg(config)
            .args(["--nofork", "--nopidfile", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .expect("the test environment needs dbus-daemon for the private bus");
        let mut output = BufReader::new(child.stdout.take().unwrap()).lines();
        let address = tokio::time::timeout(WATCHDOG, output.next_line())
            .await
            .expect("private bus startup timed out")
            .unwrap()
            .expect("private bus produced no address");
        Self {
            _directory: directory,
            child,
            address,
        }
    }

    pub async fn connect(&self) -> Connection {
        tokio::time::timeout(
            WATCHDOG,
            zbus::connection::Builder::address(self.address.as_str())
                .unwrap()
                .method_timeout(Duration::from_secs(2))
                .max_queued(8)
                .build(),
        )
        .await
        .unwrap()
        .unwrap()
    }

    pub fn stop(&mut self) {
        self.child.start_kill().unwrap();
    }
}

pub(super) struct OpenCall {
    pub method: &'static str,
    pub path: OwnedObjectPath,
    pub client: OwnedUniqueName,
    pub parent: String,
    pub options: HashMap<String, OwnedValue>,
    reply: Option<oneshot::Sender<OwnedObjectPath>>,
}

impl OpenCall {
    pub fn complete(&mut self) {
        self.complete_as(self.path.clone());
    }

    pub fn complete_as(&mut self, path: OwnedObjectPath) {
        self.reply
            .take()
            .unwrap()
            .send(path)
            .expect("OpenFile is still waiting for its reply");
    }
}

struct Chooser {
    calls: mpsc::Sender<OpenCall>,
    closed: mpsc::Sender<OwnedObjectPath>,
    fail_close: Arc<AtomicBool>,
}

#[zbus::interface(name = "org.freedesktop.portal.FileChooser")]
impl Chooser {
    async fn open_file(
        &self,
        parent: String,
        _title: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.file_call("OpenFile", parent, options, connection, header)
            .await
    }

    async fn save_file(
        &self,
        parent: String,
        _title: String,
        options: HashMap<String, OwnedValue>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        self.file_call("SaveFile", parent, options, connection, header)
            .await
    }
}

impl Chooser {
    async fn file_call(
        &self,
        method: &'static str,
        parent: String,
        mut options: HashMap<String, OwnedValue>,
        connection: &Connection,
        header: Header<'_>,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        let token = options
            .remove("handle_token")
            .and_then(|value| String::try_from(value).ok())
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("missing handle token".into()))?;
        let client: OwnedUniqueName = header.sender().unwrap().to_owned().into();
        let sender = client.as_str().trim_start_matches(':').replace('.', "_");
        let path = OwnedObjectPath::try_from(format!(
            "/org/freedesktop/portal/desktop/request/{sender}/{token}"
        ))
        .unwrap();
        connection
            .object_server()
            .at(
                path.clone(),
                Request {
                    path: path.clone(),
                    closed: self.closed.clone(),
                    fail_close: self.fail_close.clone(),
                },
            )
            .await
            .unwrap();
        let (reply, received) = oneshot::channel();
        self.calls
            .send(OpenCall {
                method,
                path: path.clone(),
                client,
                parent,
                options,
                reply: Some(reply),
            })
            .await
            .unwrap();
        received
            .await
            .map_err(|_| zbus::fdo::Error::Failed("test dropped OpenFile reply".into()))
    }
}

struct Request {
    path: OwnedObjectPath,
    closed: mpsc::Sender<OwnedObjectPath>,
    fail_close: Arc<AtomicBool>,
}

#[zbus::interface(name = "org.freedesktop.portal.Request")]
impl Request {
    async fn close(&self) -> zbus::fdo::Result<()> {
        self.closed.send(self.path.clone()).await.unwrap();
        if self.fail_close.load(Ordering::Acquire) {
            Err(zbus::fdo::Error::Failed("synthetic Close failure".into()))
        } else {
            Ok(())
        }
    }
}

pub(super) struct Portal {
    pub bus: Bus,
    pub service: Connection,
    pub client: Connection,
    calls: mpsc::Receiver<OpenCall>,
    closed: mpsc::Receiver<OwnedObjectPath>,
    fail_close: Arc<AtomicBool>,
}

impl Portal {
    pub async fn start() -> Self {
        let bus = Bus::start().await;
        let service = bus.connect().await;
        let (calls_tx, calls) = mpsc::channel(4);
        let (closed_tx, closed) = mpsc::channel(4);
        let fail_close = Arc::new(AtomicBool::new(false));
        service
            .object_server()
            .at(
                "/org/freedesktop/portal/desktop",
                Chooser {
                    calls: calls_tx,
                    closed: closed_tx,
                    fail_close: fail_close.clone(),
                },
            )
            .await
            .unwrap();
        service.request_name(DESTINATION).await.unwrap();
        let client = bus.connect().await;
        Self {
            bus,
            service,
            client,
            calls,
            closed,
            fail_close,
        }
    }

    pub async fn opened(&mut self) -> OpenCall {
        tokio::time::timeout(WATCHDOG, self.calls.recv())
            .await
            .expect("the chooser never called OpenFile")
            .expect("portal dropped its call observer")
    }

    pub fn no_open(&mut self) {
        assert!(self.calls.try_recv().is_err());
    }

    pub fn fail_closes(&self) {
        self.fail_close.store(true, Ordering::Release);
    }

    pub async fn closed(&mut self) -> OwnedObjectPath {
        tokio::time::timeout(WATCHDOG, self.closed.recv())
            .await
            .expect("the chooser did not close its request")
            .unwrap()
    }

    pub async fn closed_before_next_open(&mut self) -> OwnedObjectPath {
        tokio::select! {
            biased;
            closed = self.closed.recv() => closed.unwrap(),
            _ = self.calls.recv() => panic!("a second dialog opened before the previous request was cleaned up"),
            _ = tokio::time::sleep(WATCHDOG) => panic!("abandoning a selection never closed its request"),
        }
    }

    pub async fn respond(&self, call: &OpenCall, status: u32, uris: &[&str]) {
        Self::response_from(
            &self.service,
            call,
            status,
            HashMap::from([("uris", Value::from(uris))]),
        )
        .await;
    }

    pub async fn response_from(
        connection: &Connection,
        call: &OpenCall,
        status: u32,
        results: HashMap<&str, Value<'_>>,
    ) {
        connection
            .emit_signal(
                Some(call.client.as_str()),
                call.path.as_str(),
                REQUEST,
                "Response",
                &(status, results),
            )
            .await
            .unwrap();
    }
}
