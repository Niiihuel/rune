//! RUNE's own updates (actualizaciones, 2026-10-19).
//!
//! Every release publishes `rune-update.json`, a manifest of its version,
//! notes, the oldest version still accepted (`minimum_version`) and one
//! artifact per platform (address, size, SHA-256), and
//! `rune-update.json.sig`, the Ed25519 signature of those exact bytes in
//! base64. The public half of the release key is compiled in
//! (`release-key.pub`); the private half lives only on the owner's machine
//! and in the release workflow's secret (`docs/UPDATES.md`).
//!
//! A check fetches both files, verifies the signature before reading the
//! JSON, and offers the release when it is newer than this program: as
//! mandatory when this program is older than `minimum_version`. Its
//! artifact downloads beside the installation, resuming a partial file,
//! and is installed only once its size and digest match what was signed
//! ([`install`]). Network, disk and every wait run on the caller's thread:
//! never the interface's.

pub mod install;
#[cfg(any(test, feature = "serve"))]
pub mod serve;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use base64::Engine as _;
use ed25519_dalek::{Signature, VerifyingKey};
use rune_i18n::{t, tr};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub use semver::Version;

/// This program's version: the workspace's, which a release's tag matches.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Where the newest release's files are, unless `RUNE_UPDATE_FEED` says
/// otherwise.
pub const RELEASES: &str = "https://github.com/Niiihuel/rune/releases/latest/download";
pub const MANIFEST: &str = "rune-update.json";
pub const SIGNATURE: &str = "rune-update.json.sig";
/// The word in an annotated tag's message that makes its version mandatory.
pub const MANDATORY_WORD: &str = "obligatoria";

const MAX_MANIFEST: usize = 64 * 1024;
const MAX_SIGNATURE: usize = 1024;
/// No artifact is larger: a manifest that says so is refused.
pub const MAX_ARTIFACT: u64 = 1 << 30;

/// The release key's public half, base64, written by
/// `scripts/update-key.sh`. Empty until the owner made one: then nothing
/// is trusted, and no update is ever installed.
const PUBLIC_KEY: &str = include_str!("../release-key.pub");

/// What one release says of itself, signed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: Version,
    /// When it was published, RFC 3339.
    pub date: String,
    /// The annotated tag's message.
    #[serde(default)]
    pub notes: String,
    /// The newest mandatory version up to this one: anything older must
    /// update before it opens.
    #[serde(default)]
    pub minimum_version: Option<Version>,
    /// By [`platform`] key.
    pub platforms: BTreeMap<String, Artifact>,
}

/// What a platform downloads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub size: u64,
    /// Lowercase hex.
    pub sha256: String,
}

/// A release newer than this program, as offered.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: Version,
    pub date: String,
    pub notes: String,
    /// This program is older than the release's `minimum_version`.
    pub mandatory: bool,
    /// `None`: the release has nothing for this platform.
    pub artifact: Option<Artifact>,
}

/// Why a check, a download or an installation did not go through. Each
/// leaves RUNE as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// This build has no release key: nothing can be trusted.
    NoKey,
    Network,
    Status(u16),
    TooLarge,
    Signature,
    Manifest,
    NoArtifact,
    Incomplete,
    Digest,
    Disk,
    /// This copy is not one the updater installed (a build from source).
    NotInstalled,
    /// The job ended without saying how (its thread stopped early).
    Interrupted,
}

impl Error {
    /// What the interface says, in its language.
    pub fn label(self) -> String {
        match self {
            Self::NoKey => t("Esta compilación no tiene la clave de las actualizaciones.").into(),
            Self::Network => t("No se pudo conectar con el servidor de actualizaciones.").into(),
            Self::Status(status) => tr!(
                "El servidor de actualizaciones respondió {status}.",
                status = status
            ),
            Self::TooLarge => t("La respuesta del servidor de actualizaciones es demasiado grande.").into(),
            Self::Signature => t("La firma de la actualización no es válida: no se instaló nada.").into(),
            Self::Manifest => t("No se pudo leer la descripción de la actualización.").into(),
            Self::NoArtifact => t("Esta versión no tiene descarga para este sistema.").into(),
            Self::Incomplete => t("La descarga quedó incompleta: se retoma al reintentar.").into(),
            Self::Digest => t("Lo descargado no coincide con lo firmado: se borró.").into(),
            Self::Disk => t("No se pudo escribir la actualización en el disco.").into(),
            Self::NotInstalled => t("Esta copia de RUNE no se instaló con su instalador: actualizala desde donde la compilaste.").into(),
            Self::Interrupted => t("La actualización se interrumpió. Reintentá para completarla.").into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label())
    }
}

impl std::error::Error for Error {}

/// This platform's key in a manifest: `linux-x86_64`, `windows-x86_64`…
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// The key releases are checked with: the compiled one; in a debug build,
/// `RUNE_UPDATE_KEY` instead when set, for a test feed signed with a
/// throwaway key. A release build never reads it.
pub fn public_key() -> Option<VerifyingKey> {
    #[cfg(debug_assertions)]
    if let Ok(key) = std::env::var("RUNE_UPDATE_KEY") {
        return parse_key(&key);
    }
    release_key()
}

/// The release key compiled in, whatever the environment says.
pub fn release_key() -> Option<VerifyingKey> {
    parse_key(PUBLIC_KEY)
}

/// A public key written as base64; a weak (small-order) one is refused.
pub fn parse_key(text: &str) -> Option<VerifyingKey> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text.trim())
        .ok()?;
    let key = VerifyingKey::from_bytes(&bytes.try_into().ok()?).ok()?;
    (!key.is_weak()).then_some(key)
}

/// The manifest in `manifest`, once `signature` (base64) proves the key's
/// owner signed those exact bytes.
pub fn verify(manifest: &[u8], signature: &str, key: &VerifyingKey) -> Result<Manifest, Error> {
    let signature = base64::engine::general_purpose::STANDARD
        .decode(signature.trim())
        .ok()
        .and_then(|bytes| Signature::from_slice(&bytes).ok())
        .ok_or(Error::Signature)?;
    key.verify_strict(manifest, &signature)
        .map_err(|_| Error::Signature)?;
    // Signed by the owner, so its fields are safe to name in the log.
    serde_json::from_slice(manifest).map_err(|error| {
        eprintln!("rune: update: unreadable manifest: {error}");
        Error::Manifest
    })
}

/// What `manifest` offers a program at `installed` on `platform`: nothing
/// when it is not newer. An artifact that could not be installed (too
/// large, not HTTPS, no digest) counts as none.
pub fn offer(manifest: Manifest, installed: &Version, platform: &str) -> Option<Release> {
    if manifest.version <= *installed {
        return None;
    }
    let mandatory = manifest
        .minimum_version
        .as_ref()
        .is_some_and(|minimum| installed < minimum);
    let artifact = manifest
        .platforms
        .get(platform)
        .filter(|artifact| usable(artifact))
        .cloned();
    Some(Release {
        version: manifest.version,
        date: manifest.date,
        notes: manifest.notes,
        mandatory,
        artifact,
    })
}

fn usable(artifact: &Artifact) -> bool {
    let https = artifact.url.starts_with("https://")
        // The test feeds of `serve`, on this machine only.
        || artifact.url.starts_with("http://127.0.0.1:");
    https
        && artifact.size > 0
        && artifact.size <= MAX_ARTIFACT
        && artifact.sha256.len() == 64
        && artifact.sha256.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// Whether an annotated tag's `message` makes its version mandatory: it
/// has the word «obligatoria», in any case.
pub fn mandatory(message: &str) -> bool {
    message
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.to_lowercase() == MANDATORY_WORD)
}

/// The newest mandatory version up to `version` among `tags`, each an
/// annotated tag's name (`v1.2.0`) and message. Names that are not a `v`
/// and a version are ignored.
pub fn minimum_version<'a>(
    version: &Version,
    tags: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Option<Version> {
    tags.into_iter()
        .filter(|(_, message)| mandatory(message))
        .filter_map(|(name, _)| Version::parse(name.strip_prefix('v')?).ok())
        .filter(|tagged| tagged <= version)
        .max()
}

/// Where a check looks: a base address holding [`MANIFEST`] and
/// [`SIGNATURE`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feed {
    pub manifest: String,
    pub signature: String,
}

impl Feed {
    pub fn at(base: &str) -> Self {
        let base = base.trim_end_matches('/');
        Self {
            manifest: format!("{base}/{MANIFEST}"),
            signature: format!("{base}/{SIGNATURE}"),
        }
    }

    /// `RUNE_UPDATE_FEED` when set (a test feed), or the releases'. What it
    /// serves is still verified with [`public_key`].
    pub fn from_env() -> Self {
        let chosen = std::env::var("RUNE_UPDATE_FEED").ok();
        Self::at(
            chosen
                .as_deref()
                .filter(|base| !base.trim().is_empty())
                .unwrap_or(RELEASES),
        )
    }

    /// Whether `RUNE_UPDATE_FEED` was set.
    pub fn chosen() -> bool {
        std::env::var_os("RUNE_UPDATE_FEED").is_some_and(|base| !base.is_empty())
    }
}

/// The client every request goes through: bounded waits, its own name.
pub fn client() -> Result<reqwest::Client, Error> {
    reqwest::Client::builder()
        .user_agent(concat!("RUNE-updater/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
        .read_timeout(Duration::from_secs(30))
        .build()
        .map_err(network)
}

/// Run `future` to its end on this thread, which must not be the
/// interface's.
pub fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime")
        .block_on(future)
}

/// The signed manifest at `feed`.
pub async fn check(
    client: &reqwest::Client,
    feed: &Feed,
    key: Option<&VerifyingKey>,
) -> Result<Manifest, Error> {
    let key = key.ok_or(Error::NoKey)?;
    let manifest = fetch(client, &feed.manifest, MAX_MANIFEST).await?;
    let signature = fetch(client, &feed.signature, MAX_SIGNATURE).await?;
    verify(
        &manifest,
        std::str::from_utf8(&signature).map_err(|_| Error::Signature)?,
        key,
    )
}

async fn fetch(client: &reqwest::Client, url: &str, limit: usize) -> Result<Vec<u8>, Error> {
    let mut response = client.get(url).send().await.map_err(network)?;
    if !response.status().is_success() {
        return Err(Error::Status(response.status().as_u16()));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        if body.len() + chunk.len() > limit {
            return Err(Error::TooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn network(error: reqwest::Error) -> Error {
    eprintln!("rune: update: {error}");
    Error::Network
}

fn disk(error: std::io::Error) -> Error {
    eprintln!("rune: update: {error}");
    Error::Disk
}

/// Download `artifact` into `folder`, telling `progress` the bytes there
/// and the whole. A partial file left by an earlier try (named by the
/// digest, so another release never resumes it) continues where it ended.
/// Returns the file once its size and SHA-256 match; one that does not
/// match is deleted.
pub async fn download(
    client: &reqwest::Client,
    artifact: &Artifact,
    folder: &Path,
    mut progress: impl FnMut(u64, u64),
) -> Result<PathBuf, Error> {
    let digest = artifact.sha256.to_ascii_lowercase();
    let done = folder.join(format!("rune-{digest}"));
    let part = folder.join(format!("rune-{digest}.part"));
    tokio::fs::create_dir_all(folder).await.map_err(disk)?;
    // Downloaded by an earlier try whose installation failed.
    if sha256(&done).await.is_ok_and(|found| found == digest) {
        return Ok(done);
    }
    let mut have = tokio::fs::metadata(&part)
        .await
        .map_or(0, |found| found.len());
    if have > artifact.size {
        have = 0;
    }
    if have < artifact.size {
        let mut request = client.get(&artifact.url);
        if have > 0 {
            request = request.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        let mut response = request.send().await.map_err(network)?;
        let resumed = response.status() == reqwest::StatusCode::PARTIAL_CONTENT
            && have > 0
            && response
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|range| range.to_str().ok())
                .is_some_and(|range| range.starts_with(&format!("bytes {have}-")));
        if !resumed {
            if response.status() != reqwest::StatusCode::OK {
                return Err(Error::Status(response.status().as_u16()));
            }
            // The server sends it whole: start over.
            have = 0;
        }
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .append(resumed)
            .truncate(!resumed)
            .open(&part)
            .await
            .map_err(disk)?;
        progress(have, artifact.size);
        let streamed = loop {
            match response.chunk().await {
                Ok(Some(chunk)) => {
                    have += chunk.len() as u64;
                    if have > artifact.size {
                        drop(file);
                        let _ = tokio::fs::remove_file(&part).await;
                        return Err(Error::Digest);
                    }
                    file.write_all(&chunk).await.map_err(disk)?;
                    progress(have, artifact.size);
                }
                Ok(None) => break Ok(()),
                Err(error) => break Err(network(error)),
            }
        };
        // What came is on the disk before an error returns, so the next
        // try resumes from its true end.
        file.sync_all().await.map_err(disk)?;
        streamed?;
    }
    if have != artifact.size {
        return Err(Error::Incomplete);
    }
    if sha256(&part).await.map_err(disk)? != digest {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(Error::Digest);
    }
    tokio::fs::rename(&part, &done).await.map_err(disk)?;
    Ok(done)
}

/// The SHA-256 of the file at `path`, lowercase hex.
async fn sha256(path: &Path) -> std::io::Result<String> {
    let mut file = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        let read = file.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex(&hasher.finalize()))
}

/// `bytes` as lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests;
