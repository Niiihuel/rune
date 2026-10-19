//! RUNE's log: what the process writes to stderr (the `rune: …` lines of
//! voice, the Gateway and the client, and any panic) is also kept in
//! `$XDG_STATE_HOME/rune/rune.log` (by default `~/.local/state/rune`), so a
//! failed call can still be reported once the terminal is gone. Rotated by
//! size: [`LIMIT`] bytes a file, [`FILES`] files. The folder is 0700 and the
//! files 0600. The lines are the ones stderr already shows: they never carry
//! a token, cookie or captcha answer, but a panic's text can quote whatever
//! string it was about, and the file keeps it (docs/SECURITY.md).
//!
//! Stderr (descriptor 2) is pointed at a pipe, and a thread copies what
//! arrives to the original stderr and to the file. No crate in between.
//! A panic waits, briefly, for that thread before it can abort; an abort or a
//! signal from C code can still end the process with its last words unread.
//!
//! On Windows (windows-base) it is the process's standard error handle that
//! points at the pipe, and the file is `%LOCALAPPDATA%\RUNE\logs\rune.log`,
//! in the user's profile, which only they may read. RUNE has no console
//! there: the file is the only place its lines go, unless stderr was
//! redirected when it started.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd, OwnedFd as Owned};
#[cfg(unix)]
use std::os::raw::{c_int, c_ulong};
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
#[cfg(windows)]
use std::os::windows::io::{AsHandle, AsRawHandle, OwnedHandle as Owned};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Bytes a file holds before the next one starts.
pub const LIMIT: u64 = 5 * 1024 * 1024;
/// Files kept: `rune.log`, `rune.log.1` and `rune.log.2`, the newest first.
pub const FILES: usize = 3;

/// Where the log lives: the platform's state directory.
pub fn folder() -> Option<PathBuf> {
    rune_platform::folders::state()
}

/// The log's files in a folder, rotated by size.
pub struct Rotating {
    folder: PathBuf,
    file: File,
    written: u64,
}

impl Rotating {
    /// Continue `folder`'s newest file, making the folder if it is not there.
    pub fn open(folder: &Path) -> io::Result<Self> {
        #[cfg(unix)]
        {
            if let Some(parent) = folder.parent() {
                // The XDG base directory, when it is missing, is made private too.
                fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(parent)?;
            }
            match fs::DirBuilder::new().mode(0o700).create(folder) {
                Err(error) if error.kind() != io::ErrorKind::AlreadyExists => return Err(error),
                // One made before, or by hand, is made private too.
                _ => fs::set_permissions(folder, fs::Permissions::from_mode(0o700))?,
            }
        }
        // The user's profile is theirs alone already.
        #[cfg(windows)]
        fs::create_dir_all(folder)?;
        let file = private(&folder.join("rune.log"))?;
        Ok(Self {
            folder: folder.into(),
            written: file.metadata()?.len(),
            file,
        })
    }

    /// Append `bytes`, first starting a new file if they would pass [`LIMIT`].
    pub fn write(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.written > 0 && self.written + bytes.len() as u64 > LIMIT {
            // Windows renames over an open file only if it lets it: the old
            // one is closed first.
            #[cfg(windows)]
            {
                self.file = private(&self.folder.join("rune.log.new"))?;
            }
            for older in (1..FILES).rev() {
                let newer = self.path(older - 1);
                if newer.exists() {
                    fs::rename(newer, self.path(older))?;
                }
            }
            self.file = private(&self.path(0))?;
            #[cfg(windows)]
            let _ = fs::remove_file(self.folder.join("rune.log.new"));
            self.written = 0;
        }
        self.file.write_all(bytes)?;
        self.written += bytes.len() as u64;
        Ok(())
    }

    fn path(&self, index: usize) -> PathBuf {
        match index {
            0 => self.folder.join("rune.log"),
            index => self.folder.join(format!("rune.log.{index}")),
        }
    }
}

/// A file only its owner reads and writes, appended to.
fn private(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path)?;
    #[cfg(unix)]
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    Ok(file)
}

/// Whether the copying thread holds bytes it read and has not written yet.
static HOLDING: AtomicBool = AtomicBool::new(false);

/// Copy everything `from` gives to `terminal` and to `log`, until it ends.
/// It never stops on a failed write: a writer to stderr whose pipe nobody
/// reads would fail, and `eprintln!` panics then.
fn copy(mut from: impl Read, mut terminal: impl Write, mut log: Option<Rotating>) {
    let mut buffer = [0; 8192];
    loop {
        let read = match from.read(&mut buffer) {
            Ok(0) => return,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        };
        HOLDING.store(true, Ordering::SeqCst);
        let _ = terminal.write_all(&buffer[..read]);
        // A full disk or a removed folder ends the file, never stderr.
        if log
            .as_mut()
            .is_some_and(|log| log.write(&buffer[..read]).is_err())
        {
            log = None;
        }
        HOLDING.store(false, Ordering::SeqCst);
    }
}

#[cfg(unix)]
extern "C" {
    // POSIX's dup2 and ioctl, from the C library std already links.
    fn dup2(from: c_int, to: c_int) -> c_int;
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    // kernel32's own, which std already links.
    fn SetStdHandle(which: u32, handle: *mut std::ffi::c_void) -> i32;
    fn PeekNamedPipe(
        pipe: *mut std::ffi::c_void,
        buffer: *mut std::ffi::c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        left: *mut u32,
    ) -> i32;
}

/// `STD_ERROR_HANDLE`: (DWORD)-12.
#[cfg(windows)]
const STD_ERROR_HANDLE: u32 = -12i32 as u32;

// shortcut: Linux's FIONREAD on x86 and ARM, the BSDs' (macOS) elsewhere;
// Linux on mips, powerpc or sparc differs: check it if RUNE builds there.
#[cfg(target_os = "linux")]
const FIONREAD: c_ulong = 0x541B;
#[cfg(all(unix, not(target_os = "linux")))]
const FIONREAD: c_ulong = 0x4004_667F;

/// Whether the pipe `reader` reads from holds bytes nobody read yet.
#[cfg(unix)]
fn unread(reader: &impl AsFd) -> bool {
    let mut bytes: c_int = 0;
    // SAFETY: FIONREAD writes one int through the pointer, alive for the call.
    let asked = unsafe {
        ioctl(
            reader.as_fd().as_raw_fd(),
            FIONREAD,
            &mut bytes as *mut c_int,
        )
    };
    asked == 0 && bytes > 0
}

/// Whether the pipe `reader` reads from holds bytes nobody read yet.
#[cfg(windows)]
fn unread(reader: &impl AsHandle) -> bool {
    let mut bytes = 0u32;
    // SAFETY: only the count is written, through a pointer alive for the call.
    let asked = unsafe {
        PeekNamedPipe(
            reader.as_handle().as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut bytes,
            std::ptr::null_mut(),
        )
    };
    asked != 0 && bytes > 0
}

/// Make each panic wait, up to `limit`, until the thread copied what stderr
/// holds: one that aborts (in a C callback, or while panicking) would end
/// the process with its own message still in the pipe.
fn wait_on_panic(reader: Owned, limit: Duration) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        let until = Instant::now() + limit;
        // shortcut: the first pause covers the thread having read the last
        // bytes but not yet marked them held; a lock around a non-blocking
        // read would close that window, if a message is ever seen cut.
        loop {
            std::thread::sleep(Duration::from_millis(1));
            let copying = HOLDING.load(Ordering::SeqCst) || unread(&reader);
            if !copying || Instant::now() >= until {
                return;
            }
        }
    }));
}

/// Point stderr at `to`.
#[cfg(unix)]
fn point_stderr(to: &impl AsFd) -> io::Result<()> {
    // SAFETY: dup2 reads two descriptor numbers and replaces 2 atomically;
    // `to` stays open for the call, and std writes to 2 by number.
    match unsafe { dup2(to.as_fd().as_raw_fd(), 2) } {
        -1 => Err(io::Error::last_os_error()),
        _ => Ok(()),
    }
}

/// Point stderr at `to`, or at nothing: std asks for the handle at each
/// write. `to` must stay open while stderr points at it.
#[cfg(windows)]
fn point_stderr(to: Option<&impl AsHandle>) -> io::Result<()> {
    let handle = to.map_or(std::ptr::null_mut(), |to| to.as_handle().as_raw_handle());
    // SAFETY: a plain call; the handle is open, or null for none.
    match unsafe { SetStdHandle(STD_ERROR_HANDLE, handle) } {
        0 => Err(io::Error::last_os_error()),
        _ => Ok(()),
    }
}

/// The stderr RUNE started with, its own copy; none for a Windows program
/// without a console.
fn original_stderr() -> Option<Owned> {
    #[cfg(unix)]
    let stderr = io::stderr().as_fd().try_clone_to_owned();
    #[cfg(windows)]
    let stderr = io::stderr().as_handle().try_clone_to_owned();
    stderr.ok()
}

/// What [`start`] changed. Dropping it gives stderr back and waits, briefly,
/// for what was written to reach the file.
pub struct Tee {
    terminal: Option<Owned>,
    /// Windows: the pipe's writing end, which stderr names without owning.
    #[cfg(windows)]
    writer: Option<io::PipeWriter>,
    copied: mpsc::Receiver<()>,
}

/// Copy stderr into the log from now on. `None`, and stderr as it was,
/// if there is no state folder or it cannot be written.
pub fn start() -> Option<Tee> {
    let mut log = match folder().map(|folder| Rotating::open(&folder)) {
        Some(Ok(log)) => log,
        Some(Err(error)) => {
            eprintln!("rune: no log file: {error}");
            return None;
        }
        None => return None,
    };
    let terminal = original_stderr();
    #[cfg(unix)]
    terminal.as_ref()?;
    let copy_to: Box<dyn Write + Send> = match &terminal {
        Some(terminal) => Box::new(File::from(terminal.try_clone().ok()?)),
        None => Box::new(io::sink()),
    };
    let (from, to) = io::pipe().ok()?;
    let reader = Owned::from(from.try_clone().ok()?);
    // Only in the file, which keeps several runs: the run and the time each
    // later line belongs to.
    let since = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs());
    let _ = log.write(format!("--- rune start {since} pid {}\n", std::process::id()).as_bytes());
    let (done, copied) = mpsc::channel();
    std::thread::Builder::new()
        .name("rune-log".into())
        .spawn(move || {
            copy(from, copy_to, Some(log));
            drop(done);
        })
        .ok()?;
    // The pipe's writing end closes here: stderr holds the only one.
    #[cfg(unix)]
    point_stderr(&to).ok()?;
    // Windows: stderr is a handle std looks up, not a copy of one, so the
    // Tee keeps the writing end open while stderr names it.
    #[cfg(windows)]
    point_stderr(Some(&to)).ok()?;
    wait_on_panic(reader, Duration::from_millis(200));
    Some(Tee {
        terminal,
        #[cfg(windows)]
        writer: Some(to),
        copied,
    })
}

impl Drop for Tee {
    fn drop(&mut self) {
        #[cfg(unix)]
        let given_back = self.terminal.as_ref().map(point_stderr);
        #[cfg(windows)]
        let given_back = Some(point_stderr(self.terminal.as_ref()));
        // Nothing names the pipe any more: it ends with this, its last end.
        #[cfg(windows)]
        drop(self.writer.take());
        if given_back.is_some_and(|given| given.is_ok()) {
            // The pipe ends when nothing else holds it; a child that does is not waited for.
            let _ = self.copied.recv_timeout(Duration::from_millis(500));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(name: &str) -> PathBuf {
        let folder = std::env::temp_dir()
            .join(format!("rune-log-{name}-{}", std::process::id()))
            .join("rune");
        let _ = fs::remove_dir_all(folder.parent().unwrap());
        folder
    }

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn files_rotate_by_size_and_keep_three() {
        let folder = folder("rotate");
        let mut log = Rotating::open(&folder).unwrap();
        let line = vec![b'x'; (LIMIT / 2) as usize];
        // Two halves fill a file; each further half starts the next.
        for _ in 0..2 * FILES + 1 {
            log.write(&line).unwrap();
        }
        log.write(b"last\n").unwrap();
        let size = |name: &str| fs::metadata(folder.join(name)).map(|m| m.len()).ok();
        assert_eq!(size("rune.log"), Some(LIMIT / 2 + 5));
        assert_eq!(size("rune.log.1"), Some(LIMIT));
        assert_eq!(size("rune.log.2"), Some(LIMIT));
        assert_eq!(size("rune.log.3"), None, "only {FILES} files are kept");
        assert!(fs::read(folder.join("rune.log"))
            .unwrap()
            .ends_with(b"last\n"));

        // A later start continues the newest file, not a fresh one.
        drop(log);
        let mut log = Rotating::open(&folder).unwrap();
        log.write(b"again\n").unwrap();
        assert_eq!(size("rune.log"), Some(LIMIT / 2 + 11));
        let _ = fs::remove_dir_all(folder.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn only_the_owner_reads_the_log() {
        let folder = folder("private");
        fs::create_dir_all(&folder).unwrap();
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(folder.join("rune.log"), b"before\n").unwrap();
        fs::set_permissions(folder.join("rune.log"), fs::Permissions::from_mode(0o644)).unwrap();
        let mut log = Rotating::open(&folder).unwrap();
        assert_eq!(mode(&folder), 0o700);
        assert_eq!(mode(&folder.join("rune.log")), 0o600);
        log.write(&vec![b'x'; LIMIT as usize]).unwrap();
        assert_eq!(mode(&folder.join("rune.log")), 0o600, "the next file too");
        assert_eq!(mode(&folder.join("rune.log.1")), 0o600);
        let _ = fs::remove_dir_all(folder.parent().unwrap());
    }

    /// The real descriptor 2, in a child process: this same test, run again.
    #[cfg(unix)]
    #[test]
    fn stderr_reaches_the_file_and_the_terminal_until_given_back() {
        const NAME: &str = "log::tests::stderr_reaches_the_file_and_the_terminal_until_given_back";
        if std::env::var_os("RUNE_LOG_TEST_CHILD").is_some() {
            let tee = start().expect("a log in the test's state folder");
            io::stderr().write_all(b"rune: voice: kept\n").unwrap();
            drop(tee);
            io::stderr()
                .write_all(b"rune: only the terminal\n")
                .unwrap();
            return;
        }
        let state = folder("tee").parent().unwrap().to_owned();
        let (child, pid) = run_child(NAME, &state);
        assert!(child.status.success(), "{child:?}");
        let terminal = String::from_utf8_lossy(&child.stderr);
        let kept = terminal.find("rune: voice: kept\n");
        assert!(kept.is_some(), "the terminal still sees it: {terminal}");
        assert!(
            kept < terminal.find("rune: only the terminal\n"),
            "{terminal}"
        );
        let file = fs::read_to_string(state.join("rune").join("rune.log")).unwrap();
        let (start, rest) = file.split_once('\n').unwrap();
        assert!(start.starts_with("--- rune start "), "{file}");
        assert!(start.ends_with(&format!(" pid {pid}")), "{file}");
        assert!(!terminal.contains("--- rune start"), "only in the file");
        assert_eq!(rest, "rune: voice: kept\n");
        let _ = fs::remove_dir_all(&state);
    }

    /// Run this binary's test `name` alone, as a child with its own stderr.
    #[cfg(unix)]
    fn run_child(name: &str, state: &Path) -> (std::process::Output, u32) {
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env("RUNE_LOG_TEST_CHILD", "1")
            .env("XDG_STATE_HOME", state)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        (child.wait_with_output().unwrap(), pid)
    }

    /// A panic that ends the process at once, as an abort does, still
    /// reaches the terminal and the file.
    #[cfg(unix)]
    #[test]
    fn a_panic_reaches_both_before_the_process_ends() {
        const NAME: &str = "log::tests::a_panic_reaches_both_before_the_process_ends";
        if std::env::var_os("RUNE_LOG_TEST_CHILD").is_some() {
            extern "C" {
                // Ends the process at once, as an abort does, with no core dump.
                fn _exit(code: c_int) -> !;
            }
            let _tee = start().expect("a log in the test's state folder");
            // The thread still has these to copy when the panic comes.
            io::stderr().write_all(&[b'.'; 48 * 1024]).unwrap();
            let _ = std::panic::catch_unwind(|| panic!("rune: voice: the last words"));
            // SAFETY: nothing after it; the Tee's drop and its wait are skipped.
            unsafe { _exit(3) }
        }
        let state = folder("panic").parent().unwrap().to_owned();
        let (child, _) = run_child(NAME, &state);
        assert_eq!(child.status.code(), Some(3), "{child:?}");
        let terminal = String::from_utf8_lossy(&child.stderr);
        assert!(
            terminal.contains("rune: voice: the last words"),
            "{terminal}"
        );
        let file = fs::read_to_string(state.join("rune").join("rune.log")).unwrap();
        assert!(file.contains("rune: voice: the last words"), "{file}");
        let _ = fs::remove_dir_all(&state);
    }

    #[test]
    fn what_stderr_gets_reaches_both_even_when_the_terminal_fails() {
        struct Gone;
        impl Write for Gone {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::ErrorKind::BrokenPipe.into())
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let lines = b"rune: voice: one\nrune: gateway: two\n";
        let folder = folder("copy");
        let mut terminal = Vec::new();
        copy(
            &lines[..],
            &mut terminal,
            Some(Rotating::open(&folder).unwrap()),
        );
        assert_eq!(terminal, lines);
        copy(&lines[..], Gone, Some(Rotating::open(&folder).unwrap()));
        let kept = fs::read(folder.join("rune.log")).unwrap();
        assert_eq!(kept, [&lines[..], &lines[..]].concat());
        let _ = fs::remove_dir_all(folder.parent().unwrap());
    }
}
