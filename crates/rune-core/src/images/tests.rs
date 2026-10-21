use super::*;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        width,
        height,
        image::Rgba([20, 80, 160, 255]),
    ))
    .write_to(&mut output, ImageFormat::Png)
    .unwrap();
    output.into_inner()
}

fn gif(colors: &[[u8; 4]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut bytes);
        for color in colors {
            encoder
                .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                    16,
                    16,
                    image::Rgba(*color),
                )))
                .unwrap();
        }
    }
    bytes
}

#[test]
fn gif_previews_decode_only_the_first_frame_with_the_existing_output_budgets() {
    let colors = [[20, 80, 160, 255], [240, 40, 20, 255]];
    let still = gif(&colors[..1]);
    let animated = gif(&colors);
    for kind in [
        ImageKind::Artwork,
        ImageKind::AttachmentPreview,
        ImageKind::Banner,
    ] {
        let first = super::normalize(&still, kind).unwrap();
        let multi = super::normalize(&animated, kind).unwrap();
        assert_eq!(first.sha256, multi.sha256);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&*multi.base64)
            .unwrap();
        let pixels = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(
            pixels.get_pixel(pixels.width() / 2, pixels.height() / 2).0,
            colors[0]
        );
        let expected = if kind == ImageKind::Artwork { 128 } else { 16 };
        assert_eq!((multi.width, multi.height), (expected, expected));
        // Corrupt data after the completed first frame cannot be traversed.
        // Animation decoding would reject this tail; a static preview must not.
        let mut invalid_tail = still[..still.len() - 1].to_vec();
        invalid_tail.extend_from_slice(b"not another frame");
        assert_eq!(
            super::normalize(&invalid_tail, kind).unwrap().sha256,
            first.sha256
        );
    }
    let many_frames = gif(&vec![colors[0]; 1000]);
    assert_eq!(
        normalize(&many_frames).unwrap().sha256,
        normalize(&still).unwrap().sha256
    );
}

#[test]
fn gif_rejects_oversized_canvas_first_frame_and_truncated_payload() {
    // Minimal 1x1 GIF, with an explicit image descriptor at byte 19. Mutating
    // metadata tests limits before allocating a large canvas or frame buffer.
    let tiny = [
        b'G', b'I', b'F', b'8', b'9', b'a', 1, 0, 1, 0, 0x80, 0, 0, 0, 0, 0, 255, 255, 255, 0x2c,
        0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 0x44, 1, 0, 0x3b,
    ];
    assert!(normalize(&tiny).is_some());
    for kind in [
        ImageKind::Artwork,
        ImageKind::AttachmentPreview,
        ImageKind::Banner,
    ] {
        let mut canvas = tiny;
        canvas[6..10].copy_from_slice(&[255, 255, 255, 255]);
        assert!(super::normalize(&canvas, kind).is_none());
        let mut frame = tiny;
        frame[24..28].copy_from_slice(&[255, 255, 255, 255]);
        assert!(super::normalize(&frame, kind).is_none());
        assert!(super::normalize(&tiny[..31], kind).is_none());
        let mut too_large = tiny.to_vec();
        too_large.resize(byte_limit(kind) + 1, 0);
        assert!(super::normalize(&too_large, kind).is_none());
    }
}

fn source(url: impl Into<String>) -> RemoteImage {
    RemoteImage {
        url: url.into(),
        kind: ImageKind::Artwork,
    }
}

fn normalize(bytes: &[u8]) -> Option<ImageData> {
    super::normalize(bytes, ImageKind::Artwork)
}

fn validated_url(source: &RemoteImage, origins: &[&str]) -> Option<url::Url> {
    validated_resource_url(&source.url, origins, false)
}

#[test]
fn images_are_normalized_to_small_pngs_and_decode_limits_precede_allocation() {
    let data = normalize(&png(240, 320)).unwrap();
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&*data.base64)
        .unwrap();
    assert_eq!(bytes.len(), data.size_bytes as usize);
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), data.sha256);
    let image = image::load_from_memory(&bytes).unwrap();
    assert_eq!((image.width(), image.height()), (128, 128));
    let rgba = image.to_rgba8();
    assert_eq!(
        rgba.get_pixel(0, 64).0[3],
        0,
        "portrait retains transparent side margins"
    );
    assert_eq!(rgba.get_pixel(64, 64).0, [20, 80, 160, 255]);
    assert!(normalize(&png(513, 1)).is_none());
    assert!(normalize(b"invalid image").is_none());
    assert!(normalize(&vec![0; MAX_BYTES + 1]).is_none());
    let mut webp = Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(64, 32)
        .write_to(&mut webp, ImageFormat::WebP)
        .unwrap();
    assert!(normalize(webp.get_ref()).is_some());
    assert!(normalize(b"GIF89a").is_none());
    assert!(normalize(&[0xff, 0xd8, 0xff, 0xe0, 0, 16, b'J', b'F', b'I', b'F', 0]).is_none());
}

#[test]
fn attachment_previews_preserve_aspect_ratio_and_accept_jpeg_with_separate_limits() {
    let small = super::normalize(&png(24, 32), ImageKind::AttachmentPreview).unwrap();
    assert_eq!(
        (small.width, small.height),
        (24, 32),
        "previews do not upscale small images"
    );
    for (width, height, expected) in [(1200, 800, (768, 512)), (800, 1200, (341, 512))] {
        let data = super::normalize(&png(width, height), ImageKind::AttachmentPreview).unwrap();
        assert_eq!((data.width, data.height), expected);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&*data.base64)
            .unwrap();
        let pixels = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(pixels.dimensions(), expected);
        assert_eq!(
            pixels.get_pixel(0, 0).0,
            [20, 80, 160, 255],
            "preview has no padding or crop"
        );
    }
    let mut jpeg = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(1024, 768)
        .write_to(&mut jpeg, ImageFormat::Jpeg)
        .unwrap();
    assert!(super::normalize(jpeg.get_ref(), ImageKind::AttachmentPreview).is_some());
    assert!(
        normalize(jpeg.get_ref()).is_none(),
        "artwork retains its original format budget"
    );
    assert!(super::normalize(&png(4097, 1), ImageKind::AttachmentPreview).is_none());
    assert!(super::normalize(&vec![0; PREVIEW_BYTES + 1], ImageKind::AttachmentPreview).is_none());
    assert!(super::normalize(b"GIF89a", ImageKind::AttachmentPreview).is_none());
    assert!(super::normalize(b"<svg/>", ImageKind::AttachmentPreview).is_none());
}

#[test]
fn profile_banners_keep_their_shape_without_padding_or_upscaling_and_enforce_limits() {
    for (width, height, expected) in [
        (1600, 600, (512, 192)),
        (400, 1200, (85, 256)),
        (80, 30, (80, 30)),
    ] {
        let data = super::normalize(&png(width, height), ImageKind::Banner).unwrap();
        assert_eq!((data.width, data.height), expected);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&*data.base64)
            .unwrap();
        let image = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(0, 0).0, [20, 80, 160, 255]);
        assert_eq!(
            image.get_pixel(image.width() - 1, image.height() - 1).0,
            [20, 80, 160, 255]
        );
    }
    assert!(super::normalize(&png(2049, 1), ImageKind::Banner).is_none());
    assert!(super::normalize(&png(2048, 1025), ImageKind::Banner).is_none());
    assert!(super::normalize(&png(2048, 1024), ImageKind::Banner).is_some());
    assert!(super::normalize(&vec![0; MAX_BYTES + 1], ImageKind::Banner).is_none());
    assert!(super::normalize(b"<svg/>", ImageKind::Banner).is_none());
    let mut jpeg = Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(600, 200)
        .write_to(&mut jpeg, ImageFormat::Jpeg)
        .unwrap();
    assert!(super::normalize(jpeg.get_ref(), ImageKind::Banner).is_some());
}

#[test]
fn only_exact_declared_https_origins_without_credentials_are_allowed() {
    let origins = ["https://cdn.example.test"];
    assert!(validated_url(
        &source("https://cdn.example.test/image.png?size=128"),
        &origins
    )
    .is_some());
    for bad in [
        "http://cdn.example.test/x",
        "https://cdn.example.test.evil.test/x",
        "https://other.example.test/x",
        "https://name:password@cdn.example.test/x",
        "https://cdn.example.test:8443/x",
        "https://cdn.example.test/x#fragment",
        "file:///tmp/image.png",
        "data:image/png;base64,eA==",
    ] {
        assert!(validated_url(&source(bad), &origins).is_none(), "{bad}");
    }
    assert!(validated_url(&source("https://cdn.example.test/x"), &[]).is_none());
}

async fn headers(socket: &mut tokio::net::TcpStream) -> String {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        assert!(bytes.len() < 8192);
        bytes.push(socket.read_u8().await.unwrap());
    }
    String::from_utf8(bytes).unwrap()
}

#[tokio::test]
async fn public_image_http_sends_no_account_headers_and_never_follows_redirects() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let trap = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target = trap.local_addr().unwrap();
    let url = url::Url::parse(&format!("http://{}/image", listener.local_addr().unwrap())).unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let request = headers(&mut socket).await.to_ascii_lowercase();
        assert!(!request.contains("authorization:") && !request.contains("cookie:"));
        assert!(request.contains("user-agent: rune/"));
        socket.write_all(format!("HTTP/1.1 302 Redirect\r\nLocation: http://{target}/trap\r\nContent-Length: 0\r\n\r\n").as_bytes()).await.unwrap();
    });
    let images = Images::new(Handle::current(), &[], Arc::new(|| {}));
    assert!(download(images.client.as_ref().unwrap(), url, MAX_BYTES)
        .await
        .is_err());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), trap.accept())
            .await
            .is_err()
    );
    server.await.unwrap();
}

#[tokio::test]
async fn response_limits_apply_to_declared_length_and_streamed_chunks() {
    for response in [
        b"HTTP/1.1 200 OK\r\nContent-Length: 1025\r\n\r\n".to_vec(),
        [
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n401\r\n".as_slice(),
            &[b'x'; 1025],
            b"\r\n0\r\n\r\n",
        ]
        .concat(),
    ] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = url::Url::parse(&format!(
            "http://{}/preview",
            listener.local_addr().unwrap()
        ))
        .unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            headers(&mut socket).await;
            socket.write_all(&response).await.unwrap();
        });
        let images = Images::new(Handle::current(), &[], Arc::new(|| {}));
        let failure = download(images.client.as_ref().unwrap(), url, 1024)
            .await
            .err()
            .unwrap();
        assert!(failure.retry.is_none() && !failure.origin_cooldown);
        server.await.unwrap();
    }
    assert_eq!(byte_limit(ImageKind::AttachmentPreview), 8 * 1024 * 1024);
    assert_eq!(byte_limit(ImageKind::Artwork), 1024 * 1024);
    assert_eq!(byte_limit(ImageKind::Banner), 1024 * 1024);
}

#[tokio::test]
async fn previews_and_banners_share_two_download_slots_and_cancel_on_logout() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let mut wanted: Vec<_> = (0..6)
        .map(|i| RemoteImage {
            url: format!("{origin}/preview{i}.jpg"),
            kind: if i % 2 == 0 {
                ImageKind::AttachmentPreview
            } else {
                ImageKind::Banner
            },
        })
        .collect();
    wanted.extend((0..2).map(|i| source(format!("{origin}/artwork{i}.png"))));
    let mut images = Images::new(Handle::current(), &[], Arc::new(|| {}));
    images.loopback = true;
    images.load(&wanted);
    let mut sockets = Vec::new();
    let mut previews = 0;
    for _ in 0..CONCURRENT {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(1), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let request = headers(&mut socket).await;
        previews += usize::from(request.contains("/preview"));
        assert!(!request.to_ascii_lowercase().contains("authorization:"));
        sockets.push(socket);
    }
    assert_eq!(previews, PREVIEW_CONCURRENT);
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    images.clear();
    for mut socket in sockets {
        assert!(
            tokio::time::timeout(Duration::from_secs(1), socket.read_u8())
                .await
                .unwrap()
                .is_err()
        );
    }
    assert!(
        images.failures(&wanted).is_empty(),
        "cancellation is not a failed download"
    );
}

#[tokio::test]
async fn visible_failures_distinguish_pending_ready_invalid_and_origin_backoff() {
    let (wake, mut woken) = mpsc::channel(8);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let requested = RemoteImage {
        url: format!("http://{}/preview.png", listener.local_addr().unwrap()),
        kind: ImageKind::AttachmentPreview,
    };
    let invalid = source("https://unapproved.test/image.png");
    let wanted = [requested.clone(), invalid.clone()];
    let mut images = Images::new(
        Handle::current(),
        &[],
        Arc::new(move || {
            let _ = wake.try_send(());
        }),
    );
    images.loopback = true;
    images.load(&wanted);
    assert_eq!(images.failures(&wanted), HashSet::from([invalid.clone()]));
    let (mut socket, _) = listener.accept().await.unwrap();
    headers(&mut socket).await;
    socket
        .write_all(b"HTTP/1.1 429 Limited\r\nRetry-After: 90\r\nContent-Length: 0\r\n\r\n")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), woken.recv())
        .await
        .unwrap();
    images.load(&wanted);
    assert_eq!(images.failures(&wanted), HashSet::from(wanted.clone()));
    images.insert(
        requested.clone(),
        Some(Arc::new(
            super::normalize(&png(10, 20), requested.kind).unwrap(),
        )),
    );
    assert_eq!(images.failures(&wanted), HashSet::from([invalid]));
    assert!(images.failures(&[]).is_empty());
}

#[tokio::test]
async fn concurrent_downloads_and_logout_are_bounded_and_old_results_are_discarded() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let wanted: Vec<_> = (0..10)
        .map(|i| source(format!("{origin}/{i}.png")))
        .collect();
    let mut images = Images::new(Handle::current(), &[], Arc::new(|| {}));
    images.loopback = true;
    assert!(images.load(&wanted).is_empty());
    let mut sockets = Vec::new();
    for _ in 0..CONCURRENT {
        let (mut socket, _) = tokio::time::timeout(Duration::from_secs(1), listener.accept())
            .await
            .unwrap()
            .unwrap();
        headers(&mut socket).await;
        sockets.push(socket);
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    let old_epoch = images.epoch;
    images.clear();
    for mut socket in sockets {
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(1), socket.read_u8())
                .await
                .unwrap()
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::UnexpectedEof
        );
    }
    images
        .sender
        .send(Completed {
            epoch: old_epoch,
            source: wanted[0].clone(),
            data: Some(Arc::new(normalize(&png(2, 2)).unwrap())),
            retry: None,
            origin_cooldown: false,
        })
        .await
        .unwrap();
    assert!(images.load(&[]).is_empty());
    assert!(images.cache.is_empty() && images.pending.is_empty());
}

#[tokio::test]
async fn successful_download_completes_off_thread_and_cache_evicts_to_its_budget() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let wanted = vec![source(format!(
        "http://{}/image.png",
        listener.local_addr().unwrap()
    ))];
    let bytes = png(10, 20);
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        headers(&mut socket).await;
        socket
            .write_all(
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", bytes.len()).as_bytes(),
            )
            .await
            .unwrap();
        socket.write_all(&bytes).await.unwrap();
    });
    let (wake, mut woken) = mpsc::channel(8);
    let mut images = Images::new(
        Handle::current(),
        &[],
        Arc::new(move || {
            let _ = wake.try_send(());
        }),
    );
    images.loopback = true;
    assert!(images.load(&wanted).is_empty());
    tokio::time::timeout(Duration::from_secs(1), woken.recv())
        .await
        .unwrap();
    assert_eq!(images.load(&wanted).len(), 1);
    server.await.unwrap();
    for index in 0..IMAGE_ENTRIES + 10 {
        images.insert(source(format!("https://example.test/{index}")), None);
    }
    assert_eq!(images.cache.len(), IMAGE_ENTRIES);
    // What is on screen is never pushed out to make room for the rest.
    assert!(images.cache.contains_key(&wanted[0]));
    // Off screen, the least recently wanted goes first.
    assert!(images.load(&[]).is_empty());
    images.insert(source("one-more"), None);
    assert_eq!(images.cache.len(), IMAGE_ENTRIES);
    assert!(!images.cache.contains_key(&wanted[0]));
    let large = Arc::new(ImageData {
        sha256: "0".repeat(64),
        size_bytes: 1,
        width: 1,
        height: 1,
        base64: Arc::from("x".repeat(CACHE_BYTES / 2 + 1)),
        animation: None,
        format: Encoding::Png,
    });
    images.insert(source("large1"), Some(large.clone()));
    images.insert(source("large2"), Some(large));
    assert!(images.bytes <= CACHE_BYTES);
    assert!(!images.cache.contains_key(&source("large1")));
}

// Pictures on disk between runs (imagenes-y-lectura).
#[tokio::test]
async fn a_downloaded_picture_is_kept_sealed_and_the_next_run_reads_it_without_the_network() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let wanted = vec![source(format!(
        "http://{}/avatar.png",
        listener.local_addr().unwrap()
    ))];
    let bytes = png(10, 20);
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        headers(&mut socket).await;
        socket
            .write_all(
                format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", bytes.len()).as_bytes(),
            )
            .await
            .unwrap();
        socket.write_all(&bytes).await.unwrap();
        // The listener goes with it: nothing answers after.
    });
    let root = tempfile::tempdir().unwrap();
    let key = rune_storage::Key::generate().unwrap();
    let (wake, mut woken) = mpsc::channel(64);
    let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        let _ = wake.try_send(());
    });
    let run = || {
        let mut images = Images::new(Handle::current(), &[], wake.clone());
        images.loopback = true;
        images.disk.set_root(Some(root.path().to_owned()));
        images.disk.attach("4:mock:1".into(), key.clone(), false);
        images.disk.set_limits(CacheLimits::default());
        images
    };
    let mut first = run();
    while first.disk.usage().is_none() {
        tokio::time::timeout(Duration::from_secs(1), woken.recv())
            .await
            .unwrap();
    }
    assert!(first.load(&wanted).is_empty());
    let shown = loop {
        tokio::time::timeout(Duration::from_secs(1), woken.recv())
            .await
            .unwrap();
        if let Some((_, data)) = first.load(&wanted).pop() {
            break data;
        }
    };
    server.await.unwrap();
    assert!(first.disk.usage().unwrap().artwork_bytes > 0);
    drop(first);
    // The next run, with the server gone, shows it from the disk, even when
    // its first frame asks before the disk is open: nothing goes to the
    // network meanwhile, so it is not downloaded again.
    let mut second = run();
    assert!(second.disk.opening());
    assert!(second.load(&wanted).is_empty());
    assert!(second.pending.is_empty());
    let again = loop {
        tokio::time::timeout(Duration::from_secs(1), woken.recv())
            .await
            .unwrap();
        if let Some((_, data)) = second.load(&wanted).pop() {
            break data;
        }
    };
    assert_eq!(again.sha256, shown.sha256);
    assert!(second.failures.is_empty());
    // Forgotten, its folder goes with the picture.
    second.disk.forget("4:mock:1".into());
    while std::fs::read_dir(root.path().join("images"))
        .unwrap()
        .next()
        .is_some()
    {
        tokio::time::timeout(Duration::from_secs(1), woken.recv())
            .await
            .unwrap();
    }
    assert!(second.disk.usage().is_none());
}

#[tokio::test]
async fn server_backoff_survives_eviction_and_waits_instead_of_retrying_each_frame() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let wanted = vec![source(format!(
        "http://{}/image.png",
        listener.local_addr().unwrap()
    ))];
    let (wake, mut woken) = mpsc::channel(8);
    let mut images = Images::new(
        Handle::current(),
        &[],
        Arc::new(move || {
            let _ = wake.try_send(());
        }),
    );
    images.loopback = true;
    images.load(&wanted);
    let (mut socket, _) = listener.accept().await.unwrap();
    headers(&mut socket).await;
    socket
        .write_all(b"HTTP/1.1 429 Limited\r\nRetry-After: 90\r\nContent-Length: 0\r\n\r\n")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), woken.recv())
        .await
        .unwrap();
    images.load(&wanted);
    assert!(images.next_retry().unwrap().duration_since(Instant::now()) > Duration::from_secs(85));
    for index in 0..IMAGE_ENTRIES + 10 {
        images.insert(source(format!("https://example.test/{index}")), None);
    }
    for _ in 0..10 {
        images.load(&wanted);
    }
    assert!(images.pending.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    let origin = url::Url::parse(&wanted[0].url)
        .unwrap()
        .origin()
        .ascii_serialization();
    images
        .cooldowns
        .insert(origin, Some(Instant::now() - Duration::from_secs(1)));
    images.load(&wanted);
    assert_eq!(images.pending.len(), 1);
    images.clear();
    assert!(images.next_retry().is_none());
}

#[tokio::test]
async fn unknown_server_retry_deadline_holds_the_origin_until_local_logout() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let wanted = vec![source(format!(
        "http://{}/image.png",
        listener.local_addr().unwrap()
    ))];
    let (wake, mut woken) = mpsc::channel(8);
    let mut images = Images::new(
        Handle::current(),
        &[],
        Arc::new(move || {
            let _ = wake.try_send(());
        }),
    );
    images.loopback = true;
    images.load(&wanted);
    let (mut socket, _) = listener.accept().await.unwrap();
    headers(&mut socket).await;
    socket
        .write_all(b"HTTP/1.1 429 Limited\r\nRetry-After: unavailable\r\nContent-Length: 0\r\n\r\n")
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(1), woken.recv())
        .await
        .unwrap();
    images.load(&wanted);
    for index in 0..IMAGE_ENTRIES + 10 {
        images.insert(source(format!("https://example.test/{index}")), None);
    }
    images.load(&wanted);
    assert!(images.pending.is_empty() && images.next_retry().is_none());
    assert!(
        tokio::time::timeout(Duration::from_millis(50), listener.accept())
            .await
            .is_err()
    );
    images.clear();
    images.load(&wanted);
    assert_eq!(images.pending.len(), 1);
}

/// An animated PNG of one 16 px frame per color.
fn apng(colors: &[[u8; 4]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 16, 16);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_animated(colors.len() as u32, 0).unwrap();
        let mut writer = encoder.write_header().unwrap();
        for color in colors {
            let frame: Vec<u8> = std::iter::repeat_n(*color, 16 * 16).flatten().collect();
            writer.write_image_data(&frame).unwrap();
        }
    }
    bytes
}

#[test]
fn an_animated_png_keeps_its_first_frame_and_plays_its_frames() {
    let colors = [[20, 80, 160, 255], [240, 40, 20, 255]];
    let animated = super::normalize(&apng(&colors), ImageKind::Artwork).unwrap();
    let still = super::normalize(&png(16, 16), ImageKind::Artwork).unwrap();
    assert_eq!(
        animated.sha256, still.sha256,
        "the first frame is the preview"
    );
    assert_eq!(animated.animation.as_ref().map(|a| a.frames.len()), Some(2));
    assert!(still.animation.is_none(), "a plain PNG does not play");
}

/// Message content (contenido-de-mensajes): an explicit picture blurred
/// past recognition, standing still.
#[test]
fn obscured_previews_are_tiny_blurred_and_still() {
    // Half white, half black: blurred, the two meet in grays.
    let sharp = image::RgbaImage::from_fn(400, 300, |x, _| {
        let value = if x < 200 { 255 } else { 0 };
        image::Rgba([value, value, value, 255])
    });
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(sharp)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    let blurred = super::normalize(bytes.get_ref(), ImageKind::Obscured).unwrap();
    assert_eq!((blurred.width, blurred.height), (OBSCURED_SIDE, 18));
    assert!(blurred.animation.is_none());
    let pixels = image::load_from_memory(
        &base64::engine::general_purpose::STANDARD
            .decode(&*blurred.base64)
            .unwrap(),
    )
    .unwrap()
    .to_rgba8();
    let middle = pixels.get_pixel(OBSCURED_SIDE / 2, 9).0[0];
    assert!((40..215).contains(&middle), "{middle}");
    // A GIF blurred does not keep its frames.
    let moving = gif(&[[255, 0, 0, 255], [0, 0, 255, 255]]);
    assert!(super::normalize(&moving, ImageKind::Obscured)
        .unwrap()
        .animation
        .is_none());
    assert_eq!(byte_limit(ImageKind::Obscured), PREVIEW_BYTES);
}

/// A ThumbHash is decoded here at once, and a bad one fails without a request.
#[tokio::test]
async fn thumbhashes_are_ready_at_once_and_bad_ones_fail_without_a_request() {
    let mut images = Images::new(Handle::current(), &[], Arc::new(|| {}));
    let hash = RemoteImage {
        url: "oPcNHZpwd3dxeHiHh3eHh9jQDPh4".into(),
        kind: ImageKind::ThumbHash,
    };
    let bad = RemoteImage {
        url: "not base64!".into(),
        kind: ImageKind::ThumbHash,
    };
    let wanted = [hash.clone(), bad.clone()];
    let ready = images.load(&wanted);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].0, hash);
    assert_eq!((ready[0].1.width, ready[0].1.height), (32, 23));
    assert!(images.pending.is_empty(), "nothing is fetched");
    assert!(images.failures(&wanted).contains(&bad));
    assert!(!images.failures(&wanted).contains(&hash));
    // Cached: the same answer the next frame.
    assert_eq!(images.load(&wanted).len(), 1);
}

/// Only a few ThumbHashes are decoded a frame; the rest, the next one.
#[tokio::test]
async fn thumbhashes_spread_over_frames() {
    let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let wakes = woken.clone();
    let mut images = Images::new(
        Handle::current(),
        &[],
        Arc::new(move || {
            wakes.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }),
    );
    // Different AC terms of one valid hash.
    let wanted: Vec<RemoteImage> = "ABCDEFGHIJ"
        .chars()
        .map(|c| RemoteImage {
            url: format!("oPcNHZpwd3dx{c}HiHh3eHh9jQDPh4"),
            kind: ImageKind::ThumbHash,
        })
        .collect();
    assert_eq!(images.load(&wanted).len(), THUMBHASHES_PER_LOAD);
    assert_eq!(woken.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(images.load(&wanted).len(), wanted.len());
    assert_eq!(woken.load(std::sync::atomic::Ordering::Relaxed), 1);
}

/// A picture the simulation saved is read from it, with no origin and no
/// network; anything else at its made-up address still fails
/// (edicion-de-perfil).
#[tokio::test]
async fn a_picture_the_provider_keeps_loads_without_any_origin() {
    use rune_provider::{mock, Command, Event, ProfileEdit, ProfileImage};
    let provider = mock::MockProvider::new(mock::MockConfig::default());
    let saved = provider.manage(Command::EditProfile(ProfileEdit {
        avatar: Some(Some(ProfileImage::new(png(64, 64).into()).unwrap())),
        ..ProfileEdit::default()
    }));
    let Some(Event::UserChanged(me)) = saved.first() else {
        panic!("{saved:?}");
    };
    let avatar = me.avatar.clone().unwrap();
    let (wake, mut woken) = mpsc::channel(8);
    let mut images = Images::new(
        Handle::current(),
        &[],
        Arc::new(move || {
            let _ = wake.try_send(());
        }),
    );
    images.hosted = Some(Arc::new(provider));
    let elsewhere = source(avatar.url.replace("/avatar/", "/otra/"));
    let wanted = [avatar.clone(), elsewhere.clone()];
    assert!(images.load(&wanted).is_empty());
    tokio::time::timeout(Duration::from_secs(5), woken.recv())
        .await
        .unwrap();
    let ready = images.load(&wanted);
    assert!(
        matches!(&ready[..], [(source, data)] if *source == avatar && data.width == THUMBNAIL),
        "{ready:?}"
    );
    assert_eq!(images.failures(&wanted), HashSet::from([elsewhere]));
}

/// A screen of density 2 gets its pictures decoded twice as large on a side
/// and in half steps, so a cached one serves nearby scales (pulido-ui).
#[test]
fn pictures_are_decoded_for_the_screen_s_density_in_steps() {
    assert_eq!(density_step(1.0), 1.0);
    assert_eq!(density_step(1.25), 1.5);
    assert_eq!(density_step(2.0), 2.0);
    assert_eq!(density_step(3.0), 2.0, "capped");
    assert_eq!(density_step(0.5), 1.0);
    assert_eq!(density_step(f32::NAN), 1.0);
    let at = |kind, width, height, density| {
        let data = normalize_at("/a.png", &png(width, height), kind, density).unwrap();
        (data.width, data.height)
    };
    let preview = ImageKind::AttachmentPreview;
    assert_eq!(at(preview, 2400, 1600, 1.0), (768, 512));
    assert_eq!(at(preview, 2400, 1600, 2.0), (1536, 1024));
    assert_eq!(at(preview, 1000, 500, 2.0), (1000, 500), "never enlarged");
    assert_eq!(at(ImageKind::Banner, 1600, 600, 2.0), (1024, 384));
    assert_eq!(at(ImageKind::Artwork, 300, 300, 1.5), (192, 192));
    assert_eq!(at(ImageKind::Artwork, 300, 300, 2.0), (256, 256));
}

/// What was decoded for another density goes; a picture at its own size
/// stays, and so does what the same step decoded.
#[tokio::test]
async fn a_new_density_forgets_the_previews_decoded_for_the_old_one() {
    let mut images = Images::new(Handle::current(), &[], Arc::new(|| {}));
    let preview = source("https://a.test/p.png");
    let original = RemoteImage {
        url: preview.url.clone(),
        kind: ImageKind::Original,
    };
    let data = Arc::new(super::normalize(&png(4, 4), ImageKind::Artwork).unwrap());
    images.insert(preview.clone(), Some(data.clone()));
    images.insert(original.clone(), Some(data));
    images.set_density(1.2);
    assert!(images.cache.contains_key(&original));
    assert!(!images.cache.contains_key(&preview));
    assert_eq!(images.bytes, 0);
    images.insert(
        preview.clone(),
        Some(Arc::new(
            super::normalize(&png(4, 4), ImageKind::Artwork).unwrap(),
        )),
    );
    images.set_density(1.5);
    assert!(images.cache.contains_key(&preview), "the same step");
}

/// The provider's address for the screen's size is what is downloaded, on
/// the same origin, and decoded for that density.
#[tokio::test]
async fn the_provider_s_address_for_a_dense_screen_is_what_is_fetched() {
    use rune_provider::{mock, Provider};
    struct Sizing(mock::MockProvider);
    impl Provider for Sizing {
        fn info(&self) -> rune_provider::ProviderInfo {
            self.0.info()
        }
        fn connect<'a>(
            &'a self,
            commands: &'a mut mpsc::Receiver<rune_provider::Command>,
            events: &'a rune_provider::EventSink,
        ) -> rune_provider::BoxFuture<'a, Result<(), rune_provider::ProviderError>> {
            self.0.connect(commands, events)
        }
        fn image_at(&self, url: &str, density: f32) -> Option<String> {
            Some(format!("{url}?size={}", (64.0 * density) as u32))
        }
    }
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let wanted = [source(format!(
        "http://{}/avatar.png",
        listener.local_addr().unwrap()
    ))];
    let (wake, mut woken) = mpsc::channel(8);
    let mut images = Images::new(
        Handle::current(),
        &[],
        Arc::new(move || {
            let _ = wake.try_send(());
        }),
    );
    images.loopback = true;
    images.hosted = Some(Arc::new(Sizing(mock::MockProvider::new(
        mock::MockConfig::default(),
    ))));
    images.set_density(2.0);
    assert!(images.load(&wanted).is_empty());
    let (mut socket, _) = listener.accept().await.unwrap();
    assert!(headers(&mut socket)
        .await
        .starts_with("GET /avatar.png?size=128 "));
    let bytes = png(300, 300);
    socket
        .write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n", bytes.len()).as_bytes())
        .await
        .unwrap();
    socket.write_all(&bytes).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), woken.recv())
        .await
        .unwrap();
    let ready = images.load(&wanted);
    assert!(
        matches!(&ready[..], [(_, data)] if data.width == 2 * THUMBNAIL),
        "{ready:?}"
    );
}
