//! RUNE's services, between the interface and a provider. The [`Client`]
//! runs the provider's sessions on a small Tokio runtime of its own, connects
//! again after a transient loss with a growing, jittered delay, keeps local
//! storage on a thread of its own, and hands the interface what happened in
//! [`Update`]s it drains each frame. Provider commands are queued without
//! waiting and answers are polled. Storage backpressure retains one pending
//! change and leaves later provider events in their bounded queue. Only
//! startup reads wait for the storage thread's answer.
//!
//! A read asked before the session is up waits for it instead of being
//! refused (`connecting`).
//!
//! No command is left hanging. One that cannot be queued fails at once; a
//! page of history or a message that gets no answer in time fails then, and
//! a message in flight when the connection drops is marked as maybe sent.
//! Events of a session that has ended are dropped by its number.

mod clips;
mod context;
mod documents;
mod files;
mod gifs;
mod group_icons;
mod images;
mod invitation_acceptance;
mod invitations;
mod local_history;
mod profile_images;
// Administration (19).
mod expression_images;
mod read_states;
mod storage;
mod uploads;
// Encrypted local cache (2026-10-18).
mod keys;
// Reactions, pins and search (2026-10-18).
mod tallies;
// Linux desktop (escritorio-linux).
mod folders;
// Own themes (temas-propios).
mod theme_files;
// AutoMod (automod).
mod automod;
// Reads before the session is up (arranque-sin-avisos).
mod connecting;

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use rune_i18n::{t, tr};
use rune_model::{ChannelId, Message, MessageId, Permissions};
use rune_provider::{
    AuthenticationState, Command, ConnectionState, Envelope, Event, EventSink, Failure,
    LatestValues, Provider, ProviderError, ProviderInfo, RequestId, Snapshot, StorageIdentity,
};
use rune_storage::{MessageMutation, Storage};
use tokio::sync::mpsc;

pub use clips::{ClipResult, MAX_CLIP_BYTES};
pub use documents::{
    highlight_page, DocumentError, DocumentResult, DocumentText, MAX_DOCUMENT_BYTES,
};
pub use expression_images::{prepare as prepare_expression_image, PickedExpressionImage};
pub use files::{Copied, DownloadResult, SaveUpdate, MAX_DOWNLOAD_BYTES, SAVE_EXISTS};
pub use gifs::{GifSearch, GiphyConfig};
pub(crate) use images::validated_resource_url;
pub use images::{
    Animation, AnimationFrame, Encoding, ImageData, IMAGE_ENTRIES, IMAGE_TEXTURE_ENTRIES,
};
// Pictures on disk between runs (imagenes-y-lectura).
pub use images::{
    CacheLimits as ImageCacheLimits, CachePart as ImageCachePart, CacheUsage as ImageCacheUsage,
};
pub use local_history::LocalReadResult;

/// Decode and fit `bytes` as the image service would the picture `source`
/// names, for pictures made on this machine (the capture's simulated ones).
/// `pixels_per_point` is the screen's, as [`Client::set_image_density`] takes it.
pub fn normalize_image(
    bytes: &[u8],
    source: &rune_model::RemoteImage,
    pixels_per_point: f32,
) -> Option<ImageData> {
    let path = url::Url::parse(&source.url).ok()?;
    images::normalize_at(
        path.path(),
        bytes,
        source.kind,
        images::density_step(pixels_per_point),
    )
}
pub use profile_images::{
    prepare as prepare_profile_image, source as profile_image_source, PickedProfileImage,
    PictureSource,
};
use storage::CacheRequest;
pub use storage::{Restored, StorageTicket, StorageWorker, StorageWriteError};
pub use uploads::ParentWindow;
// Linux desktop (escritorio-linux).
pub use folders::ChosenFolder;
// Own themes (temas-propios).
pub use theme_files::ThemeFile;

/// Messages fetched per page of history, as most services serve them.
pub const PAGE: usize = 50;
/// Messages kept on disk per conversation.
pub const CACHED_PER_CHANNEL: usize = 500;

/// What happened, for the interface.
#[derive(Clone, Debug, PartialEq)]
// Provider events are the common case; keep their existing value API without
// adding one heap allocation per message just to shrink the rare cache update.
#[allow(clippy::large_enum_variant)]
pub enum Update {
    /// From the provider, or about a command that never got its answer.
    Provider(Event),
    /// Kept on disk from an earlier run, oldest first: shown while the
    /// provider is asked for the newest.
    Cached {
        channel: ChannelId,
        messages: Vec<Message>,
    },
    /// What the local store kept of the account a provider named after
    /// verifying it, once its key was in: its last snapshot, drafts and
    /// reading marks, to show while the session loads, never as confirmed.
    Restored(Box<Restored>),
}

/// When to connect again after a loss: `base` doubled for each attempt up to
/// `max`, give or take a fifth so many clients do not return at once.
#[derive(Clone, Copy, Debug)]
pub struct Backoff {
    pub base: Duration,
    pub max: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self {
            base: Duration::from_millis(500),
            max: Duration::from_secs(30),
        }
    }
}

impl Backoff {
    /// The wait before attempt `attempt` (from 1), shifted by `jitter` in
    /// [-1, 1] by up to a fifth.
    pub fn delay(&self, attempt: u32, jitter: f64) -> Duration {
        let doubled = self
            .base
            .saturating_mul(1u32 << attempt.saturating_sub(1).min(16));
        let delay = doubled.min(self.max).as_secs_f64() * (1.0 + 0.2 * jitter.clamp(-1.0, 1.0));
        Duration::from_secs_f64(delay.max(0.0))
    }
}

/// A number in [-1, 1] that differs between attempts and processes.
fn jitter(attempt: u32) -> f64 {
    let seed = std::process::id() as u64 ^ (attempt as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    let mixed = (seed ^ (seed >> 29)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    (mixed >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0
}

/// How long an answer may take before the command counts as failed.
#[derive(Clone, Copy, Debug)]
pub struct Deadlines {
    pub history: Duration,
    pub send: Duration,
}

impl Default for Deadlines {
    fn default() -> Self {
        Self {
            history: Duration::from_secs(20),
            send: Duration::from_secs(30),
        }
    }
}

/// How the client starts.
pub struct ClientConfig {
    /// Called from other threads when something is ready to be polled.
    pub wake: Arc<dyn Fn() + Send + Sync>,
    /// Where the local store lives; `None` keeps nothing between runs.
    pub storage: Option<PathBuf>,
    /// Where each account's key for the store lives: the system keyring.
    /// `None` keeps every account's data in memory; settings are kept anyway.
    pub keyring: Option<Arc<dyn rune_platform::secrets::SecretStore>>,
    pub backoff: Backoff,
    pub deadlines: Deadlines,
    // Pictures on disk between runs (imagenes-y-lectura).
    /// Where downloaded pictures are kept between runs, sealed with the
    /// key of the store's account (`$XDG_CACHE_HOME/rune`). `None`, or an
    /// account without its key, keeps them in memory only.
    pub image_cache: Option<PathBuf>,
}

/// A history request in flight.
#[derive(Clone, Copy, Debug)]
struct Loading {
    request: RequestId,
    before: Option<MessageId>,
    since: Instant,
}

/// The single pins popout read, registered only after command admission.
#[derive(Clone, Copy, Debug)]
struct LoadingPins {
    channel: ChannelId,
    request: RequestId,
    since: Instant,
}

enum PendingStorage {
    Messages(Vec<MessageMutation>),
    Snapshot(Box<Snapshot>),
    /// An account's key, and the snapshot of its session if it was ready
    /// before the key came.
    Unlock(Box<storage::Unlocking>, Option<Box<Snapshot>>),
    /// What is kept of an account the person forgot.
    Forget(String),
}

/// A read revision baseline may still be arriving; only a known failure
/// should warn that new choices cannot be kept.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ReadMarksState {
    Pending,
    Writable,
    Unavailable,
}

impl ReadMarksState {
    fn restored(succeeded: bool) -> Self {
        if succeeded {
            Self::Writable
        } else {
            Self::Unavailable
        }
    }
}

/// How long startup waits for the key of an account known from the start.
const STARTUP_KEY: Duration = Duration::from_secs(2);

/// An account's name in the store: the provider's length first, so no
/// provider and account can pass for another pair.
fn namespace(identity: StorageIdentity) -> String {
    format!(
        "{}:{}:{}",
        identity.provider.len(),
        identity.provider,
        identity.account.0
    )
}

/// RUNE's connection to one provider.
pub struct Client {
    runtime: uploads::RuntimeOwner,
    uploads: uploads::Uploads,
    images: images::Images,
    downloads: files::Downloads,
    documents: documents::Documents,
    clips: clips::Clips,
    invitations: invitations::Invitations,
    invitation_acceptance: invitation_acceptance::PendingAcceptance,
    gifs: gifs::GifSearch,
    group_icons: group_icons::GroupIcons,
    profile_images: profile_images::ProfileImages,
    expression_images: expression_images::ExpressionImages,
    info: ProviderInfo,
    commands: mpsc::Sender<Command>,
    events: mpsc::Receiver<Envelope>,
    latest: LatestValues,
    /// The newest session seen; events of earlier ones are stale.
    generation: u64,
    storage: Option<StorageWorker>,
    storage_error: Option<String>,
    storage_notice: Option<&'static str>,
    account: String,
    storage_identity: Option<StorageIdentity>,
    storage_active: bool,
    read_marks_state: ReadMarksState,
    pending_storage: Option<PendingStorage>,
    storage_paused_at: Option<Instant>,
    cache_epoch: u64,
    cache_requests: HashMap<ChannelId, CacheRequest>,
    next_cache_request: u64,
    deadlines: Deadlines,
    next_request: RequestId,
    next_nonce: u64,
    loading: HashMap<ChannelId, Loading>,
    pins: Option<LoadingPins>,
    context: Option<context::LoadingContext>,
    local_history: local_history::LocalHistory,
    read_receipts: read_states::ReadReceipts,
    live: HashSet<RequestId>,
    /// Messages sent and not answered yet, by nonce.
    sending: HashMap<u64, (ChannelId, Instant)>,
    /// Votes sent and not answered yet (2026-10-18).
    votes: tallies::Votes,
    /// Failures the client found itself, to report on the next poll.
    found: VecDeque<Event>,
    // Sends and uploads (envios-y-subidas).
    /// The files drafts kept from an earlier run, until each comes back.
    draft_files: HashMap<ChannelId, Vec<rune_storage::DraftFile>>,
    // Encrypted local cache (2026-10-18).
    wake: Arc<dyn Fn() + Send + Sync>,
    keyring: Option<Arc<dyn rune_platform::secrets::SecretStore>>,
    key_sender: std::sync::mpsc::Sender<keys::KeyEvent>,
    key_events: std::sync::mpsc::Receiver<keys::KeyEvent>,
    /// The account the provider declared from the start, if any; the others
    /// are named by event and end with their session.
    static_identity: Option<StorageIdentity>,
    /// The key of `account` was handed to the storage worker.
    unlocked: bool,
    /// The key of `account` was asked of the keyring and has not come yet.
    fetching: bool,
    /// The account of the newest `Ready` since `account` was taken.
    ready_account: Option<rune_model::UserId>,
    /// That `Ready`, if it was of `account` and came before its key.
    unsaved_ready: Option<Box<Snapshot>>,
    /// The keyring's last operation: each waits for the one before, so a key
    /// is never made twice nor kept after its deletion.
    keyring_last: Option<tokio::task::JoinHandle<()>>,
    // Linux desktop (escritorio-linux).
    /// A folder being chosen through the system's dialog.
    folders: folders::Folders,
    // Own themes (temas-propios).
    /// A theme file being imported or exported.
    theme_files: theme_files::ThemeFiles,
    // Reads before the session is up (arranque-sin-avisos).
    connecting: connecting::Connecting,
}

impl Client {
    /// Start talking to `provider`: the first session begins at once.
    pub fn start(provider: Arc<dyn Provider>, config: ClientConfig) -> Self {
        let runtime = uploads::RuntimeOwner::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .thread_name("rune-core")
                .enable_all()
                .build()
                .expect("the runtime starts"),
        );
        let info = provider.info();
        let mut images = images::Images::new(
            runtime.handle().clone(),
            provider.image_origins(),
            config.wake.clone(),
        );
        // Profile editing (edicion-de-perfil): what the provider keeps itself.
        images.hosted = Some(provider.clone());
        images.disk.set_root(config.image_cache.clone());
        let downloads = files::Downloads::new(
            runtime.handle().clone(),
            provider.image_origins(),
            config.wake.clone(),
        );
        let documents = documents::Documents::new(
            runtime.handle().clone(),
            provider.image_origins(),
            config.wake.clone(),
        );
        let clips = clips::Clips::new(
            runtime.handle().clone(),
            provider.image_origins(),
            config.wake.clone(),
        );
        let invitations = invitations::Invitations::new(
            runtime.handle().clone(),
            provider.clone(),
            config.wake.clone(),
        );
        let gifs = gifs::GifSearch::new(runtime.handle().clone(), None, config.wake.clone());
        let storage_identity = provider.storage_identity();
        let legacy_namespace = provider.legacy_storage_namespace();
        let account = storage_identity.map(namespace).unwrap_or_default();
        let (commands, receiver) = mpsc::channel(256);
        let upload_limits = (info.capabilities.attachments && info.capabilities.send_messages)
            .then(|| provider.upload_limits())
            .flatten()
            .filter(|limits| {
                limits.max_files > 0 && limits.max_file_bytes > 0 && limits.max_message_bytes > 0
            });
        let group_icons = group_icons::GroupIcons::new(
            runtime.handle().clone(),
            commands.clone(),
            config.wake.clone(),
        );
        let profile_images =
            profile_images::ProfileImages::new(runtime.handle().clone(), config.wake.clone());
        let expression_images =
            expression_images::ExpressionImages::new(runtime.handle().clone(), config.wake.clone());
        let uploads = uploads::Uploads::new(
            runtime.handle().clone(),
            commands.clone(),
            upload_limits,
            info.capabilities.idempotent_send,
            config.wake.clone(),
        );
        let (sender, events) = mpsc::channel(1024);
        let latest = LatestValues::default();
        let sink = EventSink::new(sender, latest.clone(), config.wake.clone());
        runtime.spawn(supervise(provider, receiver, sink, config.backoff));
        // A display name is not an account identity. Never cache an
        // account's data merely because an interactive adapter was given a
        // path: without an identity the store keeps settings only.
        let (storage, storage_error) = match config.storage.map(|path| {
            Storage::open(&path)
                .and_then(|storage| {
                    if let (Some(legacy), Some(identity)) = (legacy_namespace, storage_identity) {
                        if storage
                            .legacy_snapshot::<Snapshot>(legacy)?
                            .and_then(|snapshot| snapshot.me)
                            .is_some_and(|user| user.id == identity.account)
                        {
                            storage.migrate_account(legacy, &account)?;
                        } else {
                            // Another account's, in plain text, that no key
                            // will ever seal and nothing reads: it goes.
                            storage.forget_account(legacy)?;
                        }
                    }
                    Ok(storage)
                })
                .map_err(|error| (path, error))
        }) {
            Some(Ok(storage)) => (
                Some(StorageWorker::start(
                    storage,
                    account.clone(),
                    config.wake.clone(),
                )),
                None,
            ),
            Some(Err((path, error))) => {
                eprintln!("rune: no local store at {}: {error}", path.display());
                (None, Some(error.to_string()))
            }
            None => (None, None),
        };
        let (key_sender, key_events) = std::sync::mpsc::channel();
        let mut client = Self {
            runtime,
            uploads,
            images,
            downloads,
            documents,
            clips,
            invitations,
            invitation_acceptance: Default::default(),
            gifs,
            group_icons,
            profile_images,
            expression_images,
            info,
            commands,
            events,
            latest,
            generation: 0,
            storage,
            storage_error,
            storage_notice: None,
            account,
            storage_identity,
            // Not before its key is in.
            storage_active: false,
            read_marks_state: ReadMarksState::Pending,
            pending_storage: None,
            storage_paused_at: None,
            cache_epoch: 0,
            cache_requests: HashMap::new(),
            next_cache_request: 1,
            deadlines: config.deadlines,
            next_request: 1,
            next_nonce: 1,
            loading: HashMap::new(),
            pins: None,
            context: None,
            local_history: Default::default(),
            read_receipts: Default::default(),
            live: HashSet::new(),
            sending: HashMap::new(),
            votes: tallies::Votes::default(),
            found: VecDeque::new(),
            draft_files: HashMap::new(),
            wake: config.wake,
            keyring: config.keyring,
            key_sender,
            key_events,
            static_identity: storage_identity,
            unlocked: false,
            fetching: false,
            ready_account: None,
            unsaved_ready: None,
            keyring_last: None,
            folders: Default::default(),
            theme_files: Default::default(),
            connecting: Default::default(),
        };
        client.request_key();
        client
    }

    /// Startup only: wait a little for the key of an account the provider
    /// named from the start, so the startup reads that follow find it. A
    /// change of provider does not wait: its key comes through `poll`.
    pub fn wait_for_key(&mut self) {
        if self.fetching {
            self.take_keys(Some(STARTUP_KEY));
            // Queued before any read startup makes.
            let _ = self.flush_pending_storage();
        }
    }

    pub fn info(&self) -> &ProviderInfo {
        &self.info
    }

    pub fn upload_limits(&self) -> Option<rune_provider::UploadLimits> {
        self.uploads.limits()
    }

    pub fn pick_attachment(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        parent: Option<ParentWindow>,
    ) -> Result<(), rune_provider::UploadError> {
        self.uploads.pick(request, channel, parent)
    }

    pub fn prepare_attachment_paths(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        paths: Vec<PathBuf>,
    ) -> Result<(), rune_provider::UploadError> {
        self.uploads.paths(request, channel, paths)
    }

    /// Prepare the files or the picture on the clipboard, as the selector's
    /// files are prepared; [`UploadError::Empty`](rune_provider::UploadError)
    /// when it holds neither. The clipboard is read on the files worker.
    pub fn paste_attachments(
        &mut self,
        request: RequestId,
        channel: ChannelId,
    ) -> Result<(), rune_provider::UploadError> {
        self.uploads.clipboard(request, channel)
    }

    /// Send the files `tokens` with `content`; a reply notifies its author
    /// only with `mention_reply`, kept for a retry as the rest of the body.
    pub fn send_attachments(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        content: String,
        reply_to: Option<MessageId>,
        mention_reply: bool,
        tokens: Vec<rune_model::UploadToken>,
    ) -> Result<u64, rune_provider::UploadError> {
        let nonce = self.fresh_nonce();
        self.uploads.send(
            request,
            channel,
            content,
            reply_to,
            mention_reply,
            tokens,
            nonce,
            self.deadlines.send,
        )?;
        Ok(nonce)
    }

    pub fn retry_attachment_send(
        &mut self,
        request: RequestId,
        nonce: u64,
    ) -> Result<(), rune_provider::UploadError> {
        self.uploads.retry(request, nonce, self.deadlines.send)
    }

    pub fn cancel_upload(&mut self, request: RequestId) {
        self.uploads.cancel(request);
    }

    /// Start a post in `forum` whose first message carries the files
    /// `tokens` prepared for that forum. Answered as a `CreatePost`: the
    /// files stay prepared until the post is made.
    #[allow(clippy::too_many_arguments)]
    pub fn create_post(
        &mut self,
        request: RequestId,
        forum: ChannelId,
        title: String,
        tags: Vec<rune_model::TagId>,
        content: String,
        tokens: Vec<rune_model::UploadToken>,
    ) -> Result<(), rune_provider::UploadError> {
        let command = Command::CreatePost {
            request,
            forum,
            title,
            tags,
            content,
            files: Vec::new(),
        };
        self.uploads.post(command, tokens)
    }

    /// Keep where the files `tokens` of `channel`'s draft are, for the next
    /// run, never their bytes; none forgets them. A pasted picture, read
    /// from no file, is not kept. Files from an earlier run still waiting
    /// for room to come back stay kept after them.
    pub fn set_draft_files(
        &self,
        channel: ChannelId,
        tokens: &[rune_model::UploadToken],
    ) -> Result<(), StorageWriteError> {
        if self.pending_storage.is_some() {
            return Err(StorageWriteError::Busy);
        }
        if self.storage_active {
            if let Some(storage) = &self.storage {
                let mut files = self.uploads.draft_files(channel, tokens);
                if let Some(waiting) = self.draft_files.get(&channel) {
                    files.extend_from_slice(waiting);
                }
                storage.set_draft_files(channel, files)?;
            }
        }
        Ok(())
    }

    /// Read the files drafts kept from an earlier run; each comes back with
    /// [`Self::restore_draft_files`]. Waits for the store's thread: startup
    /// only, for an account named from the start (the simulation). An
    /// account named later gets its own with [`Update::Restored`].
    pub fn load_draft_files(&mut self) {
        if let Some(storage) = self.storage.as_ref().filter(|_| self.storage_active) {
            self.draft_files = storage.draft_files().into_iter().collect();
        }
    }

    /// Whether `channel`'s draft has files from an earlier run to bring
    /// back, for the account whose store is in use.
    pub fn has_draft_files(&self, channel: ChannelId) -> bool {
        self.storage_active && self.draft_files.contains_key(&channel)
    }

    /// Prepare again, as `request`, the files `channel`'s draft kept from an
    /// earlier run, each checked as when it was chosen and the ones gone or
    /// changed named (`UploadUpdate::Skipped`); answered by
    /// [`Self::poll_upload`]. Only a busy service lets them wait for
    /// another try.
    pub fn restore_draft_files(
        &mut self,
        request: RequestId,
        channel: ChannelId,
    ) -> Result<(), rune_provider::UploadError> {
        if !self.has_draft_files(channel) {
            return Err(rune_provider::UploadError::NotFound);
        }
        let files = self.draft_files[&channel].clone();
        let result = self.uploads.restore(request, channel, files);
        if result != Err(rune_provider::UploadError::Busy) {
            self.draft_files.remove(&channel);
        }
        result
    }

    /// Send `command`, an app's modal answered (an `Interact` without
    /// files), with the files `tokens` prepared for its channel, which its
    /// answers name. Answered as an `Interact`: the files stay prepared
    /// until the app answers.
    pub fn interact_with_files(
        &mut self,
        command: Command,
        tokens: Vec<rune_model::UploadToken>,
    ) -> Result<(), rune_provider::UploadError> {
        self.uploads.post(command, tokens)
    }

    pub fn discard_attachment(&mut self, token: rune_model::UploadToken) {
        self.uploads.discard(token);
    }

    pub fn poll_upload(&mut self) -> Option<rune_provider::UploadUpdate> {
        self.uploads.poll()
    }

    /// Save an explicitly selected attachment; no account client is involved.
    pub fn download_file(
        &mut self,
        source: rune_model::RemoteFile,
        filename: &str,
        size: u64,
    ) -> Result<(), &'static str> {
        self.downloads.start(source, filename, size)
    }

    pub fn poll_downloads(&mut self) -> Vec<DownloadResult> {
        self.downloads.poll()
    }

    /// Put an explicitly chosen attachment on the system clipboard: its
    /// picture's pixels or the file itself (contenido-de-mensajes). It is
    /// fetched as a download is; [`Self::poll_downloads`] reports it.
    pub fn copy_file(
        &mut self,
        source: rune_model::RemoteFile,
        filename: &str,
        size: u64,
        copied: Copied,
    ) -> Result<(), &'static str> {
        self.downloads.copy(source, filename, size, copied)
    }

    /// «Save as»: the system dialog asks where, then the attachment downloads
    /// there under exactly the chosen name: over an existing regular file
    /// only once the person says so ([`SaveUpdate::Exists`], then
    /// [`Self::replace_save`]), never over a folder or a link.
    /// The link, size and a download slot are checked before the dialog opens.
    pub fn save_attachment_as(
        &mut self,
        request: RequestId,
        source: rune_model::RemoteFile,
        filename: &str,
        size: u64,
        parent: Option<ParentWindow>,
    ) -> Result<(), &'static str> {
        let url = self.downloads.admit(&source, size)?;
        let (cancel, signal) = tokio::sync::watch::channel(false);
        let chosen = self
            .uploads
            .choose_destination(request, files::safe_filename(filename), parent, signal)
            .map_err(|_| t("Esperá a que se cierre el otro diálogo de archivos."))?;
        self.downloads.save(request, url, size, chosen, cancel)
    }

    /// Close the dialog of `request`, or stop its download and remove the
    /// partial file; [`Self::poll_saves`] then reports it cancelled.
    pub fn cancel_save(&mut self, request: RequestId) -> bool {
        self.downloads.cancel_save(request)
    }

    // Linux desktop (escritorio-linux).
    /// Replace the file already at the name chosen for `request`'s «Save
    /// as», as the person confirmed when [`SaveUpdate::Exists`] asked.
    pub fn replace_save(&mut self, request: RequestId) -> bool {
        self.downloads.replace_save(request)
    }

    /// Let the person choose a folder through the system's dialog, over
    /// `parent` (the call's sounds): it comes back with
    /// [`Self::poll_folders`]. It takes the one dialog of attaching and
    /// «Save as»; nothing in the folder is opened.
    pub fn choose_folder(
        &mut self,
        request: u64,
        parent: Option<ParentWindow>,
    ) -> Result<(), &'static str> {
        let (cancel, signal) = tokio::sync::watch::channel(false);
        let chosen = self
            .uploads
            .choose_folder(request, parent, signal)
            .map_err(|_| t("Esperá a que se cierre el otro diálogo de archivos."))?;
        self.folders.wait(
            self.runtime.handle(),
            request,
            chosen,
            cancel,
            self.wake.clone(),
        );
        Ok(())
    }

    /// Close the folder dialog under way: what it chooses is dropped.
    pub fn cancel_folder(&mut self) {
        self.folders.cancel();
    }

    pub fn poll_folders(&mut self) -> Vec<ChosenFolder> {
        self.folders.poll()
    }

    // Own themes (temas-propios).
    /// Let the person choose a theme file through the system's dialog,
    /// over `parent`, and read it, at most 64 KB; it comes back with
    /// [`Self::poll_theme_files`], unchecked.
    pub fn import_theme(&mut self, request: u64, parent: Option<ParentWindow>) {
        self.theme_files
            .import(self.runtime.handle(), request, parent, self.wake.clone());
    }

    /// «Save as» for a theme file: the system's dialog asks where, then
    /// `contents` is written there as a new file, never over another; it
    /// comes back with [`Self::poll_theme_files`]. It takes the one dialog
    /// of attaching and «Save as».
    pub fn export_theme(
        &mut self,
        request: u64,
        filename: &str,
        contents: String,
        parent: Option<ParentWindow>,
    ) -> Result<(), &'static str> {
        let (cancel, signal) = tokio::sync::watch::channel(false);
        let chosen = self
            .uploads
            .choose_destination(request, files::safe_filename(filename), parent, signal)
            .map_err(|_| t("Esperá a que se cierre el otro diálogo de archivos."))?;
        self.theme_files.export(
            self.runtime.handle(),
            request,
            chosen,
            cancel,
            contents,
            self.wake.clone(),
        );
        Ok(())
    }

    pub fn poll_theme_files(&mut self) -> Vec<ThemeFile> {
        self.theme_files.poll()
    }

    pub fn poll_saves(&mut self) -> Vec<SaveUpdate> {
        self.downloads.poll_saves()
    }

    /// Render a document only after an explicit request, isolated from the UI.
    pub fn preview_document(
        &mut self,
        request: u64,
        source: rune_model::RemoteFile,
        size: u64,
    ) -> Result<(), DocumentError> {
        self.documents.start(request, source, size)
    }

    /// Another page of the document already open for `request`, with
    /// `marks` highlighted on it.
    pub fn document_page(
        &mut self,
        request: u64,
        page: u32,
        marks: Vec<rune_model::PdfMark>,
    ) -> Result<(), DocumentError> {
        self.documents.page(request, page, marks)
    }

    pub fn cancel_document(&mut self, request: u64) {
        self.documents.cancel(request);
    }

    pub fn poll_document(&mut self) -> Option<DocumentResult> {
        self.documents.poll()
    }

    /// Read the text of the document open for `request`, for its search.
    pub fn document_text(&mut self, request: u64) -> Result<(), DocumentError> {
        self.documents.text(request)
    }

    pub fn poll_document_text(&mut self) -> Option<DocumentText> {
        self.documents.poll_text()
    }

    /// Fetch a voice message's audio after the account plays it; replaces
    /// the one still downloading.
    pub fn load_voice(
        &mut self,
        request: u64,
        source: rune_model::RemoteFile,
        size: u64,
    ) -> Result<(), &'static str> {
        self.clips.start(request, source, size)
    }

    pub fn cancel_voice(&mut self, request: u64) {
        self.clips.cancel(request);
    }

    pub fn poll_voice(&mut self) -> Option<ClipResult> {
        self.clips.poll()
    }

    pub fn invitation_target(&self, url: &str) -> Option<rune_provider::InvitationTarget> {
        self.invitations.target(url)
    }

    pub fn preview_invitation(
        &mut self,
        request: RequestId,
        target: rune_provider::InvitationTarget,
    ) {
        self.invitations.start(request, target);
    }

    pub fn cancel_invitation(&mut self, request: RequestId) {
        self.invitations.cancel(request);
    }

    pub fn poll_invitation(&mut self) -> Option<rune_provider::InvitationResult> {
        self.invitations.poll()
    }

    /// Look up the server of an invitation card in the timeline, beside the
    /// dialog's lookup; at most eight run at once.
    pub fn preview_invitation_card(
        &mut self,
        request: RequestId,
        target: rune_provider::InvitationTarget,
    ) {
        self.invitations.start_card(request, target);
    }

    pub fn poll_invitation_card(&mut self) -> Option<rune_provider::InvitationResult> {
        self.invitations.poll_card()
    }

    /// Configure an independent GIF service, never the account provider's key.
    pub fn configure_gifs(&mut self, config: Option<gifs::GiphyConfig>) {
        self.gifs.configure(config);
    }

    pub fn gifs_available(&self) -> bool {
        self.gifs.available()
    }

    pub fn search_gifs(&mut self, request: u64, query: rune_provider::GifQuery, offset: u32) {
        self.gifs.start(request, query, offset);
    }

    /// Let the person choose a picture for the group `channel`, through the
    /// system's selector. It reaches the provider as a small PNG; a refusal
    /// comes back as the group's failure.
    pub fn pick_group_icon(&mut self, channel: ChannelId, parent: Option<ParentWindow>) {
        self.group_icons.pick(channel, parent);
    }

    /// Let the person choose a picture for the profile, through the system's
    /// selector: it comes back with [`Client::poll_profile_images`], made a
    /// bounded PNG of `kind`'s shape with its preview, or a GIF when it
    /// moves and `animated` allows it, and the whole picture to frame it.
    /// The dialog opens over `parent`. Nothing is sent.
    pub fn pick_profile_image(
        &mut self,
        request: u64,
        kind: rune_provider::ProfileImageKind,
        animated: bool,
        parent: Option<ParentWindow>,
    ) {
        self.profile_images.pick(request, kind, animated, parent);
    }

    /// Frame the picture last chosen again; it comes back as a choice does.
    pub fn frame_profile_image(
        &mut self,
        request: u64,
        kind: rune_provider::ProfileImageKind,
        crop: rune_provider::Crop,
        animated: bool,
    ) {
        self.profile_images.frame(request, kind, crop, animated);
    }

    /// Stop the choice of a profile picture under way.
    pub fn cancel_profile_image(&mut self) {
        self.profile_images.cancel();
    }

    pub fn poll_profile_images(&mut self) -> Vec<PickedProfileImage> {
        self.profile_images.poll()
    }

    /// Let the person choose the picture of a new emoji or sticker, through
    /// the system's selector: it comes back with
    /// [`Client::poll_expression_images`], made what `kind` asks for, with
    /// its preview. Nothing is sent.
    pub fn pick_expression_image(
        &mut self,
        request: u64,
        kind: rune_provider::ExpressionKind,
        parent: Option<ParentWindow>,
    ) {
        self.expression_images.pick(request, kind, parent);
    }

    pub fn poll_expression_images(&mut self) -> Vec<PickedExpressionImage> {
        self.expression_images.poll()
    }

    pub fn cancel_gifs(&mut self, request: u64) {
        self.gifs.cancel(request);
    }

    pub fn poll_gifs(&mut self) -> Option<rune_provider::GifResult> {
        self.gifs.poll()
    }

    /// Native image components request only public references they currently
    /// need. The service never receives the provider's account credentials.
    pub fn load_images(
        &mut self,
        wanted: &[rune_model::RemoteImage],
    ) -> Vec<(rune_model::RemoteImage, Arc<ImageData>)> {
        // The disk follows the account's store: a session it does not trust
        // (another account's Ready, a cache cut short) keeps nothing there.
        self.images.disk.trusted = self.storage_active;
        self.images.load(wanted)
    }

    // Pictures on disk between runs (imagenes-y-lectura).
    /// The budgets and retention of the pictures kept on disk. Nothing is
    /// opened before the first ones, which can come after the key.
    pub fn set_image_cache_limits(&mut self, limits: ImageCacheLimits) {
        self.images.disk.set_limits(limits);
    }

    /// The screen's pixels for each logical one: pictures are decoded for
    /// it, in steps, so a high density screen gets them sharp. The host
    /// passes it every frame; only a new step costs anything.
    pub fn set_image_density(&mut self, pixels_per_point: f32) {
        self.images.set_density(pixels_per_point);
    }

    /// What each part of the pictures on disk keeps, once the account's
    /// disk is open; `None` keeps them in memory only.
    pub fn image_cache_usage(&mut self) -> Option<ImageCacheUsage> {
        self.images.disk.usage()
    }

    /// Delete one part of the account's pictures on disk now.
    pub fn clear_image_cache(&mut self, part: ImageCachePart) {
        self.images.disk.clear(part);
    }

    /// Visible image failures, including provider-directed temporary backoff.
    pub fn image_failures(
        &self,
        wanted: &[rune_model::RemoteImage],
    ) -> HashSet<rune_model::RemoteImage> {
        self.images.failures(wanted)
    }

    /// Why persistence is unavailable or has failed, when applicable.
    pub fn storage_error(&self) -> Option<&str> {
        self.storage_error.as_deref().or(self.storage_notice)
    }

    /// Up to `max` of what happened since the last poll, oldest first:
    /// reliable events, then the latest values, then the failures the client
    /// found, then what the local store answered. Stale events are dropped,
    /// and messages are written to the local store in one batch.
    pub fn poll(&mut self, max: usize) -> Vec<Update> {
        let mut updates = Vec::new();
        let mut save = Vec::new();
        // A key waits for the pending write: it becomes one.
        if self.pending_storage.is_none() {
            self.take_keys(None);
        }
        let writable = self.flush_pending_storage().is_ok();
        if writable {
            self.resume_storage_deadlines(Instant::now());
        } else {
            self.storage_paused_at.get_or_insert_with(Instant::now);
        }
        while writable && updates.len() < max {
            let Ok(envelope) = self.events.try_recv() else {
                break;
            };
            if let Some(event) = self.accept(envelope, &mut save) {
                updates.push(Update::Provider(event));
            }
            if !save.is_empty() {
                self.pending_storage = Some(PendingStorage::Messages(std::mem::take(&mut save)));
            }
            if self.flush_pending_storage().is_err() {
                self.storage_paused_at.get_or_insert_with(Instant::now);
                break;
            }
        }
        let latest = self
            .latest
            .lock()
            .map(|mut latest| latest.drain().collect::<Vec<_>>())
            .unwrap_or_default();
        for (key, envelope) in latest {
            if envelope.generation == self.generation {
                updates.push(Update::Provider(envelope.event));
            } else if envelope.generation > self.generation {
                // Ready may still be behind storage backpressure. Keep its
                // latest values until the reliable session event is consumed.
                if let Ok(mut latest) = self.latest.lock() {
                    if latest
                        .get(&key)
                        .is_none_or(|held| held.generation < envelope.generation)
                    {
                        latest.insert(key, envelope);
                    }
                }
            }
        }
        self.expire(Instant::now());
        self.found.extend(self.group_icons.take_failures());
        updates.extend(self.found.drain(..).map(Update::Provider));
        if let Some(storage) = &self.storage {
            self.storage_error = storage.error();
            // Preview answers are optional. Always release their shared slots,
            // even when reliable provider events used this frame's quota, so
            // a terminal local-read answer can make progress.
            for _ in 0..storage::CACHED_ANSWERS {
                let Some((request, messages)) = storage.try_cached() else {
                    break;
                };
                let channel = request.channel;
                if !self.storage_active
                    || request.epoch != self.cache_epoch
                    || self.cache_requests.get(&channel) != Some(&request)
                {
                    continue;
                }
                self.cache_requests.remove(&channel);
                if updates.len() < max {
                    updates.push(Update::Cached { channel, messages });
                }
            }
            if let Some((account, mut restored)) = storage.try_restored() {
                let current = self.storage_identity.map(|identity| identity.account);
                if self.storage_active
                    && account == self.account
                    && current == Some(restored.account)
                {
                    // Like startup's read: without the marks kept, new ones
                    // stay in memory, so an older one cannot hide a newer.
                    self.read_marks_state = ReadMarksState::restored(restored.read_marks.is_ok());
                    self.storage_notice = (self.read_marks_state == ReadMarksState::Unavailable).then_some(t(
                        "Las marcas de lectura nuevas quedan sólo en memoria porque no se restauró su base local. Reiniciá o borrá los datos locales para restaurar esa persistencia.",
                    ));
                    // Its drafts' files come back as each conversation
                    // opens (envios-y-subidas): Core keeps them till then.
                    self.draft_files = std::mem::take(&mut restored.draft_files)
                        .into_iter()
                        .collect();
                    updates.push(Update::Restored(Box::new(restored)));
                }
            }
        }
        updates
    }

    /// Keep the books for one event of the provider; `None` drops it.
    fn accept(&mut self, envelope: Envelope, save: &mut Vec<MessageMutation>) -> Option<Event> {
        if envelope.generation < self.generation {
            return None;
        }
        if envelope.generation > self.generation {
            self.read_receipts.end_session(&mut self.found);
            self.invitation_acceptance.clear();
            self.invalidate_cached_reads();
            self.invalidate_local_history(rune_model::LocalReadError::Cancelled);
            if matches!(
                &envelope.event,
                Event::Ready(_)
                    | Event::Authentication(
                        AuthenticationState::SignedOut | AuthenticationState::Failed { .. }
                    )
            ) {
                self.context = None;
            } else {
                self.fail_context(t("La sesión cambió antes de abrir el mensaje."));
            }
            if matches!(
                envelope.event,
                Event::Ready(_)
                    | Event::Authentication(
                        AuthenticationState::SignedOut | AuthenticationState::Failed { .. }
                    )
            ) {
                self.pins = None;
            } else {
                self.fail_pins(t("la sesión cambió antes de cargar los mensajes fijados"));
            }
        }
        self.generation = envelope.generation;
        // AutoMod (automod): its refusal is the failure of its command.
        let mut event = automod::worded(envelope.event);
        if !self.uploads.observe(self.generation, &event) {
            if let Event::MessageCreated { message, nonce } = &mut event {
                if message.delivery != rune_model::Delivery::Sent
                    || message.id.0 == 0
                    || message.channel.0 == 0
                    || message.author.0 == 0
                    || message
                        .attachments
                        .iter()
                        .any(|attachment| attachment.id.0 == 0)
                {
                    return None;
                }
                // A colliding nonce, or an incomplete file confirmation, is
                // still a provider message. Do not correlate it to our send.
                *nonce = None;
            } else {
                return None;
            }
        }
        if !self
            .read_receipts
            .observe(&event, self.info.capabilities.read_state, &mut self.found)
        {
            return None;
        }
        if !self.accept_context(&event, save) {
            return None;
        }
        if !self.invitation_acceptance.observe(&event) {
            return None;
        }
        // Reactions, pins and search: live changes reach the store as well.
        self.votes.observe(&event);
        if self.storage_active {
            save.extend(tallies::mutation(&event));
        }
        match &event {
            Event::Authentication(
                AuthenticationState::SignedOut | AuthenticationState::Failed { .. },
            ) => {
                self.images.clear();
                self.downloads.clear();
                self.documents.clear();
                self.clips.clear();
                self.invitations.clear();
                self.gifs.clear();
                self.loading.clear();
                self.pins = None;
                self.live.clear();
                self.sending.clear();
                self.votes.clear();
                self.found.clear();
                save.clear();
                self.invalidate_cached_reads();
                self.storage_active = false;
                self.read_marks_state = ReadMarksState::Unavailable;
                self.invalidate_local_history(rune_model::LocalReadError::Unavailable);
                // An account named by event ends with its session.
                if self.storage_identity != self.static_identity {
                    self.adopt(None);
                }
            }
            // Encrypted local cache (2026-10-18): the core's own.
            Event::StorageIdentity(identity) => {
                self.adopt(Some(*identity));
                return None;
            }
            Event::ForgetStorage(identity) => {
                self.forget_account(*identity);
                return None;
            }
            // What a search of the whole history found is kept apart from it.
            Event::SearchResults {
                result: Ok(page), ..
            } if self.storage_active => {
                let found: Vec<Message> = page
                    .messages
                    .iter()
                    .filter(|message| !message.components.ephemeral)
                    .cloned()
                    .collect();
                if !found.is_empty() {
                    save.push(MessageMutation::Found(found));
                }
            }
            Event::HistoryLoaded {
                request,
                channel,
                messages,
                before,
                reached_start,
            } => {
                if !self.live.remove(request) {
                    return None;
                }
                if self
                    .loading
                    .get(channel)
                    .is_some_and(|loading| loading.request == *request)
                {
                    self.loading.remove(channel);
                }
                if self.storage_active {
                    save.push(MessageMutation::History {
                        channel: *channel,
                        before: *before,
                        messages: messages.clone(),
                        reached_start: *reached_start,
                    });
                }
            }
            Event::PinsLoaded {
                channel, request, ..
            }
            | Event::Failed {
                failure: Failure::Pins { channel, request },
                ..
            } => {
                if !self
                    .pins
                    .is_some_and(|pins| pins.channel == *channel && pins.request == *request)
                {
                    return None;
                }
                self.pins = None;
            }
            Event::MessageCreated { message, nonce } => {
                if let Some(nonce) = nonce {
                    self.sending.remove(nonce);
                }
                // A reply only the account sees lives for this session (P3).
                if self.storage_active && !message.components.ephemeral {
                    save.push(MessageMutation::Upsert(Box::new(message.clone())));
                }
            }
            Event::MessageUpdated(message)
                if self.storage_active && !message.components.ephemeral =>
            {
                save.push(MessageMutation::Upsert(Box::new(message.clone())));
            }
            Event::MessagePatched(patch) if self.storage_active => {
                save.push(MessageMutation::Patch(Box::new(patch.clone())));
            }
            Event::MessageDeleted { channel, message } if self.storage_active => {
                save.push(MessageMutation::Delete {
                    channel: *channel,
                    message: *message,
                });
            }
            Event::Failed {
                failure: Failure::Send { nonce, .. },
                ..
            } => {
                self.sending.remove(nonce);
            }
            Event::Failed {
                failure: Failure::History { request, channel },
                ..
            } => {
                self.live.remove(request);
                if self
                    .loading
                    .get(channel)
                    .is_some_and(|loading| loading.request == *request)
                {
                    self.loading.remove(channel);
                }
            }
            Event::Ready(snapshot) => {
                // A new session answers nothing asked of the old one.
                self.loading.clear();
                self.pins = None;
                self.live.clear();
                self.invalidate_cached_reads();
                self.invalidate_local_history(rune_model::LocalReadError::Cancelled);
                let me = snapshot.me.as_ref().map(|user| user.id);
                self.ready_account = me;
                let same = self
                    .storage_identity
                    .is_some_and(|identity| me == Some(identity.account));
                self.storage_active = self.storage.is_some() && self.unlocked && same;
                if self.storage_active {
                    // The worker may still be restoring its revision baseline.
                    // READY alone cannot turn that pending read into a failure.
                    if self.read_marks_state == ReadMarksState::Unavailable {
                        self.storage_notice = Some(t("Las marcas de lectura nuevas quedan sólo en memoria porque no se restauró su base local. Reiniciá o borrá los datos locales para restaurar esa persistencia."));
                    }
                    self.pending_storage = Some(PendingStorage::Snapshot(snapshot.clone()));
                } else if same && self.fetching {
                    // Its key is on its way: the snapshot is kept once it is in.
                    self.unsaved_ready = Some(snapshot.clone());
                } else if self.storage.is_some() && self.storage_identity.is_some() && !same {
                    self.read_marks_state = ReadMarksState::Unavailable;
                    self.storage_notice = Some(t("La cuenta recibida no coincide con la caché local; la sesión queda en memoria."));
                }
            }
            Event::Connection(
                ConnectionState::Reconnecting { .. } | ConnectionState::Disconnected { .. },
            ) => {
                self.fail_pins(t(
                    "se perdió la conexión antes de cargar los mensajes fijados",
                ));
                self.loading.clear();
                self.live.clear();
                self.invalidate_cached_reads();
                // They left; whether they arrived, nobody can say.
                for (nonce, (channel, _)) in self.sending.drain() {
                    self.found.push_back(Event::Failed {
                        failure: Failure::Send { channel, nonce },
                        reason: t("se perdió la conexión antes de la confirmación").into(),
                        ambiguous: true,
                    });
                }
                self.votes.lost(&mut self.found);
            }
            Event::Connection(ConnectionState::Connecting) => {
                self.fail_pins(t("la conexión cambió antes de cargar los mensajes fijados"));
            }
            // The action waits on a person: its deadline moves past the
            // whole time the verification may take.
            Event::ChallengeRequired { failure, .. } => {
                let now = Instant::now();
                self.votes.challenged(failure, now);
                match failure {
                    Failure::Send { nonce, .. } => {
                        if let Some((_, since)) = self.sending.get_mut(nonce) {
                            *since = now + rune_provider::CHALLENGE_LIFETIME;
                        }
                    }
                    Failure::SendFiles { request, .. } => self.uploads.challenged(
                        *request,
                        now,
                        rune_provider::CHALLENGE_LIFETIME + self.deadlines.send,
                    ),
                    _ => {}
                }
            }
            Event::ChannelRemoved { channel }
                if self.pins.is_some_and(|pins| pins.channel == *channel) =>
            {
                self.fail_pins(t("la conversación ya no está disponible"));
            }
            Event::ChannelChanged(channel)
                if self.pins.is_some_and(|pins| pins.channel == channel.id)
                    && (!channel.kind.has_messages()
                        || !channel
                            .permissions
                            .contains(Permissions::VIEW | Permissions::READ_HISTORY)) =>
            {
                self.fail_pins(t("ya no se puede leer el historial de esta conversación"));
            }
            _ => {}
        }
        // After the books above: a new session's Ready ended the old reads.
        let held = self.connecting.observe(&event);
        if let Event::Connection(ConnectionState::Disconnected { reason }) = &event {
            for failure in held.iter().filter_map(Failure::of) {
                self.found.push_back(Event::Failed {
                    failure,
                    reason: reason.clone(),
                    ambiguous: false,
                });
            }
        } else {
            for command in held {
                // The interface asks a new session again for the history;
                // a page asked now has its whole wait from now.
                if let Command::LoadHistory {
                    request, channel, ..
                } = &command
                {
                    if !self.live.contains(request) {
                        continue;
                    }
                    if let Some(loading) = self.loading.get_mut(channel) {
                        loading.since = Instant::now();
                    }
                }
                let failure = Failure::of(&command);
                if self.commands.try_send(command).is_err() {
                    self.refused(failure);
                }
            }
        }
        Some(event)
    }

    fn fail_pins(&mut self, reason: &str) {
        if let Some(pins) = self.pins.take() {
            self.found.push_back(Event::Failed {
                failure: Failure::Pins {
                    channel: pins.channel,
                    request: pins.request,
                },
                reason: reason.into(),
                ambiguous: false,
            });
        }
    }

    /// Fail what waited too long for its answer.
    fn expire(&mut self, now: Instant) {
        self.uploads.expire(now);
        self.uploads.failures(&mut self.found);
        if let Some(event) = self.invitation_acceptance.expire(now) {
            self.found.push_back(event);
        }
        if self.storage_paused_at.is_some() {
            return;
        }
        self.read_receipts
            .expire(now, self.deadlines.history, &mut self.found);
        if self.context.is_some_and(|scope| {
            now.saturating_duration_since(scope.since) >= self.deadlines.history
        }) {
            self.fail_context(t("No se recibió el contexto a tiempo. Volvé a intentarlo."));
        }
        let late: Vec<ChannelId> = self
            .loading
            .iter()
            // A page held till the session is up was not asked yet.
            .filter(|(_, loading)| {
                self.connecting.connected()
                    && now.saturating_duration_since(loading.since) >= self.deadlines.history
            })
            .map(|(channel, _)| *channel)
            .collect();
        for channel in late {
            if let Some(loading) = self.loading.remove(&channel) {
                self.live.remove(&loading.request);
                self.found.push_back(Event::Failed {
                    failure: Failure::History {
                        request: loading.request,
                        channel,
                    },
                    reason: tr!(
                        "sin respuesta en {seconds} s",
                        seconds = self.deadlines.history.as_secs()
                    ),
                    ambiguous: false,
                });
            }
        }
        if self
            .pins
            .is_some_and(|pins| now.saturating_duration_since(pins.since) >= self.deadlines.history)
        {
            self.fail_pins(&tr!(
                "sin respuesta en {seconds} s",
                seconds = self.deadlines.history.as_secs()
            ));
        }
        let late: Vec<u64> = self
            .sending
            .iter()
            .filter(|(_, (_, since))| now.saturating_duration_since(*since) >= self.deadlines.send)
            .map(|(nonce, _)| *nonce)
            .collect();
        for nonce in late {
            if let Some((channel, _)) = self.sending.remove(&nonce) {
                self.found.push_back(Event::Failed {
                    failure: Failure::Send { channel, nonce },
                    reason: tr!(
                        "sin confirmación en {seconds} s",
                        seconds = self.deadlines.send.as_secs()
                    ),
                    ambiguous: true,
                });
            }
        }
        // A vote waits as long as a message does.
        self.votes.expire(now, self.deadlines.send, &mut self.found);
    }

    /// When the next deadline falls, so the host polls again then.
    pub fn next_deadline(&self, now: Instant) -> Option<Duration> {
        let history = self
            .loading
            .values()
            .filter(|_| self.storage_paused_at.is_none())
            .map(|loading| loading.since + self.deadlines.history);
        let sends = self
            .sending
            .values()
            .filter(|_| self.storage_paused_at.is_none())
            .map(|(_, since)| *since + self.deadlines.send);
        let pins = self
            .pins
            .filter(|_| self.storage_paused_at.is_none())
            .map(|pins| pins.since + self.deadlines.history);
        let context = self
            .context
            .filter(|_| self.storage_paused_at.is_none())
            .map(|scope| scope.since + self.deadlines.history);
        let votes = self
            .votes
            .deadline(self.deadlines.send)
            .filter(|_| self.storage_paused_at.is_none());
        history
            .chain(sends)
            .chain(votes)
            .chain(pins)
            .chain(context)
            .chain(self.local_read_deadline())
            .chain(
                self.read_receipts
                    .deadline(self.deadlines.history)
                    .filter(|_| self.storage_paused_at.is_none()),
            )
            .chain(self.invitation_acceptance.deadline())
            .chain(self.uploads.deadline())
            .chain(self.images.next_retry())
            .min()
            .map(|at| at.saturating_duration_since(now))
    }

    /// Ask for the newest page of `channel`, and what the local store has of
    /// it meanwhile. Nothing is asked twice while an answer is on its way.
    pub fn load_latest(&mut self, channel: ChannelId) {
        if self.storage_active && self.pending_storage.is_none() {
            if let Some(storage) = &self.storage {
                let request = CacheRequest {
                    epoch: self.cache_epoch,
                    request: self.next_cache_request,
                    channel,
                };
                self.next_cache_request += 1;
                if storage.request_cached(request, PAGE) {
                    self.cache_requests.insert(channel, request);
                }
            }
        }
        self.load(channel, None);
    }

    /// Ask for the page before `before`.
    pub fn load_older(&mut self, channel: ChannelId, before: MessageId) {
        self.load(channel, Some(before));
    }

    fn load(&mut self, channel: ChannelId, before: Option<MessageId>) {
        if self
            .loading
            .get(&channel)
            .is_some_and(|loading| loading.before == before)
        {
            return;
        }
        if let Some(previous) = self.loading.remove(&channel) {
            self.cancel(previous.request);
        }
        let request = self.next_request;
        self.next_request += 1;
        self.live.insert(request);
        self.loading.insert(
            channel,
            Loading {
                request,
                before,
                since: Instant::now(),
            },
        );
        self.command(Command::LoadHistory {
            request,
            channel,
            before,
            limit: PAGE,
        });
    }

    /// Whether a page of `channel` is on its way.
    pub fn is_loading(&self, channel: ChannelId) -> bool {
        self.loading.contains_key(&channel)
    }

    /// A newest-page refresh is distinct from an older-page read.
    pub fn is_loading_latest(&self, channel: ChannelId) -> bool {
        self.loading
            .get(&channel)
            .is_some_and(|loading| loading.before.is_none())
    }

    /// Stop every history request but those of `keep`: what was opened and
    /// left before its answer came is not worth the wait.
    pub fn cancel_history_except(&mut self, keep: ChannelId) {
        self.cancel_local_history_except(keep);
        if let Some(scope) = self.context.filter(|scope| scope.channel != keep) {
            self.command(Command::Cancel(scope.request));
        }
        self.cache_requests.retain(|channel, _| *channel == keep);
        let stale: Vec<ChannelId> = self
            .loading
            .keys()
            .copied()
            .filter(|channel| *channel != keep)
            .collect();
        for channel in stale {
            if let Some(loading) = self.loading.remove(&channel) {
                self.cancel(loading.request);
            }
        }
    }

    fn cancel_channel_history(&mut self, channel: ChannelId) {
        self.cache_requests.remove(&channel);
        if let Some(loading) = self.loading.remove(&channel) {
            self.cancel(loading.request);
        }
    }

    fn cancel(&mut self, request: RequestId) {
        self.live.remove(&request);
        self.command(Command::Cancel(request));
    }

    /// Send `content` to `channel`. Returns the nonce the answer will carry,
    /// so the message shown meanwhile can be replaced or marked.
    pub fn send_message(
        &mut self,
        channel: ChannelId,
        content: String,
        reply_to: Option<MessageId>,
        mention_reply: bool,
    ) -> u64 {
        let nonce = self.next_nonce;
        self.next_nonce += 1;
        self.resend(channel, content, reply_to, nonce, mention_reply);
        nonce
    }

    /// Send again under `nonce`: the same one only for a provider whose sends
    /// are idempotent, so a message that maybe arrived is not repeated.
    pub fn resend(
        &mut self,
        channel: ChannelId,
        content: String,
        reply_to: Option<MessageId>,
        nonce: u64,
        mention_reply: bool,
    ) {
        self.next_nonce = self.next_nonce.max(nonce + 1);
        self.sending.insert(nonce, (channel, Instant::now()));
        self.command(Command::SendMessage {
            channel,
            content,
            reply_to,
            nonce,
            mention_reply,
        });
    }

    /// Send one sticker as a separate message and return its acknowledgement nonce.
    pub fn send_sticker(&mut self, channel: ChannelId, sticker: rune_model::StickerId) -> u64 {
        let nonce = self.fresh_nonce();
        self.resend_sticker(channel, sticker, nonce);
        nonce
    }

    /// Retry only after an explicit user action, with the host's chosen nonce.
    pub fn resend_sticker(
        &mut self,
        channel: ChannelId,
        sticker: rune_model::StickerId,
        nonce: u64,
    ) {
        self.next_nonce = self.next_nonce.max(nonce + 1);
        self.sending.insert(nonce, (channel, Instant::now()));
        self.command(Command::SendSticker {
            channel,
            sticker,
            nonce,
        });
    }

    /// A nonce for a send of the host's own.
    pub fn fresh_nonce(&mut self) -> u64 {
        let nonce = self.next_nonce;
        self.next_nonce += 1;
        nonce
    }

    /// Queue `command` for the provider. One that cannot be queued (the
    /// provider is not keeping up, or the session is gone) fails at once,
    /// with the failure of what it was for.
    pub fn command(&mut self, mut command: Command) {
        let read_scope = match self
            .read_receipts
            .prepare(&command, self.info.capabilities.read_state)
        {
            Ok(scope) => scope,
            Err(reason) => {
                if let Some(failure) = Failure::of(&command) {
                    self.found.push_back(Event::Failed {
                        failure,
                        reason: reason.into(),
                        ambiguous: false,
                    });
                }
                return;
            }
        };
        if matches!(command, Command::MarkRead { .. }) && read_scope.is_none() {
            return;
        }
        if let Command::Cancel(request) = &command {
            self.uploads.cancel(*request);
            self.read_receipts.cancel(*request, &mut self.found);
            self.cancel_local_read(*request);
            if self.pins.is_some_and(|pins| pins.request == *request) {
                self.pins = None;
            }
            if self.context.is_some_and(|scope| scope.request == *request) {
                self.context = None;
            }
        }
        if let Command::LoadContext { limit, .. } = &mut command {
            *limit = (*limit).clamp(1, 100);
        }
        if let Command::LoadContext { channel, .. } = &command {
            self.cancel_channel_history(*channel);
        }
        if let Some(limit) = invitation_acceptance::limit(&command) {
            if let Command::AcceptInvitation { expires_at, .. }
            | Command::VerifyInvitation { expires_at, .. } = &mut command
            {
                *expires_at = (*expires_at).min(Instant::now() + limit);
            }
        }
        let supported = match &command {
            Command::AcceptInvitation { destination, .. }
            | Command::VerifyInvitation { destination, .. } => {
                self.info.capabilities.accepts(*destination)
            }
            _ => true,
        };
        match self
            .invitation_acceptance
            .begin(&command, supported, Instant::now())
        {
            Ok(true) => {}
            Ok(false) => return,
            Err(error) => {
                let Some(Failure::Invitation {
                    request,
                    destination,
                }) = Failure::of(&command)
                else {
                    unreachable!("only invitation commands may be rejected by this gate");
                };
                self.found.push_back(Event::InvitationAcceptance {
                    request,
                    destination,
                    result: Err(error),
                });
                return;
            }
        }
        let signing_out = matches!(command, Command::SignOut);
        let pins = match &command {
            Command::LoadPins { channel, request } => Some((*channel, *request)),
            _ => None,
        };
        let context = match &command {
            Command::LoadContext {
                request,
                channel,
                anchor,
                limit,
            } => Some(context::LoadingContext {
                request: *request,
                channel: *channel,
                anchor: *anchor,
                limit: *limit,
                since: Instant::now(),
                dirty: false,
            }),
            _ => None,
        };
        let failure = Failure::of(&command);
        let vote = tallies::Votes::of(&command);
        // A read held till the session is up is as good as queued.
        let refused = self
            .connecting
            .hold(command)
            .is_some_and(|command| self.commands.try_send(command).is_err());
        if refused {
            self.refused(failure);
        } else {
            if let Some(scope) = read_scope {
                self.read_receipts.admitted(scope);
            }
            if let Some(vote) = vote {
                self.votes.sent(vote, Instant::now());
            }
            if let Some(scope) = context {
                if let Some(previous) = self.context.replace(scope) {
                    let _ = self.commands.try_send(Command::Cancel(previous.request));
                }
            }
            if let Some((channel, request)) = pins {
                self.pins = Some(LoadingPins {
                    channel,
                    request,
                    since: Instant::now(),
                });
            }
            if signing_out {
                self.connecting.clear();
                self.votes.clear();
                self.uploads.clear();
                self.read_receipts.end_session(&mut self.found);
                self.pins = None;
                self.context = None;
                self.downloads.clear();
                self.documents.clear();
                self.clips.clear();
                self.invitations.clear();
                self.gifs.clear();
                self.invalidate_cached_reads();
                self.storage_active = false;
                self.read_marks_state = ReadMarksState::Unavailable;
                self.invalidate_local_history(rune_model::LocalReadError::Unavailable);
            }
        }
    }

    /// A command the provider's queue had no room for fails at once.
    fn refused(&mut self, failure: Option<Failure>) {
        if let Some(failure) = failure {
            if let Failure::Send { nonce, .. } = &failure {
                self.sending.remove(nonce);
            }
            if let Failure::History { request, channel } = &failure {
                self.live.remove(request);
                self.loading.remove(channel);
            }
            let event = Event::Failed {
                failure,
                reason: t("la cola de comandos está llena").into(),
                ambiguous: false,
            };
            self.invitation_acceptance.observe(&event);
            self.found.push_back(event);
        }
    }

    /// The last session's snapshot from the local store, to show at once.
    /// Waits for the store's thread, so it is meant for startup.
    pub fn cached_snapshot(&self) -> Option<Snapshot> {
        if !self.storage_active {
            return None;
        }
        let snapshot = self.storage.as_ref()?.snapshot()?;
        self.storage_identity
            .filter(|identity| snapshot.me.as_ref().map(|user| user.id) == Some(identity.account))
            .map(|_| snapshot)
    }

    /// Drafts kept from an earlier run. Waits for the store's thread.
    pub fn drafts(&self) -> Vec<(ChannelId, String)> {
        if !self.storage_active {
            return Vec::new();
        }
        self.storage
            .as_ref()
            .map(StorageWorker::drafts)
            .unwrap_or_default()
    }

    /// The explicit account namespace admitted for persistence, if any.
    pub fn storage_account(&self) -> Option<rune_model::UserId> {
        self.storage_active
            .then_some(self.storage_identity)
            .flatten()
            .map(|identity| identity.account)
    }

    /// Startup-only read on the SQLite worker. Errors are distinct from an
    /// account which has not saved any reading choices yet.
    pub fn read_marks(
        &mut self,
    ) -> Result<Vec<(ChannelId, rune_model::ReadMark)>, rune_model::LocalReadError> {
        if !self.storage_active {
            return Err(rune_model::LocalReadError::Unavailable);
        }
        let result = self
            .storage
            .as_ref()
            .ok_or(rune_model::LocalReadError::Unavailable)?
            .read_marks();
        // Without a restored revision baseline, acknowledging a new revision
        // could hide a write that the disk correctly ignores as older.
        self.read_marks_state = ReadMarksState::restored(result.is_ok());
        self.storage_notice = (self.read_marks_state == ReadMarksState::Unavailable).then_some(t(
            "Las marcas de lectura nuevas quedan sólo en memoria porque no se restauró su base local. Reiniciá o borrá los datos locales para restaurar esa persistencia.",
        ));
        result
    }

    /// Keep an account-local choice through the reliable write queue. Providers
    /// without admitted storage keep the choice in the caller's RAM only.
    pub fn set_read_mark(
        &self,
        account: rune_model::UserId,
        channel: ChannelId,
        mark: rune_model::ReadMark,
    ) -> Result<(), StorageWriteError> {
        if self.read_marks_state != ReadMarksState::Writable
            || self.storage_account() != Some(account)
        {
            return Ok(());
        }
        if self.pending_storage.is_some() {
            return Err(StorageWriteError::Busy);
        }
        if let Some(storage) = &self.storage {
            storage.set_read_mark(channel, mark)?;
        }
        Ok(())
    }

    /// Keep what is being written in `channel`, or forget it with `None`.
    pub fn set_draft(
        &self,
        channel: ChannelId,
        text: Option<String>,
    ) -> Result<(), StorageWriteError> {
        if self.pending_storage.is_some() {
            return Err(StorageWriteError::Busy);
        }
        if self.storage_active {
            if let Some(storage) = &self.storage {
                storage.set_draft(channel, text)?;
            }
        }
        Ok(())
    }

    /// Every setting kept from earlier runs, the device's and each
    /// account's, with any provider. Waits for the store's thread, so it is
    /// meant for startup.
    pub fn settings(&self) -> Vec<(String, String)> {
        self.storage
            .as_ref()
            .map(StorageWorker::settings)
            .unwrap_or_default()
    }

    /// Keep `value` under `key`, on the store's thread.
    pub fn set_setting(&self, key: &str, value: Option<String>) -> Result<(), StorageWriteError> {
        if self.pending_storage.is_some() {
            return Err(StorageWriteError::Busy);
        }
        if let Some(storage) = &self.storage {
            storage.set_setting(key, value)?;
        }
        Ok(())
    }

    /// Delete everything the local store keeps for this account.
    pub fn forget_local_data(&mut self) -> Result<(), StorageWriteError> {
        // A store kept for settings only holds nothing of the account's.
        if self.storage_identity.is_none() {
            return Ok(());
        }
        // Neither does the session of another account, which may not erase
        // what is kept of this one.
        let identity = self.storage_identity.map(|identity| identity.account);
        if self
            .ready_account
            .is_some_and(|ready| Some(ready) != identity)
        {
            return Ok(());
        }
        self.flush_pending_storage()?;
        if let Some(storage) = &self.storage {
            if self.unlocked {
                storage.forget()?;
            } else {
                // Without its key (no keyring, one that failed, or one not
                // in yet) its rows go all the same: deleting needs none.
                storage
                    .forget_account(self.account.clone())
                    .map_err(|error| error.kind)?;
            }
            if self.storage_active {
                self.read_marks_state = ReadMarksState::Writable;
                self.storage_notice = None;
            }
        }
        // And its pictures on disk (imagenes-y-lectura).
        self.images.disk.erase(self.account.clone());
        self.draft_files.clear();
        self.invalidate_cached_reads();
        self.invalidate_local_history(rune_model::LocalReadError::Unavailable);
        Ok(())
    }

    fn invalidate_cached_reads(&mut self) {
        self.cache_epoch = self.cache_epoch.wrapping_add(1);
        self.cache_requests.clear();
    }

    /// Keep the data of `identity`'s account from now on, once its key is
    /// in; `None` keeps no account's. The worker drops the key it held.
    fn adopt(&mut self, identity: Option<StorageIdentity>) {
        if identity == self.storage_identity && (self.unlocked || self.fetching) {
            return;
        }
        self.storage_identity = identity;
        self.account = identity.map(namespace).unwrap_or_default();
        self.unlocked = false;
        self.fetching = false;
        self.storage_active = false;
        self.read_marks_state = ReadMarksState::Pending;
        self.ready_account = None;
        self.unsaved_ready = None;
        // Another account's drafts never get these files.
        self.draft_files.clear();
        self.invalidate_cached_reads();
        self.invalidate_local_history(rune_model::LocalReadError::Unavailable);
        if let Some(storage) = &self.storage {
            storage.lock();
        }
        // Nor its pictures: they wait for its key (imagenes-y-lectura).
        self.images.disk.detach();
        self.request_key();
    }

    /// Ask the keyring for the key of `account`, off this thread. Whether
    /// one was asked for.
    fn request_key(&mut self) -> bool {
        if self.storage.is_none() || self.storage_identity.is_none() {
            return false;
        }
        let Some(keyring) = self.keyring.clone() else {
            self.storage_notice = Some(t(
                "Sin llavero del sistema: los datos de la cuenta quedan sólo en memoria.",
            ));
            return false;
        };
        let account = self.account.clone();
        self.fetching = true;
        self.on_keyring(async move {
            let key = keys::fetch(keyring, account.clone()).await;
            keys::KeyEvent::Fetched(account, key)
        });
        true
    }

    /// Run `operation` on the keyring once every one asked before is done.
    fn on_keyring(
        &mut self,
        operation: impl std::future::Future<Output = keys::KeyEvent> + Send + 'static,
    ) {
        let (before, sender, wake) = (
            self.keyring_last.take(),
            self.key_sender.clone(),
            self.wake.clone(),
        );
        self.keyring_last = Some(self.runtime.spawn(async move {
            if let Some(before) = before {
                let _ = before.await;
            }
            let _ = sender.send(operation.await);
            wake();
        }));
    }

    /// Take what the keyring answered. With `wait`, wait that long at most
    /// for the key asked for (startup only).
    fn take_keys(&mut self, wait: Option<Duration>) {
        let deadline = wait.map(|wait| Instant::now() + wait);
        loop {
            let event = match deadline {
                Some(deadline) => self
                    .key_events
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .ok(),
                None => self.key_events.try_recv().ok(),
            };
            let Some(event) = event else {
                return;
            };
            match event {
                keys::KeyEvent::Fetched(account, key) => {
                    if account != self.account || self.unlocked || !self.fetching {
                        continue;
                    }
                    self.fetching = false;
                    match key {
                        // At startup the host reads what was kept itself.
                        Ok((key, fresh)) => self.unlock(key, fresh, deadline.is_none()),
                        Err(_) => {
                            self.storage_notice = Some(t(
                                "El llavero del sistema no dio la clave de los datos locales: los datos de la cuenta quedan sólo en memoria.",
                            ));
                        }
                    }
                    if deadline.is_some() {
                        return;
                    }
                }
                keys::KeyEvent::Deleted(Err(_)) => {
                    self.storage_notice = Some(t(
                        "Se borraron los datos locales de la cuenta, pero no su clave del llavero del sistema.",
                    ));
                }
                keys::KeyEvent::Deleted(Ok(())) => {}
            }
        }
    }

    /// Hand the worker the key of `account`, `fresh` if made just now; with
    /// `restore`, what was kept of it comes back as [`Update::Restored`].
    fn unlock(&mut self, key: rune_storage::Key, fresh: bool, restore: bool) {
        let Some(identity) = self.storage_identity else {
            return;
        };
        // A Ready of another account came first: nothing of this one is
        // kept until another identity is taken, and the key goes unused.
        if self
            .ready_account
            .is_some_and(|account| account != identity.account)
        {
            return;
        }
        self.unlocked = true;
        self.storage_active = true;
        // What it said was of the account before, or of its missing key.
        self.storage_notice = None;
        // Its pictures, sealed with the same key (imagenes-y-lectura).
        self.images
            .disk
            .attach(self.account.clone(), key.clone(), fresh);
        self.pending_storage = Some(PendingStorage::Unlock(
            Box::new(storage::Unlocking {
                account: self.account.clone(),
                key,
                fresh,
                restore: restore.then_some(identity.account),
                // A session already ready shows its own; the kept one would
                // only take room.
                snapshot: self.ready_account.is_none(),
            }),
            self.unsaved_ready.take(),
        ));
    }

    /// Delete what is kept of a forgotten account, and its key.
    fn forget_account(&mut self, identity: StorageIdentity) {
        let account = namespace(identity);
        if account == self.account {
            self.adopt(None);
        }
        // Even with no store open (a newer file, a damaged one): the key
        // goes, so an old copy of the file stays unreadable.
        if let Some(keyring) = self.keyring.clone() {
            let forgotten = account.clone();
            self.on_keyring(async move {
                keys::KeyEvent::Deleted(keys::delete(keyring, &forgotten).await)
            });
        }
        // Its pictures on disk too (imagenes-y-lectura).
        self.images.disk.forget(account.clone());
        if self.storage.is_some() {
            self.pending_storage = Some(PendingStorage::Forget(account));
        }
    }

    fn resume_storage_deadlines(&mut self, now: Instant) {
        let Some(paused) = self.storage_paused_at.take() else {
            return;
        };
        self.read_receipts.resume(paused, now);
        for loading in self.loading.values_mut() {
            loading.since += now.saturating_duration_since(paused.max(loading.since));
        }
        if let Some(pins) = self.pins.as_mut() {
            pins.since += now.saturating_duration_since(paused.max(pins.since));
        }
        if let Some(context) = self.context.as_mut() {
            context.since += now.saturating_duration_since(paused.max(context.since));
        }
        for (_, since) in self.sending.values_mut() {
            *since += now.saturating_duration_since(paused.max(*since));
        }
        self.votes.resume(paused, now);
    }

    fn flush_pending_storage(&mut self) -> Result<(), StorageWriteError> {
        let Some(pending) = self.pending_storage.take() else {
            return Ok(());
        };
        let Some(storage) = &self.storage else {
            return Ok(());
        };
        let result = match pending {
            PendingStorage::Messages(changes) => storage
                .apply(changes)
                .map_err(|error| (PendingStorage::Messages(error.value), error.kind)),
            PendingStorage::Snapshot(snapshot) => storage
                .save_snapshot_owned(snapshot)
                .map_err(|error| (PendingStorage::Snapshot(error.value), error.kind)),
            PendingStorage::Forget(account) => storage
                .forget_account(account)
                .map_err(|error| (PendingStorage::Forget(error.value), error.kind)),
            PendingStorage::Unlock(unlocking, ready) => match storage.unlock(unlocking) {
                // The session's snapshot, ready before the key, goes next.
                Ok(_) => {
                    self.pending_storage = ready.map(PendingStorage::Snapshot);
                    return self.flush_pending_storage();
                }
                Err(error) => Err((PendingStorage::Unlock(error.value, ready), error.kind)),
            },
        };
        match result {
            Ok(_) => Ok(()),
            Err((_, StorageWriteError::TooLarge)) => {
                // Retrying an individually oversized payload can never succeed.
                // Accepted writes still drain; this session no longer reads or
                // adds account cache data until a fresh Ready is verified.
                self.storage_active = false;
                self.invalidate_local_history(rune_model::LocalReadError::Unavailable);
                self.invalidate_cached_reads();
                self.storage_notice = Some(
                    t("La sesión supera el límite de caché pendiente; continúa en memoria. Los datos nuevos no se guardan localmente."),
                );
                Ok(())
            }
            Err((pending, kind)) => {
                self.pending_storage = Some(pending);
                Err(kind)
            }
        }
    }

    /// A FIFO barrier after all changes already observed by this client.
    pub fn flush_storage(&mut self) -> Result<Option<StorageTicket>, StorageWriteError> {
        self.invalidate_local_history(rune_model::LocalReadError::Cancelled);
        self.flush_pending_storage()?;
        self.storage.as_ref().map(StorageWorker::flush).transpose()
    }

    pub fn storage_committed(&self, ticket: StorageTicket) -> bool {
        self.storage
            .as_ref()
            .is_none_or(|storage| storage.committed(ticket))
    }

    /// The account the store keeps rows under.
    pub fn account(&self) -> &str {
        &self.account
    }

    /// Run `future` on the client's runtime, for work of the host that waits.
    pub fn spawn(&self, future: impl std::future::Future<Output = ()> + Send + 'static) {
        self.runtime.spawn(future);
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        // Invalidate only small live controls before handing runtime shutdown
        // away. Its command receiver and sealed buffers stay on service threads.
        self.uploads.close();
    }
}

/// Run sessions of `provider` until the client goes away, connecting again
/// after a transient loss. Each session gets the next number.
async fn supervise(
    provider: Arc<dyn Provider>,
    mut commands: mpsc::Receiver<Command>,
    sink: EventSink,
    backoff: Backoff,
) {
    let mut attempt = 0;
    let mut generation = 0;
    loop {
        generation += 1;
        let session = sink.for_session(generation);
        if session
            .send(Event::Connection(ConnectionState::Connecting))
            .await
            .is_err()
        {
            return;
        }
        let started = Instant::now();
        match provider.connect(&mut commands, &session).await {
            Ok(()) => return,
            Err(ProviderError::Fatal(reason)) => {
                let _ = session
                    .send(Event::Connection(ConnectionState::Disconnected { reason }))
                    .await;
                return;
            }
            Err(ProviderError::Transient(reason)) => {
                // A session that held for a while starts the count again.
                attempt = if started.elapsed() > Duration::from_secs(20) {
                    1
                } else {
                    attempt + 1
                };
                let delay = backoff.delay(attempt, jitter(attempt));
                eprintln!("rune: connection lost ({reason}); attempt {attempt} in {delay:?}");
                let retry_in_ms = delay.as_millis() as u64;
                let reconnecting = Event::Connection(ConnectionState::Reconnecting {
                    attempt,
                    retry_in_ms,
                });
                if session.send(reconnecting).await.is_err() {
                    return;
                }
                tokio::time::sleep(delay).await;
                if commands.is_closed() {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests;
