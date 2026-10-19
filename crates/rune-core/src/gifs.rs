//! Optional GIPHY search, independent of chat credentials. Only the active page
//! lives in memory; media URLs and pixels are not kept as a reusable cache.

use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rune_model::{ImageKind, RemoteImage};
use rune_provider::{
    GifAttribution, GifError, GifItem, GifPage, GifQuery, GifResult, GifThumbnail, RequestId,
};
use serde_json::Value;
use tokio::{
    runtime::Handle,
    sync::{mpsc, Semaphore},
    task::{JoinHandle, JoinSet},
};

const PAGE_SIZE: usize = 20;
const MAX_OFFSET: u32 = 480;
const RESPONSE_BYTES: usize = 256 * 1024;
const THUMBNAIL_BYTES: usize = 1024 * 1024;
const THUMBNAIL_CONCURRENT: usize = 4;
const MEDIA_HOSTS: &[&str] = &[
    "media.giphy.com",
    "media0.giphy.com",
    "media1.giphy.com",
    "media2.giphy.com",
    "media3.giphy.com",
    "media4.giphy.com",
    "i.giphy.com",
];

/// The host reads its own application's key from the environment. It must not
/// pass it through command-line arguments, chat state, logs or persisted UI.
#[derive(Clone)]
pub struct GiphyConfig {
    key: Arc<str>,
}

impl GiphyConfig {
    pub fn new(key: String) -> Result<Self, GifError> {
        if key.is_empty()
            || key.len() > 256
            || !key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return Err(GifError::NotConfigured);
        }
        Ok(Self {
            key: Arc::from(key),
        })
    }
}

impl std::fmt::Debug for GiphyConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GiphyConfig([REDACTED])")
    }
}

type Cooldown = Arc<Mutex<Option<(Instant, Duration)>>>;

#[derive(Clone)]
struct Api {
    client: reqwest::Client,
    config: GiphyConfig,
    base: url::Url,
    gate: Arc<Semaphore>,
    cooldown: Cooldown,
    #[cfg(test)]
    thumbnails: bool,
    #[cfg(test)]
    media_origin: Option<String>,
    #[cfg(test)]
    thumbnail_budget: Duration,
}

/// A single cancellable search with generation-checked results and no cache.
pub struct GifSearch {
    runtime: Handle,
    api: Option<Api>,
    gate: Arc<Semaphore>,
    cooldown: Cooldown,
    wake: Arc<dyn Fn() + Send + Sync>,
    sender: mpsc::Sender<(u64, GifResult)>,
    receiver: mpsc::Receiver<(u64, GifResult)>,
    pending: Option<(RequestId, JoinHandle<()>)>,
    epoch: u64,
}

impl GifSearch {
    pub fn new(
        runtime: Handle,
        config: Option<GiphyConfig>,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let mut service = Self {
            runtime,
            api: None,
            gate: Arc::new(Semaphore::new(THUMBNAIL_CONCURRENT)),
            cooldown: Arc::default(),
            wake,
            sender,
            receiver,
            pending: None,
            epoch: 0,
        };
        service.configure(config);
        service
    }

    /// Reconfiguration drops previous results and tasks. Transport backoff
    /// survives configuration and account changes instead of evading a limit.
    pub fn configure(&mut self, config: Option<GiphyConfig>) {
        self.clear();
        self.api = config.and_then(|config| {
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(5))
                .timeout(Duration::from_secs(10))
                .user_agent(concat!("RUNE/", env!("CARGO_PKG_VERSION"), " GIF search"))
                .build()
                .ok()?;
            Some(Api {
                client,
                config,
                base: url::Url::parse("https://api.giphy.com/v1/gifs/").ok()?,
                gate: self.gate.clone(),
                cooldown: self.cooldown.clone(),
                #[cfg(test)]
                thumbnails: true,
                #[cfg(test)]
                media_origin: None,
                #[cfg(test)]
                thumbnail_budget: Duration::from_secs(8),
            })
        });
    }

    pub fn available(&self) -> bool {
        self.api.is_some()
    }

    pub fn start(&mut self, request: RequestId, query: GifQuery, offset: u32) {
        self.clear();
        let epoch = self.epoch;
        let (api, sender, wake) = (self.api.clone(), self.sender.clone(), self.wake.clone());
        self.pending = Some((
            request,
            self.runtime.spawn(async move {
                let result = match api {
                    None => Err(GifError::NotConfigured),
                    Some(api) if valid_query(&query, offset) => {
                        tokio::time::timeout(Duration::from_secs(25), api.page(&query, offset))
                            .await
                            .unwrap_or(Err(GifError::Unavailable))
                    }
                    Some(_) => Err(GifError::InvalidQuery),
                };
                if sender
                    .send((
                        epoch,
                        GifResult {
                            request,
                            query,
                            offset,
                            result,
                        },
                    ))
                    .await
                    .is_ok()
                {
                    wake();
                }
            }),
        ));
    }

    pub fn cancel(&mut self, request: RequestId) {
        if self
            .pending
            .as_ref()
            .is_some_and(|(active, _)| *active == request)
        {
            self.clear();
        }
    }

    pub fn clear(&mut self) {
        self.epoch = self.epoch.wrapping_add(1);
        if let Some((_, task)) = self.pending.take() {
            task.abort();
        }
        while self.receiver.try_recv().is_ok() {}
    }

    pub fn poll(&mut self) -> Option<GifResult> {
        while let Ok((epoch, result)) = self.receiver.try_recv() {
            if epoch == self.epoch {
                self.pending = None;
                return Some(result);
            }
        }
        None
    }
}

impl Drop for GifSearch {
    fn drop(&mut self) {
        self.clear();
    }
}

fn valid_query(query: &GifQuery, offset: u32) -> bool {
    offset <= MAX_OFFSET
        && match query {
            GifQuery::Trending => true,
            GifQuery::Search(value) => {
                !value.trim().is_empty()
                    && value.chars().count() <= 50
                    && !value.chars().any(char::is_control)
            }
        }
}

fn delayed(cooldown: &Cooldown) -> Option<GifError> {
    cooldown
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .and_then(|(at, delay)| {
            let left = delay.saturating_sub(at.elapsed());
            (!left.is_zero()).then_some(GifError::RateLimited {
                retry_after_secs: left.as_secs().saturating_add(1),
            })
        })
}

impl Api {
    fn url(&self, query: &GifQuery, offset: u32) -> url::Url {
        let mut url = self
            .base
            .join(match query {
                GifQuery::Trending => "trending",
                GifQuery::Search(_) => "search",
            })
            .expect("fixed relative API route");
        let mut pairs = url.query_pairs_mut();
        pairs
            .append_pair("api_key", &self.config.key)
            .append_pair("limit", &PAGE_SIZE.to_string())
            .append_pair("offset", &offset.to_string())
            .append_pair("rating", "pg-13");
        if let GifQuery::Search(query) = query {
            pairs
                .append_pair("q", query)
                .append_pair("lang", rune_i18n::current().code());
        }
        drop(pairs);
        url
    }

    async fn bytes(&self, url: url::Url, max: usize) -> Result<Vec<u8>, GifError> {
        if let Some(error) = delayed(&self.cooldown) {
            return Err(error);
        }
        // No chat authorization headers or cookies. Errors never include the
        // request URL, which contains this application's search API key.
        let mut response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| GifError::Unavailable)?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let seconds = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .filter(|seconds| *seconds > 0)
                .unwrap_or(3600);
            let mut cooldown = self
                .cooldown
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let remaining = cooldown
                .map(|(at, delay)| delay.saturating_sub(at.elapsed()))
                .unwrap_or_default();
            *cooldown = Some((Instant::now(), remaining.max(Duration::from_secs(seconds))));
            return Err(GifError::RateLimited {
                retry_after_secs: seconds,
            });
        }
        if matches!(response.status().as_u16(), 401 | 403) {
            return Err(GifError::AccessDenied);
        }
        if response.status() != reqwest::StatusCode::OK {
            return Err(GifError::Unavailable);
        }
        if response
            .content_length()
            .is_some_and(|length| length > max as u64)
        {
            return Err(GifError::InvalidResponse);
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| GifError::Unavailable)? {
            if bytes.len().saturating_add(chunk.len()) > max {
                return Err(GifError::InvalidResponse);
            }
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    async fn page(&self, query: &GifQuery, offset: u32) -> Result<GifPage, GifError> {
        let bytes = self.bytes(self.url(query, offset), RESPONSE_BYTES).await?;
        let mut page = decode(&bytes, offset)?;
        #[cfg(test)]
        if !self.thumbnails {
            return Ok(page);
        }
        let mut jobs = JoinSet::new();
        for (index, item) in page.items.iter().enumerate() {
            let (api, source) = (self.clone(), item.preview.clone());
            jobs.spawn(async move { (index, api.thumbnail(source).await) });
        }
        let budget = Duration::from_secs(8);
        #[cfg(test)]
        let budget = self.thumbnail_budget.min(budget);
        let deadline = tokio::time::Instant::now() + budget;
        while let Ok(Some(result)) = tokio::time::timeout_at(deadline, jobs.join_next()).await {
            if let Ok((index, thumbnail)) = result {
                // Preserve every result in API order. Slow/broken media falls
                // back per item instead of failing an otherwise valid page.
                page.items[index].thumbnail = thumbnail;
            }
        }
        Ok(page)
    }

    async fn thumbnail(&self, source: RemoteImage) -> Option<GifThumbnail> {
        let permit = self.gate.clone().acquire_owned().await.ok()?;
        let url = media_url(&source.url);
        #[cfg(test)]
        let url = url.or_else(|| {
            let candidate = url::Url::parse(&source.url).ok()?;
            (candidate.scheme() == "http"
                && candidate.host_str() == Some("127.0.0.1")
                && self.media_origin.as_deref()
                    == Some(candidate.origin().ascii_serialization().as_str()))
            .then_some(candidate)
        });
        #[cfg(test)]
        let url = url.map(|url| {
            if url.scheme() == "https" {
                if let Some(origin) = &self.media_origin {
                    let mut local = url::Url::parse(origin).expect("fixture origin");
                    local.set_path(url.path());
                    local.set_query(url.query());
                    return local;
                }
            }
            url
        });
        let url = url?;
        let bytes = self.bytes(url, THUMBNAIL_BYTES).await.ok()?;
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            let image = crate::images::normalize(&bytes, ImageKind::Artwork)?;
            Some(GifThumbnail {
                sha256: image.sha256,
                size_bytes: image.size_bytes,
                width: image.width,
                height: image.height,
                png_base64: image.base64,
            })
        })
        .await
        .ok()
        .flatten()
    }
}

fn checked_url(raw: &str, hosts: &[&str]) -> Option<url::Url> {
    if raw.len() > 2048
        || raw
            .chars()
            .any(|value| value.is_control() || value.is_whitespace())
        || raw.contains('\\')
    {
        return None;
    }
    let url = url::Url::parse(raw).ok()?;
    (url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none()
        && url.port().is_none()
        && hosts.contains(&url.host_str()?))
    .then_some(url)
}

fn media_url(raw: &str) -> Option<url::Url> {
    checked_url(raw, MEDIA_HOSTS)
}

fn decode(bytes: &[u8], offset: u32) -> Result<GifPage, GifError> {
    let invalid = || GifError::InvalidResponse;
    let value: Value = serde_json::from_slice(bytes).map_err(|_| invalid())?;
    if value["meta"]["status"].as_u64() != Some(200) {
        return Err(invalid());
    }
    let values = value["data"]
        .as_array()
        .filter(|values| values.len() <= PAGE_SIZE)
        .ok_or_else(invalid)?;
    let pagination = &value["pagination"];
    let count = pagination["count"]
        .as_u64()
        .filter(|count| *count == values.len() as u64)
        .ok_or_else(invalid)?;
    if pagination["offset"].as_u64() != Some(u64::from(offset)) {
        return Err(invalid());
    }
    let total = pagination["total_count"].as_u64().ok_or_else(invalid)?;
    let mut seen = HashSet::new();
    let mut items = Vec::with_capacity(values.len());
    for value in values {
        let id = value["id"]
            .as_str()
            .filter(|id| {
                !id.is_empty() && id.len() <= 128 && id.bytes().all(|c| c.is_ascii_alphanumeric())
            })
            .ok_or_else(invalid)?;
        if !seen.insert(id) || value["type"].as_str() != Some("gif") {
            return Err(invalid());
        }
        let title = value["title"].as_str().unwrap_or("GIF");
        if title.len() > 512 || title.chars().any(char::is_control) {
            return Err(invalid());
        }
        let share = value["url"].as_str().ok_or_else(invalid)?;
        if !checked_url(share, &["giphy.com", "www.giphy.com"])
            .is_some_and(|url| url.path().starts_with("/gifs/"))
        {
            return Err(invalid());
        }
        let image = &value["images"]["fixed_width_small_still"];
        let preview = image["url"]
            .as_str()
            .filter(|url| media_url(url).is_some())
            .ok_or_else(invalid)?;
        let creator = value["username"]
            .as_str()
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        if creator
            .as_ref()
            .is_some_and(|value| value.len() > 128 || value.chars().any(char::is_control))
        {
            return Err(invalid());
        }
        items.push(GifItem {
            id: id.into(),
            title: title.into(),
            creator,
            url: share.into(),
            preview: RemoteImage {
                url: preview.into(),
                kind: ImageKind::Artwork,
            },
            thumbnail: None,
        });
    }
    let next = u64::from(offset) + count;
    Ok(GifPage {
        items,
        next_offset: (count > 0 && next < total && next <= u64::from(MAX_OFFSET))
            .then_some(next as u32),
        attribution: GifAttribution {
            label: "Powered By GIPHY".into(),
            url: "https://giphy.com/".into(),
            image: Some(GifThumbnail {
                sha256: "839e790cd4c78b5aa3a416a6bfafc6ff3502e6d0447812ab7fcfed182a1ecb2e".into(),
                size_bytes: 1627,
                width: 200,
                height: 42,
                png_base64: include_str!("../../../assets/giphy-attribution.png.base64").into(),
            }),
        },
        categories: Vec::new(),
    })
}

#[cfg(test)]
mod tests;
