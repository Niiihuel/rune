//! Pictures kept on disk between runs (imagenes-y-lectura), as Serein keeps
//! them: the bytes as they were downloaded, in two bounded parts, so a burst
//! of large media never pushes out the small artwork seen everywhere. What
//! is not shown again for the retention goes, and past a budget the least
//! recently shown goes first.
//!
//! Unlike Serein's, each account's pictures are sealed with that account's
//! key, the one of its local store (XChaCha20-Poly1305, associated data the
//! account, the part and the file's name), so without the keyring nothing
//! reaches the disk and an account forgotten leaves nothing readable. Files
//! are named by a digest of their URL keyed with that key, so a name does
//! not confirm which picture it is. Every operation here blocks: the
//! [`DiskCache`] runs them on the core's runtime, never on the UI thread.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant, SystemTime};

use rune_model::ImageKind;
use rune_storage::{Key, SEALED_OVERHEAD};
use sha2::{Digest, Sha256};
use tokio::{runtime::Handle, task::JoinHandle};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const MIB: u64 = 1024 * 1024;
/// Under `$XDG_CACHE_HOME/rune`.
const FOLDER: &str = "images";

/// The two parts of the cache: conversation media, and the small artwork
/// reused everywhere.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CachePart {
    /// Previews of attachments and cards, banners, clips and pictures at
    /// their own size.
    Media,
    /// Avatars, emoji, stickers, icons and badges.
    Artwork,
}

impl CachePart {
    fn of(kind: ImageKind) -> Self {
        if kind == ImageKind::Artwork {
            Self::Artwork
        } else {
            Self::Media
        }
    }

    fn index(self) -> usize {
        self as usize
    }

    fn name(self) -> &'static str {
        match self {
            Self::Media => "media",
            Self::Artwork => "artwork",
        }
    }

    /// The usual size of one of its files, as Serein reckons them: the
    /// budget's bytes over it are how many files it may hold, which bounds
    /// a scan of its folder.
    fn usual_file(self) -> u64 {
        match self {
            Self::Media => 256 * 1024,
            Self::Artwork => 32 * 1024,
        }
    }
}

/// What the disk may keep: each part's bytes, and how long a picture not
/// shown again stays.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheLimits {
    pub media_bytes: u64,
    pub artwork_bytes: u64,
    pub retention: Duration,
}

impl Default for CacheLimits {
    /// Serein's: 1 GiB of media, 128 MiB of artwork, 90 days.
    fn default() -> Self {
        Self {
            media_bytes: 1024 * MIB,
            artwork_bytes: 128 * MIB,
            retention: 90 * DAY,
        }
    }
}

impl CacheLimits {
    fn bytes(&self, part: CachePart) -> u64 {
        match part {
            CachePart::Media => self.media_bytes,
            CachePart::Artwork => self.artwork_bytes,
        }
    }
}

/// What each part keeps now, in bytes on disk.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CacheUsage {
    pub media_bytes: u64,
    pub artwork_bytes: u64,
}

/// One part's folder and what it holds.
struct Part {
    folder: PathBuf,
    bytes: u64,
    files: u64,
    budget: u64,
    max_files: u64,
    pruned: Instant,
}

impl Part {
    fn set_budget(&mut self, part: CachePart, budget: u64) {
        self.budget = budget;
        self.max_files = (budget / part.usual_file()).max(64);
    }

    /// Delete leftovers and what expired, and count the rest; when `reserve`
    /// more bytes and one more file would not fit, the oldest go until they
    /// fit under 90 % of the budget, so a full part does not scan its
    /// folder on every write.
    fn prune(&mut self, retention: Duration, reserve: Option<u64>) -> io::Result<()> {
        let now = SystemTime::now();
        let mut kept = Vec::new();
        let (mut bytes, mut files) = (0u64, 0u64);
        for entry in fs::read_dir(&self.folder)? {
            let entry = entry?;
            // Not followed: a link is not one of these files.
            let metadata = entry.metadata()?;
            if !metadata.is_file() {
                continue;
            }
            let modified = metadata.modified()?;
            let ours = entry.file_name().to_str().is_some_and(valid_name);
            // What a write left half done has no such name either.
            if !ours || now.duration_since(modified).unwrap_or_default() >= retention {
                fs::remove_file(entry.path())?;
                continue;
            }
            bytes += metadata.len();
            files += 1;
            kept.push((modified, entry.path(), metadata.len()));
        }
        let (more_bytes, more_files) = reserve.map_or((0, 0), |bytes| (bytes, 1));
        if bytes + more_bytes > self.budget || files + more_files > self.max_files {
            let (goal_bytes, goal_files) = (self.budget / 10 * 9, self.max_files / 10 * 9);
            kept.sort_by_key(|(modified, ..)| *modified);
            for (_, path, len) in kept {
                if bytes + more_bytes <= goal_bytes && files + more_files <= goal_files {
                    break;
                }
                fs::remove_file(path)?;
                bytes -= len;
                files -= 1;
            }
        }
        self.bytes = bytes;
        self.files = files;
        self.pruned = Instant::now();
        Ok(())
    }

    /// Keep `sealed` as `name`, making room for it first. It is written
    /// beside the others under a name no scan takes for a picture (`0600`,
    /// gone if dropped half written) and moved in whole, so a reader finds
    /// the old file or the new one, never half of it.
    fn keep(&mut self, name: &str, sealed: &[u8], retention: Duration) -> io::Result<()> {
        let size = sealed.len() as u64;
        if size > self.budget {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let crowded = self.bytes + size > self.budget || self.files >= self.max_files;
        if crowded || self.pruned.elapsed() >= DAY {
            self.prune(retention, Some(size))?;
        }
        let path = self.folder.join(name);
        let replaced = fs::symlink_metadata(&path)
            .ok()
            .filter(fs::Metadata::is_file)
            .map(|old| old.len());
        let mut beside = tempfile::NamedTempFile::new_in(&self.folder)?;
        beside.write_all(sealed)?;
        beside.persist(&path)?;
        match replaced {
            Some(old) => self.bytes = self.bytes.saturating_sub(old),
            None => self.files += 1,
        }
        self.bytes += size;
        Ok(())
    }

    /// Delete the kept file at `path`, `size` bytes long.
    fn discard(&mut self, path: &Path, size: u64) {
        if fs::remove_file(path).is_ok() {
            self.bytes = self.bytes.saturating_sub(size);
            self.files = self.files.saturating_sub(1);
        }
    }

    /// Empty it: its folder goes, and comes back with nothing in it.
    fn clear(&mut self) -> io::Result<()> {
        remove_folder(&self.folder)?;
        (self.bytes, self.files) = (0, 0);
        private_folder(&self.folder)
    }
}

/// One account's pictures on disk.
pub(super) struct Disk {
    /// The account's name in the store, part of each file's place.
    account: String,
    key: Key,
    parts: [Part; 2],
    retention: Duration,
    /// Forgotten: nothing is read or written any more.
    closed: bool,
    usage: Arc<[AtomicU64; 2]>,
}

impl Disk {
    /// Open, making them, the folders of `account` under `root`. With
    /// `fresh` its key was made just now: what was sealed with the lost one
    /// would never open, so it goes first.
    fn open(
        root: &Path,
        account: &str,
        key: Key,
        fresh: bool,
        limits: CacheLimits,
    ) -> io::Result<Self> {
        let folder = folder(root, account);
        if fresh {
            remove_folder(&folder)?;
        }
        for path in [root.to_owned(), root.join(FOLDER), folder.clone()] {
            private_folder(&path)?;
        }
        let mut disk = Self {
            account: account.to_owned(),
            key,
            parts: [CachePart::Media, CachePart::Artwork].map(|part| {
                let mut kept = Part {
                    folder: folder.join(part.name()),
                    bytes: 0,
                    files: 0,
                    budget: 0,
                    max_files: 0,
                    pruned: Instant::now(),
                };
                kept.set_budget(part, limits.bytes(part));
                kept
            }),
            retention: limits.retention,
            closed: false,
            usage: Arc::default(),
        };
        for part in &mut disk.parts {
            private_folder(&part.folder)?;
            part.prune(disk.retention, None)?;
        }
        disk.publish();
        Ok(disk)
    }

    /// The picture kept as `url` for `kind`. A file there that cannot be
    /// one (past its kind's size, not shown within the retention, sealed
    /// with another key or at another place) goes.
    pub(super) fn read(&mut self, kind: ImageKind, url: &str) -> Option<Vec<u8>> {
        if self.closed {
            return None;
        }
        let part = CachePart::of(kind);
        let name = self.key.file_name(url);
        let path = self.parts[part.index()].folder.join(&name);
        // Appending gives the right to change its time; nothing is appended.
        let mut file = fs::File::options()
            .read(true)
            .append(true)
            .open(&path)
            .ok()?;
        let found = file.metadata().ok()?;
        let largest = super::byte_limit(kind) + SEALED_OVERHEAD;
        // A time ahead of the clock counts as just now.
        let shown_lately = found.modified().ok()?.elapsed().unwrap_or_default() < self.retention;
        let size = usize::try_from(found.len()).unwrap_or(usize::MAX);
        let mut plain = None;
        if found.is_file() && size <= largest && shown_lately {
            let mut sealed = vec![0; size];
            if file.read_exact(&mut sealed).is_ok() {
                plain = self
                    .key
                    .open_file(&self.account, part.name(), &name, &sealed);
            }
        }
        if plain.is_some() {
            // Seen now: the last of its part to go.
            let _ = file.set_modified(SystemTime::now());
        } else {
            drop(file);
            self.parts[part.index()].discard(&path, found.len());
            self.publish();
        }
        plain
    }

    /// Keep `bytes`, the picture `url` names for `kind`; when it cannot be
    /// kept the disk stays as it was and nobody is told: the picture shows
    /// from memory all the same.
    pub(super) fn write(&mut self, kind: ImageKind, url: &str, bytes: &[u8]) {
        if self.closed || bytes.is_empty() || bytes.len() > super::byte_limit(kind) {
            return;
        }
        let part = CachePart::of(kind);
        let name = self.key.file_name(url);
        let Some(sealed) = self.key.seal_file(&self.account, part.name(), &name, bytes) else {
            return;
        };
        let retention = self.retention;
        if self.parts[part.index()]
            .keep(&name, &sealed, retention)
            .is_ok()
        {
            self.publish();
        }
    }

    /// Empty `part` now; pictures shown keep showing from memory.
    fn clear(&mut self, part: CachePart) -> io::Result<()> {
        let result = self.parts[part.index()].clear();
        self.publish();
        result
    }

    /// The person's new limits: what falls outside them now is deleted at
    /// once, and room they add is used as pictures come.
    fn set_limits(&mut self, limits: CacheLimits) -> io::Result<()> {
        self.retention = limits.retention;
        let mut result = Ok(());
        for part in [CachePart::Media, CachePart::Artwork] {
            let kept = &mut self.parts[part.index()];
            kept.set_budget(part, limits.bytes(part));
            result = result.and(kept.prune(limits.retention, None));
        }
        self.publish();
        result
    }

    fn publish(&self) {
        for part in [CachePart::Media, CachePart::Artwork] {
            self.usage[part.index()].store(self.parts[part.index()].bytes, Ordering::Relaxed);
        }
    }
}

/// A name [`Key::file_name`] gives: 64 lowercase hexadecimal digits.
fn valid_name(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

/// The folder of `account` under `root`: a digest without a key, since
/// forgetting an account finds it without one. It hides the account's id
/// from a glance, but anyone who guesses the id can confirm it.
fn folder(root: &Path, account: &str) -> PathBuf {
    let digest = Sha256::digest(format!("rune images\0{account}").as_bytes());
    root.join(FOLDER)
        .join(format!("{:x}", digest).get(..32).unwrap_or_default())
}

/// `path` as a folder only its owner opens, made if missing.
fn private_folder(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        builder.mode(0o700);
        builder.create(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    builder.create(path)
}

fn remove_folder(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

type Shared = Arc<Mutex<Disk>>;

/// An account's disk once open, with what it keeps, read without waiting
/// for an operation that holds it.
struct Opened {
    account: String,
    disk: Shared,
    usage: Arc<[AtomicU64; 2]>,
    /// The ones it opened with.
    limits: CacheLimits,
}

/// The disk of the account whose key is in, for [`super::Images`]: opened,
/// changed, cleared and forgotten in order on the core's runtime, while
/// reads and writes of downloads take it as they run.
pub(crate) struct DiskCache {
    runtime: Handle,
    /// `$XDG_CACHE_HOME/rune`; `None` keeps nothing on disk.
    root: Option<PathBuf>,
    /// `None` until the host says them: opening with others could delete
    /// what the person's budget keeps.
    limits: Option<CacheLimits>,
    /// The account whose key came, waiting for the limits.
    waiting: Option<(String, Key, bool)>,
    /// The account whose disk is opening, with its key: downloads wait for
    /// it rather than ask the network for what it may keep.
    opening: Option<(String, Key)>,
    current: Option<Opened>,
    /// Every disk opened that may still be in use: forgetting its account
    /// closes it, so a download finishing late writes nothing.
    opened: Vec<Weak<Mutex<Disk>>>,
    /// Counts attachments: an open finishing after another came is dropped.
    generation: u64,
    /// What each open ended in: its disk, or nothing when it failed.
    sender: std::sync::mpsc::Sender<(u64, Option<Opened>)>,
    results: std::sync::mpsc::Receiver<(u64, Option<Opened>)>,
    /// The last operation: each waits for the one before.
    last: Option<JoinHandle<()>>,
    wake: Arc<dyn Fn() + Send + Sync>,
    /// Whether the session is the account's own, as its store verified it:
    /// downloads of any other neither read nor keep pictures here.
    pub(crate) trusted: bool,
}

impl DiskCache {
    pub(crate) fn new(runtime: Handle, wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, results) = std::sync::mpsc::channel();
        Self {
            runtime,
            root: None,
            limits: None,
            waiting: None,
            opening: None,
            current: None,
            opened: Vec::new(),
            generation: 0,
            sender,
            results,
            last: None,
            wake,
            trusted: true,
        }
    }

    pub(crate) fn set_root(&mut self, root: Option<PathBuf>) {
        self.root = root;
    }

    /// Run `operation` once every one asked before is done, off this thread.
    fn then(&mut self, operation: impl FnOnce() + Send + 'static) {
        let (before, wake) = (self.last.take(), self.wake.clone());
        self.last = Some(self.runtime.spawn(async move {
            if let Some(before) = before {
                let _ = before.await;
            }
            let _ = tokio::task::spawn_blocking(operation).await;
            wake();
        }));
    }

    /// Keep the pictures of `account`, sealed with `key`, from now on.
    pub(crate) fn attach(&mut self, account: String, key: Key, fresh: bool) {
        self.detach();
        self.waiting = Some((account, key, fresh));
        self.open_waiting();
    }

    /// Keep no account's pictures from now on; the ones kept stay.
    pub(crate) fn detach(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.current = None;
        self.waiting = None;
        self.opening = None;
    }

    fn open_waiting(&mut self) {
        let (Some(root), Some(limits)) = (self.root.clone(), self.limits) else {
            return;
        };
        let Some((account, key, fresh)) = self.waiting.take() else {
            return;
        };
        self.opening = Some((account.clone(), key.clone()));
        let (generation, sender) = (self.generation, self.sender.clone());
        self.then(move || {
            let opened = Disk::open(&root, &account, key, fresh, limits)
                .map_err(|error| eprintln!("rune: no picture cache on disk: {error}"))
                .ok()
                .map(|disk| Opened {
                    account,
                    usage: disk.usage.clone(),
                    disk: Arc::new(Mutex::new(disk)),
                    limits,
                });
            let _ = sender.send((generation, opened));
        });
    }

    /// The budgets and retention the person chose; the first ones open the
    /// disk of an account whose key came before them.
    pub(crate) fn set_limits(&mut self, limits: CacheLimits) {
        if self.limits == Some(limits) {
            return;
        }
        self.limits = Some(limits);
        self.open_waiting();
        if let Some(disk) = self.current() {
            self.then(move || {
                if let Ok(mut disk) = disk.lock() {
                    let _ = disk.set_limits(limits);
                }
            });
        }
    }

    /// Take the open that finished, if it is still wanted.
    fn take_opened(&mut self) {
        while let Ok((generation, opened)) = self.results.try_recv() {
            if generation != self.generation {
                continue;
            }
            self.opening = None;
            let Some(opened) = opened else {
                continue;
            };
            self.opened.retain(|old| old.strong_count() > 0);
            self.opened.push(Arc::downgrade(&opened.disk));
            // The person chose others while it opened.
            if let Some(limits) = self.limits.filter(|limits| *limits != opened.limits) {
                let disk = opened.disk.clone();
                self.then(move || {
                    if let Ok(mut disk) = disk.lock() {
                        let _ = disk.set_limits(limits);
                    }
                });
            }
            self.current = Some(opened);
        }
    }

    /// The disk to read and write now, once its open finished.
    pub(super) fn current(&mut self) -> Option<Shared> {
        self.take_opened();
        self.current.as_ref().map(|opened| opened.disk.clone())
    }

    /// Whether the account's disk is still opening: what it may keep is not
    /// asked of the network meanwhile.
    pub(super) fn opening(&mut self) -> bool {
        self.take_opened();
        self.opening.is_some()
    }

    /// What each part keeps, once the account's disk is open.
    pub(crate) fn usage(&mut self) -> Option<CacheUsage> {
        self.take_opened();
        let usage = &self.current.as_ref()?.usage;
        Some(CacheUsage {
            media_bytes: usage[0].load(Ordering::Relaxed),
            artwork_bytes: usage[1].load(Ordering::Relaxed),
        })
    }

    /// Empty one part of the account's disk.
    pub(crate) fn clear(&mut self, part: CachePart) {
        if let Some(disk) = self.current() {
            self.then(move || {
                if let Ok(mut disk) = disk.lock() {
                    let _ = disk.clear(part);
                }
            });
        }
    }

    /// Delete what is kept of `account`, as «Borrar datos locales» does: its
    /// open disk stays open, empty; without its key, its folder goes.
    pub(crate) fn erase(&mut self, account: String) {
        self.take_opened();
        if self
            .current
            .as_ref()
            .is_some_and(|opened| opened.account == account)
        {
            self.clear(CachePart::Media);
            self.clear(CachePart::Artwork);
            return;
        }
        // Still opening: that open would bring back what goes, and keep
        // writing to a folder no longer there, so it opens again after.
        let reopen = match &self.opening {
            Some((opening, key)) if *opening == account => Some(key.clone()),
            _ => None,
        };
        if reopen.is_some() {
            self.detach();
        }
        self.remove(account.clone());
        if let Some(key) = reopen {
            self.attach(account, key, false);
        }
    }

    /// Delete every picture kept of `account`, its folder too, and close
    /// its disks, here, opening or still held by a download.
    pub(crate) fn forget(&mut self, account: String) {
        self.take_opened();
        let current = self.current.as_ref().map(|opened| &opened.account);
        let opening = self.opening.as_ref().map(|(account, _)| account);
        let waiting = self.waiting.as_ref().map(|(account, ..)| account);
        if [current, opening, waiting].contains(&Some(&account)) {
            self.detach();
        }
        self.remove(account);
    }

    /// Close the disks of `account`, here or still held by a download, and
    /// delete its folder, after every operation asked before.
    fn remove(&mut self, account: String) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let opened = self.opened.clone();
        self.then(move || {
            // Closed under its lock: a write that waits for it writes nothing.
            for disk in opened.iter().filter_map(Weak::upgrade) {
                if let Ok(mut disk) = disk.lock() {
                    if disk.account == account {
                        disk.closed = true;
                    }
                }
            }
            if let Err(error) = remove_folder(&folder(&root, &account)) {
                eprintln!("rune: an account's pictures stay on disk: {error}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(media: u64, artwork: u64) -> CacheLimits {
        CacheLimits {
            media_bytes: media,
            artwork_bytes: artwork,
            retention: DAY,
        }
    }

    fn open(root: &Path, key: &Key, limits: CacheLimits) -> Disk {
        Disk::open(root, "4:mock:1", key.clone(), false, limits).unwrap()
    }

    fn files(folder: &Path) -> Vec<PathBuf> {
        let mut found: Vec<_> = fs::read_dir(folder)
            .map(|entries| entries.map(|entry| entry.unwrap().path()).collect())
            .unwrap_or_default();
        found.sort();
        found
    }

    #[test]
    fn pictures_come_back_sealed_private_and_only_to_their_account() {
        let root = tempfile::tempdir().unwrap();
        let key = Key::generate().unwrap();
        let mut disk = open(root.path(), &key, CacheLimits::default());
        let picture = b"\x89PNG not really, but bytes";
        disk.write(ImageKind::Artwork, "https://cdn.test/a.png", picture);
        disk.write(
            ImageKind::AttachmentPreview,
            "https://cdn.test/b.png",
            b"preview",
        );
        assert_eq!(
            disk.read(ImageKind::Artwork, "https://cdn.test/a.png")
                .as_deref(),
            Some(&picture[..])
        );
        assert!(disk
            .read(ImageKind::Artwork, "https://cdn.test/b.png")
            .is_none());
        assert_eq!(
            disk.read(ImageKind::Clip, "https://cdn.test/b.png")
                .as_deref(),
            Some(&b"preview"[..])
        );
        let usage = disk.usage.clone();
        assert_eq!(
            usage[1].load(Ordering::Relaxed),
            (picture.len() + SEALED_OVERHEAD) as u64
        );
        // Neither the URL nor the picture is written as it is, nor a name
        // anyone could work out from the URL.
        let account = folder(root.path(), "4:mock:1");
        assert!(!account.to_string_lossy().contains("mock"));
        let stored = files(&account.join("artwork"));
        assert_eq!(stored.len(), 1);
        let unkeyed = format!("{:x}", Sha256::digest(b"https://cdn.test/a.png"));
        assert_ne!(stored[0].file_name().unwrap().to_str(), Some(&*unkeyed));
        let sealed = fs::read(&stored[0]).unwrap();
        assert!(!sealed.windows(4).any(|window| window == b"PNG "));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&stored[0]), 0o600);
            for folder in [
                root.path().to_owned(),
                root.path().join(FOLDER),
                account.clone(),
                account.join("media"),
            ] {
                assert_eq!(mode(&folder), 0o700, "{}", folder.display());
            }
        }
        // Another key does not even find it under its names...
        let other = Key::generate().unwrap();
        let mut stranger = Disk::open(
            root.path(),
            "4:mock:1",
            other,
            false,
            CacheLimits::default(),
        )
        .unwrap();
        assert!(stranger
            .read(ImageKind::Artwork, "https://cdn.test/a.png")
            .is_none());
        assert_eq!(files(&account.join("artwork")), stored);
        // ...and moved to another picture's name it does not open, and goes.
        let moved = account
            .join("artwork")
            .join(key.file_name("https://cdn.test/c.png"));
        fs::rename(&stored[0], &moved).unwrap();
        assert!(disk
            .read(ImageKind::Artwork, "https://cdn.test/c.png")
            .is_none());
        assert!(files(&account.join("artwork")).is_empty());
        assert_eq!(disk.usage[1].load(Ordering::Relaxed), 0);
    }

    #[test]
    fn budgets_evict_the_oldest_and_retention_and_leftovers_go_on_open() {
        let root = tempfile::tempdir().unwrap();
        let key = Key::generate().unwrap();
        let each = 10_000;
        let sealed = (each + SEALED_OVERHEAD) as u64;
        let mut disk = open(root.path(), &key, limits(10 * sealed, 64 * 1024 * 1024));
        let url = |n: usize| format!("https://cdn.test/{n}.png");
        let base = SystemTime::now() - Duration::from_secs(3600);
        for n in 0..10 {
            disk.write(ImageKind::AttachmentPreview, &url(n), &vec![n as u8; each]);
            let path = disk.parts[0].folder.join(key.file_name(&url(n)));
            let file = fs::File::options().write(true).open(path).unwrap();
            file.set_modified(base + Duration::from_secs(n as u64))
                .unwrap();
        }
        assert_eq!(disk.parts[0].files, 10);
        // Seen again, the first is now the newest.
        assert!(disk.read(ImageKind::AttachmentPreview, &url(0)).is_some());
        disk.write(ImageKind::AttachmentPreview, &url(10), &vec![10; each]);
        // Down to 90 % with the new one: the oldest two not seen again went.
        assert!(disk.parts[0].bytes <= 9 * sealed);
        for gone in [1, 2] {
            assert!(disk
                .read(ImageKind::AttachmentPreview, &url(gone))
                .is_none());
        }
        for kept in [0, 3, 9, 10] {
            assert!(
                disk.read(ImageKind::AttachmentPreview, &url(kept))
                    .is_some(),
                "{kept}"
            );
        }
        // Too large for its part, or for its kind: not kept.
        disk.write(
            ImageKind::Artwork,
            &url(20),
            &vec![0; super::super::MAX_BYTES + 1],
        );
        assert!(disk.read(ImageKind::Artwork, &url(20)).is_none());
        // A leftover and an expired file go when the disk opens again.
        let media = disk.parts[0].folder.clone();
        fs::write(media.join(".tmpX4ab9Q"), b"half").unwrap();
        let expired = media.join(key.file_name(&url(3)));
        fs::File::options()
            .write(true)
            .open(&expired)
            .unwrap()
            .set_modified(SystemTime::now() - 2 * DAY)
            .unwrap();
        drop(disk);
        let mut disk = open(root.path(), &key, limits(10 * sealed, 64 * 1024 * 1024));
        assert!(!media.join(".tmpX4ab9Q").exists() && !expired.exists());
        assert!(disk.read(ImageKind::AttachmentPreview, &url(9)).is_some());
        // A smaller budget deletes at once; clearing one part leaves the other.
        disk.write(ImageKind::Artwork, &url(30), b"emoji");
        disk.set_limits(limits(2 * sealed, 64 * 1024 * 1024))
            .unwrap();
        assert!(disk.parts[0].bytes <= sealed && disk.parts[0].files >= 1);
        disk.clear(CachePart::Media).unwrap();
        assert_eq!(disk.usage[0].load(Ordering::Relaxed), 0);
        assert!(disk.read(ImageKind::Artwork, &url(30)).is_some());
    }

    #[test]
    fn a_fresh_key_starts_empty_and_forgetting_closes_disks_in_use() {
        let root = tempfile::tempdir().unwrap();
        let key = Key::generate().unwrap();
        let mut disk = open(root.path(), &key, CacheLimits::default());
        disk.write(ImageKind::Artwork, "https://cdn.test/a.png", b"a");
        drop(disk);
        let account = folder(root.path(), "4:mock:1");
        let fresh = Key::generate().unwrap();
        let disk = Disk::open(
            root.path(),
            "4:mock:1",
            fresh.clone(),
            true,
            CacheLimits::default(),
        )
        .unwrap();
        assert!(files(&account.join("artwork")).is_empty());
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let mut cache = DiskCache::new(runtime.handle().clone(), Arc::new(|| {}));
        cache.set_root(Some(root.path().to_owned()));
        // A download still holds the disk the account had.
        let held = Arc::new(Mutex::new(disk));
        cache.opened.push(Arc::downgrade(&held));
        cache.forget("4:mock:1".into());
        runtime.block_on(cache.last.take().unwrap()).unwrap();
        assert!(!account.exists());
        let mut held = held.lock().unwrap();
        held.write(ImageKind::Artwork, "https://cdn.test/b.png", b"b");
        assert!(
            !account.exists()
                && held
                    .read(ImageKind::Artwork, "https://cdn.test/b.png")
                    .is_none()
        );
    }

    #[test]
    fn the_disk_waits_for_the_limits_and_a_stale_open_is_dropped() {
        let root = tempfile::tempdir().unwrap();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let mut cache = DiskCache::new(runtime.handle().clone(), Arc::new(|| {}));
        cache.set_root(Some(root.path().to_owned()));
        cache.attach("4:mock:1".into(), Key::generate().unwrap(), false);
        assert!(cache.last.is_none() && cache.usage().is_none());
        cache.set_limits(CacheLimits::default());
        runtime.block_on(cache.last.take().unwrap()).unwrap();
        assert_eq!(cache.usage(), Some(CacheUsage::default()));
        // Another account took the key while the first one's disk opened.
        cache.attach("4:mock:2".into(), Key::generate().unwrap(), false);
        cache.attach("4:mock:3".into(), Key::generate().unwrap(), false);
        let generation = cache.generation;
        runtime.block_on(cache.last.take().unwrap()).unwrap();
        let disk = cache.current().unwrap();
        assert_eq!(disk.lock().unwrap().account, "4:mock:3");
        assert_eq!(cache.generation, generation);
        cache.detach();
        assert!(cache.current().is_none() && cache.usage().is_none());
    }

    /// A cache on its own runtime whose operations wait until the test lets
    /// them go.
    fn held_cache(
        root: &Path,
    ) -> (
        tokio::runtime::Runtime,
        DiskCache,
        std::sync::mpsc::Sender<()>,
    ) {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let mut cache = DiskCache::new(runtime.handle().clone(), Arc::new(|| {}));
        cache.set_root(Some(root.to_owned()));
        let (release, held) = std::sync::mpsc::channel::<()>();
        cache.then(move || {
            let _ = held.recv();
        });
        (runtime, cache, release)
    }

    #[test]
    fn erasing_while_the_disk_opens_leaves_it_open_empty_and_writable() {
        let root = tempfile::tempdir().unwrap();
        let key = Key::generate().unwrap();
        let mut before = open(root.path(), &key, CacheLimits::default());
        before.write(ImageKind::Artwork, "https://cdn.test/a.png", b"a");
        drop(before);
        let (runtime, mut cache, release) = held_cache(root.path());
        cache.set_limits(CacheLimits::default());
        cache.attach("4:mock:1".into(), key, false);
        assert!(cache.opening());
        cache.erase("4:mock:1".into());
        assert!(cache.opening());
        release.send(()).unwrap();
        runtime.block_on(cache.last.take().unwrap()).unwrap();
        let disk = cache.current().expect("opened again after the erase");
        assert!(!cache.opening());
        assert_eq!(cache.usage(), Some(CacheUsage::default()));
        let mut disk = disk.lock().unwrap();
        assert!(disk
            .read(ImageKind::Artwork, "https://cdn.test/a.png")
            .is_none());
        disk.write(ImageKind::Artwork, "https://cdn.test/b.png", b"b");
        assert_eq!(
            disk.read(ImageKind::Artwork, "https://cdn.test/b.png")
                .as_deref(),
            Some(&b"b"[..])
        );
    }

    #[test]
    fn limits_chosen_while_the_disk_opens_reach_it() {
        let root = tempfile::tempdir().unwrap();
        let (runtime, mut cache, release) = held_cache(root.path());
        cache.set_limits(CacheLimits::default());
        cache.attach("4:mock:1".into(), Key::generate().unwrap(), false);
        let chosen = limits(MIB, MIB);
        cache.set_limits(chosen);
        release.send(()).unwrap();
        runtime.block_on(cache.last.take().unwrap()).unwrap();
        let disk = cache.current().unwrap();
        runtime.block_on(cache.last.take().unwrap()).unwrap();
        let disk = disk.lock().unwrap();
        assert_eq!(disk.retention, chosen.retention);
        assert_eq!(disk.parts[0].budget, MIB);
    }

    #[test]
    fn an_open_that_fails_lets_downloads_go_to_the_network() {
        let root = tempfile::tempdir().unwrap();
        // Its folder cannot be made: a file stands where it goes.
        let taken = root.path().join("rune");
        fs::write(&taken, b"").unwrap();
        let (runtime, mut cache, release) = held_cache(&taken);
        cache.set_limits(CacheLimits::default());
        cache.attach("4:mock:1".into(), Key::generate().unwrap(), false);
        assert!(cache.opening());
        release.send(()).unwrap();
        runtime.block_on(cache.last.take().unwrap()).unwrap();
        assert!(!cache.opening() && cache.current().is_none());
    }
}
