use std::os::unix::net::UnixStream;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use super::*;

const WATCHDOG: Duration = Duration::from_secs(5);

fn plane(bytes: &[u8], stride: usize) -> Plane<'_> {
    Plane { bytes, stride }
}

fn wait_for(video: &Video, done: impl Fn(&VideoSnapshot) -> bool) -> VideoSnapshot {
    let start = Instant::now();
    loop {
        let snapshot = video.snapshot();
        if done(&snapshot) {
            return snapshot;
        }
        assert!(start.elapsed() < WATCHDOG, "still {snapshot:?}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn wait_flag(flag: &AtomicBool) {
    let start = Instant::now();
    while !flag.load(Ordering::Acquire) {
        assert!(start.elapsed() < WATCHDOG, "never released");
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn previews_fit_720p_keep_their_shape_and_never_grow() {
    assert_eq!(fit(3840, 2160, PREVIEW_MAX), (1280, 720));
    assert_eq!(fit(2560, 1600, PREVIEW_MAX), (1152, 720));
    assert_eq!(fit(5120, 1440, PREVIEW_MAX), (1280, 360));
    assert_eq!(fit(640, 480, PREVIEW_MAX), (640, 480));
    assert_eq!(fit(8192, 1, PREVIEW_MAX), (1280, 1));
}

#[test]
fn packed_rgb_is_read_in_its_order_with_stride_padding_and_made_opaque() {
    // 2×2 BGRx with four bytes of padding at the end of each row.
    let bgrx = [
        1, 2, 3, 0, 4, 5, 6, 0, 9, 9, 9, 9, //
        7, 8, 9, 0, 10, 11, 12, 0, 9, 9, 9, 9,
    ];
    let frame = convert(Pixels::Bgr4, 2, 2, &[plane(&bgrx, 12)]).unwrap();
    assert_eq!((frame.width, frame.height), (2, 2));
    assert_eq!(
        frame.rgba,
        [3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]
    );
    let rgbx = convert(Pixels::Rgb4, 2, 2, &[plane(&bgrx, 12)]).unwrap();
    assert_eq!(&rgbx.rgba[..4], &[1, 2, 3, 255]);
}

#[test]
fn camera_yuv_reaches_black_white_and_full_red() {
    // YUY2: Y0 U Y1 V. Black, white; then a red pair.
    let yuy2 = [16, 128, 235, 128, 81, 90, 81, 240];
    let frame = convert(Pixels::Yuy2, 2, 2, &[plane(&yuy2, 4)]).unwrap();
    assert_eq!(&frame.rgba[..8], &[0, 0, 0, 255, 255, 255, 255, 255]);
    let red = &frame.rgba[8..12];
    assert!(red[0] > 240 && red[1] < 10 && red[2] < 10, "{red:?}");
    // NV12: a 2×2 luma plane, then one U/V pair for all four.
    let luma = [235, 235, 235, 235];
    let chroma = [128, 128];
    let frame = convert(Pixels::Nv12, 2, 2, &[plane(&luma, 2), plane(&chroma, 2)]).unwrap();
    assert!(frame
        .rgba
        .chunks(4)
        .all(|pixel| pixel == [255, 255, 255, 255]));
}

#[test]
fn frames_the_mapping_does_not_hold_are_refused() {
    let short = [0u8; 15];
    assert!(convert(Pixels::Bgr4, 2, 2, &[plane(&short, 8)]).is_none());
    assert!(
        convert(Pixels::Bgr4, 2, 2, &[plane(&[0; 16], 4)]).is_none(),
        "a stride under the row"
    );
    assert!(convert(Pixels::Nv12, 2, 2, &[plane(&[0; 4], 2)]).is_none());
    assert!(convert(Pixels::Bgr4, 0, 2, &[plane(&[], 0)]).is_none());
    assert!(convert(Pixels::Bgr4, MAX_SOURCE_SIDE + 1, 1, &[plane(&[0; 4], 4)]).is_none());
    let big = vec![0u8; 3840 * 4 * 2160];
    let frame = convert(Pixels::Bgr4, 3840, 2160, &[plane(&big, 3840 * 4)]).unwrap();
    assert_eq!(frame.rgba.len(), 1280 * 720 * 4, "downscaled, not copied");
}

#[test]
fn the_queue_keeps_only_the_newest_frames() {
    let queue = FrameQueue::default();
    let frame = |n: u8| Frame {
        width: 1,
        height: 1,
        rgba: vec![n, n, n, 255],
    };
    for n in 0..5 {
        queue.push(frame(n));
    }
    assert!(lock(&queue.frames).len() <= QUEUE_FRAMES);
    assert_eq!(queue.newest(), Some(frame(4)));
    assert_eq!(queue.newest(), None);
    assert_eq!(queue.queued.load(Ordering::Relaxed), 5);
    assert_eq!(queue.dropped.load(Ordering::Relaxed), 4);
}

#[test]
fn a_denied_source_ends_with_the_reason_and_never_has_a_frame() {
    let video = Video::spawn(VideoKind::Camera, None, None, |_| {
        Box::pin(async { Err(CaptureError::Denied) })
    });
    let snapshot = wait_for(&video, |snapshot| {
        matches!(snapshot.state, VideoState::Ended(_))
    });
    assert_eq!(
        snapshot.state,
        VideoState::Ended(Some("El sistema no dio permiso.".into()))
    );
    assert_eq!(video.take_frame(), None);
    assert_eq!(snapshot.frames, 0);
}

#[test]
fn dropping_while_the_system_asks_cancels_the_request() {
    let released = Arc::new(AtomicBool::new(false));
    let flag = released.clone();
    let video = Video::spawn(VideoKind::Screen, None, None, move |mut cancel| {
        Box::pin(async move {
            cancelled(&mut cancel).await;
            flag.store(true, Ordering::Release);
            Err(CaptureError::Cancelled)
        })
    });
    assert_eq!(video.snapshot().state, VideoState::Starting);
    drop(video);
    wait_flag(&released);
}

/// A session that the test ends, and that says when it was released.
struct FakeSession {
    ended: Option<tokio::sync::oneshot::Receiver<()>>,
    closed: Arc<AtomicBool>,
}

impl Session for FakeSession {
    fn ended(&mut self) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(async move {
            if let Some(ended) = self.ended.take() {
                let _ = ended.await;
            }
            CaptureError::Closed.to_string()
        })
    }

    fn close(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move { self.closed.store(true, Ordering::Release) })
    }
}

/// A source whose remote nobody answers: PipeWire waits on it, with no frame.
fn granted(
    ended: tokio::sync::oneshot::Receiver<()>,
    closed: Arc<AtomicBool>,
) -> (Video, UnixStream) {
    let (remote, peer) = UnixStream::pair().unwrap();
    let video = Video::spawn(VideoKind::Screen, None, None, move |_| {
        Box::pin(async move {
            Ok(Source {
                remote: remote.into(),
                node: Some(42),
                session: Some(Box::new(FakeSession {
                    ended: Some(ended),
                    closed,
                })),
                restore: Some("portal-choice".into()),
            })
        })
    });
    (video, peer)
}

#[test]
fn the_system_ending_the_session_ends_the_preview_and_releases_it() {
    let closed = Arc::new(AtomicBool::new(false));
    let (end, ended) = tokio::sync::oneshot::channel();
    let (video, _peer) = granted(ended, closed.clone());
    end.send(()).unwrap();
    let snapshot = wait_for(&video, |snapshot| {
        matches!(snapshot.state, VideoState::Ended(_))
    });
    assert_eq!(
        snapshot.state,
        VideoState::Ended(Some("El sistema terminó la sesión.".into()))
    );
    // The portal's token for the choice outlives it, for the next share.
    assert_eq!(snapshot.restore_token.as_deref(), Some("portal-choice"));
    wait_flag(&closed);
}

#[test]
fn stopping_a_granted_source_releases_its_session() {
    let closed = Arc::new(AtomicBool::new(false));
    let (_end, ended) = tokio::sync::oneshot::channel();
    let (video, _peer) = granted(ended, closed.clone());
    std::thread::sleep(Duration::from_millis(50));
    drop(video);
    wait_flag(&closed);
}

#[test]
fn a_remote_that_hangs_up_ends_the_preview_and_releases_the_session() {
    let closed = Arc::new(AtomicBool::new(false));
    let (_end, ended) = tokio::sync::oneshot::channel();
    let (video, peer) = granted(ended, closed.clone());
    drop(peer);
    let snapshot = wait_for(&video, |snapshot| {
        matches!(snapshot.state, VideoState::Ended(_))
    });
    assert!(
        matches!(snapshot.state, VideoState::Ended(Some(_))),
        "{snapshot:?}"
    );
    assert_eq!(video.take_frame(), None);
    wait_flag(&closed);
}

#[test]
fn a_shared_screen_keeps_one_frame_for_the_encoder_the_newest() {
    assert_eq!(Shared::new().capture(), (PREVIEW_MAX, PREVIEW_INTERVAL));
    let sending = ((1920, 1080), Duration::from_millis(15));
    let shared = Shared {
        sending: Some(sending),
        ..Shared::new()
    };
    assert_eq!(shared.capture(), sending);
    let frame = |shade| Frame {
        width: 2,
        height: 2,
        rgba: vec![shade; 16],
    };
    shared.keep(frame(1));
    shared.keep(frame(2));
    assert_eq!(shared.queue.dropped.load(Ordering::Relaxed), 1);
    assert_eq!(lock(&shared.raw).take(), Some(frame(2)));
    shared.keep(frame(3));
    shared.end(None);
    assert_eq!(lock(&shared.raw).take(), None, "ending lets go of it");
    shared.keep(frame(4));
    assert_eq!(lock(&shared.raw).take(), None, "nothing after the end");
    assert_eq!(lock(&shared.stats).replaced, 1, "said in the diagnostics");
}

#[test]
fn the_pace_keeps_a_jittery_source_at_its_rate_and_halves_one_twice_as_fast() {
    let start = Instant::now() + Duration::from_secs(1);
    let interval = Duration::from_secs(1) / 30;
    // Arrivals at 30 a second, each up to 4 ms early or late.
    let jitter = |n: u32| Duration::from_micros(u64::from(n * 7_919 % 8_000));
    let arrivals: Vec<Instant> = (1..=300)
        .map(|n| start + interval * n + jitter(n) - Duration::from_millis(4))
        .collect();
    let mut pace = Pace::default();
    let kept = arrivals
        .iter()
        .filter(|at| pace.keep(**at, interval))
        .count();
    assert_eq!(kept, 300, "every frame of a source at the rate asked");
    // Twice as fast: every other one, evenly.
    let mut pace = Pace::default();
    let fast: Vec<Instant> = (1..=300).map(|n| start + interval / 2 * n).collect();
    let kept: Vec<_> = fast.iter().filter(|at| pace.keep(**at, interval)).collect();
    assert!((149..=151).contains(&kept.len()), "{}", kept.len());
    assert!(
        kept.windows(2)
            .all(|pair| pair[1].duration_since(*pair[0]).abs_diff(interval)
                < Duration::from_micros(1))
    );
    // After a second without frames the next ones do not come in a burst.
    let mut pace = Pace::default();
    let resumed = start + Duration::from_secs(1);
    assert!(pace.keep(start, interval));
    assert!(pace.keep(resumed, interval));
    assert!(!pace.keep(resumed + Duration::from_millis(5), interval));
    assert!(pace.keep(resumed + interval, interval));
}

/// What scaling a laptop's screen to what is sent costs, per frame. In
/// release: `cargo test --release -p rune-media scaling_cost -- --ignored --nocapture`.
#[test]
#[ignore]
fn scaling_cost() {
    for ((width, height), max) in [
        ((2560, 1600), (1280, 720)),
        ((1920, 1080), (1280, 720)),
        ((2560, 1600), (1920, 1080)),
        ((1280, 720), (1280, 720)),
    ] {
        let bytes: Vec<u8> = (0..width * height * 4).map(|n| (n % 251) as u8).collect();
        let source = [plane(&bytes, width as usize * 4)];
        let runs = 60;
        let started = Instant::now();
        for _ in 0..runs {
            let frame = convert_within(Pixels::Bgr4, width, height, &source, max).unwrap();
            std::hint::black_box(frame);
        }
        println!(
            "{width}×{height} → dentro de {}×{}: {:.2} ms por cuadro",
            max.0,
            max.1,
            started.elapsed().as_secs_f64() * 1000.0 / f64::from(runs)
        );
    }
}
