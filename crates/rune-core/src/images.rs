//! Credential-free artwork and attachment previews, separate from account HTTP.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Cursor,
    sync::Arc,
    time::{Duration, Instant},
};

use base64::Engine as _;
use image::{ImageFormat, ImageReader};
use rune_model::{ImageKind, RemoteImage, Rgb};
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
/// A wide picture's file: a server's banner at 1024 px, as a dense screen
/// asks for it, is a PNG past 1 MiB (one measured 1.07 MiB, 2026-10-25).
const BANNER_BYTES: usize = 4 * 1024 * 1024;
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
/// largest preview size, 64 RGBA textures occupy at most 96 MiB of pixels,
/// four times that on a screen of density 2 (see [`density_step`]); limn
/// evicts the least recently drawn first.
pub const IMAGE_TEXTURE_ENTRIES: usize = 64;
const CACHE_BYTES: usize = 16 * 1024 * 1024;
const RETRY_DELAY: Duration = Duration::from_secs(30);
/// A picture the server failed to give (5xx) is asked again this soon,
/// alone: a page's preview the proxy fetches the first time can take long
/// enough upstream to fail once (2026-10-21).
const SERVER_RETRY: Duration = Duration::from_secs(5);
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

/// Cached sources by how recently they were wanted, the oldest first. A hit
/// moves its source to the newest place in O(log n), where a queue searched
/// for it on every hit.
#[derive(Default)]
struct Recency {
    newest: u64,
    stamps: HashMap<RemoteImage, u64>,
    by_stamp: BTreeMap<u64, RemoteImage>,
}

impl Recency {
    fn touch(&mut self, source: &RemoteImage) {
        self.forget(source);
        self.newest += 1;
        self.stamps.insert(source.clone(), self.newest);
        self.by_stamp.insert(self.newest, source.clone());
    }

    fn forget(&mut self, source: &RemoteImage) {
        if let Some(stamp) = self.stamps.remove(source) {
            self.by_stamp.remove(&stamp);
        }
    }

    fn retain(&mut self, keep: impl Fn(&RemoteImage) -> bool) {
        self.stamps.retain(|source, _| keep(source));
        self.by_stamp.retain(|_, source| keep(source));
    }

    /// The least recently wanted source that `shown` does not name.
    fn oldest_not_in(&self, shown: &HashSet<RemoteImage>) -> Option<RemoteImage> {
        self.by_stamp
            .values()
            .find(|source| !shown.contains(*source))
            .cloned()
    }

    fn oldest_first(&self) -> impl Iterator<Item = &RemoteImage> {
        self.by_stamp.values()
    }

    fn clear(&mut self) {
        self.stamps.clear();
        self.by_stamp.clear();
    }
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
    /// The picture's dominant color, for an artwork (an avatar's tile takes it).
    pub color: Option<Rgb>,
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
    /// The screen's pixels for each logical one, in [`density_step`]s:
    /// what pictures are decoded for.
    density: f32,
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
    order: Recency,
    /// What the last `load` was asked for: on screen now, so never pushed
    /// out of the cache to make room (see `insert`).
    shown: HashSet<RemoteImage>,
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
            density: 1.0,
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
            order: Recency::default(),
            shown: HashSet::new(),
            bytes: 0,
            epoch: 0,
            failures: HashMap::new(),
            cooldowns: HashMap::new(),
            retry_at: None,
            hosted: None,
        }
    }

    /// Decode for a screen of `pixels_per_point`. A new step forgets what was
    /// decoded for the old one, which loads again from the disk or the
    /// network at its new size; the pictures at their own size stay.
    pub fn set_density(&mut self, pixels_per_point: f32) {
        let step = density_step(pixels_per_point);
        if step == self.density {
            return;
        }
        self.density = step;
        self.epoch = self.epoch.wrapping_add(1);
        for (_, task) in self.pending.drain() {
            task.abort();
        }
        self.cache
            .retain(|source, _| source.kind == ImageKind::Original);
        self.order
            .retain(|source| source.kind == ImageKind::Original);
        // What is left weighs nothing (see `weight`).
        self.bytes = 0;
    }

    /// Poll completed work, retain only the bounded requested set and start
    /// at most four requests. A full queue leaves initials until a later frame.
    pub fn load(&mut self, wanted: &[RemoteImage]) -> Vec<(RemoteImage, Arc<ImageData>)> {
        // Before the results arrive, so that none of them pushes out what is
        // on screen now.
        self.shown = wanted
            .iter()
            .filter(|source| source.url.len() <= 2048)
            .take(IMAGE_ENTRIES)
            .cloned()
            .collect();
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
        let wanted_set: HashSet<&RemoteImage> = wanted.iter().copied().collect();
        self.pending.retain(|source, task| {
            if wanted_set.contains(source) {
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
                    self.order.forget(source);
                } else {
                    self.retry_at = Some(self.retry_at.map_or(at, |old| old.min(at)));
                }
            }
            if let Some(data) = self.cache.get(source) {
                self.order.touch(source);
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
                let density = self.density;
                let task = self.runtime.spawn(async move {
                    let kind = source.kind;
                    let data = tokio::task::spawn_blocking(move || {
                        decoded(&bytes, kind, density).map(Arc::new)
                    })
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
            // The provider's own address for this screen's size, on the same
            // origin; it is what the disk keeps, so each size is kept apart.
            let (url, address) = match self
                .hosted
                .as_ref()
                .and_then(|provider| provider.image_at(&source.url, self.density))
                .and_then(|raw| {
                    let sized = self.url(&RemoteImage {
                        url: raw.clone(),
                        kind: source.kind,
                    })?;
                    (sized.origin() == url.origin()).then_some((sized, raw))
                }) {
                Some(sized) => sized,
                None => (url, source.url.clone()),
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
            let density = self.density;
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
                // Streams (transmisiones): a stream's picture is stale in two
                // minutes; it is never kept, and one that did not load is
                // asked for again, its address being the same each time.
                let stream = path.starts_with("/streams/");
                let disk = disk.filter(|_| !stream);
                // Kept from an earlier run: the network is not asked.
                let kept = match disk.clone() {
                    Some(disk) => {
                        let wanted = address.clone();
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
                        let wanted = address;
                        (
                            tokio::task::spawn_blocking(move || {
                                let _permit = permit;
                                let _preview_permit = preview_permit;
                                let data = normalize_at(&path, &bytes, kind, density).map(Arc::new);
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
                let retry = match (&data, stream) {
                    (None, true) => retry.or(Some(RETRY_DELAY)),
                    _ => retry,
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
                self.order.forget(&other);
                self.failures.remove(&other);
            }
        }
        if let Some(Some(old)) = self.cache.remove(&source) {
            self.bytes -= weight(&source, &old);
        }
        self.bytes += data.as_ref().map_or(0, |data| weight(&source, data));
        self.cache.insert(source.clone(), data);
        self.order.touch(&source);
        // A denser screen's pictures take more bytes each: the same count fits.
        let budget = (CACHE_BYTES as f32 * self.density * self.density) as usize;
        // The least recently wanted goes first, but never what is on screen
        // now: pushed out, it would be asked for again ahead of what waits
        // behind it, and the last pictures asked for (a member list's
        // faces) would never get their turn (2026-10-21). With everything
        // on screen, the budget waits; the entries stay bounded by what one
        // load may ask for.
        while self.cache.len() > IMAGE_ENTRIES || self.bytes > budget {
            let Some(old) = self.order.oldest_not_in(&self.shown) else {
                break;
            };
            self.order.forget(&old);
            self.failures.remove(&old);
            if let Some(Some(data)) = self.cache.remove(&old) {
                self.bytes -= weight(&old, &data);
            }
        }
        let mut animated: usize = self
            .cache
            .values()
            .flatten()
            .filter_map(|data| data.animation.as_ref())
            .map(|animation| animation.bytes())
            .sum();
        for old in self.order.oldest_first() {
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
        ImageKind::Artwork | ImageKind::ThumbHash => MAX_BYTES,
        ImageKind::Banner => BANNER_BYTES,
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
    // One picture the server could not give is that picture's, not a
    // reason to hold every other one of its origin.
    if response.status().is_server_error() {
        return Err(DownloadError {
            retry: Some(SERVER_RETRY),
            origin_cooldown: false,
        });
    }
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
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
    // Its origin and the answer only: a path can name a private file.
    let origin = response.url().origin().ascii_serialization();
    if response.status() != reqwest::StatusCode::OK {
        eprintln!(
            "rune: a picture from {origin} did not load ({})",
            response.status()
        );
        return Err(DownloadError::permanent());
    }
    let too_large = |bytes: u64| {
        eprintln!(
            "rune: a picture from {origin} did not load (200 OK, {} KiB past its {} KiB)",
            bytes / 1024,
            max_bytes / 1024
        );
        DownloadError::permanent()
    };
    if let Some(declared) = response.content_length().filter(|n| *n > max_bytes as u64) {
        return Err(too_large(declared));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| DownloadError::transient())?
    {
        let read = bytes.len().saturating_add(chunk.len());
        if read > max_bytes {
            return Err(too_large(read as u64));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

/// [`normalize`] for the picture at `path`: artwork whose path ends in
/// `.json` is an animated vector (Lottie) sticker, drawn from its vectors.
/// Only sticker URLs name one; nothing else is read as vectors.
/// It is decoded for a screen of `density` (a [`density_step`]).
pub(crate) fn normalize_at(
    path: &str,
    bytes: &[u8],
    kind: ImageKind,
    density: f32,
) -> Option<ImageData> {
    if bytes.is_empty() || bytes.len() > byte_limit(kind) {
        return None;
    }
    if kind != ImageKind::Artwork || !path.ends_with(".json") {
        return decoded(bytes, kind, density);
    }
    let side = scaled(THUMBNAIL, density);
    let (first, animation) = lottie::decode(bytes, side, |frame| fit(frame, kind, density))?;
    still(first, animation.map(Arc::new), kind)
}

/// [`normalize_at`] for a screen of density 1 and a path that is not
/// vectors: the GIF picker's stills.
pub(crate) fn normalize(bytes: &[u8], kind: ImageKind) -> Option<ImageData> {
    normalize_at("", bytes, kind, 1.0)
}

/// The step of a screen's pixels per logical pixel that pictures are
/// decoded for: the next half step up, from 1 to 2, so a cached picture
/// serves nearby scales; denser screens get 2 (shortcut: past 2 the
/// textures would pass limn's memory bound; raise both together).
pub fn density_step(pixels_per_point: f32) -> f32 {
    if !pixels_per_point.is_finite() {
        return 1.0;
    }
    ((pixels_per_point * 2.0).ceil() / 2.0).clamp(1.0, 2.0)
}

/// `side` on a screen of `density`.
fn scaled(side: u32, density: f32) -> u32 {
    (side as f32 * density).round() as u32
}

/// [`normalize`] without a download's byte budget: for a file the provider
/// keeps itself, bounded by `ProfileImage::MAX_ANIMATED_BYTES` instead (a
/// moving profile picture may pass a still one's budget). Decoding keeps
/// its own bounds.
fn decoded(bytes: &[u8], kind: ImageKind, density: f32) -> Option<ImageData> {
    if bytes.is_empty() {
        return None;
    }
    match kind {
        ImageKind::Clip => return clip(bytes, density),
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
        .then(|| {
            animation::decode(bytes, format, kind, limits, |frame, kind| {
                fit(frame, kind, density)
            })
        })
        .flatten()
        .map(Arc::new);
    let frame = fit(decoded, kind, density);
    still(
        if obscured { obscure(frame) } else { frame },
        animation,
        kind,
    )
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
fn still(
    frame: image::RgbaImage,
    animation: Option<Arc<Animation>>,
    kind: ImageKind,
) -> Option<ImageData> {
    let (width, height) = frame.dimensions();
    let color = (kind == ImageKind::Artwork)
        .then(|| dominant_color(&frame))
        .flatten();
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
        color,
    })
}

/// The most common color of `frame`'s opaque pixels, on a 32-pixel copy,
/// with 4 bits a channel. Ties go to the lower bucket, so the same picture
/// always gives the same color. `None` if no pixel is opaque.
pub fn dominant_color(frame: &image::RgbaImage) -> Option<Rgb> {
    let small = image::imageops::thumbnail(frame, 32, 32);
    // Per bucket: the pixel count and the sum of each channel.
    let mut buckets: HashMap<u16, (u64, [u64; 3])> = HashMap::new();
    for pixel in small.pixels().filter(|pixel| pixel[3] >= 128) {
        let key = u16::from(pixel[0] >> 4) << 8
            | u16::from(pixel[1] >> 4) << 4
            | u16::from(pixel[2] >> 4);
        let (count, sums) = buckets.entry(key).or_default();
        *count += 1;
        for (sum, channel) in sums.iter_mut().zip(pixel.0) {
            *sum += u64::from(channel);
        }
    }
    let (_, (count, sums)) = buckets
        .into_iter()
        .max_by_key(|(key, (count, _))| (*count, std::cmp::Reverse(*key)))?;
    let [red, green, blue] = sums.map(|sum| (sum / count) as u32);
    Some(Rgb(red << 16 | green << 8 | blue))
}

/// A clip, such as a GIF site's: its first picture as the still and every
/// picture as an animation, under the same budgets as an animated preview.
fn clip(bytes: &[u8], density: f32) -> Option<ImageData> {
    let (first, animation) = animation::clip(bytes, |frame| fit(frame, ImageKind::Clip, density))?;
    still(first, animation.map(Arc::new), ImageKind::Clip)
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
        color: None,
    })
}

/// The size a preview of `kind` is shown at on a screen of `density`;
/// animation frames use it too.
fn fit(decoded: image::DynamicImage, kind: ImageKind, density: f32) -> image::RgbaImage {
    let preview = matches!(
        kind,
        ImageKind::AttachmentPreview | ImageKind::Clip | ImageKind::Obscured
    );
    let banner = kind == ImageKind::Banner;
    let thumbnail = scaled(THUMBNAIL, density);
    let (width, height) = if preview {
        (PREVIEW_WIDTH, PREVIEW_HEIGHT)
    } else if banner {
        (BANNER_WIDTH, BANNER_HEIGHT)
    } else {
        (THUMBNAIL, THUMBNAIL)
    };
    let (width, height) = (scaled(width, density), scaled(height, density));
    let resized = decoded
        .resize(
            if preview || banner {
                width.min(decoded.width())
            } else {
                width
            },
            if preview || banner {
                height.min(decoded.height())
            } else {
                height
            },
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    if preview || banner {
        return resized;
    }
    let mut canvas = image::RgbaImage::new(thumbnail, thumbnail);
    image::imageops::overlay(
        &mut canvas,
        &resized,
        i64::from((thumbnail - resized.width()) / 2),
        i64::from((thumbnail - resized.height()) / 2),
    );
    canvas
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod animation_tests;
