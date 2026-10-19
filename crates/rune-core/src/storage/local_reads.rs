//! Cancelable local reads share the worker's byte budget with reliable writes.

use rune_model::{
    CachedContext, ChannelId, LocalReadError, MessageId, SearchCursor, SearchPage, SearchQuery,
};
use std::mem::size_of;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::Arc;
use std::time::Duration;

use super::{
    Answer, AnswerSlot, Job, MessageMutation, Reservation, Shared, Storage, StorageWorker,
    StorageWriteError, SubmitError, IDLE,
};

const SEARCH_CREDIT: usize = 64 * 1024;
const CONTEXT_CREDIT: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalReadTag {
    pub epoch: u64,
    pub request: u64,
    pub channel: ChannelId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SearchRequest {
    pub tag: LocalReadTag,
    pub query: Box<SearchQuery>,
    pub cursor: Option<SearchCursor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextRequest {
    pub tag: LocalReadTag,
    pub anchor: MessageId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LocalReadReply {
    Search {
        tag: LocalReadTag,
        result: Result<SearchPage, LocalReadError>,
    },
    Context {
        tag: LocalReadTag,
        result: Result<CachedContext, LocalReadError>,
    },
}

impl StorageWorker {
    pub fn request_search(&self, request: SearchRequest) -> Result<(), SubmitError<SearchRequest>> {
        self.request_local(ReadRequest::Search(request))
            .map_err(|error| {
                let ReadRequest::Search(value) = error.value else {
                    unreachable!()
                };
                SubmitError {
                    kind: error.kind,
                    value,
                }
            })
    }

    pub fn request_context(
        &self,
        request: ContextRequest,
    ) -> Result<(), SubmitError<ContextRequest>> {
        self.request_local(ReadRequest::Context(request))
            .map_err(|error| {
                let ReadRequest::Context(value) = error.value else {
                    unreachable!()
                };
                SubmitError {
                    kind: error.kind,
                    value,
                }
            })
    }

    fn request_local(&self, request: ReadRequest) -> Result<(), SubmitError<ReadRequest>> {
        if self.jobs.is_none() || self.shared.closed.load(Ordering::Acquire) {
            return Err(SubmitError {
                kind: StorageWriteError::Closed,
                value: request,
            });
        }
        if self
            .shared
            .read_busy
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(SubmitError {
                kind: StorageWriteError::Busy,
                value: request,
            });
        }
        let request_id = request.tag().request;
        let cancellation = self.shared.read_cancellation.load(Ordering::Acquire);
        if let Err(error) = self.submit(Job::Local(ReadJob {
            request,
            cancellation,
        })) {
            self.shared.read_busy.store(false, Ordering::Release);
            let Job::Local(job) = error.value else {
                unreachable!()
            };
            return Err(SubmitError {
                kind: error.kind,
                value: job.request,
            });
        }
        self.shared
            .read_request
            .store(request_id, Ordering::Release);
        Ok(())
    }

    pub fn cancel_read(&self, request: u64) {
        if self.shared.read_request.load(Ordering::Acquire) == request {
            self.cancel_all_reads();
        }
    }

    pub fn cancel_all_reads(&self) {
        self.shared.read_cancellation.fetch_add(1, Ordering::AcqRel);
        if let Some(thread) = &self.thread {
            thread.thread().unpark();
        }
        (self.shared.wake)();
    }

    pub fn try_local_read(&self) -> Option<LocalReadReply> {
        let answer = self.local.try_recv().ok()?;
        let mut delivered = answer.value;
        if delivered.cancellation != self.shared.read_cancellation.load(Ordering::Acquire) {
            delivered.reply.fail(LocalReadError::Cancelled);
        }
        self.shared.read_busy.store(false, Ordering::Release);
        Some(delivered.reply)
    }
}

impl LocalReadReply {
    fn fail(&mut self, error: LocalReadError) {
        match self {
            Self::Search { result, .. } => *result = Err(error),
            Self::Context { result, .. } => *result = Err(error),
        }
    }
}

enum ReadRequest {
    Search(SearchRequest),
    Context(ContextRequest),
}

impl ReadRequest {
    fn tag(&self) -> LocalReadTag {
        match self {
            Self::Search(request) => request.tag,
            Self::Context(request) => request.tag,
        }
    }
}

pub(super) struct ReadJob {
    request: ReadRequest,
    cancellation: u64,
}

impl ReadJob {
    pub(super) fn bytes(&self) -> usize {
        match &self.request {
            ReadRequest::Search(request) => SEARCH_CREDIT
                .saturating_add(size_of::<SearchQuery>())
                .saturating_add(request.query.text.capacity()),
            ReadRequest::Context(_) => CONTEXT_CREDIT,
        }
    }
}

pub(super) struct Delivered {
    reply: LocalReadReply,
    cancellation: u64,
}

struct Pending {
    delivered: Delivered,
    reservation: Reservation,
}

struct SearchSession {
    epoch: u64,
    query: SearchQuery,
    cursor: Option<SearchCursor>,
    before: Option<MessageId>,
    valid: bool,
    cancellation: u64,
    _reservation: Reservation,
}

struct Task {
    tag: LocalReadTag,
    cancellation: u64,
    kind: TaskKind,
    reservation: Reservation,
}

enum TaskKind {
    Search {
        before: Option<MessageId>,
        hits: Vec<rune_model::SearchHit>,
    },
    Context(MessageId),
    Failed {
        error: LocalReadError,
        search: bool,
    },
}

#[derive(Default)]
pub(super) struct ReadRunner {
    session: Option<SearchSession>,
    task: Option<Task>,
    pending: Option<Pending>,
    next_cursor: u64,
}

impl ReadRunner {
    pub(super) fn working(&self) -> bool {
        self.task.is_some() || self.pending.is_some()
    }

    pub(super) fn wait_timeout(&self) -> Duration {
        if self.task.is_some() {
            Duration::ZERO
        } else if self.pending.is_some() {
            Duration::from_millis(16)
        } else {
            IDLE
        }
    }

    pub(super) fn invalidate(&mut self) {
        if let Some(session) = &mut self.session {
            session.valid = false;
        }
    }

    pub(super) fn observe(&mut self, job: &Job) {
        let Some(session) = &self.session else {
            return;
        };
        let channel = session.query.channel;
        let invalid = match job {
            Job::Apply(changes) => changes.iter().any(|change| match change {
                MessageMutation::Upsert(message) => message.channel == channel,
                MessageMutation::Patch(patch) => patch.channel == channel,
                MessageMutation::History {
                    channel: changed, ..
                }
                | MessageMutation::Delete {
                    channel: changed, ..
                } => *changed == channel,
                // A search reads what searches found too.
                MessageMutation::Found(messages) => {
                    messages.iter().any(|message| message.channel == channel)
                }
                // Reactions and votes change nothing a local search reads.
                MessageMutation::Tally { .. } => false,
            }),
            Job::Forget => true,
            _ => false,
        };
        if invalid {
            self.invalidate();
        }
    }

    pub(super) fn begin(
        &mut self,
        job: ReadJob,
        mut reservation: Reservation,
        shared: &Arc<Shared>,
    ) {
        debug_assert!(self.task.is_none() && self.pending.is_none());
        let tag = job.request.tag();
        let kind = match job.request {
            ReadRequest::Context(request) => {
                self.session = None;
                TaskKind::Context(request.anchor)
            }
            ReadRequest::Search(request) => {
                let validation = request.query.validate().and_then(|()| {
                    if request.query.channel != tag.channel {
                        Err(LocalReadError::InvalidQuery)
                    } else {
                        Ok(())
                    }
                });
                if let Err(error) = validation {
                    TaskKind::Failed {
                        error,
                        search: true,
                    }
                } else if let Some(cursor) = request.cursor {
                    match &mut self.session {
                        Some(session)
                            if session.valid
                                && session.epoch == tag.epoch
                                && session.cancellation == job.cancellation
                                && session.query == *request.query
                                && session.cursor == Some(cursor) =>
                        {
                            session.cursor = None;
                            TaskKind::Search {
                                before: session.before,
                                hits: Vec::with_capacity(20),
                            }
                        }
                        _ => TaskKind::Failed {
                            error: LocalReadError::StaleCursor,
                            search: true,
                        },
                    }
                } else {
                    let bytes =
                        size_of::<SearchSession>().saturating_add(request.query.text.capacity());
                    self.session = Some(SearchSession {
                        epoch: tag.epoch,
                        query: *request.query,
                        cursor: None,
                        before: None,
                        valid: true,
                        cancellation: job.cancellation,
                        _reservation: reservation.split(bytes),
                    });
                    TaskKind::Search {
                        before: None,
                        hits: Vec::with_capacity(20),
                    }
                }
            }
        };
        self.task = Some(Task {
            tag,
            cancellation: job.cancellation,
            kind,
            reservation,
        });
        (shared.wake)();
    }

    pub(super) fn tick(
        &mut self,
        storage: &Storage,
        account: &str,
        answers: &SyncSender<Answer<Delivered>>,
        shared: &Arc<Shared>,
    ) {
        let cancellation = shared.read_cancellation.load(Ordering::Acquire);
        if self
            .session
            .as_ref()
            .is_some_and(|session| session.cancellation != cancellation)
        {
            self.session = None;
        }
        if let Some(mut task) = self.task.take() {
            let cancelled =
                || task.cancellation != shared.read_cancellation.load(Ordering::Acquire);
            let search = matches!(
                task.kind,
                TaskKind::Search { .. } | TaskKind::Failed { search: true, .. }
            );
            let reply = if cancelled() {
                Some(if search {
                    LocalReadReply::Search {
                        tag: task.tag,
                        result: Err(LocalReadError::Cancelled),
                    }
                } else {
                    LocalReadReply::Context {
                        tag: task.tag,
                        result: Err(LocalReadError::Cancelled),
                    }
                })
            } else {
                match &mut task.kind {
                    TaskKind::Failed { error, search } => Some(if *search {
                        LocalReadReply::Search {
                            tag: task.tag,
                            result: Err(*error),
                        }
                    } else {
                        LocalReadReply::Context {
                            tag: task.tag,
                            result: Err(*error),
                        }
                    }),
                    TaskKind::Context(anchor) => {
                        let result = storage
                            .cached_context(account, task.tag.channel, *anchor, &cancelled)
                            .and_then(bound_context);
                        Some(LocalReadReply::Context {
                            tag: task.tag,
                            result,
                        })
                    }
                    TaskKind::Search { before, hits } => {
                        let result = match self.session.as_ref().filter(|session| session.valid) {
                            Some(session) => storage.search_chunk(
                                account,
                                &session.query,
                                *before,
                                20 - hits.len(),
                                &cancelled,
                            ),
                            None => Err(LocalReadError::StaleCursor),
                        };
                        match result {
                            Err(error) => Some(LocalReadReply::Search {
                                tag: task.tag,
                                result: Err(error),
                            }),
                            Ok(chunk) => {
                                *before = chunk.before;
                                hits.extend(chunk.hits);
                                if hits.len() == 20 || !chunk.more {
                                    let continuation = if chunk.more {
                                        self.next_cursor = self
                                            .next_cursor
                                            .checked_add(1)
                                            .expect("local search cursor overflow");
                                        let cursor = SearchCursor(self.next_cursor);
                                        if let Some(session) = &mut self.session {
                                            session.before = *before;
                                            session.cursor = Some(cursor);
                                        }
                                        Some(cursor)
                                    } else {
                                        self.session = None;
                                        None
                                    };
                                    Some(LocalReadReply::Search {
                                        tag: task.tag,
                                        result: Ok(SearchPage {
                                            hits: std::mem::take(hits),
                                            continuation,
                                        }),
                                    })
                                } else {
                                    None
                                }
                            }
                        }
                    }
                }
            };
            if let Some(reply) = reply {
                self.pending = Some(Pending {
                    delivered: Delivered {
                        reply,
                        cancellation: task.cancellation,
                    },
                    reservation: task.reservation,
                });
            } else {
                self.task = Some(task);
            }
        }
        let Some(mut pending) = self.pending.take() else {
            return;
        };
        if pending.delivered.cancellation != cancellation {
            pending.delivered.reply.fail(LocalReadError::Cancelled);
        }
        let Some(slot) = AnswerSlot::reserve(shared) else {
            self.pending = Some(pending);
            return;
        };
        let answer = Answer {
            value: pending.delivered,
            _reservation: pending.reservation,
            _slot: slot,
        };
        match answers.try_send(answer) {
            Ok(()) => (shared.wake)(),
            Err(TrySendError::Full(answer)) => {
                self.pending = Some(Pending {
                    delivered: answer.value,
                    reservation: answer._reservation,
                });
            }
            Err(TrySendError::Disconnected(_)) => {
                shared.read_busy.store(false, Ordering::Release);
            }
        }
    }
}

fn bound_context(mut context: CachedContext) -> Result<CachedContext, LocalReadError> {
    while super::messages_bytes(&context.messages).saturating_add(size_of::<LocalReadReply>())
        > CONTEXT_CREDIT
    {
        let position = context
            .messages
            .iter()
            .position(|message| message.id == context.anchor)
            .ok_or(LocalReadError::NotFoundLocal)?;
        if context.messages.len() == 1 {
            return Err(LocalReadError::TooLarge);
        }
        if position >= context.messages.len() - position - 1 {
            context.messages.remove(0);
            context.has_older_cached = true;
        } else {
            context.messages.pop();
            context.has_newer_cached = true;
        }
    }
    Ok(context)
}
