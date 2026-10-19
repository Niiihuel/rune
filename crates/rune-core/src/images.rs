//! Credential-free artwork and attachment previews, separate from account HTTP.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::Cursor,
    sync::Arc,
    time::{Duration, Instant},
};

use base64::Engine as _;
use image::{ImageFormat, ImageReader};
use rune_model::{ImageKind, RemoteImage};
use sha2::{Digest, Sha256};
use tokio::{
    runtime::Handle,
    sync::{mpsc, Semaphore},
    task::JoinHandle,
};

const MAX_BYTES: usize = 1024 * 1024;
// Stickers ignore the CDN size parameter and commonly arrive at 320 px.
const MAX_SIDE: u32 = 512;
const THUMBNAIL: u32 = 128;
const PREVIEW_BYTES: usize = 8 * 1024 * 1024;
const PREVIEW_SIDE: u32 = 4096;
const PREVIEW_WIDTH: u32 = 768;
const PREVIEW_HEIGHT: u32 = 512;
const BANNER_SIDE: u32 = 2048;
const BANNER_PIXELS: u64 = 2 * 1024 * 1024;
const BANNER_WIDTH: u32 = 512;
const BANNER_HEIGHT: u32 = 256;
const PREVIEW_CONCURRENT: usize = 2;
/// The longer side of a blurred preview (contenido-de-mensajes).
const OBSCURED_SIDE: u32 = 24;
/// ThumbHashes decoded in one `load`: each is decoded and encoded on the
/// caller's thread, so a channel full of them spreads over a few frames.
const THUMBHASHES_PER_LOAD: usize = 8;
const CONCURRENT: usize = 4;
pub const IMAGE_ENTRIES: usize = 256;
/// GPU textures have a separate bound from the encoded-byte cache. At the
/// largest preview size, 64 RGBA textures occupy at most 96 MiB of pixels.
pub const IMAGE_TEXTURE_ENTRIES: usize = 64;
const CACHE_BYTES: usize = 16 * 1024 * 1024;
const RETRY_DELAY: Duration = Duration::from_secs(30);
/// Decoded animation frames across the whole cache; past it the least
/// recently wanted previews keep only their first frame.
const ANIMATIONS_BYTES: usize = 64 * 1024 * 1024;

mod animation;
pub use animation::{Animation, AnimationFrame};
/// Animated vector (Lottie) stickers.
mod lottie;
// Message content (contenido-de-mensajes).
mod thumbhash;
// Pictures on disk between runs (imagenes-y-lectura).
pub(crate) mod disk;
pub use disk::{CacheLimits, CachePart, CacheUsage};

/// How [`ImageData::base64`] is encoded: a normalized PNG, or a picture at
/// its own size kept as its file was.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Encoding {
    #[default]
    Png,
    Jpeg,
    Webp,
    Gif,
}

/// A validated, normalized PNG for an existing native UI image component.
#[derive(Clone)]
pub struct ImageData {
    pub sha256: String,
    pub size_bytes: u32,
    pub width: u32,
    pub height: u32,
    pub base64: Arc<str>,
    /// Frames of an animated GIF or WebP, or of a clip, at this preview's size.
    pub animation: Option<Arc<Animation>>,
    pub format: Encoding,
}

impl std::fmt::Debug for ImageData {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImageData")
            .field("sha256", &self.sha256)
            .field("size_bytes", &self.size_bytes)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("animation", &self.animation)
            .finish_non_exhaustive()
    }
}

struct Completed {
    epoch: u64,
    source: RemoteImage,
    data: Option<Arc<ImageData>>,
    retry: Option<Duration>,
    origin_cooldown: bool,
}

struct DownloadError {
    retry: Option<Duration>,
    origin_cooldown: bool,
}

impl DownloadError {
    fn permanent() -> Self {
        Self {
            retry: None,
            origin_cooldown: false,
        }
    }

    fn transient() -> Self {
        Self {
            retry: Some(RETRY_DELAY),
            origin_cooldown: false,
        }
    }
}

pub(crate) struct Images {
    #[cfg(test)]
    pub(crate) loopback: bool,
    runtime: Handle,
    client: Option<reqwest::Client>,
    origins: &'static [&'static str],
    wake: Arc<dyn Fn() + Send + Sync>,
    gate: Arc<Semaphore>,
    preview_gate: Arc<Semaphore>,
    sender: mpsc::Sender<Completed>,
    results: mpsc::Receiver<Completed>,
    pending: HashMap<RemoteImage, JoinHandle<()>>,
    cache: HashMap<RemoteImage, Option<Arc<ImageData>>>,
    order: VecDeque<RemoteImage>,
    bytes: usize,
    epoch: u64,
    failures: HashMap<RemoteImage, Instant>,
    // Server-directed backoff belongs to an allowed origin, not an evictable
    // artwork entry. None means wait until the next local account session.
    cooldowns: HashMap<String, Option<Instant>>,
    retry_at: Option<Instant>,
    // Profile editing (edicion-de-perfil).
    /// The provider, for the pictures it keeps itself (`hosted_image`).
    pub(crate) hosted: Option<Arc<dyn rune_provider::Provider>>,
    // Pictures on disk between runs (imagenes-y-lectura).
    /// The account's pictures kept from earlier runs, read before asking
    /// the network and written once a download proved to be a picture.
    pub(crate) disk: disk::DiskCache,
}

impl Images {
    pub fn new(
        runtime: Handle,
        origins: &'static [&'static str],
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (sender, results) = mpsc::channel(CONCURRENT * 2);
        Self {
            #[cfg(test)]
            loopback: false,
            disk: disk::DiskCache::new(runtime.clone(), wake.clone()),
            runtime,
            origins,
            wake,
            client: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(15))
                .user_agent(concat!(
                    "RUNE/",
                    env!("CARGO_PKG_VERSION"),
                    " public images"
                ))
                .build()
                .ok(),
            gate: Arc::new(Semaphore::new(CONCURRENT)),
            preview_gate: Arc::new(Semaphore::new(PREVIEW_CONCURRENT)),
            sender,
            results,
            pending: HashMap::new(),
            cache: HashMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            epoch: 0,
            failures: HashMap::new(),
            cooldowns: HashMap::new(),
            retry_at: None,
            hosted: None,
        }
    }

    /// Poll completed work, retain only the bounded requested set and start
    /// at most four requests. A full queue leaves initials until a later frame.
    pub fn load(&mut self, wanted: &[RemoteImage]) -> Vec<(RemoteImage, Arc<ImageData>)> {
        while let Ok(result) = self.results.try_recv() {
            if result.epoch != self.epoch {
                continue;
            }
            self.pending.remove(&result.source);
            if result.origin_cooldown {
                if let Some(url) = self.url(&result.source) {
                    let at = result
                        .retry
                        .and_then(|delay| Instant::now().checked_add(delay));
                    self.cooldowns
                        .entry(url.origin().ascii_serialization())
                        .and_modify(|old| {
                            *old = match (*old, at) {
                                (Some(old), Some(at)) => Some(old.max(at)),
                                _ => None,
                            }
                        })
                        .or_insert(at);
                }
                continue;
            }
            if let Some(delay) = result.retry {
                if let Some(at) = Instant::now().checked_add(delay) {
                    self.failures.insert(result.source.clone(), at);
                }
            }
            self.insert(result.source, result.data);
        }
        let mut unique = HashSet::new();
        let wanted: Vec<_> = wanted
            .iter()
            // A malformed persisted descriptor must not expand the cache's
            // bounded key storage before URL validation rejects it.
            .filter(|source| source.url.len() <= 2048)
            .filter(|source| unique.insert((*source).clone()))
            .take(IMAGE_ENTRIES)
            .collect();
        // Stop downloads which no currently displayed component needs. Decodes
        // already running keep the semaphore until their bounded work finishes.
        self.pending.retain(|source, task| {
            if wanted.contains(&source) {
                true
            } else {
                task.abort();
                false
            }
        });
        let mut ready = Vec::new();
        self.retry_at = None;
        let mut thumbhashes = 0;
        // On disk only for the account's own session; while its disk opens,
        // nothing it may keep is asked of the network (imagenes-y-lectura).
        let (disk, opening) = if self.disk.trusted {
            (self.disk.current(), self.disk.opening())
        } else {
            (None, false)
        };
        for source in wanted {
            if let Some(at) = self.failures.get(source).copied() {
                if at <= Instant::now() {
                    self.failures.remove(source);
                    self.cache.remove(source);
                    self.order.retain(|old| old != source);
                } else {
                    self.retry_at = Some(self.retry_at.map_or(at, |old| old.min(at)));
                }
            }
            if let Some(data) = self.cache.get(source) {
                self.order.retain(|old| old != source);
                self.order.push_back(source.clone());
                if let Some(data) = data {
                    ready.push((source.clone(), data.clone()));
                }
                continue;
            }
            // A ThumbHash is decoded here at once: there is nothing to fetch.
            if source.kind == ImageKind::ThumbHash {
                thumbhashes += 1;
                if thumbhashes > THUMBHASHES_PER_LOAD {
                    // The rest, the next frame.
                    if thumbhashes == THUMBHASHES_PER_LOAD + 1 {
                        (self.wake)();
                    }
                    continue;
                }
                let data = thumbhash::image(&source.url).map(Arc::new);
                ready.extend(data.clone().map(|data| (source.clone(), data)));
                self.insert(source.clone(), data);
                continue;
            }
            if opening || self.pending.contains_key(source) || self.pending.len() >= CONCURRENT {
                continue;
            }
            if source.kind != ImageKind::Artwork
                && self
                    .pending
                    .keys()
                    .filter(|key| key.kind != ImageKind::Artwork)
                    .count()
                    >= PREVIEW_CONCURRENT
            {
                continue;
            }
            // Kept by the provider itself: read as a download would be, as
            // large at most as a profile picture made on this device.
            if let Some(bytes) = self
                .hosted
                .as_ref()
                .and_then(|provider| provider.hosted_image(&source.url))
                .filter(|bytes| bytes.len() <= rune_provider::ProfileImage::MAX_ANIMATED_BYTES)
            {
                let (source, sender, wake, epoch) = (
                    source.clone(),
                    self.sender.clone(),
                    self.wake.clone(),
                    self.epoch,
                );
                let key = source.clone();
                let task = self.runtime.spawn(async move {
                    let kind = source.kind;
                    let data =
                        tokio::task::spawn_blocking(move || decoded(&bytes, kind).map(Arc::new))
                            .await
                            .ok()
                            .flatten();
                    let done = Completed {
                        epoch,
                        source,
                        data,
                        retry: None,
                        origin_cooldown: false,
                    };
                    if sender.send(done).await.is_ok() {
                        wake();
                    }
                });
                self.pending.insert(key, task);
                continue;
            }
            let Some(url) = self.url(source) else {
                self.insert(source.clone(), None);
                continue;
            };
            let origin = url.origin().ascii_serialization();
            if let Some(at) = self.cooldowns.get(&origin).copied() {
                if let Some(at) = at {
                    if at <= Instant::now() {
                        self.cooldowns.remove(&origin);
                    } else {
                        self.retry_at = Some(self.retry_at.map_or(at, |old| old.min(at)));
                        continue;
                    }
                } else {
                    continue;
                }
            }
            let Some(client) = self.client.clone() else {
                continue;
            };
            let (source, sender, wake, gate, preview_gate, epoch) = (
                source.clone(),
                self.sender.clone(),
                self.wake.clone(),
                self.gate.clone(),
                self.preview_gate.clone(),
                self.epoch,
            );
            let key = source.clone();
            let disk = disk.clone();
            let task = self.runtime.spawn(async move {
                let preview_permit = if source.kind != ImageKind::Artwork {
                    let Ok(permit) = preview_gate.acquire_owned().await else {
                        return;
                    };
                    Some(permit)
                } else {
                    None
                };
                let Ok(permit) = gate.acquire_owned().await else {
                    return;
                };
                let kind = source.kind;
                let path = url.path().to_owned();
                // Kept from an earlier run: the network is not asked.
                let kept = match disk.clone() {
                    Some(disk) => {
                        let wanted = source.url.clone();
                        tokio::task::spawn_blocking(move || disk.lock().ok()?.read(kind, &wanted))
                            .await
                            .ok()
                            .flatten()
                    }
                    None => None,
                };
                let fetched = kept.is_none();
                let bytes = match kept {
                    Some(bytes) => Ok(bytes),
                    None => download(&client, url, byte_limit(kind)).await,
                };
                let (data, retry, origin_cooldown) = match bytes {
                    Ok(bytes) => {
                        let wanted = source.url.clone();
                        (
                            tokio::task::spawn_blocking(move || {
                                let _permit = permit;
                                let _preview_permit = preview_permit;
                                let data = normalize_at(&path, &bytes, kind).map(Arc::new);
                                // Kept for the next run once it proved to be a picture.
                                if let (true, Some(_), Some(disk)) = (fetched, &data, disk) {
                                    if let Ok(mut disk) = disk.lock() {
                                        disk.write(kind, &wanted, &bytes);
                                    }
                                }
                                data
                            })
                            .await
                            .ok()
                            .flatten(),
                            None,
                            false,
                        )
                    }
                    Err(error) => (None, error.retry, error.origin_cooldown),
                };
                if sender
                    .send(Completed {
                        epoch,
                        source,
                        data,
                        retry,
                        origin_cooldown,
                    })
                    .await
                    .is_ok()
                {
                    wake();
                }
            });
            self.pending.insert(key, task);
        }
        ready
    }

    fn url(&self, source: &RemoteImage) -> Option<url::Url> {
        let loopback = false;
        #[cfg(test)]
        let loopback = loopback || self.loopback;
        validated_resource_url(&source.url, self.origins, loopback)
    }

    /// Failed or currently backed-off visible sources. No transport error or
    /// signed URL is formatted for display, and pending/ready sources are absent.
    pub fn failures(&self, wanted: &[RemoteImage]) -> HashSet<RemoteImage> {
        wanted
            .iter()
            .take(IMAGE_ENTRIES)
            .filter(|source| {
                if matches!(self.cache.get(*source), Some(Some(_))) {
                    return false;
                }
                if self.client.is_none() || source.url.len() > 2048 {
                    return true;
                }
                let Some(url) = self.url(source) else {
                    return true;
                };
                matches!(self.cache.get(*source), Some(None))
                    || self
                        .cooldowns
                        .get(&url.origin().ascii_serialization())
                        .is_some_and(|at| at.is_none_or(|at| at > Instant::now()))
            })
            .cloned()
            .collect()
    }

    fn insert(&mut self, source: RemoteImage, data: Option<Arc<ImageData>>) {
        // One picture at its own size at a time, kept apart from the
        // previews' budget so that neither pushes the other out.
        if source.kind == ImageKind::Original {
            let others: Vec<_> = self
                .cache
                .keys()
                .filter(|key| key.kind == ImageKind::Original && **key != source)
                .cloned()
                .collect();
            for other in others {
                self.cache.remove(&other);
                self.order.retain(|old| old != &other);
                self.failures.remove(&other);
            }
        }
        if let Some(Some(old)) = self.cache.remove(&source) {
            self.bytes -= weight(&source, &old);
        }
        self.order.retain(|old| old != &source);
        self.bytes += data.as_ref().map_or(0, |data| weight(&source, data));
        self.cache.insert(source.clone(), data);
        self.order.push_back(source);
        while self.cache.len() > IMAGE_ENTRIES || self.bytes > CACHE_BYTES {
            if let Some(old) = self.order.pop_front() {
                self.failures.remove(&old);
                if let Some(Some(data)) = self.cache.remove(&old) {
                    self.bytes -= weight(&old, &data);
                }
            }
        }
        let mut animated: usize = self
            .cache
            .values()
            .flatten()
            .filter_map(|data| data.animation.as_ref())
            .map(|animation| animation.bytes())
            .sum();
        for old in &self.order {
            if animated <= ANIMATIONS_BYTES {
                break;
            }
            if let Some(Some(data)) = self.cache.get_mut(old) {
                if let Some(animation) = &data.animation {
                    animated -= animation.bytes();
                    *data = Arc::new(ImageData {
                        animation: None,
                        ..(**data).clone()
                    });
                }
            }
        }
    }

    pub fn clear(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        for (_, task) in self.pending.drain() {
            task.abort();
        }
        self.cache.clear();
        self.order.clear();
        self.bytes = 0;
        self.failures.clear();
        self.cooldowns.clear();
        self.retry_at = None;
        while self.results.try_recv().is_ok() {}
    }

    pub fn next_retry(&self) -> Option<Instant> {
        self.retry_at
    }
}

impl Drop for Images {
    fn drop(&mut self) {
        self.clear();
    }
}

/// What an entry counts against the cache's bytes: an original, nothing.
fn weight(source: &RemoteImage, data: &ImageData) -> usize {
    if source.kind == ImageKind::Original {
        0
    } else {
        data.base64.len()
    }
}

pub(crate) fn validated_resource_url(
    raw: &str,
    origins: &[&str],
    loopback: bool,
) -> Option<url::Url> {
    if raw.len() > 2048 || raw.chars().any(char::is_control) || raw.contains('\\') {
        return None;
    }
    let url = url::Url::parse(raw).ok()?;
    if !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.host_str().is_none()
    {
        return None;
    }
    // HTTP loopback can only be enabled by local fixtures, never production.
    if cfg!(test) && loopback && url.scheme() == "http" && url.host_str() == Some("127.0.0.1") {
        return Some(url);
    }
    if url.scheme() != "https"
        || url.port().is_some()
        || !origins.contains(&url.origin().ascii_serialization().as_str())
    {
        return None;
    }
    Some(url)
}

fn byte_limit(kind: ImageKind) -> usize {
    match kind {
        ImageKind::Artwork | ImageKind::Banner | ImageKind::ThumbHash => MAX_BYTES,
        ImageKind::AttachmentPreview
        | ImageKind::Clip
        | ImageKind::Original
        | ImageKind::Obscured => PREVIEW_BYTES,
    }
}

async fn download(
    client: &reqwest::Client,
    url: url::Url,
    max_bytes: usize,
) -> Result<Vec<u8>, DownloadError> {
    // This client has no account headers, cookies or bearer credentials.
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| DownloadError::transient())?;
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
        || response.status().is_server_error()
    {
        // Unknown Retry-After syntax is held until a new local session instead
        // of guessing a shorter wait than the server requested.
        let delay = match response.headers().get(reqwest::header::RETRY_AFTER) {
            Some(value) => value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<f64>().ok())
                .and_then(|seconds| Duration::try_from_secs_f64(seconds).ok()),
            None => Some(RETRY_DELAY),
        };
        return Err(DownloadError {
            retry: delay.map(|delay| delay.max(RETRY_DELAY)),
            origin_cooldown: true,
        });
    }
    if response.status() != reqwest::StatusCode::OK
        || response
            .content_length()
            .is_some_and(|n| n > max_bytes as u64)
    {
        return Err(DownloadError::permanent());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| DownloadError::transient())?
    {
        if bytes.len().saturating_add(chunk.len()) > max_bytes {
            return Err(DownloadError::permanent());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// [`normalize`] for the picture at `path`: artwork whose path ends in
/// `.json` is an animated vector (Lottie) sticker, drawn from its vectors.
/// Only sticker URLs name one; nothing else is read as vectors.
pub(crate) fn normalize_at(path: &str, bytes: &[u8], kind: ImageKind) -> Option<ImageData> {
    if kind != ImageKind::Artwork || !path.ends_with(".json") {
        return normalize(bytes, kind);
    }
    if bytes.is_empty() || bytes.len() > byte_limit(kind) {
        return None;
    }
    let (first, animation) = lottie::decode(bytes, THUMBNAIL, |frame| fit(frame, kind))?;
    still(first, animation.map(Arc::new))
}

pub(crate) fn normalize(bytes: &[u8], kind: ImageKind) -> Option<ImageData> {
    if bytes.len() > byte_limit(kind) {
        return None;
    }
    decoded(bytes, kind)
}

/// [`normalize`] without a download's byte budget: for a file the provider
/// keeps itself, bounded by `ProfileImage::MAX_ANIMATED_BYTES` instead (a
/// moving profile picture may pass a still one's budget). Decoding keeps
/// its own bounds.
fn decoded(bytes: &[u8], kind: ImageKind) -> Option<ImageData> {
    if bytes.is_empty() {
        return None;
    }
    match kind {
        ImageKind::Clip => return clip(bytes),
        ImageKind::Original => return original(bytes),
        _ => {}
    }
    let format = image::guess_format(bytes).ok()?;
    let obscured = kind == ImageKind::Obscured;
    let preview = kind == ImageKind::AttachmentPreview || obscured;
    let banner = kind == ImageKind::Banner;
    if !(matches!(
        format,
        ImageFormat::Png | ImageFormat::WebP | ImageFormat::Gif
    ) || (preview || banner) && format == ImageFormat::Jpeg)
    {
        return None;
    }
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(if preview {
        PREVIEW_SIDE
    } else if banner {
        BANNER_SIDE
    } else {
        MAX_SIDE
    });
    limits.max_image_height = limits.max_image_width;
    limits.max_alloc = Some(if preview {
        80 * 1024 * 1024
    } else if banner {
        16 * 1024 * 1024
    } else {
        4 * 1024 * 1024
    });
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits.clone());
    // Check the wide-artwork area before the decoder allocates pixel storage.
    if banner {
        let (width, height) = ImageReader::with_format(Cursor::new(bytes), format)
            .into_dimensions()
            .ok()?;
        if u64::from(width) * u64::from(height) > BANNER_PIXELS {
            return None;
        }
    }
    // The ordinary GIF decoder reads only the first frame. Animation frames
    // are collected separately, under their own frame and byte budgets.
    let decoded = reader.decode().ok()?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return None;
    }
    // A blurred picture stands still: its frames would only cost memory.
    let animation = (!banner && !obscured)
        .then(|| animation::decode(bytes, format, kind, limits, fit))
        .flatten()
        .map(Arc::new);
    let frame = fit(decoded, kind);
    still(if obscured { obscure(frame) } else { frame }, animation)
}

/// `frame` past recognition: shrunk to a few pixels a side and blurred, so
/// the host's scaling up to the preview's box blurs it further.
fn obscure(frame: image::RgbaImage) -> image::RgbaImage {
    let side = OBSCURED_SIDE as f32 / frame.width().max(frame.height()) as f32;
    let small = image::imageops::resize(
        &frame,
        ((frame.width() as f32 * side).round() as u32).max(1),
        ((frame.height() as f32 * side).round() as u32).max(1),
        image::imageops::FilterType::Triangle,
    );
    image::imageops::fast_blur(&small, 1.5)
}

/// `frame` as the PNG shown while nothing plays, with the frames that do.
fn still(frame: image::RgbaImage, animation: Option<Arc<Animation>>) -> Option<ImageData> {
    let (width, height) = frame.dimensions();
    let mut output = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(frame)
        .write_to(&mut output, ImageFormat::Png)
        .ok()?;
    let bytes = output.into_inner();
    Some(ImageData {
        sha256: format!("{:x}", Sha256::digest(&bytes)),
        size_bytes: bytes.len() as u32,
        width,
        height,
        base64: Arc::from(base64::engine::general_purpose::STANDARD.encode(&bytes)),
        animation,
        format: Encoding::Png,
    })
}

/// A clip, such as a GIF site's: its first picture as the still and every
/// picture as an animation, under the same budgets as an animated preview.
fn clip(bytes: &[u8]) -> Option<ImageData> {
    let (first, animation) = animation::clip(bytes, |frame| fit(frame, ImageKind::Clip))?;
    still(first, animation.map(Arc::new))
}

/// A preview's picture at its own size, for the viewer: decoded once to
/// check it is whole and within bounds, then kept as its file is, since the
/// host decodes it again to show it.
fn original(bytes: &[u8]) -> Option<ImageData> {
    let format = image::guess_format(bytes).ok()?;
    let encoding = match format {
        ImageFormat::Png => Encoding::Png,
        ImageFormat::Jpeg => Encoding::Jpeg,
        ImageFormat::WebP => Encoding::Webp,
        ImageFormat::Gif => Encoding::Gif,
        _ => return None,
    };
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(PREVIEW_SIDE);
    limits.max_image_height = Some(PREVIEW_SIDE);
    limits.max_alloc = Some(80 * 1024 * 1024);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits);
    let decoded = reader.decode().ok()?;
    if decoded.width() == 0 || decoded.height() == 0 {
        return None;
    }
    Some(ImageData {
        sha256: format!("{:x}", Sha256::digest(bytes)),
        size_bytes: u32::try_from(bytes.len()).ok()?,
        width: decoded.width(),
        height: decoded.height(),
        base64: Arc::from(base64::engine::general_purpose::STANDARD.encode(bytes)),
        animation: None,
        format: encoding,
    })
}

/// The size a preview of `kind` is shown at; animation frames use it too.
fn fit(decoded: image::DynamicImage, kind: ImageKind) -> image::RgbaImage {
    let preview = matches!(
        kind,
        ImageKind::AttachmentPreview | ImageKind::Clip | ImageKind::Obscured
    );
    let banner = kind == ImageKind::Banner;
    let resized = decoded
        .resize(
            if preview {
                PREVIEW_WIDTH.min(decoded.width())
            } else if banner {
                BANNER_WIDTH.min(decoded.width())
            } else {
                THUMBNAIL
            },
            if preview {
                PREVIEW_HEIGHT.min(decoded.height())
            } else if banner {
                BANNER_HEIGHT.min(decoded.height())
            } else {
                THUMBNAIL
            },
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    if preview || banner {
        return resized;
    }
    let mut thumbnail = image::RgbaImage::new(THUMBNAIL, THUMBNAIL);
    image::imageops::overlay(
        &mut thumbnail,
        &resized,
        i64::from((THUMBNAIL - resized.width()) / 2),
        i64::from((THUMBNAIL - resized.height()) / 2),
    );
    thumbnail
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod animation_tests;
