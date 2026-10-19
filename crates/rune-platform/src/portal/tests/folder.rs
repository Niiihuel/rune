//! Choosing a folder (escritorio-linux): OpenFile with `directory`, one
//! strict local URI back, and nothing opened or listed.

use super::*;
use crate::file_dialogs::{FolderRequest, ParentWindow};

#[tokio::test]
async fn a_folder_is_asked_for_with_directory_and_comes_back_unopened() {
    let mut portal = bus::Portal::start().await;
    let directory = tempfile::tempdir().unwrap();
    // Never made: a chooser that opened or listed it would fail.
    let chosen = directory.path().join("sonidos ü");
    let uri = url::Url::from_file_path(&chosen).unwrap().to_string();
    let request = FolderRequest {
        id: PickId(901),
        parent: ParentWindow::wayland("exported-handle"),
        current_folder: Some(directory.path().to_owned()),
    };
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    let selecting = tokio::spawn(async move { chooser.choose_folder(&request, signal).await });
    let mut call = portal.opened().await;
    assert_eq!(call.method, "OpenFile");
    assert_eq!(call.parent, "wayland:exported-handle");
    assert!(bool::try_from(call.options.remove("directory").unwrap()).unwrap());
    assert!(!bool::try_from(call.options.remove("multiple").unwrap()).unwrap());
    assert!(call.options.contains_key("current_folder"));
    call.complete();
    portal.respond(&call, 0, &[&uri]).await;
    assert_eq!(result(selecting).await.unwrap(), chosen);
    assert!(!chosen.exists());
}

#[tokio::test]
async fn a_folder_choice_is_cancelled_or_refused_like_a_file() {
    let mut portal = bus::Portal::start().await;
    let (_cancel, signal) = watch::channel(false);
    let mut chooser = FileChooser::with_connection(portal.client.clone()).unwrap();
    for (index, (status, uris, expected)) in [
        (1, vec![], PickError::Cancelled),
        (0, vec!["file:///a", "file:///b"], PickError::TooMany),
        (
            0,
            vec!["https://example.invalid/sounds"],
            PickError::InvalidResponse,
        ),
        (0, vec!["file:///tmp/../etc"], PickError::InvalidResponse),
    ]
    .into_iter()
    .enumerate()
    {
        let request = FolderRequest {
            id: PickId(910 + index as u64),
            parent: None,
            current_folder: None,
        };
        let mut selecting = Box::pin(chooser.choose_folder(&request, signal.clone()));
        let mut call = tokio::select! {
            call = portal.opened() => call,
            result = &mut selecting => panic!("ended before OpenFile: {result:?}"),
        };
        call.complete();
        portal.respond(&call, status, &uris).await;
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(5), selecting)
                .await
                .unwrap(),
            Err(expected),
            "case {index}"
        );
    }
    let invalid = FolderRequest {
        id: PickId(0),
        parent: None,
        current_folder: None,
    };
    assert_eq!(
        chooser.choose_folder(&invalid, signal).await,
        Err(PickError::InvalidRequest)
    );
}

#[test]
fn only_a_printable_exported_handle_names_a_wayland_parent() {
    assert!(ParentWindow::wayland("").is_none());
    assert!(ParentWindow::wayland("with space").is_none());
    assert!(ParentWindow::wayland("ü").is_none());
    assert!(ParentWindow::wayland(&"a".repeat(257)).is_none());
    assert_eq!(
        ParentWindow::wayland("a1-B2").unwrap().as_str(),
        "wayland:a1-B2"
    );
}
