//! One cancellable public preview, independent of account HTTP and persisted state.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rune_provider::{
    InvitationError, InvitationPreview, InvitationResult, InvitationTarget, Provider, RequestId,
};

/// The most timeline card lookups in flight at once.
pub(crate) const CARDS: usize = 8;
use tokio::{runtime::Handle, sync::mpsc, task::JoinHandle};

pub(crate) struct Invitations {
    runtime: Handle,
    provider: Arc<dyn Provider>,
    wake: Arc<dyn Fn() + Send + Sync>,
    sender: mpsc::Sender<(u64, InvitationResult)>,
    receiver: mpsc::Receiver<(u64, InvitationResult)>,
    pending: Option<(RequestId, JoinHandle<()>)>,
    card_sender: mpsc::Sender<(u64, InvitationResult)>,
    card_receiver: mpsc::Receiver<(u64, InvitationResult)>,
    cards: Vec<JoinHandle<()>>,
    epoch: u64,
    card_epoch: u64,
    cooldown: Arc<Mutex<Option<(Instant, Duration)>>>,
}

impl Invitations {
    pub fn new(
        runtime: Handle,
        provider: Arc<dyn Provider>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let (card_sender, card_receiver) = mpsc::channel(CARDS);
        Self {
            runtime,
            provider,
            wake,
            sender,
            receiver,
            pending: None,
            card_sender,
            card_receiver,
            cards: Vec::new(),
            epoch: 0,
            card_epoch: 0,
            cooldown: Arc::default(),
        }
    }

    pub fn target(&self, url: &str) -> Option<InvitationTarget> {
        self.provider.invitation_target(url)
    }

    pub fn start(&mut self, request: RequestId, target: InvitationTarget) {
        self.clear_dialog();
        let epoch = self.epoch;
        let sender = self.sender.clone();
        let wake = self.wake.clone();
        let lookup = lookup(self.provider.clone(), target.clone(), self.cooldown.clone());
        self.pending = Some((
            request,
            self.runtime.spawn(async move {
                let result = InvitationResult {
                    request,
                    target,
                    result: lookup.await,
                };
                if sender.send((epoch, result)).await.is_ok() {
                    wake();
                }
            }),
        ));
    }

    /// A lookup for an invitation card of the timeline. Up to [`CARDS`] run
    /// beside the dialog's, which never cancels them; more answer at once
    /// as unavailable, without asking the provider.
    pub fn start_card(&mut self, request: RequestId, target: InvitationTarget) {
        self.cards.retain(|task| !task.is_finished());
        let epoch = self.card_epoch;
        let sender = self.card_sender.clone();
        let wake = self.wake.clone();
        let lookup = (self.cards.len() < CARDS)
            .then(|| lookup(self.provider.clone(), target.clone(), self.cooldown.clone()));
        let task = self.runtime.spawn(async move {
            let result = match lookup {
                Some(lookup) => lookup.await,
                None => Err(InvitationError::Unavailable),
            };
            let result = InvitationResult {
                request,
                target,
                result,
            };
            if sender.send((epoch, result)).await.is_ok() {
                wake();
            }
        });
        self.cards.push(task);
    }

    pub fn poll_card(&mut self) -> Option<InvitationResult> {
        while let Ok((epoch, result)) = self.card_receiver.try_recv() {
            if epoch == self.card_epoch {
                return Some(result);
            }
        }
        None
    }

    pub fn cancel(&mut self, request: RequestId) {
        if self
            .pending
            .as_ref()
            .is_some_and(|(active, _)| *active == request)
        {
            self.clear_dialog();
        }
    }

    /// Drop all account-adjacent results, preserving public transport rate limits.
    pub fn clear(&mut self) {
        self.clear_dialog();
        self.card_epoch = self.card_epoch.wrapping_add(1);
        for task in self.cards.drain(..) {
            task.abort();
        }
        while self.card_receiver.try_recv().is_ok() {}
    }

    fn clear_dialog(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        if let Some((_, task)) = self.pending.take() {
            task.abort();
        }
        while self.receiver.try_recv().is_ok() {}
    }

    pub fn poll(&mut self) -> Option<InvitationResult> {
        while let Ok((epoch, result)) = self.receiver.try_recv() {
            if epoch != self.epoch {
                continue;
            }
            self.pending = None;
            return Some(result);
        }
        None
    }
}

/// The provider's answer for `target`, unless a recent rate limit or the
/// provider's capabilities already answer it, within 15 seconds.
fn lookup(
    provider: Arc<dyn Provider>,
    target: InvitationTarget,
    cooldown: Arc<Mutex<Option<(Instant, Duration)>>>,
) -> impl std::future::Future<Output = Result<InvitationPreview, InvitationError>> {
    let delayed = cooldown
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .and_then(|(at, delay)| {
            let left = delay.saturating_sub(at.elapsed());
            (!left.is_zero()).then_some(left.as_secs().saturating_add(1))
        });
    async move {
        if let Some(retry_after_secs) = delayed {
            return Err(InvitationError::RateLimited { retry_after_secs });
        }
        if !provider.info().capabilities.invitation_preview {
            return Err(InvitationError::Unsupported);
        }
        let result =
            tokio::time::timeout(Duration::from_secs(15), provider.inspect_invitation(target))
                .await
                .unwrap_or(Err(InvitationError::Unavailable));
        // Record transport limits before delivery. Closing a modal may
        // discard its queued result, but must not discard Retry-After.
        if let Err(InvitationError::RateLimited { retry_after_secs }) = result {
            let mut limit = cooldown
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let left = limit
                .map(|(at, delay)| delay.saturating_sub(at.elapsed()))
                .unwrap_or_default();
            *limit = Some((
                Instant::now(),
                left.max(Duration::from_secs(retry_after_secs)),
            ));
        }
        result
    }
}

impl Drop for Invitations {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_provider::{
        BoxFuture, Capabilities, Command, EventSink, InvitationPreview, ProviderError,
        ProviderInfo, ProviderKind,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct PublicProvider {
        calls: AtomicUsize,
        dropped: Arc<AtomicUsize>,
    }
    struct DropWatch(Arc<AtomicUsize>);
    impl Drop for DropWatch {
        fn drop(&mut self) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    impl Provider for PublicProvider {
        fn info(&self) -> ProviderInfo {
            ProviderInfo {
                name: "Prueba".into(),
                kind: ProviderKind::Simulated,
                capabilities: Capabilities {
                    invitation_preview: true,
                    ..Default::default()
                },
                notice: String::new(),
            }
        }
        fn inspect_invitation(
            &self,
            target: InvitationTarget,
        ) -> BoxFuture<'_, Result<InvitationPreview, InvitationError>> {
            Box::pin(async move {
                self.calls.fetch_add(1, Ordering::SeqCst);
                let _watch = DropWatch(self.dropped.clone());
                match target.key.as_str() {
                    "wait" => std::future::pending().await,
                    "rate" => Err(InvitationError::RateLimited {
                        retry_after_secs: 120,
                    }),
                    _ => Err(InvitationError::Invalid),
                }
            })
        }
        fn connect<'a>(
            &'a self,
            _: &'a mut mpsc::Receiver<Command>,
            _: &'a EventSink,
        ) -> BoxFuture<'a, Result<(), ProviderError>> {
            Box::pin(async { Ok(()) })
        }
    }
    fn target(key: &str) -> InvitationTarget {
        InvitationTarget {
            key: key.into(),
            url: "https://example.invalid/invite".into(),
        }
    }
    async fn result(service: &mut Invitations) -> InvitationResult {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(result) = service.poll() {
                    return result;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn replacing_and_closing_preview_drops_work_and_filters_stale_results() {
        let dropped = Arc::new(AtomicUsize::new(0));
        let provider = Arc::new(PublicProvider {
            calls: AtomicUsize::new(0),
            dropped: dropped.clone(),
        });
        let mut service = Invitations::new(Handle::current(), provider.clone(), Arc::new(|| {}));
        service.start(1, target("wait"));
        while provider.calls.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
        service.start(2, target("done"));
        assert_eq!(result(&mut service).await.request, 2);
        assert_eq!(dropped.load(Ordering::SeqCst), 2);
        service.start(3, target("wait"));
        while provider.calls.load(Ordering::SeqCst) < 3 {
            tokio::task::yield_now().await;
        }
        service.cancel(3);
        tokio::task::yield_now().await;
        assert!(service.poll().is_none());
        assert_eq!(dropped.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn card_lookups_are_bounded_survive_the_dialog_and_end_with_the_session() {
        let provider = Arc::new(PublicProvider {
            calls: AtomicUsize::new(0),
            dropped: Arc::new(AtomicUsize::new(0)),
        });
        let mut service = Invitations::new(Handle::current(), provider.clone(), Arc::new(|| {}));
        for request in 0..CARDS as u64 {
            service.start_card(request, target("wait"));
        }
        while provider.calls.load(Ordering::SeqCst) < CARDS {
            tokio::task::yield_now().await;
        }
        // One more than the bound answers without asking the provider.
        service.start_card(99, target("wait"));
        let extra = tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Some(result) = service.poll_card() {
                    return result;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(
            (extra.request, extra.result),
            (99, Err(InvitationError::Unavailable))
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), CARDS);
        // The dialog's own lookup neither waits for nor cancels the cards.
        service.start(100, target("done"));
        assert_eq!(result(&mut service).await.request, 100);
        assert_eq!(
            service.cards.iter().filter(|t| !t.is_finished()).count(),
            CARDS
        );
        service.clear();
        tokio::task::yield_now().await;
        assert!(service.cards.is_empty() && service.poll_card().is_none());
    }

    #[tokio::test]
    async fn public_rate_limits_survive_modal_and_session_cleanup() {
        let provider = Arc::new(PublicProvider {
            calls: AtomicUsize::new(0),
            dropped: Arc::new(AtomicUsize::new(0)),
        });
        let mut service = Invitations::new(Handle::current(), provider.clone(), Arc::new(|| {}));
        service.start(1, target("rate"));
        assert!(matches!(
            result(&mut service).await.result,
            Err(InvitationError::RateLimited { .. })
        ));
        service.clear();
        service.start(2, target("done"));
        assert!(matches!(
            result(&mut service).await.result,
            Err(InvitationError::RateLimited { .. })
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn closing_before_polling_a_completed_rate_limit_still_prevents_another_request() {
        let provider = Arc::new(PublicProvider {
            calls: AtomicUsize::new(0),
            dropped: Arc::new(AtomicUsize::new(0)),
        });
        let mut service = Invitations::new(Handle::current(), provider.clone(), Arc::new(|| {}));
        service.start(1, target("rate"));
        while service.receiver.is_empty() {
            tokio::task::yield_now().await;
        }
        service.clear();
        service.start(2, target("done"));
        assert!(matches!(
            result(&mut service).await.result,
            Err(InvitationError::RateLimited { .. })
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }
}
