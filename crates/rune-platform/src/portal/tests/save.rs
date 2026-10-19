use super::*;
use crate::file_dialogs::{ParentWindow, SaveRequest};

fn save_request() -> SaveRequest {
    SaveRequest {
        id: PickId(801),
        parent: None,
        suggested_name: " sugerido ü.txt ".into(),
        current_folder: None,
    }
}

#[tokio::test]
async fn save_file_passes_save_options_and_does_not_create_a_new_destination_after_an_early_response(
) {
    let mut portal = bus::Portal::start().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(" elegido ü.txt ");
    let uri = url::Url::from_file_path(&path).unwrap().to_string();
    let mut options = save_request();
    options.current_folder = Some(directory.path().to_owned());
    options.parent = Some(ParentWindow::x11(std::num::NonZeroU64::new(0x123).unwrap()));
    let expected_folder = [directory.path().as_os_str().as_encoded_bytes(), &[0]].concat();
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let mut selecting = tokio::spawn(async move { chooser.save_file(&options, signal).await });
    let mut call = tokio::select! {
        call = portal.opened() => call,
        result = &mut selecting => panic!("save ended before SaveFile: {result:?}"),
    };
    assert_eq!(call.method, "SaveFile");
    assert_eq!(call.parent, "x11:123");
    assert_eq!(
        String::try_from(call.options.remove("current_name").unwrap()).unwrap(),
        " sugerido ü.txt "
    );
    assert_eq!(
        Vec::<u8>::try_from(call.options.remove("current_folder").unwrap()).unwrap(),
        expected_folder
    );
    assert!(bool::try_from(call.options.remove("modal").unwrap()).unwrap());
    assert!(!call.options.contains_key("multiple"));
    assert!(!call.options.contains_key("current_file"));
    portal.respond(&call, 0, &[&uri]).await;
    assert!(
        !selecting.is_finished(),
        "an early destination still needs handle validation"
    );
    call.complete();
    let destination = result(selecting).await.unwrap();
    let debug = format!("{destination:?}");
    assert!(!debug.contains(&path.to_string_lossy().to_string()));
    assert!(!debug.contains("elegido"));
    assert_eq!(destination.into_path(), path);
    assert!(!path.exists(), "choosing a destination must not create it");
}

#[tokio::test]
async fn save_file_does_not_truncate_or_resolve_an_existing_destination_symlink() {
    let mut portal = bus::Portal::start().await;
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original");
    std::fs::write(&original, b"keep this existing content").unwrap();
    let link = directory.path().join("chosen-link");
    std::os::unix::fs::symlink(&original, &link).unwrap();
    let uri = url::Url::from_file_path(&link).unwrap().to_string();
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.save_file(&save_request(), signal).await });
    let mut call = portal.opened().await;
    assert_eq!(call.method, "SaveFile");
    call.complete();
    portal.respond(&call, 0, &[&uri]).await;
    assert_eq!(result(selecting).await.unwrap().into_path(), link);
    assert_eq!(
        std::fs::read(&original).unwrap(),
        b"keep this existing content"
    );
    assert!(link.symlink_metadata().unwrap().is_symlink());
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[tokio::test]
async fn save_file_requires_exactly_one_strict_local_uri() {
    let mut portal = bus::Portal::start().await;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    for (index, (uris, expected)) in [
        (vec![], PickError::InvalidResponse),
        (
            vec!["file:///not-opened/a", "file:///not-opened/b"],
            PickError::TooMany,
        ),
        (
            vec!["https://example.invalid/file"],
            PickError::InvalidResponse,
        ),
        (
            vec!["file://example.invalid/file"],
            PickError::InvalidResponse,
        ),
        (
            vec!["file:///tmp/link%2F..%2Ffile"],
            PickError::InvalidResponse,
        ),
        (vec!["file:///tmp/../file"], PickError::InvalidResponse),
        (vec!["file:///tmp/file?other"], PickError::InvalidResponse),
        (vec!["file:///tmp/%00file"], PickError::InvalidResponse),
        (vec!["file:///"], PickError::InvalidResponse),
        (vec!["file:///tmp/"], PickError::InvalidResponse),
    ]
    .into_iter()
    .enumerate()
    {
        let mut options = save_request();
        options.id = PickId(810 + index as u64);
        let mut selecting = Box::pin(chooser.save_file(&options, signal.clone()));
        let mut call = tokio::select! {
            call = portal.opened() => call,
            result = &mut selecting => panic!("save ended before SaveFile: {result:?}"),
        };
        assert_eq!(call.method, "SaveFile");
        call.complete();
        portal.respond(&call, 0, &uris).await;
        assert_eq!(selecting.await.unwrap_err(), expected);
    }
}

#[tokio::test]
async fn save_file_never_publishes_an_early_destination_from_a_mismatched_handle() {
    let mut portal = bus::Portal::start().await;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.save_file(&save_request(), signal).await });
    let mut call = portal.opened().await;
    portal
        .respond(&call, 0, &["file:///not-created/destination"])
        .await;
    call.complete_as(OwnedObjectPath::try_from("/unrelated_save_request").unwrap());
    assert_eq!(portal.closed().await, call.path);
    assert!(matches!(
        result(selecting).await,
        Err(PickError::InvalidResponse)
    ));
}

#[tokio::test]
async fn save_file_ignores_responses_from_a_foreign_sender_or_path() {
    let mut portal = bus::Portal::start().await;
    let stranger = portal.bus.connect().await;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.save_file(&save_request(), signal).await });
    let mut call = portal.opened().await;
    let body = HashMap::from([("uris", Value::from(vec!["file:///not-created/forged"]))]);
    bus::Portal::response_from(&stranger, &call, 0, body).await;
    portal
        .service
        .emit_signal(
            Some(call.client.as_str()),
            format!("{}_other", call.path).as_str(),
            REQUEST,
            "Response",
            &(
                0u32,
                HashMap::from([("uris", Value::from(vec!["file:///not-created/other"]))]),
            ),
        )
        .await
        .unwrap();
    call.complete();
    portal.respond(&call, 1, &[]).await;
    assert!(matches!(result(selecting).await, Err(PickError::Cancelled)));
}

#[tokio::test]
async fn save_file_cancellation_and_a_dropped_owner_close_only_the_owned_request() {
    for drop_owner in [false, true] {
        let mut portal = bus::Portal::start().await;
        let (cancel, signal) = watch::channel(false);
        let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
        let selecting =
            tokio::spawn(async move { chooser.save_file(&save_request(), signal).await });
        let mut call = portal.opened().await;
        call.complete();
        if drop_owner {
            drop(cancel);
        } else {
            cancel.send_replace(true);
        }
        assert_eq!(portal.closed().await, call.path);
        assert!(matches!(result(selecting).await, Err(PickError::Cancelled)));
    }
}

#[tokio::test]
async fn save_file_invalid_or_cancelled_requests_never_call_the_portal() {
    let mut portal = bus::Portal::start().await;
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let mut invalid = save_request();
    invalid.suggested_name = "../unsafe".into();
    let (_cancel, signal) = watch::channel(false);
    assert!(matches!(
        chooser.save_file(&invalid, signal).await,
        Err(PickError::InvalidRequest)
    ));
    let (_cancel, signal) = watch::channel(true);
    assert!(matches!(
        chooser.save_file(&save_request(), signal).await,
        Err(PickError::Cancelled)
    ));
    portal.no_open();
}

#[tokio::test]
async fn dropping_save_file_retires_and_drains_the_same_chooser_before_another_dialog() {
    let mut portal = bus::Portal::start().await;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    chooser.address = Some(Address::try_from(portal.bus.address.as_str()).unwrap());
    let save_options = save_request();
    let mut saving = Box::pin(chooser.save_file(&save_options, signal.clone()));
    let mut first = tokio::select! {
        call = portal.opened() => call,
        result = &mut saving => panic!("save ended before SaveFile: {result:?}"),
    };
    first.complete();
    std::future::poll_fn(|cx| match saving.as_mut().poll(cx) {
        Poll::Pending => Poll::Ready(()),
        Poll::Ready(result) => panic!("no response was sent: {result:?}"),
    })
    .await;
    drop(saving);
    assert!(chooser.cleanup.retired.load(Ordering::Acquire));
    let open_options = request();
    let mut opening = Box::pin(chooser.choose_files(&open_options, signal));
    let closed = tokio::select! {
        biased;
        closed = portal.closed_before_next_open() => closed,
        result = &mut opening => panic!("OpenFile bypassed SaveFile cleanup: {result:?}"),
    };
    assert_eq!(closed, first.path);
    let mut next = tokio::select! {
        call = portal.opened() => call,
        result = &mut opening => panic!("open failed after draining save: {result:?}"),
    };
    assert_eq!(next.method, "OpenFile");
    assert_ne!(
        next.client, first.client,
        "retired connection must be renewed, keeping the chooser"
    );
    next.complete();
    portal.respond(&next, 1, &[]).await;
    assert!(matches!(opening.await, Err(PickError::Cancelled)));
}
