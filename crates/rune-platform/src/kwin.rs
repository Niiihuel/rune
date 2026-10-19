//! Hiding the window in the tray on KDE Plasma under Wayland. KWin honors
//! minimizing a Wayland window but leaves it in the taskbar; a script run
//! through its scripting interface (`org.kde.KWin` at `/Scripting`) can
//! also take it off the taskbar, the pager and the window switcher. Serein
//! does the same (`platform/src/compositor/kwin.rs`, read 2026-10-18); this
//! is RUNE's own script and client.
//!
//! The scripts live in `$XDG_RUNTIME_DIR`, mode 0600, and name the window
//! by RUNE's process id only. A worker thread owns the D-Bus connection;
//! until KWin answered, [`KWin::ready`] is false and the caller minimizes.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::timeout;

const SERVICE: &str = "org.kde.KWin";
const SCRIPTING: &str = "/Scripting";
const SCRIPTING_INTERFACE: &str = "org.kde.kwin.Scripting";
const SCRIPT_INTERFACE: &str = "org.kde.kwin.Script";
const TIMEOUT: Duration = Duration::from_secs(2);

/// Where the window should be.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Shown,
    /// Minimized and off the taskbar, the pager and the switcher.
    Hidden,
}

/// The worker that carries out the latest [`Place`] asked for. Dropping it
/// unloads RUNE's script and removes its files.
pub struct KWin {
    wanted: watch::Sender<Place>,
    ready: Arc<AtomicBool>,
}

impl KWin {
    /// On a KDE session under Wayland, the worker; `None` elsewhere, where
    /// the window hides or minimizes by itself.
    pub fn detect() -> Option<Self> {
        std::env::var_os("WAYLAND_DISPLAY")?;
        let desktop = std::env::var("XDG_CURRENT_DESKTOP").ok()?;
        if !desktop
            .split(':')
            .any(|name| name.eq_ignore_ascii_case("KDE"))
        {
            return None;
        }
        let folder = crate::local_socket::runtime_dir()?;
        Some(Self::start(None, folder, std::process::id()))
    }

    fn start(address: Option<String>, folder: PathBuf, pid: u32) -> Self {
        let (wanted, asked) = watch::channel(Place::Shown);
        let ready = Arc::new(AtomicBool::new(false));
        let answered = ready.clone();
        let _ = std::thread::Builder::new()
            .name("rune-kwin".into())
            .spawn(move || {
                let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                else {
                    return;
                };
                runtime.block_on(work(address, folder, pid, asked, answered));
            });
        Self { wanted, ready }
    }

    /// KWin's scripting interface answered: [`Self::set`] hides the window.
    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Put the window there; only the newest wish is carried out. Never blocks.
    pub fn set(&self, place: Place) {
        self.wanted.send_replace(place);
    }
}

/// The script that puts the windows of process `pid` at `place`. It runs
/// on KWin 6 (`windowList`, `activeWindow`, `desktops`) and 5.
fn script(place: Place, pid: u32) -> String {
    let hide = place == Place::Hidden;
    let show = if hide {
        ""
    } else {
        "
        if (!window.onAllDesktops) {
            if (six) window.desktops = [workspace.currentDesktop];
            else window.desktop = workspace.currentDesktop;
        }
        if (six) workspace.activeWindow = window;
        else workspace.activeClient = window;"
    };
    format!(
        "// RUNE: its window {what} the tray.
(function () {{
    var six = typeof workspace.windowList === \"function\";
    var windows = six ? workspace.windowList() : workspace.clientList();
    for (var at = 0; at < windows.length; at++) {{
        var window = windows[at];
        if (window.pid !== {pid}) continue;
        window.skipTaskbar = {hide};
        window.skipPager = {hide};
        window.skipSwitcher = {hide};
        window.minimized = {hide};{show}
    }}
}})();
",
        what = if hide { "into" } else { "out of" },
    )
}

/// Write `text` at `path`, readable by this user only.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(text.as_bytes())
}

async fn work(
    address: Option<String>,
    folder: PathBuf,
    pid: u32,
    mut asked: watch::Receiver<Place>,
    ready: Arc<AtomicBool>,
) {
    let name = format!("rune-tray-{pid}");
    let paths = [Place::Shown, Place::Hidden].map(|place| {
        let suffix = if place == Place::Hidden {
            "hidden"
        } else {
            "shown"
        };
        folder.join(format!("{name}-{suffix}.js"))
    });
    let Some(connection) = timeout(TIMEOUT, connect(address, &name, pid, &paths))
        .await
        .ok()
        .flatten()
    else {
        eprintln!("rune: KWin's scripting did not answer; closing to the tray minimizes");
        for path in &paths {
            let _ = std::fs::remove_file(path);
        }
        return;
    };
    ready.store(true, Ordering::Release);
    while asked.changed().await.is_ok() {
        let place = *asked.borrow_and_update();
        let path = &paths[usize::from(place == Place::Hidden)];
        if timeout(TIMEOUT, run(&connection, &name, path))
            .await
            .ok()
            .flatten()
            .is_none()
        {
            eprintln!("rune: KWin did not run the script that hides the window");
        }
    }
    let _ = timeout(TIMEOUT, unload(&connection, &name)).await;
    for path in &paths {
        let _ = std::fs::remove_file(path);
    }
}

/// The session's bus (or `address`), once KWin's scripting answers there,
/// with both scripts written.
async fn connect(
    address: Option<String>,
    name: &str,
    pid: u32,
    paths: &[PathBuf; 2],
) -> Option<zbus::Connection> {
    let builder = match &address {
        Some(address) => zbus::connection::Builder::address(address.as_str()),
        None => zbus::connection::Builder::session(),
    }
    .ok()?;
    let connection = builder.method_timeout(TIMEOUT).build().await.ok()?;
    let _: bool = call(
        &connection,
        SCRIPTING,
        SCRIPTING_INTERFACE,
        "isScriptLoaded",
        &(name,),
    )
    .await?;
    for (place, path) in [Place::Shown, Place::Hidden].into_iter().zip(paths) {
        write_private(path, &script(place, pid)).ok()?;
    }
    Some(connection)
}

/// Load the script at `path` and run it. The one before is unloaded only
/// now: KWin reads a script after `run` returns.
async fn run(connection: &zbus::Connection, name: &str, path: &Path) -> Option<()> {
    unload(connection, name).await?;
    let id: i32 = call(
        connection,
        SCRIPTING,
        SCRIPTING_INTERFACE,
        "loadScript",
        &(path.to_str()?, name),
    )
    .await?;
    if id < 0 {
        return None;
    }
    // KWin 6 puts a loaded script under /Scripting, KWin 5 at the root.
    for object in [format!("{SCRIPTING}/Script{id}"), format!("/{id}")] {
        let ran: Option<()> = call(connection, &object, SCRIPT_INTERFACE, "run", &()).await;
        if ran.is_some() {
            return Some(());
        }
    }
    None
}

async fn unload(connection: &zbus::Connection, name: &str) -> Option<bool> {
    call(
        connection,
        SCRIPTING,
        SCRIPTING_INTERFACE,
        "unloadScript",
        &(name,),
    )
    .await
}

async fn call<B, R>(
    connection: &zbus::Connection,
    path: &str,
    interface: &str,
    method: &str,
    body: &B,
) -> Option<R>
where
    B: zbus::export::serde::Serialize + zbus::zvariant::DynamicType,
    R: for<'d> zbus::zvariant::DynamicDeserialize<'d>,
{
    connection
        .call_method(Some(SERVICE), path, Some(interface), method, body)
        .await
        .ok()?
        .body()
        .deserialize()
        .ok()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::portal::tests::bus::Bus;

    /// What the fake KWin was asked: each script loaded (its path, name
    /// and text, as read at load), each one run, and how many unloads.
    #[derive(Clone, Default)]
    struct Seen {
        loaded: Arc<Mutex<Vec<(String, String, String)>>>,
        ran: Arc<Mutex<Vec<String>>>,
        unloaded: Arc<Mutex<usize>>,
    }

    struct Scripting {
        seen: Seen,
    }

    #[zbus::interface(name = "org.kde.kwin.Scripting")]
    impl Scripting {
        #[zbus(name = "isScriptLoaded")]
        fn is_script_loaded(&self, _name: String) -> bool {
            false
        }

        /// Its id is its place among those loaded, from 1, as KWin's are.
        #[zbus(name = "loadScript")]
        fn load_script(&self, path: String, name: String) -> i32 {
            let text = std::fs::read_to_string(&path).unwrap();
            let mut loaded = self.seen.loaded.lock().unwrap();
            loaded.push((path, name, text));
            loaded.len() as i32
        }

        #[zbus(name = "unloadScript")]
        fn unload_script(&self, _name: String) -> bool {
            *self.seen.unloaded.lock().unwrap() += 1;
            true
        }
    }

    /// KWin 6's object for the script `id`: served before any is loaded,
    /// since an object server cannot grow while it answers a call.
    struct Script {
        id: usize,
        seen: Seen,
    }

    #[zbus::interface(name = "org.kde.kwin.Script")]
    impl Script {
        #[zbus(name = "run")]
        fn run(&self) {
            let text = self.seen.loaded.lock().unwrap()[self.id - 1].2.clone();
            self.seen.ran.lock().unwrap().push(text);
        }
    }

    async fn until(check: impl Fn() -> bool) {
        timeout(Duration::from_secs(5), async {
            while !check() {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("KWin's fake never saw it");
    }

    #[tokio::test]
    async fn hiding_and_showing_run_rune_s_script_for_its_process_only() {
        let bus = Bus::start().await;
        let service = bus.connect().await;
        let seen = Seen::default();
        let server = service.object_server();
        server
            .at(SCRIPTING, Scripting { seen: seen.clone() })
            .await
            .unwrap();
        for id in 1..=2 {
            let script = Script {
                id,
                seen: seen.clone(),
            };
            server
                .at(format!("/Scripting/Script{id}"), script)
                .await
                .unwrap();
        }
        service.request_name(SERVICE).await.unwrap();
        let folder = tempfile::tempdir().unwrap();
        let kwin = KWin::start(Some(bus.address.clone()), folder.path().into(), 42);
        until(|| kwin.ready()).await;
        let hidden = folder.path().join("rune-tray-42-hidden.js");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(&hidden).unwrap().permissions().mode() & 0o777,
            0o600
        );
        kwin.set(Place::Hidden);
        until(|| seen.ran.lock().unwrap().len() == 1).await;
        kwin.set(Place::Shown);
        until(|| seen.ran.lock().unwrap().len() == 2).await;
        let ran = seen.ran.lock().unwrap().clone();
        assert!(ran[0].contains("window.pid !== 42"), "{}", ran[0]);
        assert!(ran[0].contains("window.skipTaskbar = true;"));
        assert!(ran[0].contains("window.minimized = true;"));
        assert!(!ran[0].contains("activeWindow"));
        assert!(ran[1].contains("window.skipTaskbar = false;"));
        assert!(ran[1].contains("workspace.activeWindow = window;"));
        let loaded = seen.loaded.lock().unwrap().clone();
        assert_eq!(loaded[0].1, "rune-tray-42");
        assert_eq!(loaded[0].0, hidden.to_str().unwrap());
        // Each run unloads the one before; dropping unloads it and cleans up.
        drop(kwin);
        until(|| *seen.unloaded.lock().unwrap() == 3).await;
        until(|| !hidden.exists()).await;
    }

    #[tokio::test]
    async fn without_kwin_it_never_gets_ready_and_leaves_nothing() {
        let bus = Bus::start().await;
        let folder = tempfile::tempdir().unwrap();
        let kwin = KWin::start(Some(bus.address.clone()), folder.path().into(), 7);
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(!kwin.ready());
        // The worker gave up: nothing was written for KWin to read.
        until(|| std::fs::read_dir(folder.path()).unwrap().next().is_none()).await;
    }
}
