//! One RUNE per person: a second start hands its links to the first, which
//! comes to the front, and exits.
//!
//! The first run listens where only its user reaches: on Linux a socket in
//! `$XDG_RUNTIME_DIR`, on Windows a named pipe of this logon session that
//! only this user may write to. A later run connects, says what it was given
//! and leaves. Without a private runtime directory (Linux) every run is on
//! its own.

#[cfg(target_os = "linux")]
use std::io::Write;
use std::io::{BufRead, BufReader, Read};
#[cfg(target_os = "linux")]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc};
use std::time::Duration;

#[cfg(target_os = "linux")]
use crate::local_socket::{self, Bound};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::Instance;

#[cfg(target_os = "linux")]
const FILE: &str = "rune-instance.sock";
/// The first line, so a stray client is told apart.
const GREETING: &str = "RUNE1";
/// What one request may carry: a few links of reasonable length.
pub const MAX_LINKS: usize = 8;
pub const MAX_LINK: usize = 2048;
const MAX_REQUEST: u64 = (MAX_LINKS * (MAX_LINK + 8) + 64) as u64;
const TIMEOUT: Duration = Duration::from_secs(2);

/// What a later run asks of the first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Come to the front.
    Show,
    /// Come to the front and open this link, as one clicked in RUNE.
    Open(String),
}

/// How this run starts.
pub enum Launch {
    /// The first: it keeps the socket and hears later runs.
    First(Instance),
    /// Another run was there and took the links: this one should exit.
    Forwarded,
    /// No private runtime directory, or the socket failed: run alone.
    Alone,
}

/// The claim on the socket; listening starts once the window can wake.
#[cfg(target_os = "linux")]
pub struct Instance {
    listener: UnixListener,
    path: PathBuf,
}

/// Claim the instance, or hand `links` to the run that has it. Blocks for
/// at most a couple of seconds: call it before the window opens.
pub fn claim(links: &[String]) -> Launch {
    #[cfg(target_os = "linux")]
    {
        match local_socket::runtime_dir() {
            Some(dir) => claim_at(&dir.join(FILE), links),
            None => Launch::Alone,
        }
    }
    #[cfg(windows)]
    {
        self::windows::claim(links)
    }
}

#[cfg(target_os = "linux")]
fn claim_at(path: &Path, links: &[String]) -> Launch {
    match local_socket::bind(path) {
        Ok(Bound::Listening(listener)) => Launch::First(Instance {
            listener,
            path: path.to_owned(),
        }),
        Ok(Bound::Taken) => match forward(path, links) {
            Ok(()) => Launch::Forwarded,
            Err(error) => {
                eprintln!("rune: the open RUNE did not answer ({error}); starting another");
                Launch::Alone
            }
        },
        Err(error) => {
            eprintln!("rune: no single-instance socket ({error}); starting alone");
            Launch::Alone
        }
    }
}

#[cfg(target_os = "linux")]
fn forward(path: &Path, links: &[String]) -> std::io::Result<()> {
    let mut stream = UnixStream::connect(path)?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream.write_all(message(links).as_bytes())
}

/// What a later run says: the greeting, `show`, and its valid links.
fn message(links: &[String]) -> String {
    let mut message = format!("{GREETING}\nshow\n");
    for link in links.iter().filter(|link| valid(link)).take(MAX_LINKS) {
        message.push_str("open ");
        message.push_str(link);
        message.push('\n');
    }
    message
}

/// A link is one line of printable text, bounded.
fn valid(link: &str) -> bool {
    !link.is_empty() && link.len() <= MAX_LINK && !link.chars().any(char::is_control)
}

#[cfg(target_os = "linux")]
impl Instance {
    /// Hear later runs on a thread of its own; `wake` runs after each request.
    pub fn listen(self, wake: Arc<dyn Fn() + Send + Sync>) -> Listener {
        let (sender, requests) = mpsc::channel();
        let Instance { listener, path } = self;
        // A thread that cannot start only means later runs are not heard.
        let _ = std::thread::Builder::new()
            .name("rune-instance".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { continue };
                    if !heard(read(stream), &sender, &wake) {
                        return;
                    }
                }
            });
        Listener {
            requests,
            path: Some(path),
        }
    }
}

/// Hand what one later run asked to the listener; `false` once nobody
/// listens any more.
fn heard(
    requests: Vec<Request>,
    sender: &mpsc::Sender<Request>,
    wake: &Arc<dyn Fn() + Send + Sync>,
) -> bool {
    if requests.is_empty() {
        return true;
    }
    for request in requests {
        if sender.send(request).is_err() {
            return false;
        }
    }
    wake();
    true
}

/// What one later run asked, if it is this user's and well formed.
#[cfg(target_os = "linux")]
fn read(stream: UnixStream) -> Vec<Request> {
    if !local_socket::same_user(&stream) || stream.set_read_timeout(Some(TIMEOUT)).is_err() {
        return Vec::new();
    }
    parse(stream)
}

/// The requests in what a later run wrote, if it greeted.
fn parse(stream: impl Read) -> Vec<Request> {
    let mut lines = BufReader::new(stream.take(MAX_REQUEST)).lines();
    if !matches!(lines.next(), Some(Ok(line)) if line == GREETING) {
        return Vec::new();
    }
    let mut requests = Vec::new();
    for line in lines.map_while(Result::ok).take(MAX_LINKS + 1) {
        if line == "show" {
            requests.push(Request::Show);
        } else if let Some(link) = line.strip_prefix("open ").filter(|link| valid(link)) {
            requests.push(Request::Open(link.to_owned()));
        }
    }
    requests
}

/// Later runs' requests. Dropping it removes the socket, so the next start
/// is the first again.
pub struct Listener {
    requests: mpsc::Receiver<Request>,
    /// The socket's file, on Linux; a pipe goes with its last handle.
    path: Option<std::path::PathBuf>,
}

impl Listener {
    pub fn poll(&self) -> Option<Request> {
        self.requests.try_recv().ok()
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn next(listener: &Listener) -> Request {
        for _ in 0..200 {
            if let Some(request) = listener.poll() {
                return request;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no request arrived");
    }

    #[test]
    fn a_second_run_hands_its_links_to_the_first_and_a_stale_socket_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(FILE);
        let Launch::First(instance) = claim_at(&path, &[]) else {
            panic!("the first run claims")
        };
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let listener = instance.listen(Arc::new(|| {}));
        let link = "https://discord.com/channels/1/2/3".to_owned();
        assert!(matches!(
            claim_at(&path, &[link.clone(), "bad\nlink".into()]),
            Launch::Forwarded
        ));
        assert_eq!(next(&listener), Request::Show);
        assert_eq!(next(&listener), Request::Open(link));
        assert_eq!(listener.poll(), None, "the bad link never arrives");
        // A stranger that does not greet is ignored.
        let mut stranger = UnixStream::connect(&path).unwrap();
        stranger.write_all(b"hello\nshow\n").unwrap();
        drop(stranger);
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(listener.poll(), None);
        drop(listener);
        assert!(!path.exists(), "the socket goes with the run");
        // A crashed run's socket, with nobody behind it, is taken over.
        drop(UnixListener::bind(&path).unwrap());
        assert!(matches!(claim_at(&path, &[]), Launch::First(_)));
    }
}

#[cfg(test)]
mod protocol_tests {
    use super::*;

    #[test]
    fn a_later_run_says_show_and_only_its_valid_links() {
        let links = [
            "https://discord.com/channels/1/2".to_owned(),
            "bad\nlink".to_owned(),
            "x".repeat(MAX_LINK + 1),
        ];
        let said = message(&links);
        assert_eq!(said, "RUNE1\nshow\nopen https://discord.com/channels/1/2\n");
        assert_eq!(
            parse(said.as_bytes()),
            [Request::Show, Request::Open(links[0].clone())]
        );
        // Without the greeting nothing is heard.
        assert!(parse(&b"show\nopen https://a\n"[..]).is_empty());
    }
}
