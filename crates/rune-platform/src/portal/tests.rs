pub(crate) mod bus;
mod idle;
mod multiple;
mod save;
// Linux desktop (escritorio-linux).
mod folder;

use std::time::Duration;

use tokio::sync::watch;

use super::*;
use crate::file_dialogs::{PickId, PickRequest};

fn request() -> PickRequest {
    PickRequest {
        id: PickId(1),
        max_files: 1,
        max_file_bytes: 1024,
        max_total_bytes: 1024,
        parent: None,
        current_folder: None,
    }
}

async fn result<T>(task: tokio::task::JoinHandle<T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), task)
        .await
        .expect("a terminal chooser result must not wait for another portal signal")
        .unwrap()
}

#[tokio::test]
async fn a_response_before_open_file_returns_is_not_lost_or_published_before_handle_validation() {
    let mut portal = bus::Portal::start().await;
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"fixture").unwrap();
    let uri = url::Url::from_file_path(file.path()).unwrap().to_string();
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let mut selecting = tokio::spawn(async move { chooser.choose_files(&request(), signal).await });
    let mut call = tokio::select! {
        call = portal.opened() => call,
        result = &mut selecting => panic!("selection terminated before OpenFile: {result:?}"),
    };
    assert!(
        call.parent.is_empty(),
        "Wayland without an exported parent uses an empty identifier"
    );
    assert!(!bool::try_from(call.options.remove("multiple").unwrap()).unwrap());
    portal.respond(&call, 0, &[&uri]).await;
    assert!(
        !selecting.is_finished(),
        "a response alone does not validate the returned request handle"
    );
    call.complete();
    let selected = result(selecting)
        .await
        .expect("the early response must reach the file worker");
    assert_eq!(selected.len(), 1);
    assert_eq!(selected[0].info().size_bytes, 7);
}

#[tokio::test]
async fn cancelling_while_open_file_returns_closes_only_that_request_without_waiting_for_response()
{
    let mut portal = bus::Portal::start().await;
    let (cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&request(), signal).await });
    let mut call = portal.opened().await;
    cancel.send_replace(true);
    call.complete();
    assert_eq!(portal.closed().await, call.path);
    assert!(matches!(result(selecting).await, Err(PickError::Cancelled)));
}

#[tokio::test]
async fn an_initial_cancellation_never_opens_a_dialog() {
    let mut portal = bus::Portal::start().await;
    let (_cancel, signal) = watch::channel(true);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    assert!(matches!(
        chooser.choose_files(&request(), signal).await,
        Err(PickError::Cancelled)
    ));
    portal.no_open();
}

#[tokio::test]
async fn the_suggested_fixture_folder_is_passed_as_nul_terminated_filesystem_bytes() {
    let mut portal = bus::Portal::start().await;
    let directory = tempfile::tempdir().unwrap();
    let mut options = request();
    options.current_folder = Some(directory.path().to_owned());
    let expected = [directory.path().as_os_str().as_encoded_bytes(), &[0]].concat();
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&options, signal).await });
    let mut call = portal.opened().await;
    let folder = Vec::<u8>::try_from(call.options.remove("current_folder").unwrap()).unwrap();
    assert_eq!(folder, expected);
    call.complete();
    portal.respond(&call, 1, &[]).await;
    assert!(matches!(result(selecting).await, Err(PickError::Cancelled)));
}

#[tokio::test]
async fn a_successful_early_response_cannot_bypass_a_mismatched_returned_handle() {
    let mut portal = bus::Portal::start().await;
    let file = tempfile::NamedTempFile::new().unwrap();
    let uri = url::Url::from_file_path(file.path()).unwrap().to_string();
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&request(), signal).await });
    let mut call = portal.opened().await;
    portal.respond(&call, 0, &[&uri]).await;
    call.complete_as(OwnedObjectPath::try_from("/an_unrelated_request").unwrap());
    assert_eq!(
        portal.closed().await,
        call.path,
        "never close a path returned for another request"
    );
    assert!(matches!(
        result(selecting).await,
        Err(PickError::InvalidResponse)
    ));
}

#[tokio::test]
async fn responses_from_another_sender_or_request_path_are_ignored() {
    let mut portal = bus::Portal::start().await;
    let stranger = portal.bus.connect().await;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&request(), signal).await });
    let mut call = portal.opened().await;
    bus::Portal::response_from(
        &stranger,
        &call,
        0,
        HashMap::from([("uris", Value::from(vec!["file:///must-not-be-opened"]))]),
    )
    .await;
    portal
        .service
        .emit_signal(
            Some(call.client.as_str()),
            format!("{}_other", call.path).as_str(),
            REQUEST,
            "Response",
            &(
                0u32,
                HashMap::from([("uris", Value::from(vec!["file:///must-not-be-opened"]))]),
            ),
        )
        .await
        .unwrap();
    call.complete();
    portal.respond(&call, 1, &[]).await;
    assert!(matches!(result(selecting).await, Err(PickError::Cancelled)));
}

#[tokio::test]
async fn releasing_the_service_name_invalidates_the_request_even_when_the_old_owner_stays_connected(
) {
    let mut portal = bus::Portal::start().await;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&request(), signal).await });
    let mut call = portal.opened().await;
    call.complete();
    assert!(portal.service.release_name(DESTINATION).await.unwrap());
    let replacement = portal.bus.connect().await;
    replacement.request_name(DESTINATION).await.unwrap();
    bus::Portal::response_from(
        &replacement,
        &call,
        0,
        HashMap::from([("uris", Value::from(vec!["file:///must-not-be-opened"]))]),
    )
    .await;
    assert_eq!(portal.closed().await, call.path);
    assert!(matches!(
        result(selecting).await,
        Err(PickError::Unavailable)
    ));
}

#[tokio::test]
async fn duplicate_early_responses_fail_instead_of_producing_a_file_grant() {
    let mut portal = bus::Portal::start().await;
    let file = tempfile::NamedTempFile::new().unwrap();
    let uri = url::Url::from_file_path(file.path()).unwrap().to_string();
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&request(), signal).await });
    let mut call = portal.opened().await;
    portal.respond(&call, 0, &[&uri]).await;
    portal.respond(&call, 0, &[&uri]).await;
    call.complete();
    assert!(matches!(
        result(selecting).await,
        Err(PickError::InvalidResponse)
    ));
}

#[tokio::test]
async fn a_dropped_cancellation_sender_cancels_the_owned_request() {
    let mut portal = bus::Portal::start().await;
    let (cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&request(), signal).await });
    let mut call = portal.opened().await;
    call.complete();
    drop(cancel);
    assert_eq!(portal.closed().await, call.path);
    assert!(matches!(result(selecting).await, Err(PickError::Cancelled)));
}

#[tokio::test]
async fn a_close_failure_retires_the_dedicated_connection() {
    let mut portal = bus::Portal::start().await;
    portal.fail_closes();
    let connection = portal.client.clone();
    let (cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(connection.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&request(), signal).await });
    let mut call = portal.opened().await;
    call.complete();
    cancel.send_replace(true);
    assert_eq!(portal.closed().await, call.path);
    assert!(matches!(result(selecting).await, Err(PickError::Cancelled)));
    assert!(
        connection.is_closed(),
        "a failed Close must not leave the request's owner connection alive"
    );
}

#[tokio::test]
async fn a_retry_renews_only_the_connection_on_the_explicit_private_bus() {
    let mut portal = bus::Portal::start().await;
    portal.fail_closes();
    let (cancel, signal) = watch::channel(false);
    let (_next_cancel, next_signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    chooser.address = Some(Address::try_from(portal.bus.address.as_str()).unwrap());
    let selecting = tokio::spawn(async move {
        assert!(matches!(
            chooser.choose_files(&request(), signal).await,
            Err(PickError::Cancelled)
        ));
        let mut next = request();
        next.id = PickId(2);
        chooser.choose_files(&next, next_signal).await
    });
    let mut first = portal.opened().await;
    first.complete();
    cancel.send_replace(true);
    assert_eq!(portal.closed().await, first.path);
    let mut next = portal.opened().await;
    assert_ne!(
        next.client, first.client,
        "the retired owner connection must not be reused"
    );
    next.complete();
    portal.respond(&next, 1, &[]).await;
    assert!(matches!(result(selecting).await, Err(PickError::Cancelled)));
}

#[tokio::test]
async fn dropping_a_selection_keeps_the_next_dialog_behind_the_previous_requests_cleanup() {
    let mut portal = bus::Portal::start().await;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let first_request = request();
    let mut first = Box::pin(chooser.choose_files(&first_request, signal.clone()));
    let mut call = tokio::select! {
        call = portal.opened() => call,
        result = &mut first => panic!("the first selection ended before OpenFile: {result:?}"),
    };
    call.complete();
    std::future::poll_fn(|context| match first.as_mut().poll(context) {
        std::task::Poll::Pending => std::task::Poll::Ready(()),
        std::task::Poll::Ready(result) => panic!("no response was sent: {result:?}"),
    })
    .await;
    drop(first);
    let mut second_request = request();
    second_request.id = PickId(2);
    let mut second = Box::pin(chooser.choose_files(&second_request, signal));
    let closed = tokio::select! {
        biased;
        closed = portal.closed_before_next_open() => closed,
        result = &mut second => panic!("the next selection bypassed request cleanup: {result:?}"),
    };
    assert_eq!(closed, call.path);
}

#[test]
fn dropping_a_selection_outside_the_runtime_still_closes_its_owned_request() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let (mut portal, mut chooser) = runtime.block_on(async {
        let portal = bus::Portal::start().await;
        let chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
        (portal, chooser)
    });
    let options = request();
    let (_cancel, signal) = watch::channel(false);
    let mut selection = Box::pin(chooser.choose_files(&options, signal));
    let mut call = runtime.block_on(async {
        tokio::select! {
            call = portal.opened() => call,
            result = &mut selection => panic!("the selection ended before OpenFile: {result:?}"),
        }
    });
    call.complete();
    runtime.block_on(std::future::poll_fn(|context| {
        match selection.as_mut().poll(context) {
            std::task::Poll::Pending => std::task::Poll::Ready(()),
            std::task::Poll::Ready(result) => panic!("no portal response was sent: {result:?}"),
        }
    }));
    assert!(tokio::runtime::Handle::try_current().is_err());
    drop(selection);
    runtime.block_on(async {
        assert_eq!(portal.closed().await, call.path);
    });
}

#[tokio::test]
async fn an_open_file_rpc_that_never_returns_is_closed_and_cannot_be_reused() {
    let mut portal = bus::Portal::start().await;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move {
        let first = chooser.choose_files(&request(), signal.clone()).await;
        let next = chooser.choose_files(&request(), signal).await;
        (first, next)
    });
    let call = portal.opened().await;
    assert_eq!(portal.closed().await, call.path);
    let (first, next) = result(selecting).await;
    assert!(matches!(
        first,
        Err(PickError::TimedOut | PickError::Unavailable)
    ));
    assert!(matches!(next, Err(PickError::Unavailable)));
    portal.no_open();
}

#[tokio::test]
async fn disconnecting_the_private_bus_terminates_a_pending_selection() {
    let mut portal = bus::Portal::start().await;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&request(), signal).await });
    let mut call = portal.opened().await;
    call.complete();
    portal.bus.stop();
    assert!(matches!(
        result(selecting).await,
        Err(PickError::Unavailable)
    ));
}

#[test]
fn response_status_shape_and_size_are_checked_before_opening_a_file() {
    fn response(status: u32, uris: Vec<&str>) -> Message {
        Message::signal(DESKTOP, REQUEST, "Response")
            .unwrap()
            .build(&(status, HashMap::from([("uris", Value::from(uris))])))
            .unwrap()
    }
    assert_eq!(
        response_uris(&response(0, vec!["file:///tmp/fixture"]), 1),
        Ok(vec!["file:///tmp/fixture".into()])
    );
    assert_eq!(
        response_uris(&response(0, Vec::new()), 1),
        Err(PickError::InvalidResponse)
    );
    assert_eq!(
        response_uris(&response(0, vec!["file:///tmp/a", "file:///tmp/b"]), 1),
        Err(PickError::TooMany)
    );
    assert_eq!(
        response_uris(&response(0, vec!["file:///tmp/a", "file:///tmp/b"]), 2),
        Ok(vec!["file:///tmp/a".into(), "file:///tmp/b".into()])
    );
    assert_eq!(
        response_uris(&response(1, Vec::new()), 1),
        Err(PickError::Cancelled)
    );
    assert_eq!(
        response_uris(&response(2, Vec::new()), 1),
        Err(PickError::Failed)
    );
    assert_eq!(
        response_uris(&response(3, Vec::new()), 1),
        Err(PickError::InvalidResponse)
    );
    assert_eq!(
        response_uris(&response(0, vec![&"x".repeat(MAX_PATH_BYTES + 1)]), 1),
        Err(PickError::TooLarge)
    );
    assert_eq!(
        response_uris(&response(0, vec![&"x".repeat(MAX_RESPONSE_BYTES + 1)]), 1),
        Err(PickError::TooLarge)
    );
    let invalid = Message::signal(DESKTOP, REQUEST, "Response")
        .unwrap()
        .build(&"wrong signature")
        .unwrap();
    assert_eq!(response_uris(&invalid, 1), Err(PickError::InvalidResponse));
}
