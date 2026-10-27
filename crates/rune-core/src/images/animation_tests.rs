use super::*;

fn gif(side: u32, frames: usize, delay_ms: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new_with_speed(&mut bytes, 30);
        for index in 0..frames {
            let shade = (index % 250) as u8;
            encoder
                .encode_frame(image::Frame::from_parts(
                    image::RgbaImage::from_pixel(side, side, image::Rgba([shade, 0, 200, 255])),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(delay_ms, 1),
                ))
                .unwrap();
        }
    }
    bytes
}

fn source(url: &str) -> RemoteImage {
    RemoteImage {
        url: url.into(),
        kind: ImageKind::AttachmentPreview,
    }
}

#[test]
fn animated_gifs_keep_frames_at_the_preview_size_and_still_ones_have_none() {
    assert!(normalize(&gif(16, 1, 0), ImageKind::AttachmentPreview)
        .unwrap()
        .animation
        .is_none());
    for kind in [ImageKind::Artwork, ImageKind::AttachmentPreview] {
        let data = normalize(&gif(16, 2, 0), kind).unwrap();
        let animation = data.animation.as_ref().expect("two frames animate");
        assert_eq!(
            (animation.width, animation.height),
            (data.width, data.height)
        );
        assert_eq!(animation.frames.len(), 2);
        for (index, frame) in animation.frames.iter().enumerate() {
            // Near-zero delays play at 100 ms, as browsers show them.
            assert_eq!(frame.delay, Duration::from_millis(100));
            assert_eq!(frame.rgba.len(), (data.width * data.height * 4) as usize);
            let center = ((data.height / 2 * data.width + data.width / 2) * 4) as usize;
            assert_eq!(frame.rgba[center..center + 4], [index as u8, 0, 200, 255]);
        }
    }
    // Banners stay still: their wide artwork has no animation budget.
    assert!(normalize(&gif(16, 2, 0), ImageKind::Banner)
        .unwrap()
        .animation
        .is_none());
    // A broken later frame refuses the animation, never the first frame.
    let animated = gif(16, 3, 0);
    let broken = normalize(
        &animated[..animated.len() - 4],
        ImageKind::AttachmentPreview,
    )
    .unwrap();
    assert!(broken.animation.is_none());
    assert_eq!(
        broken.sha256,
        normalize(&animated, ImageKind::AttachmentPreview)
            .unwrap()
            .sha256
    );
}

#[test]
fn long_animations_halve_their_frame_rate_within_frame_and_byte_budgets() {
    let many = normalize(&gif(8, 600, 50), ImageKind::Artwork).unwrap();
    let animation = many.animation.unwrap();
    assert!(animation.frames.len() <= animation::MAX_FRAMES);
    let total: Duration = animation.frames.iter().map(|frame| frame.delay).sum();
    assert_eq!(
        total,
        Duration::from_millis(600 * 50),
        "the loop keeps its length"
    );
    // Too many frames to read at all keeps the still preview only.
    let endless = normalize(&gif(2, 1001, 50), ImageKind::Artwork).unwrap();
    assert!(endless.animation.is_none());
    // 512 x 512 preview frames take 1 MiB each: 16 MiB holds 16 of 40.
    let large = normalize(&gif(512, 40, 40), ImageKind::AttachmentPreview).unwrap();
    let animation = large.animation.unwrap();
    assert!(animation.bytes() <= animation::MAX_ANIMATION_BYTES);
    assert!(animation.frames.len() < 40);
    let total: Duration = animation.frames.iter().map(|frame| frame.delay).sum();
    assert_eq!(total, Duration::from_millis(40 * 40));
}

#[test]
fn playback_loops_over_the_frame_delays() {
    let frame = |ms| AnimationFrame {
        delay: Duration::from_millis(ms),
        rgba: vec![0; 4],
    };
    let animation = Animation {
        width: 1,
        height: 1,
        frames: vec![frame(100), frame(50)],
    };
    let at = |ms| animation.frame_at(Duration::from_millis(ms));
    assert_eq!(at(0), (0, Duration::from_millis(100)));
    assert_eq!(at(99), (0, Duration::from_millis(1)));
    assert_eq!(at(100), (1, Duration::from_millis(50)));
    assert_eq!(at(150), (0, Duration::from_millis(100)));
    assert_eq!(at(1_000_170), (1, Duration::from_millis(30)));
}

#[tokio::test]
async fn cached_animations_share_one_budget_and_old_ones_keep_their_first_frame() {
    let mut images = Images::new(Handle::current(), &[], Arc::new(|| {}));
    let animated = || {
        Arc::new(ImageData {
            sha256: "0".repeat(64),
            size_bytes: 1,
            width: 1,
            height: 1,
            base64: Arc::from("x"),
            animation: Some(Arc::new(Animation {
                width: 1024,
                height: 1024,
                frames: (0..4)
                    .map(|_| AnimationFrame {
                        delay: Duration::from_millis(100),
                        rgba: vec![0; 1024 * 1024 * 4],
                    })
                    .collect(),
            })),
            format: Encoding::Png,
            color: None,
        })
    };
    // Five 16 MiB animations exceed the 64 MiB budget by one.
    for index in 0..5 {
        images.insert(source(&format!("a{index}")), Some(animated()));
    }
    let still = |index: usize| {
        images.cache[&source(&format!("a{index}"))]
            .as_ref()
            .unwrap()
            .animation
            .is_none()
    };
    assert!(still(0), "the least recently wanted is stripped");
    assert!((1..5).all(|index| !still(index)));
    assert_eq!(images.cache.len(), 5, "no preview is evicted for it");
    images.clear();
    assert!(images.cache.is_empty(), "signing out frees every frame");
}

#[cfg(target_os = "linux")]
fn media_fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../rune-media/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

#[cfg(target_os = "linux")]
#[test]
fn a_gif_sites_clip_becomes_its_first_picture_and_an_animation_of_all() {
    let data = normalize(&media_fixture("loop.mp4"), ImageKind::Clip).unwrap();
    assert_eq!(
        (data.width, data.height, data.format),
        (160, 90, Encoding::Png)
    );
    let animation = data.animation.as_ref().expect("it plays");
    assert_eq!((animation.width, animation.height), (160, 90));
    assert_eq!(animation.frames.len(), 10);
    let lasts: Duration = animation.frames.iter().map(|frame| frame.delay).sum();
    assert!((lasts.as_secs_f64() - 1.0).abs() < 0.05, "{lasts:?}");
    // B-frames come out in the order they show.
    let reordered = normalize(&media_fixture("clip.mp4"), ImageKind::Clip).unwrap();
    assert_eq!(reordered.animation.unwrap().frames.len(), 30);
    assert!(normalize(b"not a clip at all", ImageKind::Clip).is_none());
    assert!(normalize(&media_fixture("hevc.mp4"), ImageKind::Clip).is_none());
    // Its bytes are not a picture: as a preview, nothing.
    assert!(normalize(&media_fixture("loop.mp4"), ImageKind::AttachmentPreview).is_none());
}

#[tokio::test]
async fn a_picture_at_its_own_size_keeps_its_file_and_its_own_budget() {
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        1200,
        900,
        image::Rgba([10, 200, 90, 255]),
    ))
    .write_to(&mut png, ImageFormat::Png)
    .unwrap();
    let png = png.into_inner();
    let data = normalize(&png, ImageKind::Original).unwrap();
    assert_eq!((data.width, data.height), (1200, 900), "not downscaled");
    assert_eq!(data.format, Encoding::Png);
    use base64::Engine as _;
    assert_eq!(
        base64::engine::general_purpose::STANDARD
            .decode(data.base64.as_bytes())
            .unwrap(),
        png,
        "kept as its file is"
    );
    assert!(normalize(&png[..png.len() / 2], ImageKind::Original).is_none());
    assert!(normalize(b"<svg/>", ImageKind::Original).is_none());
    // One at a time, and never pushing previews out.
    let mut images = Images::new(Handle::current(), &[], Arc::new(|| {}));
    let original = |url: &str| RemoteImage {
        url: url.into(),
        kind: ImageKind::Original,
    };
    let large = Arc::new(ImageData {
        base64: Arc::from("x".repeat(CACHE_BYTES + 1)),
        ..data.clone()
    });
    images.insert(source("preview"), Some(Arc::new(data.clone())));
    images.insert(original("a"), Some(large.clone()));
    images.insert(original("b"), Some(large));
    assert!(images.cache.contains_key(&source("preview")));
    assert!(!images.cache.contains_key(&original("a")));
    assert!(images.cache.contains_key(&original("b")));
    assert!(images.bytes <= CACHE_BYTES);
}
