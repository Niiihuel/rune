use super::*;
use tokio::{io::AsyncReadExt, net::TcpListener};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "rune-download-test-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn names(&self) -> Vec<String> {
        std::fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect()
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

async fn request(socket: &mut tokio::net::TcpStream) {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        assert!(bytes.len() < 8192);
        bytes.push(socket.read_u8().await.unwrap());
    }
    let headers = String::from_utf8(bytes).unwrap().to_ascii_lowercase();
    assert!(headers.starts_with("get /file "));
    assert!(!headers.contains("authorization:") && !headers.contains("cookie:"));
}

async fn serve(response: Vec<u8>) -> (url::Url, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = url::Url::parse(&format!("http://{}/file", listener.local_addr().unwrap())).unwrap();
    let task = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        socket.write_all(&response).await.unwrap();
    });
    (url, task)
}

fn client() -> reqwest::Client {
    Downloads::new(Handle::current(), &[], Arc::new(|| {}))
        .client
        .take()
        .unwrap()
}

#[test]
fn filenames_cannot_escape_the_directory_and_keep_valid_utf8() {
    for name in [
        "../../file.txt",
        "C:\\Users\\other\\file.txt",
        "/tmp/a",
        "\0\n",
        "...",
        "  ",
    ] {
        let safe = safe_filename(name);
        assert_eq!(Path::new(&safe).components().count(), 1);
        assert!(!safe.contains(['/', '\\']) && !safe.chars().any(char::is_control));
        assert!(!safe.is_empty());
    }
    assert_eq!(safe_filename("..."), "archivo");
    assert_eq!(safe_filename("foto de José.png"), "foto de José.png");
    assert!(safe_filename(&"😀".repeat(100)).len() <= 180);
}

#[tokio::test]
async fn downloads_are_complete_private_and_never_overwrite_existing_files() {
    let directory = Directory::new();
    std::fs::write(directory.0.join("sample.txt"), "existing").unwrap();
    let (url, server) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello".to_vec()).await;
    let path = download_to(&client(), url, &directory.0, "sample.txt", 5)
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(path.file_name().unwrap(), "sample (1).txt");
    assert_eq!(std::fs::read(&path).unwrap(), b"hello");
    assert_eq!(
        std::fs::read(directory.0.join("sample.txt")).unwrap(),
        b"existing"
    );
    assert_eq!(
        directory.names().len(),
        2,
        "no partial file survives publication"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

#[cfg(unix)]
#[tokio::test]
async fn an_existing_symlink_is_not_followed_or_replaced() {
    let directory = Directory::new();
    let original = directory.0.join("original");
    std::fs::write(&original, "keep").unwrap();
    std::os::unix::fs::symlink(&original, directory.0.join("sample.txt")).unwrap();
    let (url, server) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello".to_vec()).await;
    let path = download_to(&client(), url, &directory.0, "sample.txt", 0)
        .await
        .unwrap();
    server.await.unwrap();
    assert_eq!(path.file_name().unwrap(), "sample (1).txt");
    assert_eq!(std::fs::read(original).unwrap(), b"keep");
    assert!(std::fs::symlink_metadata(directory.0.join("sample.txt"))
        .unwrap()
        .is_symlink());
}

#[tokio::test]
async fn incomplete_oversized_and_partial_responses_never_publish_a_file() {
    let directory = Directory::new();
    for (response, expected) in [
        (
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                MAX_DOWNLOAD_BYTES + 1
            )
            .into_bytes(),
            0,
        ),
        (
            b"HTTP/1.1 200 OK\r\nContent-Length: 8\r\n\r\nshort".to_vec(),
            8,
        ),
        (
            b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello".to_vec(),
            6,
        ),
        (
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n"
                .to_vec(),
            4,
        ),
        (
            b"HTTP/1.1 206 Partial Content\r\nContent-Length: 5\r\n\r\nhello".to_vec(),
            0,
        ),
    ] {
        let (url, server) = serve(response).await;
        assert!(
            download_to(&client(), url, &directory.0, "sample.txt", expected)
                .await
                .is_err()
        );
        server.await.unwrap();
        assert!(
            directory.names().is_empty(),
            "errors remove incomplete files"
        );
    }
}

#[tokio::test]
async fn redirects_cannot_escape_the_validated_origin() {
    let directory = Directory::new();
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (url, server) = serve(
        format!(
            "HTTP/1.1 302 Found\r\nLocation: http://{}/trap\r\nContent-Length: 0\r\n\r\n",
            trap.local_addr().unwrap()
        )
        .into_bytes(),
    )
    .await;
    assert!(download_to(&client(), url, &directory.0, "sample.txt", 0)
        .await
        .is_err());
    server.await.unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(50), trap.accept())
            .await
            .is_err()
    );
    assert!(directory.names().is_empty());
}

#[tokio::test]
async fn cancellation_removes_an_incomplete_stream() {
    let directory = Directory::new();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = url::Url::parse(&format!("http://{}/file", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10000\r\n\r\nfirst")
            .await
            .unwrap();
        let mut byte = [0];
        let _ = socket.read(&mut byte).await;
    });
    let path = directory.0.clone();
    let download =
        tokio::spawn(async move { download_to(&client(), url, &path, "sample.txt", 10000).await });
    tokio::time::timeout(Duration::from_secs(2), async {
        while directory.names().is_empty() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    download.abort();
    assert!(download.await.unwrap_err().is_cancelled());
    tokio::time::timeout(Duration::from_secs(2), async {
        while !directory.names().is_empty() {
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    })
    .await
    .unwrap();
    server.abort();
}

#[tokio::test]
async fn admission_is_bounded_and_logout_cancels_without_network() {
    let mut downloads = Downloads::new(
        Handle::current(),
        &["https://files.example.test"],
        Arc::new(|| {}),
    );
    let source = || RemoteFile {
        url: "https://files.example.test/file".into(),
    };
    assert!(downloads
        .start(
            RemoteFile {
                url: "https://other.example.test/file".into()
            },
            "file",
            0
        )
        .is_err());
    assert!(downloads
        .start(source(), "file", MAX_DOWNLOAD_BYTES + 1)
        .is_err());
    downloads.start(source(), "one", 0).unwrap();
    downloads.start(source(), "two", 0).unwrap();
    assert!(downloads.start(source(), "three", 0).is_err());
    // The current-thread runtime hasn't yielded to the spawned tasks.
    downloads.clear();
    assert!(downloads.active.is_empty() && downloads.poll().is_empty());
    tokio::task::yield_now().await;
    assert!(downloads.poll().is_empty());
}

fn chosen(path: Result<PathBuf, UploadError>) -> oneshot::Receiver<Result<PathBuf, UploadError>> {
    let (reply, answer) = oneshot::channel();
    reply.send(path).unwrap();
    answer
}

async fn save(
    url: url::Url,
    path: Result<PathBuf, UploadError>,
    size: u64,
) -> Result<String, Option<&'static str>> {
    save_answering(url, path, size, false).await.0
}

/// A save whose person says `replace` if asked about a file at the name;
/// and whether it was asked.
async fn save_answering(
    url: url::Url,
    path: Result<PathBuf, UploadError>,
    size: u64,
    replace: bool,
) -> (Result<String, Option<&'static str>>, bool) {
    let (saving, received) = (AtomicBool::new(false), AtomicU64::new(0));
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(|| {});
    let exists = std::sync::OnceLock::new();
    let (word, replaced) = oneshot::channel();
    if replace {
        word.send(()).unwrap();
    } else {
        drop(word);
    }
    let result = save_to(
        &client(),
        url,
        size,
        chosen(path),
        &saving,
        &received,
        &wake,
        Ask {
            exists: &exists,
            replaced,
        },
    )
    .await;
    (result, exists.get().is_some())
}

#[tokio::test]
async fn save_as_publishes_exactly_the_chosen_name_and_never_over_what_is_there() {
    let directory = Directory::new();
    let target = directory.0.join(" elegido ü.txt");
    let (url, server) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello".to_vec()).await;
    assert_eq!(
        save(url, Ok(target.clone()), 5).await,
        Ok(" elegido ü.txt".into())
    );
    server.await.unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    assert_eq!(
        directory.names(),
        [" elegido ü.txt"],
        "no partial or suffixed copy"
    );
    // The same name again: asked before any network, never a suffix; the
    // person not saying to replace it writes nothing (08.3).
    let unused = url::Url::parse("http://127.0.0.1:9/file").unwrap();
    assert_eq!(
        save_answering(unused.clone(), Ok(target.clone()), 5, false).await,
        (Err(None), true)
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"hello");
    // A link already at the name stays a link to what it was (a Unix
    // link: windows-base).
    #[cfg(unix)]
    {
        let original = directory.0.join("original");
        std::fs::write(&original, b"keep").unwrap();
        let link = directory.0.join("link");
        std::os::unix::fs::symlink(&original, &link).unwrap();
        assert_eq!(
            save(unused, Ok(link.clone()), 5).await,
            Err(Some(SAVE_EXISTS))
        );
        assert_eq!(std::fs::read(&link).unwrap(), b"keep");
    }
    #[cfg(not(unix))]
    drop(unused);
}

// Linux desktop (escritorio-linux): replacing with the person's word (08.3).
#[tokio::test]
async fn a_file_at_the_name_is_replaced_whole_only_when_the_person_says_so() {
    let directory = Directory::new();
    let target = directory.0.join("viejo.txt");
    std::fs::write(&target, b"old content").unwrap();
    let (url, server) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nnew".to_vec()).await;
    assert_eq!(
        save_answering(url, Ok(target.clone()), 3, true).await,
        (Ok("viejo.txt".into()), true)
    );
    server.await.unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    assert_eq!(directory.names(), ["viejo.txt"], "no partial file stays");
    // A download that fails leaves the file as it was.
    let (url, server) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nno".to_vec()).await;
    assert!(matches!(
        save_answering(url, Ok(target.clone()), 3, true).await,
        (Err(Some(_)), true)
    ));
    server.await.unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    assert_eq!(directory.names(), ["viejo.txt"]);
    // A folder at the name is not asked about, nor replaced.
    let folder = directory.0.join("carpeta");
    std::fs::create_dir(&folder).unwrap();
    let unused = url::Url::parse("http://127.0.0.1:9/file").unwrap();
    assert_eq!(
        save_answering(unused, Ok(folder.clone()), 3, true).await,
        (Err(Some(SAVE_EXISTS)), false)
    );
    assert!(folder.is_dir());
}

#[tokio::test]
async fn the_interface_is_asked_once_and_the_download_waits_for_its_word() {
    let directory = Directory::new();
    let target = directory.0.join("existe.txt");
    std::fs::write(&target, b"old").unwrap();
    let mut downloads = Downloads::new(Handle::current(), &[], Arc::new(|| {}));
    let (url, server) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nnew".to_vec()).await;
    let (cancel, _dialog) = watch::channel(false);
    downloads
        .save(9, url, 3, chosen(Ok(target.clone())), cancel)
        .unwrap();
    let mut polled = Vec::new();
    for _ in 0..500 {
        polled = downloads.poll_saves();
        if !polled.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        polled,
        [SaveUpdate::Exists {
            request: 9,
            filename: "existe.txt".into()
        }]
    );
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        downloads.poll_saves().is_empty(),
        "asked once, nothing downloads"
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"old");
    assert!(downloads.replace_save(9));
    assert!(!downloads.replace_save(9), "once");
    let mut saved = None;
    for _ in 0..500 {
        saved = downloads
            .poll_saves()
            .into_iter()
            .find(|update| matches!(update, SaveUpdate::Saved { .. }));
        if saved.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    server.await.unwrap();
    assert_eq!(
        saved,
        Some(SaveUpdate::Saved {
            request: 9,
            filename: "existe.txt".into()
        })
    );
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    assert!(!downloads.replace_save(9), "nothing waits any more");
}

#[tokio::test]
async fn another_file_put_at_the_name_after_asking_is_kept() {
    let directory = Directory::new();
    let target = directory.0.join("cambiado.txt");
    std::fs::write(&target, b"asked about").unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = url::Url::parse(&format!("http://{}/file", listener.local_addr().unwrap())).unwrap();
    let racer = target.clone();
    let swap = directory.0.join("swap");
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        // Someone else puts another file at the name after the question.
        std::fs::write(&swap, b"theirs").unwrap();
        std::fs::rename(&swap, &racer).unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nmine")
            .await
            .unwrap();
    });
    assert_eq!(
        save_answering(url, Ok(target.clone()), 4, true).await,
        (
            Err(Some(
                "El archivo cambió mientras se descargaba. Volvé a guardarlo."
            )),
            true
        )
    );
    server.await.unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"theirs");
    assert_eq!(directory.names(), ["cambiado.txt"], "no partial file stays");
}

#[tokio::test]
async fn a_name_created_while_downloading_is_kept_and_the_partial_removed() {
    let directory = Directory::new();
    let target = directory.0.join("carrera.txt");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = url::Url::parse(&format!("http://{}/file", listener.local_addr().unwrap())).unwrap();
    let racer = target.clone();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        request(&mut socket).await;
        // Someone else takes the name after the check, before the commit.
        std::fs::write(&racer, b"theirs").unwrap();
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nmine")
            .await
            .unwrap();
    });
    assert_eq!(
        save(url, Ok(target.clone()), 4).await,
        Err(Some(SAVE_EXISTS))
    );
    server.await.unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"theirs");
    assert_eq!(directory.names(), ["carrera.txt"]);
}

#[tokio::test]
async fn a_closed_dialog_or_a_short_body_writes_nothing() {
    let directory = Directory::new();
    let unused = url::Url::parse("http://127.0.0.1:9/file").unwrap();
    assert_eq!(
        save(unused, Err(UploadError::Cancelled), 5).await,
        Err(None)
    );
    let (url, server) = serve(b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nhel".to_vec()).await;
    let target = directory.0.join("corto.txt");
    assert!(matches!(save(url, Ok(target), 5).await, Err(Some(_))));
    server.await.unwrap();
    assert!(directory.names().is_empty(), "no partial file stays");
}

#[tokio::test]
async fn save_as_shares_the_two_slots_and_cancelling_closes_its_dialog() {
    let mut downloads = Downloads::new(
        Handle::current(),
        &["https://files.example.test"],
        Arc::new(|| {}),
    );
    let source = RemoteFile {
        url: "https://files.example.test/file".into(),
    };
    let url = downloads.admit(&source, 0).unwrap();
    let (reply, answer) = oneshot::channel();
    let (cancel, dialog) = watch::channel(false);
    downloads.save(7, url, 0, answer, cancel).unwrap();
    downloads.start(source.clone(), "one", 0).unwrap();
    assert!(downloads.admit(&source, 0).is_err(), "two in all");
    assert!(downloads.cancel_save(7));
    assert!(*dialog.borrow(), "the dialog is told to close");
    tokio::task::yield_now().await;
    assert!(
        reply.is_closed(),
        "nothing waits for a destination any more"
    );
    assert!(!downloads.cancel_save(7), "once");
    assert!(downloads.admit(&source, 0).is_ok());
}

/// Message content (contenido-de-mensajes): what a copy hands the
/// clipboard, without touching this session's clipboard.
#[test]
fn a_copied_picture_is_its_pixels_within_bounds_and_a_file_replaces_the_last_one() {
    let directory = Directory::new();
    let picture = directory.0.join("a.png");
    image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255]))
        .save(&picture)
        .unwrap();
    let (width, height, rgba) = copy::pixels(&picture).unwrap();
    assert_eq!((width, height, rgba.len()), (3, 2, 24));
    assert_eq!(&rgba[..4], &[1, 2, 3, 255]);
    let wide = directory.0.join("wide.png");
    image::RgbaImage::new(4097, 1).save(&wide).unwrap();
    assert!(copy::pixels(&wide).is_err(), "past the bound");
    let text = directory.0.join("b.png");
    std::fs::write(&text, "not a picture").unwrap();
    assert!(copy::pixels(&text).is_err());

    // The file copied before goes; one still arriving stays.
    std::fs::remove_file(&picture).unwrap();
    std::fs::remove_file(&wide).unwrap();
    std::fs::remove_file(&text).unwrap();
    std::fs::write(directory.0.join("old.mp4"), "old").unwrap();
    // A copied file may be named like a partial one.
    std::fs::write(directory.0.join("older.part"), "older").unwrap();
    std::fs::write(directory.0.join(".rune-1-2.part"), "arriving").unwrap();
    let partial = directory.0.join(".rune-1-3.part");
    std::fs::write(&partial, "new").unwrap();
    let kept = copy::keep(&directory.0, &partial, "clip.mp4").unwrap();
    assert_eq!(kept, directory.0.join("clip.mp4"));
    assert_eq!(std::fs::read_to_string(&kept).unwrap(), "new");
    let mut names = directory.names();
    names.sort();
    assert_eq!(names, [".rune-1-2.part", ".rune-1-3.part", "clip.mp4"]);
}
