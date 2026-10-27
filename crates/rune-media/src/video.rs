//! A local preview of a shared screen or a camera.
//!
//! The desktop portal grants the source in its own dialog; nothing is read
//! before it does. PipeWire then delivers frames on a thread of this
//! source's own. Each one kept is downscaled to at most 1280×720, at most 15
//! a second, into a queue of two that drops the oldest: the interface takes
//! the newest when it paints. Frames are never saved, sent or transmitted.
//!
//! Dropping the [`Video`] handle releases everything without waiting on the
//! caller's thread: the portal's request or session, the PipeWire stream and
//! its remote.
//!
//! A screen shared in a call ([`Video::share`]), or a camera sent to one
//! ([`Video::send_camera`]), is kept at the size and rate it is sent at
//! instead, for [`encode`]: one frame waits at most, the newest; the preview
//! gets them at the rate sent, at most 30 a second, scaled to 1280×720.
//!
//! Windows has no portal (windows-medios): RUNE's own picker lists the
//! screens and windows ([`sources`]), Windows.Graphics.Capture reads the one
//! chosen and Media Foundation the cameras, into the same queues as here.

use std::collections::VecDeque;
#[cfg(target_os = "linux")]
use std::future::Future;
#[cfg(target_os = "linux")]
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use rune_i18n::t;
#[cfg(target_os = "linux")]
use rune_platform::screencast::{self, CancelSignal, CaptureError, ParentWindow, ScreenCast};

/// Someone else's stream, decoded on its own thread.
pub mod decode;
/// The account's shared screen or camera, encoded on its own thread.
pub mod encode;
/// Who encodes and decodes: the GPU's engines, OpenH264 after them.
mod engine;
/// The GPU's H264 encoder and decoder, through GStreamer.
#[cfg(target_os = "linux")]
pub mod gpu;
#[cfg(target_os = "linux")]
mod pipewire;
/// The sound of the account's shared screen, from the other applications.
pub mod sound;
/// Room in a stream's SPS for the GPU's decoders (GStreamer's).
#[cfg(target_os = "linux")]
mod sps;
// Windows (windows-medios).
#[cfg(windows)]
mod windows;
/// No GPU's encoder or decoder on Windows yet: OpenH264 does it.
#[cfg(windows)]
pub use self::windows::gpu;
#[cfg(windows)]
pub use self::windows::sources;

/// The largest preview, in pixels; larger sources are scaled down to fit.
pub const PREVIEW_MAX: (u32, u32) = (1280, 720);
/// The shortest time between two preview frames: at most 15 a second.
pub const PREVIEW_INTERVAL: Duration = Duration::from_millis(66);
/// The shortest time between two preview frames of a shared screen or camera,
/// at most: 30 a second, or the rate sent when that is lower.
pub const SHARE_PREVIEW_INTERVAL: Duration = Duration::from_micros(33_333);
/// Frames waiting for the interface; older ones are dropped.
pub const QUEUE_FRAMES: usize = 2;
/// The largest source side accepted, in pixels.
const MAX_SOURCE_SIDE: u32 = 8192;

/// What is previewed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoKind {
    Screen,
    Camera,
}

/// How far a source got.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum VideoState {
    /// Waiting for the system's dialog, or for the first frame.
    #[default]
    Starting,
    Running,
    /// Over, with the reason when it was not stopped from RUNE.
    Ended(Option<String>),
}

/// A screen or a window to share, as RUNE's own picker offers it where the
/// system has no dialog of its own (Windows).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenSource {
    /// What to ask [`Video::share`] for it by, while it stays.
    pub id: String,
    pub name: String,
    /// A window, not a whole screen.
    pub window: bool,
}

/// The screens and windows RUNE's picker offers: none on Linux, where the
/// portal's dialog chooses.
#[cfg(target_os = "linux")]
pub fn sources() -> Vec<ScreenSource> {
    Vec::new()
}

/// A camera the system offers, as the portal's remote shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraInfo {
    /// What to ask for it by; stable while it stays connected.
    pub id: String,
    pub name: String,
}

/// Coherent state of one source. No pixels.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VideoSnapshot {
    pub state: VideoState,
    /// The cameras the remote shows; empty for a screen.
    pub cameras: Vec<CameraInfo>,
    /// Frames queued for the preview, and those dropped unseen.
    pub frames: u64,
    pub dropped: u64,
    /// The portal's token to share the same screen again without its
    /// picker, once it gave one: kept with the preferences, never a
    /// credential.
    pub restore_token: Option<String>,
}

/// One preview frame: straight RGBA rows, top to bottom, opaque.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The newest frames, bounded: pushing past the bound drops the oldest.
#[derive(Default)]
struct FrameQueue {
    frames: Mutex<VecDeque<Frame>>,
    queued: AtomicU64,
    dropped: AtomicU64,
}

impl FrameQueue {
    fn push(&self, frame: Frame) {
        let mut frames = lock(&self.frames);
        if frames.len() >= QUEUE_FRAMES {
            frames.pop_front();
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        frames.push_back(frame);
        self.queued.fetch_add(1, Ordering::Relaxed);
    }

    /// The newest frame; the ones before it were never going to be shown.
    fn newest(&self) -> Option<Frame> {
        let mut frames = lock(&self.frames);
        let newest = frames.pop_back();
        self.dropped
            .fetch_add(frames.len() as u64, Ordering::Relaxed);
        frames.clear();
        newest
    }

    fn clear(&self) {
        lock(&self.frames).clear();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Shared {
    state: Mutex<VideoState>,
    cameras: Mutex<Vec<CameraInfo>>,
    queue: FrameQueue,
    stop: AtomicBool,
    /// Kept for sending: the largest frame and the least time between two.
    sending: Option<((u32, u32), Duration)>,
    /// The newest frame kept for sending, until the encoder takes it.
    raw: Mutex<Option<Frame>>,
    /// The portal's restore token of the screen chosen.
    restore: Mutex<Option<String>>,
    /// What the source delivers, for the share's diagnostic line.
    stats: Mutex<CaptureStats>,
    /// Woken with each preview frame, so the interface paints it then.
    wake: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

/// What a source delivered, counted since it started: for the shared
/// screen's diagnostic line (`rune: voice share: video …`). No pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CaptureStats {
    /// The size PipeWire negotiated, its pixel layout and how its buffers
    /// come (`memfd`, `memptr` or `dmabuf`); empty until it negotiated.
    pub size: (u32, u32),
    pub format: &'static str,
    pub memory: &'static str,
    /// Its frame rate and the most it may go to, as fractions: `0/1` is a
    /// source that sends only when something changes.
    pub rate: (u32, u32),
    pub max_rate: (u32, u32),
    /// Buffers PipeWire delivered, and the frames kept from them.
    pub delivered: u64,
    pub kept: u64,
    /// Time converting and scaling the kept frames, in microseconds.
    pub converting_micros: u64,
    /// Frames kept for sending that a newer one replaced before the
    /// encoder took them: it, or the network behind it, did not keep up.
    pub replaced: u64,
}

/// Which frames of a source to keep, one per `interval` on average: a
/// source at that very rate keeps every frame though its arrivals jitter,
/// one twice as fast every other, and none comes in a burst after a stall.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Pace {
    due: Option<Instant>,
}

impl Pace {
    pub(crate) fn keep(&mut self, now: Instant, interval: Duration) -> bool {
        // A third of an interval early still counts as on time; half is
        // the next frame of a source twice as fast.
        if self.due.is_some_and(|due| now + interval / 3 < due) {
            return false;
        }
        // Late by more than an interval: the rhythm starts again from now.
        let base = match self.due {
            Some(due) if now <= due + interval => due,
            _ => now,
        };
        self.due = Some(base + interval);
        true
    }
}

impl Shared {
    fn new() -> Self {
        Self {
            state: Mutex::new(VideoState::Starting),
            cameras: Mutex::new(Vec::new()),
            queue: FrameQueue::default(),
            stop: AtomicBool::new(false),
            sending: None,
            raw: Mutex::new(None),
            restore: Mutex::new(None),
            stats: Mutex::new(CaptureStats::default()),
            wake: OnceLock::new(),
        }
    }

    /// The largest frame kept and the least time between two.
    fn capture(&self) -> ((u32, u32), Duration) {
        self.sending.unwrap_or((PREVIEW_MAX, PREVIEW_INTERVAL))
    }

    /// A frame kept for sending; one that was never taken is dropped.
    fn keep(&self, frame: Frame) {
        if self.stopped() {
            return;
        }
        if lock(&self.raw).replace(frame).is_some() {
            self.queue.dropped.fetch_add(1, Ordering::Relaxed);
            lock(&self.stats).replaced += 1;
        }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    /// The first end wins; nothing is queued after it.
    fn end(&self, reason: Option<String>) {
        self.stop.store(true, Ordering::Release);
        let mut state = lock(&self.state);
        if !matches!(*state, VideoState::Ended(_)) {
            *state = VideoState::Ended(reason);
        }
        drop(state);
        self.queue.clear();
        lock(&self.raw).take();
    }

    fn push(&self, frame: Frame) {
        if self.stopped() {
            return;
        }
        let mut state = lock(&self.state);
        if *state == VideoState::Starting {
            *state = VideoState::Running;
        }
        drop(state);
        self.queue.push(frame);
        if let Some(wake) = self.wake.get() {
            wake();
        }
    }

    fn running(&self) -> bool {
        *lock(&self.state) == VideoState::Running
    }

    /// Registers the wake of the preview: see [`Video::on_frame`].
    fn on_frame(&self, wake: impl Fn() + Send + Sync + 'static) {
        let _ = self.wake.set(Box::new(wake));
    }

    /// A frame delivered at `now`: counted and, if `keeper` keeps it,
    /// converted by `make` within the size kept and handed to the preview
    /// and, while sending, to the encoder. A preview while sending follows
    /// the rate sent, at most 30 a second, and is scaled to [`PREVIEW_MAX`].
    pub(crate) fn deliver(
        &self,
        keeper: &mut Keeper,
        now: Instant,
        mut make: impl FnMut((u32, u32)) -> Option<Frame>,
    ) {
        lock(&self.stats).delivered += 1;
        let (max, interval) = self.capture();
        if !keeper.pace.keep(now, interval) {
            return;
        }
        let Some(frame) = make(max) else {
            return;
        };
        {
            let mut stats = lock(&self.stats);
            stats.kept += 1;
            stats.converting_micros += now.elapsed().as_micros() as u64;
        }
        let Some((_, sending)) = self.sending else {
            return self.push(frame);
        };
        if keeper
            .preview
            .keep(now, sending.max(SHARE_PREVIEW_INTERVAL))
        {
            let fits = frame.width <= PREVIEW_MAX.0 && frame.height <= PREVIEW_MAX.1;
            let preview = if fits {
                Some(frame.clone())
            } else {
                make(PREVIEW_MAX)
            };
            if let Some(preview) = preview {
                self.push(preview);
            }
        }
        self.keep(frame);
    }
}

/// What a source's reader keeps from frame to frame: which frames to keep,
/// and which ones went to the preview while sending.
#[derive(Default)]
pub(crate) struct Keeper {
    pace: Pace,
    preview: Pace,
}

/// A portal session that outlives the request: the system can end it.
#[cfg(target_os = "linux")]
trait Session: Send {
    /// Why the system ended it.
    fn ended(&mut self) -> Pin<Box<dyn Future<Output = String> + Send + '_>>;
    /// Release it.
    fn close(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send>>;
}

#[cfg(target_os = "linux")]
impl Session for ScreenCast {
    fn ended(&mut self) -> Pin<Box<dyn Future<Output = String> + Send + '_>> {
        Box::pin(async move { ScreenCast::ended(self).await.to_string() })
    }

    fn close(self: Box<Self>) -> Pin<Box<dyn Future<Output = ()> + Send>> {
        Box::pin(async move {
            let mut cast = *self;
            ScreenCast::close(&mut cast).await;
        })
    }
}

/// What the portal granted.
#[cfg(target_os = "linux")]
struct Source {
    remote: std::os::fd::OwnedFd,
    /// The node to read; `None` lets PipeWire pick (a camera).
    node: Option<u32>,
    session: Option<Box<dyn Session>>,
    /// The portal's token to share the same choice again.
    restore: Option<String>,
}

#[cfg(target_os = "linux")]
type Opening = Pin<Box<dyn Future<Output = Result<Source, CaptureError>> + Send>>;

/// One previewed source. Creating it asks the system at once; dropping it
/// releases the source. Nothing here blocks the caller.
pub struct Video {
    shared: Arc<Shared>,
    /// Dropped with the handle: that cancels whatever the worker awaits.
    #[cfg(target_os = "linux")]
    _cancel: tokio::sync::watch::Sender<bool>,
}

#[cfg(target_os = "linux")]
impl Video {
    /// Ask the system to share a monitor or a window: its picker chooses,
    /// unless `restore` names a choice it remembers.
    pub fn screen(parent: Option<ParentWindow>, restore: Option<String>) -> Self {
        Self::spawn(VideoKind::Screen, None, None, screen(parent, restore))
    }

    /// Ask the system for a screen to send: kept within `max` pixels, at
    /// most `fps` a second, for [`Video::frames`]; previewed as well.
    pub fn share(
        parent: Option<ParentWindow>,
        restore: Option<String>,
        max: (u32, u32),
        fps: u32,
    ) -> Self {
        Self::spawn(
            VideoKind::Screen,
            None,
            Some(sending(max, fps)),
            screen(parent, restore),
        )
    }

    /// Ask the system for the cameras and preview `device`, one of the
    /// [`CameraInfo::id`]s it showed before, or the system's choice.
    pub fn camera(device: Option<String>) -> Self {
        Self::camera_kept(device, None)
    }

    /// [`Video::camera`], to send: kept within `max` pixels, at most `fps`
    /// a second, for [`Video::frames`]; previewed as well.
    pub fn send_camera(device: Option<String>, max: (u32, u32), fps: u32) -> Self {
        Self::camera_kept(device, Some(sending(max, fps)))
    }

    fn camera_kept(device: Option<String>, sending: Option<((u32, u32), Duration)>) -> Self {
        Self::spawn(VideoKind::Camera, device, sending, |cancel| {
            Box::pin(async move {
                Ok(Source {
                    remote: screencast::open_camera(cancel).await?,
                    node: None,
                    session: None,
                    restore: None,
                })
            })
        })
    }

    fn spawn(
        kind: VideoKind,
        device: Option<String>,
        sending: Option<((u32, u32), Duration)>,
        open: impl FnOnce(CancelSignal) -> Opening + Send + 'static,
    ) -> Self {
        let shared = Arc::new(Shared {
            sending,
            ..Shared::new()
        });
        let (cancel, signal) = tokio::sync::watch::channel(false);
        let worker = shared.clone();
        let spawned = std::thread::Builder::new()
            .name("rune-video".into())
            .spawn(move || work(worker, kind, device, open, signal));
        if spawned.is_err() {
            shared.end(Some(t("No se pudo iniciar la vista previa.").into()));
        }
        Self {
            shared,
            _cancel: cancel,
        }
    }
}

impl Video {
    /// The frames kept for sending, the newest only, from any thread; nothing
    /// for a preview.
    pub fn frames(&self) -> impl FnMut() -> Option<Frame> + Send + 'static {
        let shared = self.shared.clone();
        move || lock(&shared.raw).take()
    }

    pub fn snapshot(&self) -> VideoSnapshot {
        VideoSnapshot {
            state: lock(&self.shared.state).clone(),
            cameras: lock(&self.shared.cameras).clone(),
            frames: self.shared.queue.queued.load(Ordering::Relaxed),
            dropped: self.shared.queue.dropped.load(Ordering::Relaxed),
            restore_token: lock(&self.shared.restore).clone(),
        }
    }

    /// Calls `wake` from the source's thread with each preview frame queued;
    /// set once, the first call wins. The host repaints then, not by polling.
    pub fn on_frame(&self, wake: impl Fn() + Send + Sync + 'static) {
        self.shared.on_frame(wake);
    }

    /// The newest frame since the last call, if any.
    pub fn take_frame(&self) -> Option<Frame> {
        self.shared.queue.newest()
    }

    /// What the source delivered so far.
    pub fn capture_stats(&self) -> CaptureStats {
        *lock(&self.shared.stats)
    }
}

impl Drop for Video {
    fn drop(&mut self) {
        self.shared.end(None);
    }
}

/// Frames kept for sending within `max`, at most `fps` a second.
fn sending(max: (u32, u32), fps: u32) -> ((u32, u32), Duration) {
    (max, Duration::from_secs(1) / fps.clamp(1, 60))
}

/// How a screen is asked for: the portal's picker chooses, or the choice
/// `restore` names.
#[cfg(target_os = "linux")]
fn screen(
    parent: Option<ParentWindow>,
    restore: Option<String>,
) -> impl FnOnce(CancelSignal) -> Opening + Send + 'static {
    move |cancel| {
        Box::pin(async move {
            let (cast, remote) =
                ScreenCast::start(parent.as_ref(), restore.as_deref(), cancel).await?;
            let node = cast
                .streams()
                .first()
                .map(|stream| stream.node)
                .ok_or(CaptureError::InvalidResponse)?;
            Ok(Source {
                remote,
                node: Some(node),
                restore: cast.restore_token().map(str::to_owned),
                session: Some(Box::new(cast)),
            })
        })
    }
}

/// The source's worker: ask the portal, read the stream, and release both
/// on whichever end comes first.
#[cfg(target_os = "linux")]
fn work(
    shared: Arc<Shared>,
    kind: VideoKind,
    device: Option<String>,
    open: impl FnOnce(CancelSignal) -> Opening,
    mut cancel: CancelSignal,
) {
    let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    else {
        return shared.end(Some(t("No se pudo iniciar la vista previa.").into()));
    };
    runtime.block_on(async move {
        // Not raced against cancellation: the request answers it by closing
        // its dialog and session itself.
        let opened = open(cancel.clone()).await;
        let Source {
            remote,
            node,
            mut session,
            restore,
        } = match opened {
            Ok(source) => source,
            Err(_) if *cancel.borrow() || cancel.has_changed().is_err() => return shared.end(None),
            Err(error) => return shared.end(Some(error.to_string())),
        };
        *lock(&shared.restore) = restore;
        let (done, finished) = tokio::sync::oneshot::channel::<()>();
        let reader = shared.clone();
        let stream = std::thread::Builder::new()
            .name("rune-video-pw".into())
            .spawn(move || {
                let result = pipewire::read(remote, node, kind, device, &reader);
                reader.end(result.err());
                drop(done);
            });
        let stream = match stream {
            Ok(stream) => Some(stream),
            Err(_) => {
                shared.end(Some(t("No se pudo iniciar la vista previa.").into()));
                None
            }
        };
        if stream.is_some() {
            let ended = async {
                match session.as_mut() {
                    Some(session) => session.ended().await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                _ = cancelled(&mut cancel) => shared.end(None),
                reason = ended => shared.end(Some(reason)),
                _ = finished => {}
            }
        }
        shared.stop.store(true, Ordering::Release);
        if let Some(session) = session {
            session.close().await;
        }
        if let Some(stream) = stream {
            // It quits within one check of the stop flag.
            let _ = stream.join();
        }
    });
}

#[cfg(target_os = "linux")]
async fn cancelled(cancel: &mut CancelSignal) {
    while !*cancel.borrow() {
        if cancel.changed().await.is_err() {
            return;
        }
    }
}

/// Pixel layouts the preview reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pixels {
    /// Four bytes a pixel, blue first; the fourth is ignored.
    Bgr4,
    /// Four bytes a pixel, red first; the fourth is ignored.
    Rgb4,
    /// Packed 4:2:2: Y0 U Y1 V for each two pixels.
    Yuy2,
    /// A plane of Y, then one of interleaved U and V at half resolution.
    Nv12,
    /// Planes of Y, U and V, the last two at half resolution: decoded H264.
    I420,
}

/// One plane of a source frame, as mapped.
struct Plane<'a> {
    bytes: &'a [u8],
    stride: usize,
}

/// `width` by `height` within `max` ([`PREVIEW_MAX`] for a preview), same
/// shape, never larger.
fn fit(width: u32, height: u32, max: (u32, u32)) -> (u32, u32) {
    let scale = (max.0 as f64 / width as f64)
        .min(max.1 as f64 / height as f64)
        .min(1.0);
    let fit = |side: u32| ((side as f64 * scale).round() as u32).max(1);
    (fit(width), fit(height))
}

/// Whether `plane` holds `rows` rows of `row` bytes at its stride.
fn holds(plane: &Plane, rows: usize, row: usize) -> bool {
    plane.stride >= row
        && rows > 0
        && plane
            .stride
            .checked_mul(rows - 1)
            .and_then(|start| start.checked_add(row))
            .is_some_and(|end| end <= plane.bytes.len())
}

/// BT.601 limited range, as cameras send it.
fn rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    let c = 298 * (i32::from(y) - 16);
    let d = i32::from(u) - 128;
    let e = i32::from(v) - 128;
    let clamp = |value: i32| ((value + 128) >> 8).clamp(0, 255) as u8;
    [
        clamp(c + 409 * e),
        clamp(c - 100 * d - 208 * e),
        clamp(c + 516 * d),
    ]
}

/// The preview of one source frame: sampled to fit [`PREVIEW_MAX`], nearest
/// pixel, as opaque RGBA. `None` when the planes do not hold the frame.
#[cfg(test)]
fn convert(pixels: Pixels, width: u32, height: u32, planes: &[Plane]) -> Option<Frame> {
    convert_within(pixels, width, height, planes, PREVIEW_MAX)
}

/// [`convert`], fit within `max` instead.
fn convert_within(
    pixels: Pixels,
    width: u32,
    height: u32,
    planes: &[Plane],
    max: (u32, u32),
) -> Option<Frame> {
    if !(1..=MAX_SOURCE_SIDE).contains(&width) || !(1..=MAX_SOURCE_SIDE).contains(&height) {
        return None;
    }
    let (w, h) = (width as usize, height as usize);
    let main = planes.first()?;
    let valid = match pixels {
        Pixels::Bgr4 | Pixels::Rgb4 => holds(main, h, w * 4),
        Pixels::Yuy2 => holds(main, h, w.div_ceil(2) * 4),
        Pixels::Nv12 => {
            holds(main, h, w) && holds(planes.get(1)?, h.div_ceil(2), w.div_ceil(2) * 2)
        }
        Pixels::I420 => {
            holds(main, h, w)
                && holds(planes.get(1)?, h.div_ceil(2), w.div_ceil(2))
                && holds(planes.get(2)?, h.div_ceil(2), w.div_ceil(2))
        }
    };
    if !valid {
        return None;
    }
    let (out_w, out_h) = fit(width, height, max);
    let columns: Vec<usize> = (0..out_w as usize)
        .map(|x| x * w / out_w as usize)
        .collect();
    let mut rgba = Vec::with_capacity(out_w as usize * out_h as usize * 4);
    for y in 0..out_h as usize {
        let sy = y * h / out_h as usize;
        let row = &main.bytes[sy * main.stride..];
        for &sx in &columns {
            let [r, g, b] = match pixels {
                Pixels::Bgr4 => [row[sx * 4 + 2], row[sx * 4 + 1], row[sx * 4]],
                Pixels::Rgb4 => [row[sx * 4], row[sx * 4 + 1], row[sx * 4 + 2]],
                Pixels::Yuy2 => {
                    let pair = sx / 2 * 4;
                    rgb(row[pair + sx % 2 * 2], row[pair + 1], row[pair + 3])
                }
                Pixels::Nv12 => {
                    let chroma = &planes[1];
                    let at = sy / 2 * chroma.stride + sx / 2 * 2;
                    rgb(row[sx], chroma.bytes[at], chroma.bytes[at + 1])
                }
                Pixels::I420 => {
                    let at = |plane: &Plane| plane.bytes[sy / 2 * plane.stride + sx / 2];
                    rgb(row[sx], at(&planes[1]), at(&planes[2]))
                }
            };
            rgba.extend_from_slice(&[r, g, b, 255]);
        }
    }
    Some(Frame {
        width: out_w,
        height: out_h,
        rgba,
    })
}

/// One camera's JPEG picture within `max`. A camera's MJPEG often leaves
/// out the Huffman tables when they are JPEG's standard ones (MJPEG in AVI
/// does too): its `AVI1` mark has the decoder fill in those missing.
fn jpeg_frame(jpeg: &[u8], max: (u32, u32)) -> Option<Frame> {
    use zune_core::{bytestream::ZCursor, colorspace::ColorSpace, options::DecoderOptions};
    let rest = jpeg.strip_prefix(&[0xFF, 0xD8])?;
    let mut marked = Vec::with_capacity(jpeg.len() + 9);
    marked.extend_from_slice(b"\xFF\xD8\xFF\xE0\x00\x07AVI1\x00");
    marked.extend_from_slice(rest);
    let side = MAX_SOURCE_SIDE as usize;
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGBA)
        .set_max_width(side)
        .set_max_height(side);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(&marked[..]), options);
    let rgba = decoder.decode().ok()?;
    let (width, height) = decoder.dimensions()?;
    let plane = Plane {
        bytes: &rgba,
        stride: width * 4,
    };
    convert_within(Pixels::Rgb4, width as u32, height as u32, &[plane], max)
}

#[cfg(test)]
mod tests;
