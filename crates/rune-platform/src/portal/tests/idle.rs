use super::*;

#[tokio::test]
async fn wait_idle_drains_abandoned_dialog_cleanup_without_renewing_or_opening_another_dialog() {
    let mut portal = bus::Portal::start().await;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let options = request();
    let mut selection = Box::pin(chooser.choose_files(&options, signal.clone()));
    let mut call = tokio::select! {
        call = portal.opened() => call,
        result = &mut selection => panic!("selection ended before OpenFile: {result:?}"),
    };
    call.complete();
    std::future::poll_fn(|cx| match selection.as_mut().poll(cx) {
        Poll::Pending => Poll::Ready(()),
        Poll::Ready(result) => panic!("no response was sent: {result:?}"),
    })
    .await;
    drop(selection);
    assert!(chooser.cleanup.retired.load(Ordering::Acquire));
    let mut idle = Box::pin(chooser.wait_idle(signal));
    let closed = tokio::select! {
        biased;
        closed = portal.closed_before_next_open() => closed,
        result = &mut idle => panic!("idle returned before owned Close: {result:?}"),
    };
    assert_eq!(closed, call.path);
    tokio::time::timeout(Duration::from_secs(5), idle)
        .await
        .unwrap()
        .unwrap();
    assert!(
        chooser.connection.is_closed(),
        "cleanup must finish before local paths are admitted"
    );
    assert!(
        chooser.cleanup.retired.load(Ordering::Acquire),
        "waiting cannot renew a retired connection"
    );
    assert!(
        chooser.address.is_none(),
        "this fixture cannot open a replacement connection"
    );
    portal.no_open();
}

#[tokio::test]
async fn wait_idle_is_cancellable_and_never_opens_a_portal_for_an_idle_chooser() {
    let mut portal = bus::Portal::start().await;
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let (_cancel, signal) = watch::channel(true);
    assert_eq!(chooser.wait_idle(signal).await, Err(PickError::Cancelled));
    let (_cancel, signal) = watch::channel(false);
    assert_eq!(chooser.wait_idle(signal).await, Ok(()));
    assert!(!chooser.connection.is_closed());
    portal.no_open();
}
