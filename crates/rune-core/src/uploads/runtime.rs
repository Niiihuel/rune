//! Runtime shutdown owns pending payload destructors, away from the UI caller.

use std::{
    ops::Deref,
    sync::{Arc, Mutex},
};

pub(crate) struct RuntimeOwner(Option<tokio::runtime::Runtime>);

impl RuntimeOwner {
    pub(crate) fn new(runtime: tokio::runtime::Runtime) -> Self {
        Self(Some(runtime))
    }
}

impl Deref for RuntimeOwner {
    type Target = tokio::runtime::Runtime;
    fn deref(&self) -> &Self::Target {
        self.0.as_ref().unwrap()
    }
}

impl Drop for RuntimeOwner {
    fn drop(&mut self) {
        let Some(runtime) = self.0.take() else {
            return;
        };
        // Keep ownership outside the spawn closure too: failure to create an
        // OS thread must not implicitly drop a Runtime in an async/UI context.
        let pending = Arc::new(Mutex::new(Some(runtime)));
        let closing = pending.clone();
        if std::thread::Builder::new()
            .name("rune-core-shutdown".into())
            .spawn(move || {
                drop(
                    closing
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take(),
                );
            })
            .is_err()
        {
            // Under OS thread exhaustion Tokio's nonwaiting shutdown is the
            // fallback. Normal off-thread destructor guarantees do not extend
            // to that exceptional condition.
            if let Some(runtime) = pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take()
            {
                runtime.shutdown_background();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::{future::pending, sync::mpsc, thread, time::Duration};

    struct Payload {
        data: Vec<u8>,
        dropped: mpsc::Sender<thread::ThreadId>,
    }

    impl AsRef<[u8]> for Payload {
        fn as_ref(&self) -> &[u8] {
            &self.data
        }
    }

    impl Drop for Payload {
        fn drop(&mut self) {
            let _ = self.dropped.send(thread::current().id());
        }
    }

    #[test]
    fn a_queued_payload_is_destroyed_outside_the_runtime_owner_caller() {
        // A current-thread scheduler makes the destructor's owner explicit:
        // a plain Runtime drop would destroy this queue on this test thread.
        let runtime = RuntimeOwner::new(
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap(),
        );
        let (dropped, observed) = mpsc::channel();
        let bytes = Bytes::from_owner(Payload {
            data: vec![7; 1024 * 1024],
            dropped,
        });
        let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
        sender.try_send(bytes).unwrap();
        drop(sender);
        let (started, ready) = tokio::sync::oneshot::channel();
        runtime.spawn(async move {
            let _ = started.send(());
            pending::<()>().await;
            let _ = receiver.recv().await;
        });
        runtime.block_on(ready).unwrap();
        let caller = thread::current().id();
        drop(runtime);
        let destructor = observed
            .recv_timeout(Duration::from_secs(3))
            .expect("shutdown must release the queued byte owner");
        assert_ne!(
            destructor, caller,
            "the UI caller must not perform the last payload drop"
        );
    }

    #[test]
    fn closing_returns_before_an_existing_blocking_operation_finishes() {
        let runtime = RuntimeOwner::new(
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap(),
        );
        let (started, ready) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let (finished, complete) = mpsc::channel();
        runtime.spawn_blocking(move || {
            started.send(()).unwrap();
            let _ = wait.recv();
            let _ = finished.send(());
        });
        ready.recv_timeout(Duration::from_secs(3)).unwrap();
        let (returned, closed) = mpsc::channel();
        let caller = thread::Builder::new()
            .name("rune-ui-close-fixture".into())
            .spawn(move || {
                drop(runtime);
                let _ = returned.send(());
            })
            .unwrap();
        let returned_before_release = closed.recv_timeout(Duration::from_secs(1)).is_ok();
        // Release even a failing baseline so the test cannot strand its worker.
        release.send(()).unwrap();
        caller.join().unwrap();
        complete.recv_timeout(Duration::from_secs(3)).unwrap();
        assert!(
            returned_before_release,
            "closing the client must not wait for its blocking pool on the UI thread"
        );
    }
}
