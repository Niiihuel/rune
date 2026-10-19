//! Screen and camera access through xdg-desktop-portal, for a local preview.
//!
//! The portal owns the choice: it shows the system's own source picker and
//! permission dialogs, so nothing here enumerates screens or opens devices.
//! A successful request only hands back the PipeWire remote the portal
//! opened for this application; reading frames from it is the caller's
//! business. Each screen session keeps a dedicated bus connection, so
//! dropping it also lets the portal release the session.
//!
//! A portal of version 4 or later is asked to remember the choice until it
//! is revoked (`persist_mode` 2) and hands back a restore token; given
//! again, the same screen is shared without its picker. The token names a
//! choice in the portal, not a credential.

use rune_i18n::{es, t};
use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::time::{timeout, Instant};
use zbus::{
    message::Type,
    names::OwnedUniqueName,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Connection, MatchRule, Message, MessageStream,
};

use crate::file_dialogs::{cancel_requested, wait_cancel};
pub use crate::file_dialogs::{CancelSignal, ParentWindow};

const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const DESKTOP: &str = "/org/freedesktop/portal/desktop";
const SCREENCAST: &str = "org.freedesktop.portal.ScreenCast";
const CAMERA: &str = "org.freedesktop.portal.Camera";
const REQUEST: &str = "org.freedesktop.portal.Request";
const SESSION: &str = "org.freedesktop.portal.Session";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const RPC_TIMEOUT: Duration = Duration::from_secs(2);
/// The person picks a source or answers the system's dialog meanwhile.
const DIALOG_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_STREAMS: usize = 8;
/// `AvailableSourceTypes` bits: a monitor and a window.
const MONITOR: u32 = 1;
const WINDOW: u32 = 2;
/// `AvailableCursorModes` bits: hidden, and drawn into the frames.
const CURSOR_HIDDEN: u32 = 1;
const CURSOR_EMBEDDED: u32 = 2;
/// The first portal version that remembers a choice, and how: until revoked.
const PERSISTS: u32 = 4;
const PERSIST_UNTIL_REVOKED: u32 = 2;

const NO_PORTAL: &str = es("No hay un portal del escritorio (xdg-desktop-portal) en esta sesión.");
const NO_SCREEN: &str = es("El portal del escritorio no permite compartir la pantalla.");
const NO_CAMERA: &str = es("El portal del escritorio no ofrece la cámara.");
const NO_DEVICE: &str = es("No hay ninguna cámara conectada.");

/// Why a source could not be had, or why it ended. Displayed as the
/// interface tells the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CaptureError {
    /// No portal, or one without this kind of source.
    Unavailable(&'static str),
    /// The system refused, or was told before to refuse.
    Denied,
    /// The person dismissed the system's dialog, or RUNE stopped asking.
    Cancelled,
    /// The system ended the session: sharing stopped from outside RUNE, or
    /// the source went away.
    Closed,
    TimedOut,
    InvalidResponse,
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unavailable(reason) => t(reason),
            Self::Denied => t("El sistema no dio permiso."),
            Self::Cancelled => t("Se canceló en el diálogo del sistema."),
            Self::Closed => t("El sistema terminó la sesión."),
            Self::TimedOut => t("El portal del escritorio no respondió a tiempo."),
            Self::InvalidResponse => t("El portal del escritorio respondió algo inválido."),
        })
    }
}

impl std::error::Error for CaptureError {}

/// One source the portal shares: its PipeWire node in the remote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScreenStream {
    pub node: u32,
    /// Its size in pixels, when the portal says.
    pub size: Option<(u32, u32)>,
    /// A window rather than a whole monitor.
    pub window: bool,
}

async fn session_bus() -> Result<Connection, CaptureError> {
    timeout(
        RPC_TIMEOUT,
        zbus::connection::Builder::session()
            .map_err(|_| CaptureError::Unavailable(NO_PORTAL))?
            .method_timeout(RPC_TIMEOUT)
            .max_queued(16)
            .build(),
    )
    .await
    .map_err(|_| CaptureError::TimedOut)?
    .map_err(|_| CaptureError::Unavailable(NO_PORTAL))
}

/// The portal's current owner, pinned: a well-known sender alone does not
/// authenticate a signal, and a new owner knows nothing of our requests.
struct Portal {
    connection: Connection,
    owner: OwnedUniqueName,
    /// This connection's unique name as portal object paths spell it.
    sender: String,
    owners: MessageStream,
}

impl Portal {
    async fn new(connection: Connection, cancel: &mut CancelSignal) -> Result<Self, CaptureError> {
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.DBus")
            .unwrap()
            .path("/org/freedesktop/DBus")
            .unwrap()
            .interface("org.freedesktop.DBus")
            .unwrap()
            .member("NameOwnerChanged")
            .unwrap()
            .add_arg(DESTINATION)
            .unwrap()
            .build();
        let owners = guard(
            MessageStream::for_match_rule(rule, &connection, Some(4)),
            cancel,
        )
        .await?;
        let bus = guard(zbus::fdo::DBusProxy::new(&connection), cancel).await?;
        let name = DESTINATION.try_into().unwrap();
        let owner = match guard_fdo(bus.get_name_owner(name), cancel).await? {
            Some(owner) => owner,
            None => {
                guard_fdo(
                    bus.start_service_by_name(DESTINATION.try_into().unwrap(), 0),
                    cancel,
                )
                .await?
                .ok_or(CaptureError::Unavailable(NO_PORTAL))?;
                guard_fdo(bus.get_name_owner(DESTINATION.try_into().unwrap()), cancel)
                    .await?
                    .ok_or(CaptureError::Unavailable(NO_PORTAL))?
            }
        };
        let sender = connection
            .unique_name()
            .ok_or(CaptureError::Unavailable(NO_PORTAL))?
            .as_str()
            .trim_start_matches(':')
            .replace('.', "_");
        Ok(Self {
            connection,
            owner,
            sender,
            owners,
        })
    }

    /// Call the portal, answering cancellation, a lost portal and a timeout.
    async fn call<B>(
        &mut self,
        path: &str,
        interface: &str,
        method: &str,
        body: &B,
        missing: &'static str,
        cancel: &mut CancelSignal,
    ) -> Result<Message, CaptureError>
    where
        B: zbus::export::serde::Serialize + zbus::zvariant::DynamicType,
    {
        let call = self.connection.call_method(
            Some(self.owner.as_str()),
            path,
            Some(interface),
            method,
            body,
        );
        tokio::select! {
            biased;
            _ = wait_cancel(cancel) => Err(CaptureError::Cancelled),
            _ = owner_lost(&mut self.owners, self.owner.as_str()) => Err(CaptureError::Unavailable(NO_PORTAL)),
            result = timeout(RPC_TIMEOUT, call) => match result {
                Err(_) => Err(CaptureError::TimedOut),
                // An interface or method the portal lacks answers with an error.
                Ok(Err(zbus::Error::MethodError(..))) => Err(CaptureError::Unavailable(missing)),
                Ok(Err(_)) => Err(CaptureError::Unavailable(NO_PORTAL)),
                Ok(Ok(reply)) => Ok(reply),
            },
        }
    }

    async fn property(
        &mut self,
        interface: &str,
        name: &str,
        missing: &'static str,
        cancel: &mut CancelSignal,
    ) -> Result<OwnedValue, CaptureError> {
        let reply = self
            .call(
                DESKTOP,
                PROPERTIES,
                "Get",
                &(interface, name),
                missing,
                cancel,
            )
            .await?;
        reply
            .body()
            .deserialize::<OwnedValue>()
            .map_err(|_| CaptureError::InvalidResponse)
    }

    /// One portal request: listen on its handle before asking, so an early
    /// answer is kept; then wait for the person. Anything but an answer
    /// closes the request, so no dialog stays behind.
    async fn request<B>(
        &mut self,
        interface: &str,
        method: &str,
        token: &str,
        body: &B,
        missing: &'static str,
        cancel: &mut CancelSignal,
    ) -> Result<HashMap<String, OwnedValue>, CaptureError>
    where
        B: zbus::export::serde::Serialize + zbus::zvariant::DynamicType,
    {
        let path = OwnedObjectPath::try_from(format!("{DESKTOP}/request/{}/{token}", self.sender))
            .map_err(|_| CaptureError::InvalidResponse)?;
        let rule = signal_rule(&self.owner, &path, REQUEST, "Response");
        let mut responses = guard(
            MessageStream::for_match_rule(rule, &self.connection, Some(4)),
            cancel,
        )
        .await?;
        let handle = match self
            .call(DESKTOP, interface, method, body, missing, cancel)
            .await
            .and_then(|reply| {
                reply
                    .body()
                    .deserialize::<OwnedObjectPath>()
                    .map_err(|_| CaptureError::InvalidResponse)
            }) {
            Ok(handle) => handle,
            Err(error) => {
                // The portal may have made the request before failing to answer.
                self.close(&path, REQUEST).await;
                return Err(error);
            }
        };
        if handle != path {
            // Never close a handle that may be someone else's.
            self.close(&path, REQUEST).await;
            return Err(CaptureError::InvalidResponse);
        }
        let deadline = Instant::now() + DIALOG_TIMEOUT;
        let failure = tokio::select! {
            biased;
            _ = wait_cancel(cancel) => CaptureError::Cancelled,
            _ = owner_lost(&mut self.owners, self.owner.as_str()) => CaptureError::Unavailable(NO_PORTAL),
            _ = tokio::time::sleep_until(deadline) => CaptureError::TimedOut,
            response = responses.next() => return answer(response),
        };
        self.close(&path, REQUEST).await;
        Err(failure)
    }

    /// Close a request or a session, without waiting long for it.
    async fn close(&self, path: &OwnedObjectPath, interface: &str) {
        let _ = timeout(
            RPC_TIMEOUT,
            self.connection.call_method(
                Some(self.owner.as_str()),
                path.as_str(),
                Some(interface),
                "Close",
                &(),
            ),
        )
        .await;
    }
}

/// A screen the person chose in the system's picker, shared until this is
/// closed or dropped, or the system ends it.
pub struct ScreenCast {
    portal: Portal,
    session: OwnedObjectPath,
    closed: MessageStream,
    streams: Vec<ScreenStream>,
    open: bool,
    /// The portal's token to share this choice again without its picker.
    restore: Option<String>,
}

/// A restore token as RUNE keeps and sends one: short and printable.
pub fn valid_restore_token(token: &str) -> bool {
    !token.is_empty() && token.len() <= 256 && token.bytes().all(|byte| byte.is_ascii_graphic())
}

impl std::fmt::Debug for ScreenCast {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ScreenCast")
            .field("streams", &self.streams)
            .field("open", &self.open)
            .finish()
    }
}

impl ScreenCast {
    /// Ask the portal for one monitor or window, with the pointer drawn in
    /// when it can be, and open its PipeWire remote. Nothing is shared until
    /// the person picks in the system's dialog, unless `restore` names a
    /// choice the portal remembers.
    pub async fn start(
        parent: Option<&ParentWindow>,
        restore: Option<&str>,
        cancel: CancelSignal,
    ) -> Result<(Self, OwnedFd), CaptureError> {
        if cancel_requested(&cancel) {
            return Err(CaptureError::Cancelled);
        }
        Self::start_on(session_bus().await?, parent, restore, cancel).await
    }

    async fn start_on(
        connection: Connection,
        parent: Option<&ParentWindow>,
        restore: Option<&str>,
        mut cancel: CancelSignal,
    ) -> Result<(Self, OwnedFd), CaptureError> {
        let cancel = &mut cancel;
        if cancel_requested(cancel) {
            return Err(CaptureError::Cancelled);
        }
        let mut portal = Portal::new(connection, cancel).await?;
        let types = u32::try_from(
            portal
                .property(SCREENCAST, "AvailableSourceTypes", NO_SCREEN, cancel)
                .await?,
        )
        .map_err(|_| CaptureError::InvalidResponse)?
            & (MONITOR | WINDOW);
        if types == 0 {
            return Err(CaptureError::Unavailable(NO_SCREEN));
        }
        let cursors = match portal
            .property(SCREENCAST, "AvailableCursorModes", NO_SCREEN, cancel)
            .await
        {
            Ok(modes) => u32::try_from(modes).unwrap_or(CURSOR_HIDDEN),
            Err(CaptureError::Unavailable(NO_SCREEN)) => CURSOR_HIDDEN,
            Err(error) => return Err(error),
        };
        let persists = match portal
            .property(SCREENCAST, "version", NO_SCREEN, cancel)
            .await
        {
            Ok(version) => u32::try_from(version).is_ok_and(|version| version >= PERSISTS),
            Err(CaptureError::Unavailable(NO_SCREEN)) => false,
            Err(error) => return Err(error),
        };
        let restore = restore
            .filter(|token| persists && valid_restore_token(token))
            .map(str::to_owned);
        let token = token()?;
        let session =
            OwnedObjectPath::try_from(format!("{DESKTOP}/session/{}/{token}", portal.sender))
                .map_err(|_| CaptureError::InvalidResponse)?;
        // Listen before the session exists, so its end is never missed.
        let closed = guard(
            MessageStream::for_match_rule(
                signal_rule(&portal.owner, &session, SESSION, "Closed"),
                &portal.connection,
                Some(4),
            ),
            cancel,
        )
        .await?;
        let mut cast = Self {
            portal,
            session,
            closed,
            streams: Vec::new(),
            open: true,
            restore,
        };
        match cast
            .negotiate(token, types, cursors, persists, parent, cancel)
            .await
        {
            Ok(remote) => Ok((cast, remote)),
            Err(error) => {
                cast.close().await;
                Err(error)
            }
        }
    }

    async fn negotiate(
        &mut self,
        session_token: String,
        types: u32,
        cursors: u32,
        persists: bool,
        parent: Option<&ParentWindow>,
        cancel: &mut CancelSignal,
    ) -> Result<OwnedFd, CaptureError> {
        let request = token()?;
        let options = HashMap::from([
            ("handle_token", Value::from(request.as_str())),
            ("session_handle_token", Value::from(session_token.as_str())),
        ]);
        let mut created = self
            .portal
            .request(
                SCREENCAST,
                "CreateSession",
                &request,
                &(options,),
                NO_SCREEN,
                cancel,
            )
            .await?;
        // The specification types it as a string; some portals send a path.
        let handle = created.remove("session_handle");
        let same = match handle.as_deref() {
            Some(Value::Str(handle)) => handle.as_str() == self.session.as_str(),
            Some(Value::ObjectPath(handle)) => handle.as_str() == self.session.as_str(),
            _ => false,
        };
        if !same {
            return Err(CaptureError::InvalidResponse);
        }
        let cursor = if cursors & CURSOR_EMBEDDED != 0 {
            CURSOR_EMBEDDED
        } else {
            CURSOR_HIDDEN
        };
        let request = token()?;
        let mut options = HashMap::from([
            ("handle_token", Value::from(request.as_str())),
            ("types", Value::from(types)),
            ("multiple", Value::from(false)),
            ("cursor_mode", Value::from(cursor)),
        ]);
        if persists {
            options.insert("persist_mode", Value::from(PERSIST_UNTIL_REVOKED));
        }
        let restore = self.restore.take();
        if let Some(restore) = &restore {
            options.insert("restore_token", Value::from(restore.as_str()));
        }
        self.portal
            .request(
                SCREENCAST,
                "SelectSources",
                &request,
                &(&self.session, options),
                NO_SCREEN,
                cancel,
            )
            .await?;
        let request = token()?;
        let options = HashMap::from([("handle_token", Value::from(request.as_str()))]);
        let parent = parent.map_or("", |parent| parent.as_str());
        let mut started = self
            .portal
            .request(
                SCREENCAST,
                "Start",
                &request,
                &(&self.session, parent, options),
                NO_SCREEN,
                cancel,
            )
            .await?;
        self.streams = streams(started.remove("streams"))?;
        // A new token replaces the one used, which the portal spent.
        self.restore = started
            .remove("restore_token")
            .and_then(|token| String::try_from(token).ok())
            .filter(|token| valid_restore_token(token))
            .or(restore.filter(|_| persists));
        let reply = self
            .portal
            .call(
                DESKTOP,
                SCREENCAST,
                "OpenPipeWireRemote",
                &(&self.session, HashMap::<&str, Value>::new()),
                NO_SCREEN,
                cancel,
            )
            .await?;
        remote(&reply)
    }

    /// What the person chose, in the order the portal gave it.
    pub fn streams(&self) -> &[ScreenStream] {
        &self.streams
    }

    /// The token to share this same choice again without the picker, when
    /// the portal remembers it.
    pub fn restore_token(&self) -> Option<&str> {
        self.restore.as_deref()
    }

    /// Wait until the system ends the session: the person stopped sharing
    /// from the system, the source went away, or the portal left the bus.
    /// Cancel-safe; once it returns the session is over.
    pub async fn ended(&mut self) -> CaptureError {
        let error = tokio::select! {
            closed = self.closed.next() => match closed {
                Some(Ok(_)) => CaptureError::Closed,
                _ => CaptureError::Unavailable(NO_PORTAL),
            },
            _ = owner_lost(&mut self.portal.owners, self.portal.owner.as_str()) => CaptureError::Unavailable(NO_PORTAL),
        };
        self.open = false;
        error
    }

    /// Stop sharing: the portal releases the source.
    pub async fn close(&mut self) {
        if std::mem::take(&mut self.open) {
            self.portal.close(&self.session, SESSION).await;
        }
    }
}

impl Drop for ScreenCast {
    fn drop(&mut self) {
        if !self.open {
            return;
        }
        // Dropping the dedicated connection also ends the session for the
        // portal; asking first is quicker when a runtime is still there.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let (connection, owner, session) = (
                self.portal.connection.clone(),
                self.portal.owner.clone(),
                self.session.clone(),
            );
            drop(runtime.spawn(async move {
                let _ = timeout(
                    RPC_TIMEOUT,
                    connection.call_method(
                        Some(owner.as_str()),
                        session.as_str(),
                        Some(SESSION),
                        "Close",
                        &(),
                    ),
                )
                .await;
            }));
        }
    }
}

/// Whether the system has a camera to offer; asks no permission.
pub async fn camera_present(cancel: CancelSignal) -> Result<bool, CaptureError> {
    camera_present_on(session_bus().await?, cancel).await
}

async fn camera_present_on(
    connection: Connection,
    mut cancel: CancelSignal,
) -> Result<bool, CaptureError> {
    let mut portal = Portal::new(connection, &mut cancel).await?;
    let present = portal
        .property(CAMERA, "IsCameraPresent", NO_CAMERA, &mut cancel)
        .await?;
    bool::try_from(present).map_err(|_| CaptureError::InvalidResponse)
}

/// Ask the system for the cameras, and open their PipeWire remote. The
/// system may ask the person first, or answer from what they chose before.
/// The cameras are released with the remote's descriptor.
pub async fn open_camera(cancel: CancelSignal) -> Result<OwnedFd, CaptureError> {
    if cancel_requested(&cancel) {
        return Err(CaptureError::Cancelled);
    }
    open_camera_on(session_bus().await?, cancel).await
}

async fn open_camera_on(
    connection: Connection,
    mut cancel: CancelSignal,
) -> Result<OwnedFd, CaptureError> {
    let cancel = &mut cancel;
    if cancel_requested(cancel) {
        return Err(CaptureError::Cancelled);
    }
    let mut portal = Portal::new(connection, cancel).await?;
    let present = portal
        .property(CAMERA, "IsCameraPresent", NO_CAMERA, cancel)
        .await?;
    if !bool::try_from(present).map_err(|_| CaptureError::InvalidResponse)? {
        return Err(CaptureError::Unavailable(NO_DEVICE));
    }
    let request = token()?;
    let options = HashMap::from([("handle_token", Value::from(request.as_str()))]);
    portal
        .request(
            CAMERA,
            "AccessCamera",
            &request,
            &(options,),
            NO_CAMERA,
            cancel,
        )
        .await?;
    let reply = portal
        .call(
            DESKTOP,
            CAMERA,
            "OpenPipeWireRemote",
            &(HashMap::<&str, Value>::new(),),
            NO_CAMERA,
            cancel,
        )
        .await?;
    remote(&reply)
}

fn signal_rule(
    owner: &OwnedUniqueName,
    path: &OwnedObjectPath,
    interface: &'static str,
    member: &'static str,
) -> MatchRule<'static> {
    MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(owner.to_string())
        .unwrap()
        .path(path.to_string())
        .unwrap()
        .interface(interface)
        .unwrap()
        .member(member)
        .unwrap()
        .build()
}

/// Returns once the portal's name has another owner, or the bus is gone.
async fn owner_lost(owners: &mut MessageStream, owner: &str) {
    loop {
        let Some(Ok(message)) = owners.next().await else {
            return;
        };
        let from_bus = message
            .header()
            .sender()
            .is_some_and(|sender| sender.as_str() == "org.freedesktop.DBus");
        let current = message
            .body()
            .deserialize::<(String, String, String)>()
            .is_ok_and(|(name, _, new)| name == DESTINATION && new == owner);
        if from_bus && !current {
            return;
        }
    }
}

async fn guard<F, T>(call: F, cancel: &mut CancelSignal) -> Result<T, CaptureError>
where
    F: std::future::Future<Output = zbus::Result<T>>,
{
    tokio::select! {
        biased;
        _ = wait_cancel(cancel) => Err(CaptureError::Cancelled),
        result = timeout(RPC_TIMEOUT, call) => result
            .map_err(|_| CaptureError::TimedOut)?
            .map_err(|_| CaptureError::Unavailable(NO_PORTAL)),
    }
}

/// Like [`guard`], for bus calls where "nobody owns it" is an answer.
async fn guard_fdo<F, T>(call: F, cancel: &mut CancelSignal) -> Result<Option<T>, CaptureError>
where
    F: std::future::Future<Output = zbus::fdo::Result<T>>,
{
    tokio::select! {
        biased;
        _ = wait_cancel(cancel) => Err(CaptureError::Cancelled),
        result = timeout(RPC_TIMEOUT, call) => match result.map_err(|_| CaptureError::TimedOut)? {
            Ok(value) => Ok(Some(value)),
            Err(zbus::fdo::Error::NameHasNoOwner(_) | zbus::fdo::Error::ServiceUnknown(_)) => Ok(None),
            Err(_) => Err(CaptureError::Unavailable(NO_PORTAL)),
        },
    }
}

fn answer(
    response: Option<zbus::Result<Message>>,
) -> Result<HashMap<String, OwnedValue>, CaptureError> {
    let message = response
        .ok_or(CaptureError::Unavailable(NO_PORTAL))?
        .map_err(|_| CaptureError::Unavailable(NO_PORTAL))?;
    if message.body().len() > MAX_RESPONSE_BYTES {
        return Err(CaptureError::InvalidResponse);
    }
    let (status, results): (u32, HashMap<String, OwnedValue>) = message
        .body()
        .deserialize()
        .map_err(|_| CaptureError::InvalidResponse)?;
    match status {
        0 => Ok(results),
        1 => Err(CaptureError::Cancelled),
        2 => Err(CaptureError::Denied),
        _ => Err(CaptureError::InvalidResponse),
    }
}

fn streams(value: Option<OwnedValue>) -> Result<Vec<ScreenStream>, CaptureError> {
    let value = value.ok_or(CaptureError::InvalidResponse)?;
    let raw = Vec::<(u32, HashMap<String, OwnedValue>)>::try_from(value)
        .map_err(|_| CaptureError::InvalidResponse)?;
    if raw.is_empty() || raw.len() > MAX_STREAMS {
        return Err(CaptureError::InvalidResponse);
    }
    raw.into_iter()
        .map(|(node, mut properties)| {
            if node == 0 {
                return Err(CaptureError::InvalidResponse);
            }
            let size = properties
                .remove("size")
                .and_then(|size| <(i32, i32)>::try_from(size).ok())
                .and_then(|(width, height)| {
                    Some((u32::try_from(width).ok()?, u32::try_from(height).ok()?))
                })
                .filter(|(width, height)| *width > 0 && *height > 0);
            let window = properties
                .remove("source_type")
                .and_then(|kind| u32::try_from(kind).ok())
                == Some(WINDOW);
            Ok(ScreenStream { node, size, window })
        })
        .collect()
}

/// The remote's descriptor, if it is the socket a PipeWire remote is.
fn remote(reply: &Message) -> Result<OwnedFd, CaptureError> {
    let fd: zbus::zvariant::OwnedFd = reply
        .body()
        .deserialize()
        .map_err(|_| CaptureError::InvalidResponse)?;
    let fd = OwnedFd::from(fd);
    let stat = rustix::fs::fstat(&fd).map_err(|_| CaptureError::InvalidResponse)?;
    if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::Socket {
        return Err(CaptureError::InvalidResponse);
    }
    Ok(fd)
}

fn token() -> Result<String, CaptureError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| CaptureError::Unavailable(NO_PORTAL))?;
    use std::fmt::Write;
    let mut token = String::with_capacity(37);
    token.push_str("rune_");
    for byte in bytes {
        write!(&mut token, "{byte:02x}").unwrap();
    }
    Ok(token)
}

#[cfg(test)]
mod tests;
