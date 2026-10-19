use super::*;

#[tokio::test]
async fn multiple_early_uris_preserve_selection_order_after_the_handle_is_validated() {
    let mut portal = bus::Portal::start().await;
    let first = tempfile::NamedTempFile::new().unwrap();
    let second = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(first.path(), b"first").unwrap();
    std::fs::write(second.path(), b"second").unwrap();
    let uris = [first.path(), second.path()]
        .map(|path| url::Url::from_file_path(path).unwrap().to_string());
    let mut options = request();
    options.max_files = 2;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&options, signal).await });
    let mut call = portal.opened().await;
    assert!(bool::try_from(call.options.remove("multiple").unwrap()).unwrap());
    portal.respond(&call, 0, &[&uris[0], &uris[1]]).await;
    assert!(
        !selecting.is_finished(),
        "an early batch cannot bypass handle validation"
    );
    call.complete();
    let files = result(selecting).await.unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        files
            .iter()
            .map(|file| file.info().size_bytes)
            .collect::<Vec<_>>(),
        [5, 6]
    );
    for (file, expected) in files
        .into_iter()
        .zip([b"first".as_slice(), b"second".as_slice()])
    {
        use std::io::{Read, Seek};
        let (mut descriptor, _, _) = file.into_parts();
        assert_eq!(descriptor.stream_position().unwrap(), 0);
        let mut bytes = Vec::new();
        descriptor.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes, expected);
    }
}

#[tokio::test]
async fn invalid_or_duplicate_uri_later_in_the_batch_cannot_open_the_first_path() {
    let mut portal = bus::Portal::start().await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("alias");
    std::fs::create_dir(&path).unwrap();
    let uri = url::Url::from_file_path(path).unwrap().to_string();
    let alias = uri.strip_suffix("alias").unwrap().to_owned() + "%61lias";
    for uris in [
        vec![uri.clone(), "file://remote.invalid/file".into()],
        vec![uri.clone(), alias],
    ] {
        let mut options = request();
        options.max_files = 2;
        let (_cancel, signal) = watch::channel(false);
        let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
        let selecting = tokio::spawn(async move { chooser.choose_files(&options, signal).await });
        let mut call = portal.opened().await;
        call.complete();
        portal
            .respond(
                &call,
                0,
                &uris.iter().map(String::as_str).collect::<Vec<_>>(),
            )
            .await;
        assert!(
            matches!(result(selecting).await, Err(PickError::InvalidResponse)),
            "opening the first directory before validating the rest would return NotRegularFile"
        );
    }
}

#[tokio::test]
async fn a_portal_cannot_exceed_the_requested_count_or_return_a_partial_batch() {
    let mut portal = bus::Portal::start().await;
    let mut options = request();
    options.max_files = 2;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&options, signal).await });
    let mut call = portal.opened().await;
    call.complete();
    portal
        .respond(
            &call,
            0,
            &[
                "file:///missing-a",
                "file:///missing-b",
                "file:///missing-c",
            ],
        )
        .await;
    assert!(matches!(result(selecting).await, Err(PickError::TooMany)));
}

#[tokio::test]
async fn cancelling_a_multiple_dialog_closes_only_its_own_request() {
    let mut portal = bus::Portal::start().await;
    let mut options = request();
    options.max_files = 8;
    let (cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_files(&options, signal).await });
    let mut call = portal.opened().await;
    assert!(bool::try_from(call.options.remove("multiple").unwrap()).unwrap());
    call.complete();
    cancel.send_replace(true);
    assert_eq!(portal.closed().await, call.path);
    assert!(matches!(result(selecting).await, Err(PickError::Cancelled)));
}
