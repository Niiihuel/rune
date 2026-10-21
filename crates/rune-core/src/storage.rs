//! SQLite runs on its own thread. Runtime submissions never wait: the host
//! keeps a rejected reliable batch and applies backpressure until it fits.
//! Tickets advance only after successful commits, in submission order.

use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use rune_i18n::t;
use rune_model::{ChannelId, Message, MessagePatch, ReadMark, UserId};
use rune_provider::Snapshot;
use rune_storage::{Key, MessageMutation, Storage};

use crate::CACHED_PER_CHANNEL;

mod local_reads;
pub use local_reads::{ContextRequest, LocalReadReply, LocalReadTag, SearchRequest};

const QUEUE: usize = 1024;
pub(crate) const CACHED_ANSWERS: usize = 16;
/// Retained payloads, including spare vector/string capacity, across both queues.
const MAX_QUEUED_BYTES: usize = 8 * 1024 * 1024;
const IDLE: Duration = Duration::from_secs(3);
const UPKEEP_EVERY: Duration = Duration::from_secs(600);
const RETRY: Duration = Duration::from_millis(250);
const STARTUP_TIMEOUT: Duration = Duration::from_secs(2);
const READ_MARKS_CREDIT: usize = 4 * 1024 * 1024;
/// What an unlock pays up front for what it restores: the marks and drafts
/// of a usual account. More takes room if there is any; else the snapshot,
/// then the drafts are left out, and at worst the marks come back as an
/// error, so they are never lost without a word.
const RESTORE_CREDIT: usize = 256 * 1024;
type ReadMarksResult = Result<Vec<(ChannelId, ReadMark)>, rune_model::LocalReadError>;
const _: () = assert!(
    READ_MARKS_CREDIT
        >= rune_storage::MAX_READ_MARKS * size_of::<(ChannelId, ReadMark)>()
            + size_of::<Answer<ReadMarksResult>>()
);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct StorageTicket(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageWriteError {
    Busy,
    Closed,
    TooLarge,
}

impl std::fmt::Display for StorageWriteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Busy => t("el almacenamiento local está ocupado"),
            Self::Closed => t("el almacenamiento local está cerrado"),
            Self::TooLarge => t("el lote supera el límite de almacenamiento pendiente"),
        })
    }
}

impl std::error::Error for StorageWriteError {}

/// Ownership stays with the caller when a reliable batch cannot be queued.
pub struct SubmitError<T> {
    pub kind: StorageWriteError,
    pub value: T,
}

impl<T> std::fmt::Debug for SubmitError<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // A rejected payload can contain private message text or signed asset URLs.
        formatter
            .debug_struct("SubmitError")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheRequest {
    pub epoch: u64,
    pub request: u64,
    pub channel: ChannelId,
}

/// An account's key for the worker, which then writes and reads that
/// account alone. With `restore`, what was kept of it comes back.
pub struct Unlocking {
    pub account: String,
    pub key: Key,
    /// Made just now: what the account sealed before is lost with its key.
    pub fresh: bool,
    pub restore: Option<UserId>,
    /// With `restore`, its snapshot too; a session already ready has its own.
    pub snapshot: bool,
}

/// What the local store kept of an account a provider named once its key
/// was in: shown until the session's own state arrives, never as confirmed.
#[derive(Clone, Debug, PartialEq)]
pub struct Restored {
    pub account: UserId,
    pub snapshot: Option<Snapshot>,
    pub drafts: Vec<(ChannelId, String)>,
    pub read_marks: ReadMarksResult,
    // Sends and uploads (envios-y-subidas).
    /// Where the files of its drafts are; Core keeps them until each
    /// conversation opens, so the host never sees them.
    pub draft_files: Vec<(ChannelId, Vec<rune_storage::DraftFile>)>,
}

struct Shared {
    bytes: AtomicUsize,
    answers: AtomicUsize,
    read_busy: AtomicBool,
    read_request: AtomicU64,
    read_cancellation: AtomicU64,
    next_ticket: AtomicU64,
    committed: AtomicU64,
    failed: AtomicBool,
    stopping: AtomicBool,
    closed: AtomicBool,
    wake: Arc<dyn Fn() + Send + Sync>,
}

struct Reservation {
    bytes: usize,
    shared: Arc<Shared>,
    notify: bool,
}

impl Reservation {
    fn reserve(shared: &Arc<Shared>, bytes: usize) -> Result<Self, StorageWriteError> {
        if bytes > MAX_QUEUED_BYTES {
            return Err(StorageWriteError::TooLarge);
        }
        shared
            .bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |held| {
                held.checked_add(bytes)
                    .filter(|total| *total <= MAX_QUEUED_BYTES)
            })
            .map_err(|_| StorageWriteError::Busy)?;
        Ok(Self {
            bytes,
            shared: shared.clone(),
            notify: true,
        })
    }

    /// Hold `other`'s bytes too, released with these.
    fn join(&mut self, mut other: Self) {
        self.bytes += std::mem::take(&mut other.bytes);
        other.notify = false;
    }

    fn split(&mut self, bytes: usize) -> Self {
        assert!(bytes <= self.bytes);
        self.bytes -= bytes;
        Self {
            bytes,
            shared: self.shared.clone(),
            notify: self.notify,
        }
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.shared.bytes.fetch_sub(self.bytes, Ordering::AcqRel);
        if self.notify {
            (self.shared.wake)();
        }
    }
}

struct Answer<T> {
    value: T,
    _reservation: Reservation,
    _slot: AnswerSlot,
}

struct AnswerSlot(Arc<Shared>);

impl AnswerSlot {
    fn reserve(shared: &Arc<Shared>) -> Option<Self> {
        shared
            .answers
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |held| {
                (held < CACHED_ANSWERS).then_some(held + 1)
            })
            .ok()?;
        Some(Self(shared.clone()))
    }
}

impl Drop for AnswerSlot {
    fn drop(&mut self) {
        self.0.answers.fetch_sub(1, Ordering::AcqRel);
        (self.0.wake)();
    }
}

#[cfg(test)]
type TestJob = Box<dyn Fn(&mut Storage) -> Result<(), rune_storage::Error> + Send>;

enum Job {
    Apply(Vec<MessageMutation>),
    Local(local_reads::ReadJob),
    Cached(CacheRequest, usize),
    SaveSnapshot(Box<Snapshot>),
    Snapshot(SyncSender<Answer<Option<Snapshot>>>),
    SetDraft(ChannelId, Option<String>),
    Drafts(SyncSender<Answer<Vec<(ChannelId, String)>>>),
    ReadMarks {
        reply: SyncSender<Answer<ReadMarksResult>>,
        slot: AnswerSlot,
    },
    SetReadMark(ChannelId, ReadMark),
    Settings(SyncSender<Answer<Vec<(String, String)>>>),
    SetSetting(String, Option<String>),
    Forget,
    Flush,
    // Sends and uploads (envios-y-subidas).
    SetDraftFiles(ChannelId, Vec<rune_storage::DraftFile>),
    DraftFiles(SyncSender<Answer<DraftFiles>>),
    // Encrypted local cache (2026-10-18). What it restores answers with the
    // slot and the credit paid when it was queued, so it always answers.
    Unlock(Box<Unlocking>, Option<AnswerSlot>),
    /// Forget every key: no account is written or read until the next unlock.
    Lock,
    /// Delete what is kept of this account, the current one or another.
    ForgetAccount(String),
    #[cfg(test)]
    Test(TestJob),
}

/// The files of each draft kept, by conversation.
type DraftFiles = Vec<(ChannelId, Vec<rune_storage::DraftFile>)>;

struct Queued {
    ticket: StorageTicket,
    job: Job,
    _reservation: Reservation,
}

/// A bounded local-store worker. Only startup reads wait for a response.
pub struct StorageWorker {
    jobs: Option<SyncSender<Queued>>,
    cached: Receiver<Answer<(CacheRequest, Vec<Message>)>>,
    local: Receiver<Answer<local_reads::Delivered>>,
    /// What an unlock restored, with the account's name in the store.
    restored: Receiver<Answer<(String, Restored)>>,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl StorageWorker {
    /// Run `storage` for `account`, whose key `storage` holds already or
    /// gets with [`StorageWorker::unlock`]; until then nothing of it is
    /// written or read.
    pub fn start(
        mut storage: Storage,
        mut account: String,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (jobs, received) = sync_channel::<Queued>(QUEUE);
        let (answer, cached) = sync_channel(CACHED_ANSWERS);
        let (local_answer, local) = sync_channel(1);
        // Each restore holds an answer slot: it never finds this full.
        let (restored_answer, restored) = sync_channel(CACHED_ANSWERS);
        let shared = Arc::new(Shared {
            bytes: AtomicUsize::new(0),
            answers: AtomicUsize::new(0),
            read_busy: AtomicBool::new(false),
            read_request: AtomicU64::new(0),
            read_cancellation: AtomicU64::new(0),
            next_ticket: AtomicU64::new(1),
            committed: AtomicU64::new(0),
            failed: AtomicBool::new(false),
            stopping: AtomicBool::new(false),
            closed: AtomicBool::new(false),
            wake,
        });
        let state = shared.clone();
        let thread = std::thread::Builder::new()
            .name("rune-storage".into())
            .spawn(move || {
                let mut upkept: Option<Instant> = None;
                let mut reads = local_reads::ReadRunner::default();
                'jobs: loop {
                    reads.tick(&storage, &account, &local_answer, &state);
                    let queued = match received.recv_timeout(reads.wait_timeout()) {
                        Ok(job) => job,
                        Err(RecvTimeoutError::Timeout) => {
                            if reads.working() {
                                continue;
                            }
                            if !account.is_empty()
                                && upkept.is_none_or(|at| at.elapsed() > UPKEEP_EVERY)
                            {
                                upkept = Some(Instant::now());
                                match storage.prune(&account, CACHED_PER_CHANNEL) {
                                    Ok(removed) if removed > 0 => reads.invalidate(),
                                    Err(_) => eprintln!("rune: could not prune the local store"),
                                    _ => {}
                                }
                            }
                            continue;
                        }
                        Err(RecvTimeoutError::Disconnected) => break,
                    };
                    // A slot is free even while this job's payload remains reserved.
                    (state.wake)();
                    let queued = match queued {
                        Queued {
                            ticket,
                            job: Job::ReadMarks { reply, slot },
                            _reservation,
                        } => {
                            let value = storage.read_marks(&account).map_err(|error| match error {
                                rune_storage::Error::ReadMarksTooLarge => {
                                    rune_model::LocalReadError::TooLarge
                                }
                                _ => rune_model::LocalReadError::Failed,
                            });
                            // The one-shot response owns the admission credit;
                            // it never needs to reserve memory after reading.
                            let _ = reply.try_send(Answer {
                                value,
                                _reservation,
                                _slot: slot,
                            });
                            state.committed.store(ticket.0, Ordering::Release);
                            (state.wake)();
                            continue;
                        }
                        Queued {
                            ticket,
                            job: Job::Local(job),
                            _reservation,
                        } => {
                            reads.begin(job, _reservation, &state);
                            state.committed.store(ticket.0, Ordering::Release);
                            (state.wake)();
                            continue;
                        }
                        queued => queued,
                    };
                    reads.observe(&queued.job);
                    if matches!(
                        queued.job,
                        Job::Unlock(..) | Job::Lock | Job::ForgetAccount(_)
                    ) {
                        // What was being read belonged to the account before.
                        reads.invalidate();
                    }
                    loop {
                        let replies = Replies { cached: &answer };
                        match execute(&mut storage, &mut account, &queued.job, replies, &state) {
                            // A locked account keeps nothing: its work is done.
                            Ok(()) | Err(rune_storage::Error::Locked) => {
                                state.failed.store(false, Ordering::Release);
                                state.committed.store(queued.ticket.0, Ordering::Release);
                                if let Queued {
                                    job: Job::Unlock(unlocking, Some(slot)),
                                    _reservation: credit,
                                    ..
                                } = queued
                                {
                                    restore(
                                        &storage,
                                        &account,
                                        &unlocking,
                                        (credit, slot),
                                        &restored_answer,
                                        &state,
                                    );
                                } else {
                                    drop(queued);
                                }
                                (state.wake)();
                                break;
                            }
                            Err(_) => {
                                state.failed.store(true, Ordering::Release);
                                (state.wake)();
                                if state.stopping.load(Ordering::Acquire) {
                                    break 'jobs;
                                }
                                // Keep this job, its budget and its position until it succeeds.
                                std::thread::park_timeout(RETRY);
                            }
                        }
                    }
                }
                state.closed.store(true, Ordering::Release);
                (state.wake)();
            })
            .expect("the storage thread starts");
        Self {
            jobs: Some(jobs),
            cached,
            local,
            restored,
            shared,
            thread: Some(thread),
        }
    }

    /// Hand the worker an account's key; ownership comes back on backpressure.
    pub fn unlock(
        &self,
        unlocking: Box<Unlocking>,
    ) -> Result<StorageTicket, SubmitError<Box<Unlocking>>> {
        let slot = match unlocking.restore.map(|_| AnswerSlot::reserve(&self.shared)) {
            Some(None) => {
                return Err(SubmitError {
                    kind: StorageWriteError::Busy,
                    value: unlocking,
                })
            }
            slot => slot.flatten(),
        };
        self.submit(Job::Unlock(unlocking, slot)).map_err(|error| {
            let Job::Unlock(value, _) = error.value else {
                unreachable!()
            };
            SubmitError {
                kind: error.kind,
                value,
            }
        })
    }

    /// Best effort: an unlock replaces the key anyway, and nothing of the
    /// account is submitted meanwhile.
    pub fn lock(&self) {
        let _ = self.submit(Job::Lock);
    }

    pub fn forget_account(&self, account: String) -> Result<StorageTicket, SubmitError<String>> {
        self.submit(Job::ForgetAccount(account)).map_err(|error| {
            let Job::ForgetAccount(value) = error.value else {
                unreachable!()
            };
            SubmitError {
                kind: error.kind,
                value,
            }
        })
    }

    /// What an unlock restored, with the account's name in the store.
    pub fn try_restored(&self) -> Option<(String, Restored)> {
        self.restored.try_recv().ok().map(|answer| answer.value)
    }

    fn submit(&self, job: Job) -> Result<StorageTicket, SubmitError<Job>> {
        let Some(jobs) = &self.jobs else {
            return Err(SubmitError {
                kind: StorageWriteError::Closed,
                value: job,
            });
        };
        if self.shared.closed.load(Ordering::Acquire) {
            return Err(SubmitError {
                kind: StorageWriteError::Closed,
                value: job,
            });
        }
        let reservation = match Reservation::reserve(&self.shared, job_bytes(&job)) {
            Ok(reservation) => reservation,
            Err(kind) => return Err(SubmitError { kind, value: job }),
        };
        let ticket = StorageTicket(self.shared.next_ticket.fetch_add(1, Ordering::Relaxed));
        let queued = Queued {
            ticket,
            job,
            _reservation: reservation,
        };
        match jobs.try_send(queued) {
            Ok(()) => Ok(ticket),
            Err(error) => {
                let kind = match &error {
                    TrySendError::Full(_) => StorageWriteError::Busy,
                    TrySendError::Disconnected(_) => StorageWriteError::Closed,
                };
                let (TrySendError::Full(mut queued) | TrySendError::Disconnected(mut queued)) =
                    error;
                // A failed submission never held capacity another caller was waiting for.
                queued._reservation.notify = false;
                Err(SubmitError {
                    kind,
                    value: queued.job,
                })
            }
        }
    }

    pub fn apply(
        &self,
        changes: Vec<MessageMutation>,
    ) -> Result<StorageTicket, SubmitError<Vec<MessageMutation>>> {
        self.submit(Job::Apply(changes)).map_err(|error| {
            let Job::Apply(value) = error.value else {
                unreachable!()
            };
            SubmitError {
                kind: error.kind,
                value,
            }
        })
    }

    /// Cache seeding for tests. Reliable provider changes use `apply`.
    #[cfg(test)]
    pub fn save(&self, messages: Vec<Message>) {
        let changes = messages
            .into_iter()
            .map(|message| MessageMutation::Upsert(Box::new(message)))
            .collect();
        // A full queue drops the seeding: it is only a cache.
        let _ = self.apply(changes);
    }

    pub fn request_cached(&self, request: CacheRequest, limit: usize) -> bool {
        self.submit(Job::Cached(request, limit.min(CACHED_PER_CHANNEL)))
            .is_ok()
    }

    pub fn try_cached(&self) -> Option<(CacheRequest, Vec<Message>)> {
        self.cached.try_recv().ok().map(|answer| answer.value)
    }

    /// Keep ownership on backpressure, so retrying never clones a session snapshot.
    pub fn save_snapshot_owned(
        &self,
        mut snapshot: Box<Snapshot>,
    ) -> Result<StorageTicket, SubmitError<Box<Snapshot>>> {
        // Ephemeral presence and voice participation belong only to the live session.
        snapshot.presences = Vec::new();
        snapshot.voice = Vec::new();
        self.submit(Job::SaveSnapshot(snapshot)).map_err(|error| {
            let Job::SaveSnapshot(value) = error.value else {
                unreachable!()
            };
            SubmitError {
                kind: error.kind,
                value,
            }
        })
    }

    /// Startup only: wait a bounded time for the store's reply.
    pub fn snapshot(&self) -> Option<Snapshot> {
        let (reply, answer) = sync_channel(1);
        self.submit(Job::Snapshot(reply)).ok()?;
        answer.recv_timeout(STARTUP_TIMEOUT).ok()?.value
    }

    pub fn set_draft(
        &self,
        channel: ChannelId,
        text: Option<String>,
    ) -> Result<StorageTicket, StorageWriteError> {
        self.submit(Job::SetDraft(channel, text))
            .map_err(|error| error.kind)
    }

    /// Startup only.
    pub fn drafts(&self) -> Vec<(ChannelId, String)> {
        let (reply, answer) = sync_channel(1);
        if self.submit(Job::Drafts(reply)).is_err() {
            return Vec::new();
        }
        answer
            .recv_timeout(STARTUP_TIMEOUT)
            .map(|answer| answer.value)
            .unwrap_or_default()
    }

    /// Keep where the files of `channel`'s draft are; none forgets them.
    pub fn set_draft_files(
        &self,
        channel: ChannelId,
        files: Vec<rune_storage::DraftFile>,
    ) -> Result<StorageTicket, StorageWriteError> {
        self.submit(Job::SetDraftFiles(channel, files))
            .map_err(|error| error.kind)
    }

    /// Startup only.
    pub fn draft_files(&self) -> DraftFiles {
        let (reply, answer) = sync_channel(1);
        if self.submit(Job::DraftFiles(reply)).is_err() {
            return Vec::new();
        }
        answer
            .recv_timeout(STARTUP_TIMEOUT)
            .map(|answer| answer.value)
            .unwrap_or_default()
    }

    /// Startup only. Failure is distinct from an account with no local choices.
    pub fn read_marks(&self) -> Result<Vec<(ChannelId, ReadMark)>, rune_model::LocalReadError> {
        let slot =
            AnswerSlot::reserve(&self.shared).ok_or(rune_model::LocalReadError::Unavailable)?;
        let (reply, answer) = sync_channel(1);
        self.submit(Job::ReadMarks { reply, slot })
            .map_err(|error| match error.kind {
                StorageWriteError::TooLarge => rune_model::LocalReadError::TooLarge,
                _ => rune_model::LocalReadError::Unavailable,
            })?;
        answer
            .recv_timeout(STARTUP_TIMEOUT)
            .map_err(|_| rune_model::LocalReadError::Failed)?
            .value
    }

    pub fn set_read_mark(
        &self,
        channel: ChannelId,
        mark: ReadMark,
    ) -> Result<StorageTicket, StorageWriteError> {
        self.submit(Job::SetReadMark(channel, mark))
            .map_err(|error| error.kind)
    }

    /// Startup only: every setting, the device's and each account's.
    pub fn settings(&self) -> Vec<(String, String)> {
        let (reply, answer) = sync_channel(1);
        if self.submit(Job::Settings(reply)).is_err() {
            return Vec::new();
        }
        answer
            .recv_timeout(STARTUP_TIMEOUT)
            .map(|answer| answer.value)
            .unwrap_or_default()
    }

    pub fn set_setting(
        &self,
        key: &str,
        value: Option<String>,
    ) -> Result<StorageTicket, StorageWriteError> {
        self.submit(Job::SetSetting(key.to_owned(), value))
            .map_err(|error| error.kind)
    }

    pub fn forget(&self) -> Result<StorageTicket, StorageWriteError> {
        self.submit(Job::Forget).map_err(|error| error.kind)
    }

    /// A FIFO fence: acknowledged only after all earlier writes committed.
    pub fn flush(&self) -> Result<StorageTicket, StorageWriteError> {
        self.cancel_all_reads();
        self.submit(Job::Flush).map_err(|error| error.kind)
    }

    pub fn committed(&self, ticket: StorageTicket) -> bool {
        self.shared.committed.load(Ordering::Acquire) >= ticket.0
    }

    /// Fixed redacted text; database errors can contain private row values.
    pub fn error(&self) -> Option<String> {
        self.shared.failed.load(Ordering::Acquire).then(|| t("No se pudieron guardar los datos locales. RUNE conserva el trabajo pendiente y volverá a intentarlo.").into())
    }
}

fn reply<T>(shared: &Arc<Shared>, answer: &SyncSender<Answer<T>>, value: T, bytes: usize) {
    let Some(slot) = AnswerSlot::reserve(shared) else {
        return;
    };
    if let Ok(reservation) =
        Reservation::reserve(shared, bytes.saturating_add(size_of::<Answer<T>>()))
    {
        if let Err(error) = answer.try_send(Answer {
            value,
            _reservation: reservation,
            _slot: slot,
        }) {
            let (TrySendError::Full(mut rejected) | TrySendError::Disconnected(mut rejected)) =
                error;
            rejected._reservation.notify = false;
        }
    }
}

/// Where the worker's optional answers go.
#[derive(Clone, Copy)]
struct Replies<'a> {
    cached: &'a SyncSender<Answer<(CacheRequest, Vec<Message>)>>,
}

/// What was kept of the account `unlocking` just unlocked, answered with
/// what was paid when it was queued ([`RESTORE_CREDIT`]).
fn restore(
    storage: &Storage,
    account: &str,
    unlocking: &Unlocking,
    (mut credit, slot): (Reservation, AnswerSlot),
    answer: &SyncSender<Answer<(String, Restored)>>,
    shared: &Arc<Shared>,
) {
    let Some(user) = unlocking.restore else {
        return;
    };
    let mut restored = Restored {
        account: user,
        snapshot: unlocking
            .snapshot
            .then(|| storage.snapshot::<Snapshot>(account).ok().flatten())
            .flatten(),
        // Each part on its own: one that fails leaves the others.
        drafts: storage.drafts(account).unwrap_or_default(),
        draft_files: storage.draft_files(account).unwrap_or_default(),
        read_marks: storage.read_marks(account).map_err(|error| match error {
            rune_storage::Error::ReadMarksTooLarge => rune_model::LocalReadError::TooLarge,
            _ => rune_model::LocalReadError::Failed,
        }),
    };
    let mut fits = |restored: &Restored| {
        let bytes = restored_bytes(restored)
            .saturating_add(account.len())
            .saturating_add(size_of::<Answer<(String, Restored)>>());
        match bytes.checked_sub(credit.bytes).filter(|more| *more > 0) {
            None => true,
            Some(more) => Reservation::reserve(shared, more)
                .map(|more| credit.join(more))
                .is_ok(),
        }
    };
    if !fits(&restored) {
        restored.snapshot = None;
        if !fits(&restored) {
            restored.drafts = Vec::new();
            restored.draft_files = Vec::new();
            if !fits(&restored) {
                restored.read_marks = Err(rune_model::LocalReadError::TooLarge);
            }
        }
    }
    let _ = answer.try_send(Answer {
        value: (account.to_owned(), restored),
        _reservation: credit,
        _slot: slot,
    });
}

fn execute(
    storage: &mut Storage,
    current: &mut String,
    job: &Job,
    replies: Replies<'_>,
    shared: &Arc<Shared>,
) -> Result<(), rune_storage::Error> {
    let account = current.as_str();
    let cached = replies.cached;
    match job {
        Job::Unlock(unlocking, _) => {
            storage.lock();
            current.clear();
            let key = unlocking.key.clone();
            if unlocking.fresh {
                storage.unlock_new(&unlocking.account, key)?;
            } else {
                storage.unlock(&unlocking.account, key)?;
            }
            current.clone_from(&unlocking.account);
            Ok(())
        }
        Job::Lock => {
            storage.lock();
            current.clear();
            Ok(())
        }
        Job::ForgetAccount(forgotten) => {
            storage.forget_account(forgotten)?;
            if forgotten == current {
                storage.lock();
                current.clear();
            }
            Ok(())
        }
        Job::Apply(changes) => storage.apply_messages(account, changes),
        Job::Local(_) => unreachable!("local reads transfer their prepaid reply budget"),
        Job::ReadMarks { .. } => {
            unreachable!("startup read marks transfer their prepaid reply budget")
        }
        Job::Cached(request, limit) => {
            let messages = storage.recent(account, request.channel, *limit)?;
            if !messages.is_empty() {
                let bytes = messages_bytes(&messages);
                reply(shared, cached, (*request, messages), bytes);
            }
            Ok(())
        }
        Job::SaveSnapshot(snapshot) => storage.save_snapshot(account, &**snapshot),
        Job::Snapshot(answer) => {
            let snapshot = storage.snapshot::<Snapshot>(account)?;
            let bytes = snapshot.as_ref().map_or(0, snapshot_bytes);
            reply(shared, answer, snapshot, bytes);
            Ok(())
        }
        Job::SetDraft(channel, text) => storage.set_draft(account, *channel, text.as_deref()),
        Job::SetReadMark(channel, mark) => storage.set_read_mark(account, *channel, *mark),
        Job::Drafts(answer) => {
            let drafts = storage.drafts(account)?;
            let bytes = vector_bytes(&drafts)
                .saturating_add(drafts.iter().map(|(_, text)| text.capacity()).sum());
            reply(shared, answer, drafts, bytes);
            Ok(())
        }
        Job::Settings(answer) => {
            let settings = storage.settings()?;
            let bytes = settings
                .iter()
                .fold(vector_bytes(&settings), |bytes, (key, value)| {
                    bytes.saturating_add(key.capacity() + value.capacity())
                });
            reply(shared, answer, settings, bytes);
            Ok(())
        }
        Job::SetSetting(key, value) => storage.set_setting(key, value.as_deref()),
        Job::Forget => storage.forget_account(account),
        Job::Flush => Ok(()),
        Job::SetDraftFiles(channel, files) => storage.set_draft_files(account, *channel, files),
        Job::DraftFiles(answer) => {
            let kept = storage.draft_files(account)?;
            let bytes = kept.iter().fold(vector_bytes(&kept), |bytes, (_, files)| {
                bytes.saturating_add(draft_files_bytes(files))
            });
            reply(shared, answer, kept, bytes);
            Ok(())
        }
        #[cfg(test)]
        Job::Test(run) => run(storage),
    }
}

impl Drop for StorageWorker {
    fn drop(&mut self) {
        self.cancel_all_reads();
        self.shared.stopping.store(true, Ordering::Release);
        self.jobs.take();
        if let Some(thread) = self.thread.take() {
            thread.thread().unpark();
            let _ = thread.join();
        }
    }
}

fn vector_bytes<T>(values: &Vec<T>) -> usize {
    values.capacity().saturating_mul(size_of::<T>())
}

fn draft_files_bytes(files: &Vec<rune_storage::DraftFile>) -> usize {
    files.iter().fold(vector_bytes(files), |bytes, file| {
        bytes.saturating_add(file.path.capacity())
    })
}

fn text_bytes(text: &Option<String>) -> usize {
    text.as_ref().map_or(0, String::capacity)
}

fn image_bytes(image: &Option<rune_model::RemoteImage>) -> usize {
    image.as_ref().map_or(0, |image| image.url.capacity())
}

fn sticker_bytes(sticker: &rune_model::Sticker) -> usize {
    sticker
        .name
        .capacity()
        .saturating_add(image_bytes(&sticker.image))
}

fn reactions_bytes(reactions: &Vec<rune_model::Reaction>) -> usize {
    reactions
        .iter()
        .fold(vector_bytes(reactions), |bytes, reaction| {
            bytes
                .saturating_add(reaction.emoji.capacity())
                .saturating_add(vector_bytes(&reaction.burst_colors))
        })
}

fn attachments_bytes(attachments: &Vec<rune_model::Attachment>) -> usize {
    attachments
        .iter()
        .fold(vector_bytes(attachments), |bytes, attachment| {
            bytes
                .saturating_add(attachment.filename.capacity())
                .saturating_add(text_bytes(&attachment.content_type))
                .saturating_add(image_bytes(&attachment.preview))
                .saturating_add(image_bytes(&attachment.placeholder))
                .saturating_add(
                    attachment
                        .download
                        .as_ref()
                        .map_or(0, |file| file.url.capacity()),
                )
        })
}

fn stickers_bytes(stickers: &Vec<rune_model::Sticker>) -> usize {
    stickers
        .iter()
        .fold(vector_bytes(stickers), |bytes, sticker| {
            bytes.saturating_add(sticker_bytes(sticker))
        })
}

fn references_bytes(references: &Vec<rune_model::TextReference>) -> usize {
    references
        .iter()
        .fold(vector_bytes(references), |bytes, reference| {
            let bytes = bytes.saturating_add(reference.source.capacity());
            if let rune_model::TextTarget::Emoji {
                name, image, jumbo, ..
            } = &reference.target
            {
                let url = |image: &Option<rune_model::RemoteImage>| {
                    image.as_ref().map_or(0, |image| image.url.capacity())
                };
                bytes
                    .saturating_add(name.capacity())
                    .saturating_add(url(image))
                    .saturating_add(url(jumbo))
            } else {
                bytes
            }
        })
}

fn message_heap_bytes(message: &Message) -> usize {
    let mut bytes = message
        .content
        .capacity()
        .saturating_add(vector_bytes(&message.mentions))
        .saturating_add(vector_bytes(&message.mention_roles))
        .saturating_add(reactions_bytes(&message.reactions))
        .saturating_add(attachments_bytes(&message.attachments))
        .saturating_add(stickers_bytes(&message.stickers))
        .saturating_add(references_bytes(&message.text_references))
        .saturating_add(call_bytes(&message.call));
    if let rune_model::Delivery::Failed { reason } | rune_model::Delivery::Unknown { reason } =
        &message.delivery
    {
        bytes = bytes.saturating_add(reason.capacity());
    }
    bytes
}

fn patch_heap_bytes(patch: &MessagePatch) -> usize {
    text_bytes(&patch.content)
        .saturating_add(patch.mentions.as_ref().map_or(0, vector_bytes))
        .saturating_add(patch.mention_roles.as_ref().map_or(0, vector_bytes))
        .saturating_add(patch.reactions.as_ref().map_or(0, reactions_bytes))
        .saturating_add(patch.attachments.as_ref().map_or(0, attachments_bytes))
        .saturating_add(patch.stickers.as_ref().map_or(0, stickers_bytes))
        .saturating_add(patch.text_references.as_ref().map_or(0, references_bytes))
        .saturating_add(call_bytes(&patch.call))
}

fn call_bytes(call: &Option<rune_model::MessageCall>) -> usize {
    call.as_ref()
        .map_or(0, |call| vector_bytes(&call.participants))
}

fn messages_bytes(messages: &Vec<Message>) -> usize {
    messages
        .iter()
        .fold(vector_bytes(messages), |bytes, message| {
            bytes.saturating_add(message_heap_bytes(message))
        })
}

fn user_heap_bytes(user: &rune_model::User) -> usize {
    user.name
        .capacity()
        .saturating_add(text_bytes(&user.display_name))
        .saturating_add(image_bytes(&user.avatar))
        .saturating_add(user.banner.image().map_or(0, |image| image.url.capacity()))
        .saturating_add(text_bytes(&user.pronouns))
        .saturating_add(text_bytes(&user.about))
        .saturating_add(identity_bytes(&user.identity))
}

/// What the marks around a name keep on the heap (P5).
fn identity_bytes(identity: &rune_model::Identity) -> usize {
    let nameplate = identity.nameplate.as_ref().map_or(0, |plate| {
        plate
            .image
            .url
            .capacity()
            .saturating_add(text_bytes(&plate.label))
            .saturating_add(image_bytes(&plate.animated))
    });
    let tag = identity.tag.as_ref().map_or(0, |tag| {
        tag.tag.capacity().saturating_add(image_bytes(&tag.badge))
    });
    image_bytes(&identity.decoration)
        .saturating_add(nameplate)
        .saturating_add(tag)
}

fn snapshot_bytes(snapshot: &Snapshot) -> usize {
    let mut bytes = size_of::<Snapshot>()
        .saturating_add(vector_bytes(&snapshot.users))
        .saturating_add(vector_bytes(&snapshot.servers))
        .saturating_add(vector_bytes(&snapshot.channels))
        .saturating_add(vector_bytes(&snapshot.members))
        .saturating_add(vector_bytes(&snapshot.presences))
        .saturating_add(vector_bytes(&snapshot.voice))
        .saturating_add(vector_bytes(&snapshot.last_messages))
        .saturating_add(vector_bytes(&snapshot.read_states))
        .saturating_add(vector_bytes(&snapshot.read_versions))
        .saturating_add(vector_bytes(&snapshot.first_unread));
    if let Some(relationships) = &snapshot.relationships {
        bytes = bytes.saturating_add(vector_bytes(relationships));
        for relationship in relationships {
            bytes = bytes.saturating_add(text_bytes(&relationship.nickname));
        }
    }
    if let Some(me) = &snapshot.me {
        bytes = bytes.saturating_add(user_heap_bytes(me));
    }
    for user in &snapshot.users {
        bytes = bytes.saturating_add(user_heap_bytes(user));
    }
    for server in &snapshot.servers {
        bytes = bytes
            .saturating_add(server.name.capacity())
            .saturating_add(image_bytes(&server.icon))
            .saturating_add(text_bytes(&server.description))
            .saturating_add(vector_bytes(&server.roles))
            .saturating_add(vector_bytes(&server.emojis))
            .saturating_add(vector_bytes(&server.stickers));
        for role in &server.roles {
            bytes = bytes
                .saturating_add(role.name.capacity())
                .saturating_add(image_bytes(&role.style.icon))
                .saturating_add(text_bytes(&role.style.emoji))
                .saturating_add(vector_bytes(&role.style.gradient));
        }
        for emoji in &server.emojis {
            bytes = bytes
                .saturating_add(emoji.name.capacity())
                .saturating_add(emoji.insert.capacity())
                .saturating_add(vector_bytes(&emoji.roles))
                .saturating_add(image_bytes(&emoji.image));
        }
        for sticker in &server.stickers {
            bytes = bytes.saturating_add(sticker_bytes(sticker));
        }
    }
    for channel in &snapshot.channels {
        bytes = bytes
            .saturating_add(channel.name.capacity())
            .saturating_add(text_bytes(&channel.topic))
            .saturating_add(vector_bytes(&channel.recipients))
            .saturating_add(vector_bytes(&channel.overwrites));
    }
    for member in &snapshot.members {
        bytes = bytes
            .saturating_add(text_bytes(&member.nick))
            .saturating_add(vector_bytes(&member.roles))
            .saturating_add(identity_bytes(&member.identity));
    }
    for (_, presence) in &snapshot.presences {
        bytes = bytes.saturating_add(text_bytes(&presence.custom));
        bytes = bytes.saturating_add(vector_bytes(&presence.activities));
        for activity in &presence.activities {
            bytes = bytes
                .saturating_add(activity.name.capacity())
                .saturating_add(text_bytes(&activity.detail))
                .saturating_add(text_bytes(&activity.state))
                .saturating_add(image_bytes(&activity.image))
                .saturating_add(image_bytes(&activity.small_image));
        }
    }
    bytes
}

fn restored_bytes(restored: &Restored) -> usize {
    restored
        .snapshot
        .as_ref()
        .map_or(0, snapshot_bytes)
        .saturating_add(vector_bytes(&restored.drafts))
        .saturating_add(
            restored
                .drafts
                .iter()
                .map(|(_, text)| text.capacity())
                .sum(),
        )
        .saturating_add(restored.read_marks.as_ref().map_or(0, vector_bytes))
        .saturating_add(
            restored
                .draft_files
                .iter()
                .fold(vector_bytes(&restored.draft_files), |bytes, (_, files)| {
                    bytes.saturating_add(draft_files_bytes(files))
                }),
        )
}

fn job_bytes(job: &Job) -> usize {
    let heap =
        match job {
            Job::Unlock(unlocking, slot) => size_of::<Unlocking>()
                .saturating_add(unlocking.account.capacity())
                .saturating_add(if slot.is_some() { RESTORE_CREDIT } else { 0 }),
            Job::ForgetAccount(account) => account.capacity(),
            Job::Apply(changes) => changes.iter().fold(vector_bytes(changes), |bytes, change| {
                bytes.saturating_add(match change {
                    MessageMutation::Upsert(message) => {
                        size_of::<Message>().saturating_add(message_heap_bytes(message))
                    }
                    MessageMutation::Patch(patch) => {
                        size_of::<MessagePatch>().saturating_add(patch_heap_bytes(patch))
                    }
                    MessageMutation::History { messages, .. }
                    | MessageMutation::Found(messages) => messages_bytes(messages),
                    MessageMutation::Delete { .. } => 0,
                    MessageMutation::Tally { change, .. } => match change {
                        rune_model::Tally::Reaction { emoji, .. }
                        | rune_model::Tally::Cleared { emoji: Some(emoji) } => emoji.capacity(),
                        rune_model::Tally::Burst { emoji, colors, .. } => {
                            emoji.capacity().saturating_add(vector_bytes(colors))
                        }
                        rune_model::Tally::Cleared { emoji: None }
                        | rune_model::Tally::Vote { .. } => 0,
                    },
                })
            }),
            Job::SaveSnapshot(snapshot) => snapshot_bytes(snapshot),
            Job::ReadMarks { .. } => READ_MARKS_CREDIT,
            Job::Local(read) => read.bytes(),
            Job::SetDraft(_, text) => text_bytes(text),
            Job::SetSetting(key, value) => key.capacity().saturating_add(text_bytes(value)),
            Job::SetDraftFiles(_, files) => draft_files_bytes(files),
            _ => 0,
        };
    size_of::<Queued>().saturating_add(heap)
}

#[cfg(test)]
mod tests;
