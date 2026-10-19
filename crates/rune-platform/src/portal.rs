//! Linux file selection through a caller-owned, asynchronous portal request.

use rune_i18n::t;
use std::{
    collections::HashMap,
    future::Future,
    pin::{pin, Pin},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    task::{Context, Poll},
    time::Duration,
};

use futures_util::StreamExt;
use tokio::{
    sync::Notify,
    time::{timeout, timeout_at, Instant},
};
use zbus::{
    message::Type,
    names::OwnedUniqueName,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
    Address, AsyncDrop, Connection, MatchRule, Message, MessageStream,
};

use crate::file_dialogs::files::FileWorker;
use crate::file_dialogs::{
    cancel_requested, wait_cancel, CancelSignal, FolderRequest, ParentWindow, PickError,
    PickRequest, SaveDestination, SaveRequest, SelectedFile, MAX_PATH_BYTES,
};

pub(crate) const DESTINATION: &str = "org.freedesktop.portal.Desktop";
const FILE_CHOOSER: &str = "org.freedesktop.portal.FileChooser";
pub(crate) const REQUEST: &str = "org.freedesktop.portal.Request";
pub(crate) const DESKTOP: &str = "/org/freedesktop/portal/desktop";
const RPC_TIMEOUT: Duration = Duration::from_secs(2);
const DIALOG_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;

/// One dialog and one file worker, driven by the application's existing runtime.
pub struct FileChooser {
    connection: Connection,
    address: Option<Address>,
    cleanup: Arc<Cleanup>,
    files: FileWorker,
}

#[derive(Clone, Copy)]
enum DialogRequest<'a> {
    Open(&'a PickRequest),
    Save(&'a SaveRequest),
    Folder(&'a FolderRequest),
}

impl<'a> DialogRequest<'a> {
    fn max_files(self) -> usize {
        match self {
            Self::Open(request) => request.max_files,
            Self::Save(_) | Self::Folder(_) => 1,
        }
    }

    fn parent(self) -> Option<&'a ParentWindow> {
        match self {
            Self::Open(request) => request.parent.as_ref(),
            Self::Save(request) => request.parent.as_ref(),
            Self::Folder(request) => request.parent.as_ref(),
        }
    }

    fn current_folder(self) -> Option<&'a std::path::Path> {
        match self {
            Self::Open(request) => request.current_folder.as_deref(),
            Self::Save(request) => request.current_folder.as_deref(),
            Self::Folder(request) => request.current_folder.as_deref(),
        }
    }

    fn method(self) -> &'static str {
        match self {
            Self::Open(_) | Self::Folder(_) => "OpenFile",
            Self::Save(_) => "SaveFile",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::Open(request) if request.max_files > 1 => t("Seleccionar archivos"),
            Self::Open(_) => t("Seleccionar archivo"),
            Self::Save(_) => t("Guardar archivo como"),
            Self::Folder(_) => t("Elegir una carpeta"),
        }
    }
}

impl FileChooser {
    pub async fn session() -> Result<Self, PickError> {
        // Capture once: a later retry must not choose a different bus from
        // changed process environment, nor replace a busy file worker.
        let address = Address::session().map_err(|_| PickError::Unavailable)?;
        let connection = tokio::time::timeout(
            RPC_TIMEOUT,
            zbus::connection::Builder::address(address.clone())
                .map_err(|_| PickError::Unavailable)?
                .method_timeout(RPC_TIMEOUT)
                .max_queued(8)
                .build(),
        )
        .await
        .map_err(|_| PickError::TimedOut)?
        .map_err(|_| PickError::Unavailable)?;
        let mut chooser = Self::with_connection(connection)?;
        chooser.address = Some(address);
        Ok(chooser)
    }

    fn with_connection(connection: Connection) -> Result<Self, PickError> {
        let runtime = tokio::runtime::Handle::try_current().map_err(|_| PickError::Unavailable)?;
        Ok(Self {
            connection,
            address: None,
            cleanup: Arc::new(Cleanup {
                runtime,
                retired: AtomicBool::new(false),
                running: AtomicBool::new(false),
                done: Notify::new(),
            }),
            files: FileWorker::new()?,
        })
    }

    pub fn choose_files<'a>(
        &'a mut self,
        request: &'a PickRequest,
        cancel: CancelSignal,
    ) -> impl Future<Output = Result<Vec<SelectedFile>, PickError>> + Send + 'a {
        let runtime = self.cleanup.runtime.clone();
        RuntimeBound {
            inner: Some(Box::pin(self.choose_files_inner(request, cancel))),
            runtime,
        }
    }

    /// Choose one inert destination without opening, creating, truncating or
    /// resolving its path. The owning service must decide its write policy
    /// separately. Open and save share this chooser's cleanup and file worker.
    pub fn save_file<'a>(
        &'a mut self,
        request: &'a SaveRequest,
        cancel: CancelSignal,
    ) -> impl Future<Output = Result<SaveDestination, PickError>> + Send + 'a {
        let runtime = self.cleanup.runtime.clone();
        RuntimeBound {
            inner: Some(Box::pin(self.save_file_inner(request, cancel))),
            runtime,
        }
    }

    /// Choose one folder (portal version 3's `directory`) without opening or
    /// listing it; an older portal answers with a file, which the caller's
    /// own read of the folder then refuses.
    pub fn choose_folder<'a>(
        &'a mut self,
        request: &'a FolderRequest,
        cancel: CancelSignal,
    ) -> impl Future<Output = Result<std::path::PathBuf, PickError>> + Send + 'a {
        let runtime = self.cleanup.runtime.clone();
        RuntimeBound {
            inner: Some(Box::pin(self.choose_folder_inner(request, cancel))),
            runtime,
        }
    }

    /// Drain this service's previous request cleanup and file-worker result.
    /// No dialog, connection, renewal or new worker is created. Local-path
    /// imports must await this before using their existing preparation worker.
    /// Drive and drop the future on the owning service; cancellation leaves any
    /// unfinished cleanup or file result retained for the next drain.
    pub fn wait_idle(
        &mut self,
        cancel: CancelSignal,
    ) -> impl Future<Output = Result<(), PickError>> + Send + '_ {
        let runtime = self.cleanup.runtime.clone();
        RuntimeBound {
            inner: Some(Box::pin(self.wait_idle_inner(cancel))),
            runtime,
        }
    }

    async fn wait_idle_inner(&mut self, mut cancel: CancelSignal) -> Result<(), PickError> {
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        self.cleanup.wait(&mut cancel).await?;
        self.files.wait_idle(cancel).await
    }

    async fn choose_files_inner(
        &mut self,
        request: &PickRequest,
        mut cancel: CancelSignal,
    ) -> Result<Vec<SelectedFile>, PickError> {
        request.validate()?;
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        let deadline = Instant::now() + DIALOG_TIMEOUT;
        self.prepare_dialog(&mut cancel, deadline).await?;
        let uris = self
            .dialog_uris(DialogRequest::Open(request), &mut cancel, deadline)
            .await?;
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        let selected = timeout_at(
            deadline,
            self.files.open_uris(
                &uris,
                request.max_files,
                request.max_file_bytes,
                request.max_total_bytes,
                cancel.clone(),
            ),
        )
        .await
        .map_err(|_| PickError::TimedOut)??;
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        Ok(selected)
    }

    async fn save_file_inner(
        &mut self,
        request: &SaveRequest,
        mut cancel: CancelSignal,
    ) -> Result<SaveDestination, PickError> {
        request.validate()?;
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        let deadline = Instant::now() + DIALOG_TIMEOUT;
        self.prepare_dialog(&mut cancel, deadline).await?;
        let uris = self
            .dialog_uris(DialogRequest::Save(request), &mut cancel, deadline)
            .await?;
        let destination =
            SaveDestination::from_uri(uris.first().ok_or(PickError::InvalidResponse)?)?;
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        Ok(destination)
    }

    async fn choose_folder_inner(
        &mut self,
        request: &FolderRequest,
        mut cancel: CancelSignal,
    ) -> Result<std::path::PathBuf, PickError> {
        request.validate()?;
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        let deadline = Instant::now() + DIALOG_TIMEOUT;
        self.prepare_dialog(&mut cancel, deadline).await?;
        let uris = self
            .dialog_uris(DialogRequest::Folder(request), &mut cancel, deadline)
            .await?;
        let folder = crate::file_dialogs::files::local_path(
            uris.first().ok_or(PickError::InvalidResponse)?,
        )?;
        if cancel_requested(&cancel) {
            return Err(PickError::Cancelled);
        }
        Ok(folder)
    }

    async fn prepare_dialog(
        &mut self,
        cancel: &mut CancelSignal,
        deadline: Instant,
    ) -> Result<(), PickError> {
        timeout_at(deadline, self.wait_idle_inner(cancel.clone()))
            .await
            .map_err(|_| PickError::TimedOut)??;
        if self.cleanup.retired.load(Ordering::Acquire) || self.connection.is_closed() {
            // A cleanup interrupted by shutdown still leaves this connection
            // retired; never reuse it or replace the pending file worker.
            if !self.connection.is_closed() {
                let _ = timeout(RPC_TIMEOUT, self.connection.clone().close()).await;
            }
            if !self.connection.is_closed() {
                return Err(PickError::Unavailable);
            }
            let address = self.address.clone().ok_or(PickError::Unavailable)?;
            let connection = rpc(
                zbus::connection::Builder::address(address)
                    .map_err(|_| PickError::Unavailable)?
                    .method_timeout(RPC_TIMEOUT)
                    .max_queued(8)
                    .build(),
                cancel,
            )
            .await?;
            self.connection = connection;
            self.cleanup.retired.store(false, Ordering::Release);
        }
        Ok(())
    }

    async fn dialog_uris(
        &self,
        request: DialogRequest<'_>,
        cancel: &mut CancelSignal,
        deadline: Instant,
    ) -> Result<Vec<String>, PickError> {
        let bus = zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .map_err(|_| PickError::Unavailable)?;
        let owner_rule = MatchRule::builder()
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
        let mut owners = Owners {
            stream: timeout_at(
                deadline,
                rpc(
                    MessageStream::for_match_rule(owner_rule, &self.connection, Some(4)),
                    cancel,
                ),
            )
            .await
            .map_err(|_| PickError::TimedOut)??,
            latest: None,
            deadline,
        };
        let result = async {
            let owner = owners
                .call(
                    async {
                        match bus.get_name_owner(DESTINATION.try_into().unwrap()).await {
                            Ok(owner) => Ok(Some(owner)),
                            Err(zbus::fdo::Error::NameHasNoOwner(_)) => Ok(None),
                            Err(error) => Err(error),
                        }
                    },
                    cancel,
                    None,
                )
                .await?;
            let owner = if let Some(owner) = owner {
                owner
            } else {
                owners
                    .call(
                        bus.start_service_by_name(DESTINATION.try_into().unwrap(), 0),
                        cancel,
                        None,
                    )
                    .await?;
                owners
                    .call(
                        bus.get_name_owner(DESTINATION.try_into().unwrap()),
                        cancel,
                        None,
                    )
                    .await?
            };
            if owners
                .latest
                .as_ref()
                .is_some_and(|latest| latest != owner.as_str())
            {
                return Err(PickError::Unavailable);
            }
            let token = token()?;
            let sender = self
                .connection
                .unique_name()
                .ok_or(PickError::Unavailable)?
                .as_str()
                .trim_start_matches(':')
                .replace('.', "_");
            let path = OwnedObjectPath::try_from(format!("{DESKTOP}/request/{sender}/{token}"))
                .map_err(|_| PickError::Failed)?;
            let rule = MatchRule::builder()
                .msg_type(Type::Signal)
                .sender(owner.as_str())
                .unwrap()
                .path(path.as_str())
                .unwrap()
                .interface(REQUEST)
                .unwrap()
                .member("Response")
                .unwrap()
                .build();
            let mut responses = owners
                .call(
                    MessageStream::for_match_rule(rule, &self.connection, Some(4)),
                    cancel,
                    Some(&owner),
                )
                .await?;
            let mut guard = RequestGuard {
                connection: self.connection.clone(),
                owner: owner.clone(),
                path: path.clone(),
                cleanup: self.cleanup.clone(),
                armed: true,
            };
            let result = self
                .interact(
                    request,
                    cancel,
                    &owner,
                    &path,
                    &token,
                    &mut owners,
                    &mut responses,
                    deadline,
                )
                .await;
            guard.armed = false;
            let _ = timeout(RPC_TIMEOUT, responses.async_drop()).await;
            result
        }
        .await;
        let _ = timeout(RPC_TIMEOUT, owners.stream.async_drop()).await;
        result
    }

    async fn interact(
        &self,
        request: DialogRequest<'_>,
        cancel: &mut CancelSignal,
        owner: &OwnedUniqueName,
        path: &OwnedObjectPath,
        token: &str,
        owners: &mut Owners,
        responses: &mut MessageStream,
        deadline: Instant,
    ) -> Result<Vec<String>, PickError> {
        if cancel_requested(cancel) {
            return Err(PickError::Cancelled);
        }
        let mut options = HashMap::from([
            ("handle_token", Value::from(token)),
            ("modal", Value::from(true)),
        ]);
        match request {
            DialogRequest::Open(request) => {
                options.insert("multiple", Value::from(request.max_files > 1));
            }
            DialogRequest::Save(request) => {
                options.insert("current_name", Value::from(request.suggested_name.as_str()));
            }
            DialogRequest::Folder(_) => {
                options.insert("multiple", Value::from(false));
                options.insert("directory", Value::from(true));
            }
        }
        let folder = request
            .current_folder()
            .map(|path| [path.as_os_str().as_encoded_bytes(), &[0]].concat());
        if let Some(folder) = &folder {
            options.insert("current_folder", Value::from(folder.as_slice()));
        }
        let parent = request.parent().map_or("", |parent| parent.as_str());
        let body = (parent, request.title(), options);
        let mut opening = pin!(self.connection.call_method(
            Some(owner.as_str()),
            DESKTOP,
            Some(FILE_CHOOSER),
            request.method(),
            &body
        ));
        let opening_deadline = (Instant::now() + RPC_TIMEOUT).min(deadline);
        let mut cancelled = false;
        let mut failure = None;
        let mut early = None;
        let mut owners_live = true;
        let mut responses_live = true;
        let accepted = loop {
            tokio::select! {
                biased;
                _ = wait_cancel(cancel), if !cancelled => { cancelled = true; failure = Some(PickError::Cancelled); }
                _ = tokio::time::sleep_until(opening_deadline) => break false,
                changed = owners.changed(), if owners_live => match changed {
                    Ok(current) if current == owner.as_str() => {},
                    _ => { owners_live = false; failure.get_or_insert(PickError::Unavailable); }
                },
                response = responses.next(), if responses_live => match response {
                    Some(Ok(message)) => {
                        if early.is_some() { failure.get_or_insert(PickError::InvalidResponse); }
                        else { early = Some(response_uris(&message, request.max_files())); }
                    }
                    _ => { responses_live = false; failure.get_or_insert(PickError::Unavailable); }
                },
                reply = &mut opening => {
                    match reply.and_then(|reply| reply.body().deserialize::<OwnedObjectPath>()) {
                        Ok(actual) if actual == *path => break true,
                        Ok(_) => { failure.get_or_insert(PickError::InvalidResponse); break false; }
                        Err(_) => { failure.get_or_insert(PickError::Unavailable); break false; }
                    }
                }
            }
        };
        if !accepted || failure.is_some() {
            self.close_request(owner, path, owners, responses).await;
            if !accepted {
                // A pending dialog call can create its request after a failed
                // Close. Retire this dedicated connection instead of reusing it.
                self.retire_connection().await;
            }
            return Err(failure.unwrap_or(PickError::TimedOut));
        }
        if let Some(response) = early {
            return response;
        }
        let result = loop {
            tokio::select! {
                biased;
                _ = wait_cancel(cancel) => break Err(PickError::Cancelled),
                changed = owners.changed() => match changed {
                    Ok(current) if current == owner.as_str() => {},
                    _ => break Err(PickError::Unavailable),
                },
                _ = tokio::time::sleep_until(deadline) => break Err(PickError::TimedOut),
                response = responses.next() => return response.ok_or(PickError::Unavailable)?
                    .map_err(|_| PickError::Unavailable).and_then(|message| response_uris(&message, request.max_files())),
            }
        };
        self.close_request(owner, path, owners, responses).await;
        result
    }

    async fn close_request(
        &self,
        owner: &OwnedUniqueName,
        path: &OwnedObjectPath,
        owners: &mut Owners,
        responses: &mut MessageStream,
    ) {
        let mut closing = pin!(self.connection.call_method(
            Some(owner.as_str()),
            path.as_str(),
            Some(REQUEST),
            "Close",
            &()
        ));
        let deadline = Instant::now() + RPC_TIMEOUT;
        let mut owners_live = true;
        let mut responses_live = true;
        loop {
            tokio::select! {
                biased;
                _ = tokio::time::sleep_until(deadline) => {
                    self.retire_connection().await;
                    break;
                },
                result = &mut closing => {
                    if result.is_err() { self.retire_connection().await; }
                    break;
                },
                changed = owners.stream.next(), if owners_live => { owners_live = changed.is_some(); }
                response = responses.next(), if responses_live => { responses_live = response.is_some(); }
            }
        }
    }

    async fn retire_connection(&self) {
        self.cleanup.retired.store(true, Ordering::Release);
        let _ = timeout(RPC_TIMEOUT, self.connection.clone().close()).await;
    }
}

struct Cleanup {
    runtime: tokio::runtime::Handle,
    retired: AtomicBool,
    running: AtomicBool,
    done: Notify,
}

impl Cleanup {
    async fn wait(&self, cancel: &mut CancelSignal) -> Result<(), PickError> {
        loop {
            let done = self.done.notified();
            if !self.running.load(Ordering::Acquire) {
                return Ok(());
            }
            tokio::select! {
                biased;
                _ = wait_cancel(cancel) => return Err(PickError::Cancelled),
                _ = done => {},
            }
        }
    }
}

struct RequestGuard {
    connection: Connection,
    owner: OwnedUniqueName,
    path: OwnedObjectPath,
    cleanup: Arc<Cleanup>,
    armed: bool,
}

impl Drop for RequestGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        self.cleanup.retired.store(true, Ordering::Release);
        if self.cleanup.running.swap(true, Ordering::AcqRel) {
            return;
        }
        let done = CleanupDone(self.cleanup.clone());
        let runtime = self.cleanup.runtime.clone();
        let (connection, owner, path) = (
            self.connection.clone(),
            self.owner.clone(),
            self.path.clone(),
        );
        drop(runtime.spawn(async move {
            // Constructed before spawn: even an unpolled or aborted task
            // releases the waiter, while its connection stays retired.
            let _done = done;
            let _ = timeout(
                RPC_TIMEOUT,
                connection.call_method(
                    Some(owner.as_str()),
                    path.as_str(),
                    Some(REQUEST),
                    "Close",
                    &(),
                ),
            )
            .await;
            let _ = timeout(RPC_TIMEOUT, connection.close()).await;
        }));
    }
}

/// zbus stream destructors schedule work. Keep their original runtime entered
/// even when a caller drops the selection future from another thread/context.
struct RuntimeBound<F> {
    inner: Option<Pin<Box<F>>>,
    runtime: tokio::runtime::Handle,
}

impl<F: Future> Future for RuntimeBound<F> {
    type Output = F::Output;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        let _entered = this.runtime.enter();
        this.inner.as_mut().unwrap().as_mut().poll(context)
    }
}

impl<F> Drop for RuntimeBound<F> {
    fn drop(&mut self) {
        let _entered = self.runtime.enter();
        drop(self.inner.take());
    }
}

struct CleanupDone(Arc<Cleanup>);

impl Drop for CleanupDone {
    fn drop(&mut self) {
        self.0.running.store(false, Ordering::Release);
        self.0.done.notify_one();
    }
}

struct Owners {
    stream: MessageStream,
    latest: Option<String>,
    deadline: Instant,
}

impl Owners {
    async fn changed(&mut self) -> Result<String, PickError> {
        let message = self
            .stream
            .next()
            .await
            .ok_or(PickError::Unavailable)?
            .map_err(|_| PickError::Unavailable)?;
        if message
            .header()
            .sender()
            .is_none_or(|sender| sender.as_str() != "org.freedesktop.DBus")
            || message.body().len() > 1024
        {
            return Err(PickError::Unavailable);
        }
        let (name, _, owner): (String, String, String) = message
            .body()
            .deserialize()
            .map_err(|_| PickError::Unavailable)?;
        if name != DESTINATION {
            return Err(PickError::Unavailable);
        }
        self.latest = Some(owner.clone());
        Ok(owner)
    }

    async fn call<F, T, E>(
        &mut self,
        call: F,
        cancel: &mut CancelSignal,
        owner: Option<&OwnedUniqueName>,
    ) -> Result<T, PickError>
    where
        F: Future<Output = Result<T, E>>,
    {
        let mut call = pin!(call);
        let deadline = (Instant::now() + RPC_TIMEOUT).min(self.deadline);
        loop {
            tokio::select! {
                biased;
                _ = wait_cancel(cancel) => return Err(PickError::Cancelled),
                _ = tokio::time::sleep_until(deadline) => return Err(PickError::TimedOut),
                changed = self.changed() => {
                    let changed = changed?;
                    if owner.is_some_and(|owner| changed != owner.as_str()) { return Err(PickError::Unavailable); }
                },
                result = &mut call => return result.map_err(|_| PickError::Unavailable),
            }
        }
    }
}

async fn rpc<F, T>(call: F, cancel: &mut CancelSignal) -> Result<T, PickError>
where
    F: Future<Output = zbus::Result<T>>,
{
    tokio::select! {
        biased;
        _ = wait_cancel(cancel) => Err(PickError::Cancelled),
        result = timeout(RPC_TIMEOUT, call) => result.map_err(|_| PickError::TimedOut)?.map_err(|_| PickError::Unavailable),
    }
}

pub(crate) fn token() -> Result<String, PickError> {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).map_err(|_| PickError::Failed)?;
    use std::fmt::Write;
    let mut token = String::with_capacity(37);
    token.push_str("rune_");
    for byte in bytes {
        write!(&mut token, "{byte:02x}").unwrap();
    }
    Ok(token)
}

fn response_uris(message: &Message, max_files: usize) -> Result<Vec<String>, PickError> {
    if message.body().len() > MAX_RESPONSE_BYTES {
        return Err(PickError::TooLarge);
    }
    let (status, mut fields): (u32, HashMap<String, OwnedValue>) = message
        .body()
        .deserialize()
        .map_err(|_| PickError::InvalidResponse)?;
    match status {
        1 => return Err(PickError::Cancelled),
        2 => return Err(PickError::Failed),
        0 => {}
        _ => return Err(PickError::InvalidResponse),
    }
    let uris = fields.remove("uris").ok_or(PickError::InvalidResponse)?;
    let uris = Vec::<String>::try_from(uris).map_err(|_| PickError::InvalidResponse)?;
    crate::file_dialogs::files::validate_count(uris.len(), max_files)?;
    if uris.iter().any(|uri| uri.len() > MAX_PATH_BYTES) {
        return Err(PickError::TooLarge);
    }
    Ok(uris)
}

#[cfg(test)]
pub(crate) mod tests;
