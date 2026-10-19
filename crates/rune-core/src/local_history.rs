//! One cancellable local read; busy storage retains only the newest intent.

use std::time::{Duration, Instant};

use rune_model::{
    CachedContext, ChannelId, LocalReadError, MessageId, SearchCursor, SearchPage, SearchQuery,
};
use rune_provider::RequestId;

use super::storage::{ContextRequest, LocalReadReply, LocalReadTag, SearchRequest};
use crate::{Client, StorageWorker, StorageWriteError};

const RETRY: Duration = Duration::from_millis(250);

#[derive(Debug, PartialEq)]
pub enum LocalReadResult {
    Search {
        request: RequestId,
        channel: ChannelId,
        result: Result<SearchPage, LocalReadError>,
    },
    Context {
        request: RequestId,
        channel: ChannelId,
        result: Result<CachedContext, LocalReadError>,
    },
}

#[derive(Clone, Copy)]
enum Kind {
    Search,
    Context,
}

enum Pending {
    Search(SearchRequest),
    Context(ContextRequest),
}

struct Active {
    tag: LocalReadTag,
    kind: Kind,
    since: Instant,
    retry_at: Instant,
    pending: Option<Pending>,
}

#[derive(Default)]
pub(super) struct LocalHistory {
    epoch: u64,
    active: Option<Active>,
    result: Option<LocalReadResult>,
}

impl Kind {
    fn failure(self, tag: LocalReadTag, error: LocalReadError) -> LocalReadResult {
        match self {
            Self::Search => LocalReadResult::Search {
                request: tag.request,
                channel: tag.channel,
                result: Err(error),
            },
            Self::Context => LocalReadResult::Context {
                request: tag.request,
                channel: tag.channel,
                result: Err(error),
            },
        }
    }
}

impl Client {
    pub fn local_history_available(&self) -> bool {
        self.storage_active && self.storage.is_some() && self.storage_error.is_none()
    }

    pub fn search_local(
        &mut self,
        request: RequestId,
        query: SearchQuery,
        cursor: Option<SearchCursor>,
    ) {
        let tag = LocalReadTag {
            epoch: self.local_history.epoch,
            request,
            channel: query.channel,
        };
        let error = query.validate().err();
        self.begin_local_read(
            tag,
            Kind::Search,
            Pending::Search(SearchRequest {
                tag,
                query: Box::new(query),
                cursor,
            }),
            error,
        );
    }

    pub fn load_cached_context(
        &mut self,
        request: RequestId,
        channel: ChannelId,
        anchor: MessageId,
    ) {
        self.cancel_channel_history(channel);
        let tag = LocalReadTag {
            epoch: self.local_history.epoch,
            request,
            channel,
        };
        let error = (channel.0 == 0 || anchor.0 == 0).then_some(LocalReadError::InvalidQuery);
        self.begin_local_read(
            tag,
            Kind::Context,
            Pending::Context(ContextRequest { tag, anchor }),
            error,
        );
    }

    fn begin_local_read(
        &mut self,
        tag: LocalReadTag,
        kind: Kind,
        pending: Pending,
        error: Option<LocalReadError>,
    ) {
        self.cancel_local_reads(None);
        self.local_history.result = None;
        if let Some(error) = error
            .or_else(|| (!self.local_history_available()).then_some(LocalReadError::Unavailable))
        {
            self.local_history.result = Some(kind.failure(tag, error));
            return;
        }
        let now = Instant::now();
        self.local_history.active = Some(Active {
            tag,
            kind,
            since: now,
            retry_at: now,
            pending: Some(pending),
        });
        self.retry_local_read(now);
    }

    pub(super) fn cancel_local_reads(&mut self, error: Option<LocalReadError>) {
        self.local_history.result = None;
        if let Some(active) = self.local_history.active.take() {
            if let Some(storage) = &self.storage {
                storage.cancel_read(active.tag.request);
            }
            self.local_history.result = error.map(|error| active.kind.failure(active.tag, error));
        }
    }

    pub(super) fn invalidate_local_history(&mut self, error: LocalReadError) {
        self.local_history.epoch = self.local_history.epoch.wrapping_add(1);
        // A completed cursor also retains an account's query. Clear it even
        // when no read is currently awaiting a reply.
        if let Some(storage) = &self.storage {
            storage.cancel_all_reads();
        }
        self.cancel_local_reads(Some(error));
    }

    pub(super) fn cancel_local_read(&mut self, request: RequestId) {
        if self
            .local_history
            .active
            .as_ref()
            .is_some_and(|active| active.tag.request == request)
        {
            self.cancel_local_reads(None);
        } else if let Some(storage) = &self.storage {
            storage.cancel_read(request);
        }
        if self
            .local_history
            .result
            .as_ref()
            .is_some_and(|result| match result {
                LocalReadResult::Search { request: held, .. }
                | LocalReadResult::Context { request: held, .. } => *held == request,
            })
        {
            self.local_history.result = None;
        }
    }

    fn retry_local_read(&mut self, now: Instant) {
        if self.local_history.active.is_none() {
            return;
        }
        if !self.local_history_available() {
            self.cancel_local_reads(Some(LocalReadError::Unavailable));
            return;
        }
        if self.local_history.active.as_ref().is_some_and(|active| {
            now.saturating_duration_since(active.since) >= self.deadlines.history
        }) {
            self.cancel_local_reads(Some(LocalReadError::Failed));
            return;
        }
        let Some(active) = self.local_history.active.as_mut() else {
            return;
        };
        if now < active.retry_at {
            return;
        }
        active.retry_at = now + RETRY;
        // Observe all already-received reliable writes before admitting this read.
        if self.pending_storage.is_some() {
            return;
        }
        let Some(pending) = active.pending.take() else {
            return;
        };
        let storage = self
            .storage
            .as_ref()
            .expect("available local history has a worker");
        let rejected = match pending {
            Pending::Search(request) => storage
                .request_search(request)
                .err()
                .map(|error| (Pending::Search(error.value), error.kind)),
            Pending::Context(request) => storage
                .request_context(request)
                .err()
                .map(|error| (Pending::Context(error.value), error.kind)),
        };
        match rejected {
            Some((pending, StorageWriteError::Busy)) => active.pending = Some(pending),
            Some((_, error)) => self.cancel_local_reads(Some(match error {
                StorageWriteError::TooLarge => LocalReadError::TooLarge,
                _ => LocalReadError::Unavailable,
            })),
            None => {}
        }
    }

    /// Drain independently of provider backpressure, releasing reply credit even
    /// when the provider filled this frame's update quota.
    pub fn poll_local_read(&mut self) -> Option<LocalReadResult> {
        self.retry_local_read(Instant::now());
        for _ in 0..16 {
            let Some(reply) = self
                .storage
                .as_ref()
                .and_then(StorageWorker::try_local_read)
            else {
                break;
            };
            let (tag, kind) = match &reply {
                LocalReadReply::Search { tag, .. } => (*tag, Kind::Search),
                LocalReadReply::Context { tag, .. } => (*tag, Kind::Context),
            };
            if !self.local_history.active.as_ref().is_some_and(|active| {
                active.tag == tag
                    && matches!(
                        (active.kind, kind),
                        (Kind::Search, Kind::Search) | (Kind::Context, Kind::Context)
                    )
            }) {
                continue;
            }
            self.local_history.active = None;
            self.local_history.result = Some(match reply {
                LocalReadReply::Search { tag, result } => LocalReadResult::Search {
                    request: tag.request,
                    channel: tag.channel,
                    result,
                },
                LocalReadReply::Context { tag, result } => LocalReadResult::Context {
                    request: tag.request,
                    channel: tag.channel,
                    result,
                },
            });
        }
        self.local_history.result.take()
    }

    pub(super) fn local_read_deadline(&self) -> Option<Instant> {
        if self.local_history.result.is_some() {
            return Some(Instant::now());
        }
        self.local_history.active.as_ref().map(|active| {
            let timeout = active.since + self.deadlines.history;
            if active.pending.is_some() {
                timeout.min(active.retry_at)
            } else {
                timeout
            }
        })
    }

    pub(super) fn cancel_local_history_except(&mut self, keep: ChannelId) {
        if self
            .local_history
            .active
            .as_ref()
            .is_some_and(|active| active.tag.channel != keep)
        {
            self.cancel_local_reads(None);
        }
    }
}
