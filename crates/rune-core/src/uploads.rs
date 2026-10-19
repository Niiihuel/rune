//! Account-scoped attachment preparation; payloads never cross UI polling.

mod actor;
mod prepare;
mod runtime;

pub use rune_platform::file_dialogs::ParentWindow;
use rune_platform::file_dialogs::{FolderRequest, SaveRequest};

use rune_model::{
    ChannelId, MessageId, PendingAttachment, Permissions, ServerId, UploadToken, UserId,
};
use rune_provider::{
    AuthenticationState, Command, ConnectionState, Event, Failure, PreparedUpload, RequestId,
    UploadError, UploadFence, UploadLimits, UploadSendScope, UploadUpdate,
};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    runtime::Handle,
    sync::{mpsc, watch, Notify},
};

pub(super) use runtime::RuntimeOwner;

const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_JOBS: usize = 8;
const REQUIRED: Permissions =
    Permissions(Permissions::VIEW.0 | Permissions::SEND.0 | Permissions::ATTACH.0);
/// What preparing files in a conversation needs: an app's modal there takes
/// files without writing in it (comandos-de-apps); sending needs [`REQUIRED`].
const STAGED: Permissions = Permissions(Permissions::VIEW.0 | Permissions::ATTACH.0);

/// Extra time for a send's bytes on a slow link (128 KiB/s): a real upload
/// of a large file outlasts a text message's deadline.
fn transfer(bytes: u64) -> Duration {
    Duration::from_secs(bytes / (128 * 1024))
}

fn pick_error(error: rune_platform::file_dialogs::PickError) -> UploadError {
    use rune_platform::file_dialogs::PickError;
    match error {
        PickError::Cancelled => UploadError::Cancelled,
        PickError::TooLarge => UploadError::TooLarge,
        PickError::TooMany => UploadError::TooMany,
        PickError::PermissionDenied => UploadError::Denied,
        PickError::NotRegularFile | PickError::InvalidRequest | PickError::InvalidResponse => {
            UploadError::InvalidFile
        }
        PickError::TimedOut => UploadError::Expired,
        PickError::Unavailable => UploadError::Unavailable,
        PickError::Failed => UploadError::Failed,
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Queued,
    Waiting,
    Failed { ambiguous: bool },
}

struct SendControl {
    scope: UploadSendScope,
    tokens: Vec<UploadToken>,
    metadata: Vec<PendingAttachment>,
    reply_to: Option<MessageId>,
    control: watch::Sender<bool>,
    fence: UploadFence,
    phase: Phase,
    failure: Option<(UploadError, bool)>,
    progress: u64,
    total: u64,
    /// Past `scope.expires_at` while a verification holds the send: the
    /// provider sends it again from the start within this.
    challenged: Option<Instant>,
}

impl SendControl {
    fn until(&self) -> Instant {
        self.challenged.map_or(self.scope.expires_at, |until| {
            until.max(self.scope.expires_at)
        })
    }
}

struct PickControl {
    account: UserId,
    channel: ChannelId,
    request: RequestId,
    token: UploadToken,
    capacity: usize,
    max_total_bytes: u64,
    cancel: watch::Sender<bool>,
    result: Option<Result<Vec<PendingAttachment>, UploadError>>,
    /// The kept files a restore left out, by name, to say before its result.
    skipped: Vec<String>,
}

struct TokenControl {
    account: UserId,
    channel: ChannelId,
    metadata: PendingAttachment,
    /// Where it was read from, when it has a path: a draft keeps it.
    path: Option<PathBuf>,
}

#[derive(Default)]
struct Control {
    account: Option<UserId>,
    generation: u64,
    connected: bool,
    /// Conversations files are prepared for: their server, whether the
    /// account reads their history (replies) and whether it writes there
    /// (sends and posts; a modal's files only ask to attach).
    allowed: HashMap<ChannelId, (Option<ServerId>, bool, bool)>,
    tokens: HashMap<UploadToken, TokenControl>,
    sends: HashMap<u64, SendControl>,
    pick: Option<PickControl>,
    closed: bool,
    /// Forum posts, or apps' modals, asked for with prepared files, by
    /// request: the files stay prepared until it is made or answered, for
    /// another try.
    posts: HashMap<RequestId, Vec<UploadToken>>,
    /// Those whose files could not go, for their failure event.
    post_failures: Vec<(Failure, UploadError)>,
}

impl Control {
    fn account_for(&self, channel: ChannelId) -> Result<UserId, UploadError> {
        if !self.connected {
            return Err(UploadError::Unavailable);
        }
        if !self.allowed.contains_key(&channel) {
            return Err(UploadError::Denied);
        }
        self.account.ok_or(UploadError::Unavailable)
    }
    fn draft_usage(&self, channel: ChannelId) -> Result<(usize, u64), UploadError> {
        self.tokens
            .iter()
            .filter(|(token, entry)| {
                entry.channel == channel
                    && !self.sends.values().any(|send| send.tokens.contains(token))
            })
            .try_fold((0usize, 0u64), |(count, bytes), (_, entry)| {
                Ok((
                    count + 1,
                    bytes
                        .checked_add(entry.metadata.size_bytes)
                        .ok_or(UploadError::TooLarge)?,
                ))
            })
    }
    /// The account, when it may also write in `channel`: send there.
    fn sender_for(&self, channel: ChannelId) -> Result<UserId, UploadError> {
        let account = self.account_for(channel)?;
        match self.allowed.get(&channel) {
            Some((_, _, true)) => Ok(account),
            _ => Err(UploadError::Denied),
        }
    }
    fn can_reply(&self, channel: ChannelId) -> bool {
        self.allowed
            .get(&channel)
            .is_some_and(|(_, history, _)| *history)
    }
    fn fail(send: &mut SendControl, reason: UploadError, ambiguous: bool) {
        if matches!(send.phase, Phase::Failed { .. }) {
            return;
        }
        send.control.send_replace(true);
        send.phase = Phase::Failed { ambiguous };
        send.failure = Some((reason, ambiguous));
    }
    fn interrupt(&mut self, reason: UploadError) {
        if let Some(pick) = &self.pick {
            pick.cancel.send_replace(true);
        }
        for send in self.sends.values_mut() {
            Self::fail(send, reason, send.phase == Phase::Waiting);
        }
    }
    fn revoke(&mut self, channel: ChannelId) {
        self.allowed.remove(&channel);
        self.tokens.retain(|_, entry| entry.channel != channel);
        if let Some(pick) = &self.pick {
            if pick.channel == channel {
                pick.cancel.send_replace(true);
            }
        }
        for send in self
            .sends
            .values_mut()
            .filter(|send| send.scope.channel == channel)
        {
            Self::fail(send, UploadError::Denied, send.phase == Phase::Waiting);
        }
    }
    fn clear(&mut self) {
        self.interrupt(UploadError::Cancelled);
        self.account = None;
        self.connected = false;
        self.allowed.clear();
        self.tokens.clear();
        self.sends.clear();
        self.posts.clear();
    }

    /// Whether `channel` takes files: a conversation, or a forum's new posts.
    fn takes_files(channel: &rune_model::Channel) -> bool {
        (channel.kind.has_messages() || channel.kind == rune_model::ChannelKind::Forum)
            && channel.permissions.contains(STAGED)
    }

    /// What is kept of `channel`, which takes files.
    fn rights(channel: &rune_model::Channel) -> (Option<ServerId>, bool, bool) {
        (
            channel.server,
            channel.permissions.contains(Permissions::READ_HISTORY),
            channel.permissions.contains(REQUIRED),
        )
    }
}

enum PickInput {
    Picker(Option<ParentWindow>),
    Paths(Vec<PathBuf>),
    /// The files or the picture on the clipboard, read on the worker.
    Clipboard,
    /// The files a draft kept from an earlier run, each opened again alone.
    Restore(Vec<rune_storage::DraftFile>),
}

enum Job {
    Pick {
        account: UserId,
        channel: ChannelId,
        request: RequestId,
        token: UploadToken,
        capacity: usize,
        max_total_bytes: u64,
        input: PickInput,
        cancel: watch::Receiver<bool>,
    },
    Send {
        scope: UploadSendScope,
        content: String,
        reply_to: Option<MessageId>,
        mention_reply: bool,
    },
    Retry(UploadSendScope),
    /// A forum post, or an app's modal, naming prepared files: `command`
    /// without them.
    Post {
        command: Command,
        tokens: Vec<UploadToken>,
    },
    /// Where to save a download, or a folder: the same chooser as picking,
    /// one dialog at a time.
    Save {
        request: Destination,
        cancel: watch::Receiver<bool>,
        reply: tokio::sync::oneshot::Sender<Result<PathBuf, UploadError>>,
    },
    #[cfg(test)]
    Insert {
        account: UserId,
        channel: ChannelId,
        prepared: PreparedUpload,
    },
    #[cfg(test)]
    RetainCancelledRead {
        answer: tokio::sync::oneshot::Receiver<prepare::Answer>,
        installed: std::sync::mpsc::Sender<()>,
    },
}

/// What a [`Job::Save`] dialog chooses: a file to save, or a folder
/// (escritorio-linux: the call's sounds).
enum Destination {
    File(SaveRequest),
    Folder(FolderRequest),
}

/// The interface side contains only metadata and cancellation controls.
pub(super) struct Uploads {
    shared: Arc<Mutex<Control>>,
    changed: Arc<Notify>,
    jobs: mpsc::Sender<Job>,
    limits: Option<UploadLimits>,
    idempotent: bool,
    next_token: u64,
    budget: Arc<prepare::Budget>,
}

impl Uploads {
    pub(super) fn new(
        runtime: Handle,
        commands: mpsc::Sender<Command>,
        limits: Option<UploadLimits>,
        idempotent: bool,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        // Files are held in memory until sent: no more than the budget fits,
        // whatever the service would take.
        let limits = limits.map(|limits| UploadLimits {
            max_file_bytes: limits.max_file_bytes.min(prepare::MEMORY_BYTES as u64),
            max_message_bytes: limits
                .max_message_bytes
                .min((prepare::MEMORY_BYTES + MAX_TEXT_BYTES) as u64),
            ..limits
        });
        let shared = Arc::new(Mutex::new(Control::default()));
        let changed = Arc::new(Notify::new());
        let (jobs, receiver) = mpsc::channel(MAX_JOBS);
        let budget = Arc::new(prepare::Budget::default());
        runtime.spawn(actor::run(
            shared.clone(),
            changed.clone(),
            receiver,
            commands,
            limits,
            wake,
            budget.clone(),
        ));
        Self {
            shared,
            changed,
            jobs,
            limits,
            idempotent,
            next_token: 1,
            budget,
        }
    }

    pub(super) fn limits(&self) -> Option<UploadLimits> {
        self.limits
    }

    pub(super) fn pick(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        parent: Option<ParentWindow>,
    ) -> Result<(), UploadError> {
        self.prepare(request, channel, PickInput::Picker(parent))
    }

    pub(super) fn paths(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        paths: Vec<PathBuf>,
    ) -> Result<(), UploadError> {
        self.prepare(request, channel, PickInput::Paths(paths))
    }

    pub(super) fn clipboard(
        &mut self,
        request: RequestId,
        channel: ChannelId,
    ) -> Result<(), UploadError> {
        self.prepare(request, channel, PickInput::Clipboard)
    }

    /// Prepare again the files `channel`'s draft kept from an earlier run.
    pub(super) fn restore(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        files: Vec<rune_storage::DraftFile>,
    ) -> Result<(), UploadError> {
        self.prepare(request, channel, PickInput::Restore(files))
    }

    /// Where the files `tokens` of `channel`'s draft were read from, in
    /// order, with their sizes, to keep them for the next run. One read
    /// from no path (a pasted picture) or from a path that is not UTF-8 is
    /// left out: shortcut: it is not restored, keep its bytes if wanted.
    pub(super) fn draft_files(
        &self,
        channel: ChannelId,
        tokens: &[UploadToken],
    ) -> Vec<rune_storage::DraftFile> {
        let control = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        tokens
            .iter()
            .filter_map(|token| control.tokens.get(token))
            .filter(|entry| entry.channel == channel)
            .filter_map(|entry| {
                Some(rune_storage::DraftFile {
                    path: entry.path.as_ref()?.to_str()?.to_owned(),
                    size_bytes: entry.metadata.size_bytes,
                })
            })
            .collect()
    }

    fn prepare(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        mut input: PickInput,
    ) -> Result<(), UploadError> {
        let limits = self.limits.ok_or(UploadError::Unavailable)?;
        // A draft's kept files wait for room instead of failing without it.
        let full = if matches!(input, PickInput::Restore(_)) {
            UploadError::Busy
        } else {
            UploadError::TooMany
        };
        if request == 0 || channel.0 == 0 {
            return Err(UploadError::InvalidFile);
        }
        let mut control = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let account = control.account_for(channel)?;
        if control.pick.is_some() {
            return Err(UploadError::Busy);
        }
        let (draft_count, draft_bytes) = control.draft_usage(channel)?;
        let (used_bytes, used_entries) = self.budget.used();
        let capacity = limits
            .max_files
            .min(prepare::MAX_ENTRIES)
            .saturating_sub(draft_count)
            .min(prepare::MAX_ENTRIES.saturating_sub(control.tokens.len().max(used_entries)));
        if capacity == 0 {
            return Err(full);
        }
        if let PickInput::Paths(paths) = &input {
            rune_platform::file_dialogs::validate_local_paths(paths, capacity)
                .map_err(pick_error)?;
            // The bounded job retains path metadata, never file contents.
            // Count allocation capacity as well as the validator's lengths.
            if paths.capacity() > prepare::MAX_ENTRIES
                || paths
                    .iter()
                    .try_fold(0usize, |sum, path| sum.checked_add(path.capacity()))
                    .is_none_or(|bytes| bytes > prepare::MAX_ENTRIES * 4096)
            {
                return Err(UploadError::TooLarge);
            }
        }
        let max_total_bytes = limits
            .max_message_bytes
            .checked_sub(draft_bytes)
            .ok_or(UploadError::TooLarge)?
            .min(prepare::MEMORY_BYTES.saturating_sub(used_bytes) as u64);
        if let PickInput::Restore(files) = &mut input {
            // Each path once (a draft can name a file twice), each checked
            // alone by the worker: one bad path never loses the rest.
            let mut seen = std::collections::HashSet::new();
            files.retain(|file| seen.insert(file.path.clone()));
            let bytes = files
                .iter()
                .fold(0u64, |sum, file| sum.saturating_add(file.size_bytes));
            // Room other files hold now frees later, so wait for it rather
            // than leave out files still there. Ones that could never fit
            // are left out by name as the worker reaches the limit.
            let could_fit = files.len() <= limits.max_files.min(prepare::MAX_ENTRIES)
                && bytes <= limits.max_message_bytes.min(prepare::MEMORY_BYTES as u64);
            if could_fit && (files.len() > capacity || bytes > max_total_bytes) {
                return Err(UploadError::Busy);
            }
        }
        if control
            .sends
            .values()
            .any(|send| send.scope.request == request)
        {
            return Err(UploadError::Busy);
        }
        let token = UploadToken(self.next_token);
        self.next_token = self
            .next_token
            .checked_add(capacity as u64)
            .ok_or(UploadError::Failed)?;
        let (cancel, signal) = watch::channel(false);
        self.jobs
            .try_send(Job::Pick {
                account,
                channel,
                request,
                token,
                capacity,
                max_total_bytes,
                input,
                cancel: signal,
            })
            .map_err(|_| UploadError::Busy)?;
        control.pick = Some(PickControl {
            account,
            channel,
            request,
            token,
            capacity,
            max_total_bytes,
            cancel,
            result: None,
            skipped: Vec::new(),
        });
        Ok(())
    }

    pub(super) fn send(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        content: String,
        reply_to: Option<MessageId>,
        mention_reply: bool,
        tokens: Vec<UploadToken>,
        nonce: u64,
        timeout: Duration,
    ) -> Result<(), UploadError> {
        let limits = self.limits.ok_or(UploadError::Unavailable)?;
        if request == 0
            || nonce == 0
            || content.capacity() > MAX_TEXT_BYTES
            || content.len() > MAX_TEXT_BYTES
        {
            return Err(UploadError::TooLarge);
        }
        if tokens.is_empty() {
            return Err(UploadError::NotFound);
        }
        if tokens.len() > limits.max_files.min(prepare::MAX_ENTRIES) {
            return Err(UploadError::TooMany);
        }
        let mut control = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let account = control.sender_for(channel)?;
        if reply_to.is_some() && !control.can_reply(channel) {
            return Err(UploadError::Denied);
        }
        if control.sends.len() >= prepare::MAX_ENTRIES
            || control.sends.contains_key(&nonce)
            || control
                .sends
                .values()
                .any(|send| send.scope.request == request)
        {
            return Err(UploadError::Busy);
        }
        if control
            .pick
            .as_ref()
            .is_some_and(|pick| pick.channel == channel)
        {
            return Err(UploadError::Busy);
        }
        let mut total = 0u64;
        let mut metadata = Vec::with_capacity(tokens.len());
        for (index, token) in tokens.iter().enumerate() {
            if tokens[..index].contains(token) {
                return Err(UploadError::InvalidFile);
            }
            let entry = control
                .tokens
                .get(token)
                .filter(|entry| entry.account == account && entry.channel == channel)
                .ok_or(UploadError::NotFound)?;
            if control
                .sends
                .values()
                .any(|send| send.tokens.contains(token))
            {
                return Err(UploadError::Busy);
            }
            if entry.metadata.size_bytes > limits.max_file_bytes {
                return Err(UploadError::TooLarge);
            }
            total = total
                .checked_add(entry.metadata.size_bytes)
                .ok_or(UploadError::TooLarge)?;
            metadata.push(entry.metadata.clone());
        }
        if total
            .checked_add(content.len() as u64)
            .is_none_or(|bytes| bytes > limits.max_message_bytes)
        {
            return Err(UploadError::TooLarge);
        }
        let scope = UploadSendScope {
            request,
            account,
            channel,
            generation: control.generation,
            nonce,
            expires_at: Instant::now() + timeout + transfer(total),
        };
        let (sender, fence) = UploadFence::new();
        self.jobs
            .try_send(Job::Send {
                scope,
                content,
                reply_to,
                mention_reply,
            })
            .map_err(|_| UploadError::Busy)?;
        control.sends.insert(
            nonce,
            SendControl {
                scope,
                tokens,
                metadata,
                reply_to,
                control: sender,
                fence,
                phase: Phase::Queued,
                failure: None,
                progress: 0,
                total,
                challenged: None,
            },
        );
        Ok(())
    }

    pub(super) fn retry(
        &mut self,
        request: RequestId,
        nonce: u64,
        timeout: Duration,
    ) -> Result<(), UploadError> {
        let mut control = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = control.sends.get(&nonce).ok_or(UploadError::NotFound)?;
        let Phase::Failed { ambiguous } = previous.phase else {
            return Err(UploadError::Busy);
        };
        if ambiguous && !self.idempotent {
            return Err(UploadError::Unavailable);
        }
        if request == 0
            || control
                .sends
                .values()
                .any(|send| send.scope.request == request)
        {
            return Err(UploadError::Busy);
        }
        let account = control.sender_for(previous.scope.channel)?;
        if previous.reply_to.is_some() && !control.can_reply(previous.scope.channel) {
            return Err(UploadError::Denied);
        }
        if previous.scope.account != account
            || previous
                .tokens
                .iter()
                .any(|token| !control.tokens.contains_key(token))
        {
            return Err(UploadError::NotFound);
        }
        let scope = UploadSendScope {
            request,
            generation: control.generation,
            expires_at: Instant::now() + timeout + transfer(previous.total),
            ..previous.scope
        };
        self.jobs
            .try_send(Job::Retry(scope))
            .map_err(|_| UploadError::Busy)?;
        let previous = control.sends.get_mut(&nonce).unwrap();
        let (sender, fence) = UploadFence::new();
        previous.scope = scope;
        previous.control = sender;
        previous.fence = fence;
        previous.phase = Phase::Queued;
        previous.failure = None;
        previous.progress = 0;
        previous.challenged = None;
        Ok(())
    }

    /// Ask where to save `suggested`; the answer comes on the receiver. The
    /// dialog closes when `cancel` turns true or its sender is dropped.
    pub(super) fn choose_destination(
        &self,
        request: RequestId,
        suggested: String,
        parent: Option<ParentWindow>,
        cancel: watch::Receiver<bool>,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<PathBuf, UploadError>>, UploadError> {
        let request = SaveRequest {
            id: rune_platform::file_dialogs::PickId(request),
            parent,
            suggested_name: suggested,
            current_folder: None,
        };
        request.validate().map_err(pick_error)?;
        self.dialog(Destination::File(request), cancel)
    }

    /// Ask for a folder (escritorio-linux), as [`Self::choose_destination`]
    /// asks for a file: the same dialog slot, nothing opened or listed.
    pub(super) fn choose_folder(
        &self,
        request: RequestId,
        parent: Option<ParentWindow>,
        cancel: watch::Receiver<bool>,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<PathBuf, UploadError>>, UploadError> {
        let request = FolderRequest {
            id: rune_platform::file_dialogs::PickId(request),
            parent,
            current_folder: None,
        };
        request.validate().map_err(pick_error)?;
        self.dialog(Destination::Folder(request), cancel)
    }

    fn dialog(
        &self,
        request: Destination,
        cancel: watch::Receiver<bool>,
    ) -> Result<tokio::sync::oneshot::Receiver<Result<PathBuf, UploadError>>, UploadError> {
        let (reply, answer) = tokio::sync::oneshot::channel();
        self.jobs
            .try_send(Job::Save {
                request,
                cancel,
                reply,
            })
            .map_err(|_| UploadError::Busy)?;
        self.changed.notify_one();
        Ok(answer)
    }

    pub(super) fn cancel(&mut self, request: RequestId) {
        let mut control = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(pick) = &control.pick {
            if pick.request == request {
                pick.cancel.send_replace(true);
            }
        }
        for send in control
            .sends
            .values_mut()
            .filter(|send| send.scope.request == request)
        {
            Control::fail(send, UploadError::Cancelled, send.phase == Phase::Waiting);
        }
        self.changed.notify_one();
    }

    /// Ask for the post `command` (a `CreatePost` without files), or the
    /// modal's answer `command` (an `Interact` without files), with the
    /// files `tokens` prepared for its channel. They stay prepared until
    /// the post is made or the app answers.
    pub(super) fn post(
        &mut self,
        command: Command,
        tokens: Vec<UploadToken>,
    ) -> Result<(), UploadError> {
        let (request, forum, modal) = match &command {
            Command::CreatePost { request, forum, .. } => (*request, *forum, false),
            Command::Interact {
                request, channel, ..
            } => (*request, *channel, true),
            _ => return Err(UploadError::InvalidFile),
        };
        let mut control = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let account = if modal {
            control.account_for(forum)?
        } else {
            control.sender_for(forum)?
        };
        let limits = self.limits.ok_or(UploadError::Unavailable)?;
        if tokens.is_empty() || tokens.len() > limits.max_files {
            return Err(UploadError::TooMany);
        }
        let mut total = 0u64;
        for (index, token) in tokens.iter().enumerate() {
            let entry = control
                .tokens
                .get(token)
                .filter(|entry| entry.account == account && entry.channel == forum)
                .ok_or(UploadError::NotFound)?;
            total = total
                .checked_add(entry.metadata.size_bytes)
                .ok_or(UploadError::TooLarge)?;
            if tokens[..index].contains(token)
                || control
                    .sends
                    .values()
                    .any(|send| send.tokens.contains(token))
                // A modal's earlier answer with them is no longer waited for.
                || (!modal && control.posts.values().flatten().any(|held| held == token))
            {
                return Err(UploadError::Busy);
            }
        }
        if total > limits.max_message_bytes {
            return Err(UploadError::TooLarge);
        }
        if control.posts.contains_key(&request) {
            return Err(UploadError::Busy);
        }
        self.jobs
            .try_send(Job::Post {
                command,
                tokens: tokens.clone(),
            })
            .map_err(|_| UploadError::Busy)?;
        // One modal is waited for at a time: a newer one with these files
        // means the last is not, and its late answer takes none of them.
        if modal {
            control
                .posts
                .retain(|_, held| !held.iter().any(|token| tokens.contains(token)));
        }
        control.posts.insert(request, tokens);
        Ok(())
    }

    pub(super) fn discard(&mut self, token: UploadToken) {
        let mut control = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        control.tokens.remove(&token);
        // A post or modal without it no longer holds it.
        control.posts.retain(|_, held| {
            held.retain(|kept| *kept != token);
            !held.is_empty()
        });
        for send in control
            .sends
            .values_mut()
            .filter(|send| send.tokens.contains(&token))
        {
            Control::fail(send, UploadError::Cancelled, send.phase == Phase::Waiting);
        }
        self.changed.notify_one();
    }

    pub(super) fn clear(&mut self) {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
        self.changed.notify_one();
    }

    pub(super) fn close(&mut self) {
        let mut state = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.closed = true;
        state.clear();
        self.changed.notify_one();
    }

    pub(super) fn observe(&mut self, generation: u64, event: &Event) -> bool {
        let mut state = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if generation != state.generation {
            state.interrupt(UploadError::Unavailable);
            state.generation = generation;
        }
        match event {
            Event::Ready(snapshot) => {
                let account = snapshot.me.as_ref().map(|me| me.id);
                if state.account != account {
                    state.clear();
                } else {
                    state.interrupt(UploadError::Unavailable);
                }
                state.account = account;
                state.connected = account.is_some();
                state.allowed = snapshot
                    .channels
                    .iter()
                    .filter(|channel| Control::takes_files(channel))
                    .map(|channel| (channel.id, Control::rights(channel)))
                    .collect();
                let allowed = &state.allowed.clone();
                state
                    .tokens
                    .retain(|_, entry| allowed.contains_key(&entry.channel));
            }
            Event::Authentication(
                AuthenticationState::SignedOut | AuthenticationState::Failed { .. },
            ) => state.clear(),
            Event::Connection(ConnectionState::Connected) => {
                state.connected = state.account.is_some()
            }
            Event::Connection(_) => {
                state.connected = false;
                state.interrupt(UploadError::Unavailable);
            }
            Event::ChannelChanged(channel) => {
                if Control::takes_files(channel) {
                    let rights = Control::rights(channel);
                    let (_, history, writes) = rights;
                    state.allowed.insert(channel.id, rights);
                    // What it may no longer send stops; its files stay prepared.
                    for send in state.sends.values_mut().filter(|send| {
                        send.scope.channel == channel.id
                            && (!writes || (!history && send.reply_to.is_some()))
                    }) {
                        Control::fail(send, UploadError::Denied, send.phase == Phase::Waiting);
                    }
                } else {
                    state.revoke(channel.id);
                }
            }
            Event::ChannelRemoved { channel } => state.revoke(*channel),
            Event::ServerRemoved { server, .. } => {
                let channels: Vec<_> = state
                    .allowed
                    .iter()
                    .filter_map(|(channel, (owner, ..))| {
                        (*owner == Some(*server)).then_some(*channel)
                    })
                    .collect();
                for channel in channels {
                    state.revoke(channel);
                }
            }
            Event::MessageCreated {
                message,
                nonce: Some(nonce),
            } => {
                if let Some(send) = state.sends.get(nonce) {
                    if send.scope.account != message.author
                        || send.scope.channel != message.channel
                        || state.account != Some(message.author)
                        || send.scope.generation != generation
                    {
                        return false;
                    }
                    let complete = message.id.0 != 0
                        && message.delivery == rune_model::Delivery::Sent
                        && message.kind == rune_model::MessageKind::Default
                        && message.attachments.len() == send.metadata.len()
                        && message.attachments.iter().enumerate().all(|(index, file)| {
                            !message.attachments[..index]
                                .iter()
                                .any(|previous| previous.id == file.id)
                        })
                        // A service may rewrite a name (Discord turns spaces
                        // into underscores); order and sizes identify the bytes.
                        && message.attachments.iter().zip(&send.metadata).all(
                            |(remote, expected)| {
                                remote.id.0 != 0
                                    && !remote.filename.is_empty()
                                    && remote.size_bytes == expected.size_bytes
                            },
                        );
                    if !complete {
                        let send = state.sends.get_mut(nonce).unwrap();
                        // A same-scope positive reply with a missing/different
                        // file cannot confirm the frozen body. Keep its bytes;
                        // this transport outcome is unknown, never text-only.
                        send.control.send_replace(true);
                        send.phase = Phase::Failed { ambiguous: true };
                        send.failure = Some((UploadError::Failed, true));
                        return false;
                    }
                    let tokens = send.tokens.clone();
                    for token in tokens {
                        state.tokens.remove(&token);
                    }
                    state.sends.remove(nonce);
                }
            }
            Event::Failed {
                failure:
                    Failure::SendFiles {
                        request,
                        channel,
                        nonce,
                    },
                ambiguous,
                ..
            } => {
                let Some(send) = state.sends.get_mut(nonce) else {
                    return false;
                };
                if send.scope.request != *request
                    || send.scope.channel != *channel
                    || !(send.phase == Phase::Waiting
                        || (matches!(send.phase, Phase::Failed { ambiguous: true }) && !*ambiguous))
                {
                    return false;
                }
                send.control.send_replace(true);
                send.phase = Phase::Failed {
                    ambiguous: *ambiguous,
                };
                send.failure = None;
            }
            // A post made, or a modal answered: its files went with it. One
            // that failed keeps them.
            Event::Created { request, .. }
            | Event::Interaction {
                request,
                done: true,
            } => {
                for token in state.posts.remove(request).unwrap_or_default() {
                    state.tokens.remove(&token);
                }
            }
            Event::Failed {
                failure: Failure::NewThread { request } | Failure::Interaction { request, .. },
                ..
            } => {
                state.posts.remove(request);
            }
            Event::UploadProgress {
                scope,
                sent_bytes,
                total_bytes,
            } => {
                let Some(send) = state.sends.get_mut(&scope.nonce) else {
                    return false;
                };
                if send.scope != *scope
                    || send.phase != Phase::Waiting
                    || *sent_bytes < send.progress
                    || sent_bytes > total_bytes
                    || *total_bytes != send.total
                {
                    return false;
                }
                send.progress = *sent_bytes;
            }
            _ => {}
        }
        self.changed.notify_one();
        true
    }

    pub(super) fn expire(&mut self, now: Instant) {
        let mut state = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for send in state.sends.values_mut().filter(|send| send.until() <= now) {
            Control::fail(send, UploadError::Expired, send.phase == Phase::Waiting);
        }
        self.changed.notify_one();
    }

    pub(super) fn deadline(&self) -> Option<Instant> {
        self.shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .sends
            .values()
            .filter(|send| !matches!(send.phase, Phase::Failed { .. }))
            .map(SendControl::until)
            .min()
    }

    /// The send `request` waits on a person's verification, from `now`:
    /// its time runs again, whole, after `wait` (the most it may wait, plus
    /// the send's own margin), and its progress starts over with the retry.
    pub(super) fn challenged(&mut self, request: RequestId, now: Instant, wait: Duration) {
        let mut state = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(send) = state
            .sends
            .values_mut()
            .find(|send| send.scope.request == request && send.phase == Phase::Waiting)
        {
            send.challenged = Some(now + wait + transfer(send.total));
            send.progress = 0;
        }
        self.changed.notify_one();
    }

    pub(super) fn failures(&mut self, found: &mut VecDeque<Event>) {
        let mut state = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (failure, error) in std::mem::take(&mut state.post_failures) {
            if let Failure::NewThread { request } | Failure::Interaction { request, .. } = &failure
            {
                state.posts.remove(request);
            }
            found.push_back(Event::Failed {
                failure,
                reason: error.label().into(),
                ambiguous: false,
            });
        }
        for send in state.sends.values_mut() {
            if let Some((error, ambiguous)) = send.failure.take() {
                found.push_back(Event::Failed {
                    failure: Failure::SendFiles {
                        request: send.scope.request,
                        channel: send.scope.channel,
                        nonce: send.scope.nonce,
                    },
                    reason: error.label().into(),
                    ambiguous,
                });
            }
        }
        let tokens = state.tokens.keys().copied().collect::<Vec<_>>();
        state.sends.retain(|_, send| {
            !matches!(send.phase, Phase::Failed { .. })
                || send.tokens.iter().any(|token| tokens.contains(token))
        });
        self.changed.notify_one();
    }

    pub(super) fn poll(&mut self) -> Option<UploadUpdate> {
        let mut state = self
            .shared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let pick = state.pick.as_mut()?;
        pick.result.as_ref()?;
        // What a restore left out is said first, while the pick is current.
        if !pick.skipped.is_empty() {
            return Some(UploadUpdate::Skipped {
                account: pick.account,
                channel: pick.channel,
                request: pick.request,
                files: std::mem::take(&mut pick.skipped),
            });
        }
        let pick = state.pick.take().unwrap();
        let result = pick.result.unwrap();
        let same_account = state.account == Some(pick.account);
        // Retire the original UI request even after sign-out/account switch.
        // The terminal carries only its old scope and a cancellation error.
        let result = result.and_then(|metadata| {
            if same_account
                && !*pick.cancel.borrow()
                && !metadata.is_empty()
                && metadata
                    .iter()
                    .all(|file| state.tokens.contains_key(&file.token))
            {
                Ok(metadata)
            } else {
                // A batch is one publication. Release every sibling through
                // the actor even when only one token was revoked after ready.
                for file in metadata {
                    state.tokens.remove(&file.token);
                }
                self.changed.notify_one();
                Err(UploadError::Cancelled)
            }
        });
        Some(UploadUpdate::Prepared {
            account: pick.account,
            channel: pick.channel,
            request: pick.request,
            result: result.map_err(|error| {
                if same_account {
                    error
                } else {
                    UploadError::Cancelled
                }
            }),
        })
    }
}

impl Drop for Uploads {
    fn drop(&mut self) {
        self.close();
    }
}

#[cfg(test)]
mod tests;
