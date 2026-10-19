//! Which known game is running, by the names of this user's processes.
//! Off unless the person turns it on.
//!
//! Every ten seconds the watcher lists `/proc`, and for each process reads
//! only where its program lives (`/proc/N/exe`) and the program name it was
//! started with (the first, at most 512-byte, argument of `/proc/N/cmdline`:
//! Wine and Proton games show their Windows name only there). It never reads
//! the rest of a command line, a process's memory or any of its files. The
//! names are matched against a list the caller gives, by their last path
//! components. The same names, listed once, are the programs a person can
//! add as a game ([`programs`]).
//!
//! On Windows the processes of this logon session are listed instead, by
//! the full path of their programs ([`windows`]).

use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

/// A game as the list knows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    /// Its application, when the list has one.
    pub application: Option<u64>,
    pub name: String,
    /// Executable names or path tails, such as `game.exe` or `bin/game`.
    pub executables: Vec<String>,
}

/// The game running now, and since when (Unix milliseconds).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Playing {
    pub application: Option<u64>,
    pub name: String,
    pub since: i64,
    /// The executable it was found by, [`normalize`]d: what the person's
    /// list of games names it by.
    pub executable: String,
}

/// A program open now, as the person may add it as a game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    /// Its last path components, [`normalize`]d, as the watcher matches them.
    pub key: String,
    /// Those components as they are on disk, to show.
    pub path: String,
}

const INTERVAL: Duration = Duration::from_secs(10);
/// Path components looked at, from the end.
const TAIL: usize = 8;
/// Processes looked at in one scan.
#[cfg(any(target_os = "linux", windows))]
const MAX_PROCESSES: usize = 4096;

/// The watcher and its thread. Dropping it stops them.
pub struct GameWatcher {
    playing: mpsc::Receiver<Option<Playing>>,
    games: Arc<Mutex<Option<Arc<Index>>>>,
    /// Asks for a scan at once; dropping it stops the worker.
    rescan: mpsc::Sender<()>,
}

impl GameWatcher {
    /// Scan now and every ten seconds; `wake` runs after each change.
    pub fn start(wake: Arc<dyn Fn() + Send + Sync>) -> Self {
        let (sender, playing) = mpsc::channel();
        let (stop, stopped) = mpsc::channel::<()>();
        let games: Arc<Mutex<Option<Arc<Index>>>> = Arc::default();
        let list = games.clone();
        let _ = std::thread::Builder::new()
            .name("rune-games".into())
            .spawn(move || {
                let mut current: Option<(String, Playing)> = None;
                loop {
                    let index = list.lock().ok().and_then(|list| list.clone());
                    if let Some(index) = index {
                        let found = choose(&index, &processes(), current.as_ref());
                        let changed = found.as_ref().map(|(key, _)| key)
                            != current.as_ref().map(|(key, _)| key);
                        if changed {
                            current = found.map(|(key, game)| {
                                let playing = Playing {
                                    application: game.application,
                                    name: game.name.clone(),
                                    since: now_ms(),
                                    executable: key.clone(),
                                };
                                (key, playing)
                            });
                            if sender
                                .send(current.as_ref().map(|(_, playing)| playing.clone()))
                                .is_err()
                            {
                                return;
                            }
                            wake();
                        }
                    }
                    match stopped.recv_timeout(INTERVAL) {
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                        // A message means a new list: scan again at once.
                        Ok(()) => {}
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
            });
        Self {
            playing,
            games,
            rescan: stop,
        }
    }

    /// Match against `games` from now on.
    pub fn set_games(&self, games: &[Game]) {
        if let Ok(mut list) = self.games.lock() {
            *list = Some(Arc::new(Index::new(games)));
        }
        let _ = self.rescan.send(());
    }

    /// The newest change: a game started (or another took over), or none runs.
    pub fn poll(&self) -> Option<Option<Playing>> {
        self.playing.try_iter().last()
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

/// The list by normalized executable name.
pub(crate) struct Index {
    games: Vec<Game>,
    by_name: HashMap<String, usize>,
}

impl Index {
    pub(crate) fn new(games: &[Game]) -> Self {
        let mut by_name = HashMap::new();
        for (at, game) in games.iter().enumerate() {
            for executable in &game.executables {
                let name = normalize(executable);
                if !name.is_empty() {
                    by_name.entry(name).or_insert(at);
                }
            }
        }
        Self {
            games: games.to_vec(),
            by_name,
        }
    }

    /// The game a process path names, by its longest known tail.
    fn find(&self, path: &str) -> Option<(String, &Game)> {
        let path = normalize(path);
        let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        let first = parts.len().saturating_sub(TAIL);
        (first..parts.len()).find_map(|from| {
            let tail = parts[from..].join("/");
            self.by_name.get(&tail).map(|&at| (tail, &self.games[at]))
        })
    }
}

/// Lowercase, forward slashes, no surrounding slashes or launcher mark.
pub fn normalize(name: &str) -> String {
    name.trim()
        .trim_start_matches('>')
        .to_lowercase()
        .replace('\\', "/")
        .trim_matches('/')
        .to_owned()
}

/// The game to report among `paths`: the one already reported while it
/// still runs, so two games do not flap; otherwise the first found.
fn choose<'a>(
    index: &'a Index,
    paths: &[String],
    current: Option<&(String, Playing)>,
) -> Option<(String, &'a Game)> {
    let found: Vec<(String, &Game)> = paths.iter().filter_map(|path| index.find(path)).collect();
    if let Some((key, _)) = current {
        if let Some(kept) = found.iter().find(|(found, _)| found == key) {
            return Some(kept.clone());
        }
    }
    found.into_iter().next()
}

/// Programs shown to choose from, at most.
const MAX_PROGRAMS: usize = 512;

/// This user's programs open now, each once, by their last path components
/// (what [`GameWatcher`] can match), sorted. Blocking: it reads `/proc`.
pub fn programs() -> Vec<Program> {
    listed(processes())
}

fn listed(paths: Vec<String>) -> Vec<Program> {
    let mut seen = std::collections::HashSet::new();
    let mut programs: Vec<Program> = paths
        .into_iter()
        .filter_map(|path| {
            let path = path.trim().trim_start_matches('>').replace('\\', "/");
            let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
            let path = parts[parts.len().saturating_sub(TAIL)..].join("/");
            let key = normalize(&path);
            (!key.is_empty() && !path.chars().any(char::is_control) && seen.insert(key.clone()))
                .then_some(Program { key, path })
        })
        .collect();
    programs.sort_by(|a, b| a.key.cmp(&b.key));
    programs.truncate(MAX_PROGRAMS);
    programs
}

/// Where each of this user's programs lives and the name it was started
/// with; `/proc` of other users' processes cannot be read anyway.
#[cfg(target_os = "linux")]
fn processes() -> Vec<String> {
    use std::io::Read;
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for entry in entries.flatten().take(MAX_PROCESSES * 2) {
        let name = entry.file_name();
        if !name
            .to_str()
            .is_some_and(|pid| pid.bytes().all(|b| b.is_ascii_digit()))
        {
            continue;
        }
        let process = entry.path();
        if let Ok(exe) = std::fs::read_link(process.join("exe")) {
            if let Some(exe) = exe.to_str() {
                paths.push(exe.to_owned());
            }
        }
        // Only the first argument, and only so much of it.
        let mut first = [0u8; 512];
        let read = std::fs::File::open(process.join("cmdline"))
            .and_then(|mut file| file.read(&mut first))
            .unwrap_or(0);
        let first = &first[..read];
        let end = first.iter().position(|b| *b == 0).unwrap_or(first.len());
        if let Ok(argument) = std::str::from_utf8(&first[..end]) {
            if !argument.is_empty() {
                paths.push(argument.to_owned());
            }
        }
        if paths.len() >= MAX_PROCESSES {
            break;
        }
    }
    paths
}

// Windows (windows-medios).
#[cfg(windows)]
mod windows;
#[cfg(windows)]
use self::windows::processes;

#[cfg(not(any(target_os = "linux", windows)))]
fn processes() -> Vec<String> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> Index {
        Index::new(&[
            Game {
                application: Some(1),
                name: "Juego".into(),
                executables: vec!["Juego.exe".into(), "linux/juego".into()],
            },
            Game {
                application: None,
                name: "Otro".into(),
                executables: vec![">otro.x86_64".into()],
            },
        ])
    }

    #[test]
    fn a_path_matches_by_its_longest_known_tail_in_any_case_and_slash() {
        let index = list();
        let found = |path: &str| index.find(path).map(|(_, game)| game.name.clone());
        assert_eq!(found(r"Z:\Games\JUEGO.EXE").as_deref(), Some("Juego"));
        assert_eq!(found("/opt/juego/linux/juego").as_deref(), Some("Juego"));
        assert_eq!(found("/usr/bin/otro.x86_64").as_deref(), Some("Otro"));
        assert_eq!(found("/usr/bin/juego"), None, "only the listed tail");
        assert_eq!(found("/usr/bin/bash"), None);
    }

    #[test]
    fn the_game_reported_stays_while_it_runs() {
        let index = list();
        let paths = vec!["/a/otro.x86_64".to_owned(), "C:/x/juego.exe".to_owned()];
        let (key, first) = choose(&index, &paths, None).unwrap();
        assert_eq!(first.name, "Otro");
        let playing = Playing {
            application: None,
            name: "Juego".into(),
            since: 0,
            executable: "juego.exe".into(),
        };
        let current = ("juego.exe".to_owned(), playing);
        let (kept, game) = choose(&index, &paths, Some(&current)).unwrap();
        assert_eq!((kept.as_str(), game.name.as_str()), ("juego.exe", "Juego"));
        assert_ne!(key, kept);
        assert!(choose(&index, &["/bin/sh".into()], Some(&current)).is_none());
    }

    #[test]
    fn programs_are_listed_once_by_their_tail_and_match_as_listed() {
        let programs = listed(vec![
            "/home/a/.local/share/Steam/steamapps/common/Juego/bin/Juego.x86_64".into(),
            "/HOME/A/.local/share/Steam/steamapps/common/juego/BIN/juego.x86_64".into(),
            r"Z:\Games\Otro.exe".into(),
            "".into(),
        ]);
        assert_eq!(
            programs,
            [
                Program {
                    key: ".local/share/steam/steamapps/common/juego/bin/juego.x86_64".into(),
                    path: ".local/share/Steam/steamapps/common/Juego/bin/Juego.x86_64".into(),
                },
                Program {
                    key: "z:/games/otro.exe".into(),
                    path: "Z:/Games/Otro.exe".into(),
                },
            ]
        );
        // Added as a game, the key finds the program again.
        let index = Index::new(&[Game {
            application: None,
            name: "Juego".into(),
            executables: vec![programs[0].path.clone()],
        }]);
        let (key, _) = index
            .find("/home/a/.local/share/Steam/steamapps/common/Juego/bin/Juego.x86_64")
            .unwrap();
        assert_eq!(key, programs[0].key);
    }

    #[cfg(any(target_os = "linux", windows))]
    #[test]
    fn this_test_finds_itself_by_name() {
        let me = std::env::current_exe().unwrap();
        let name = me.file_name().unwrap().to_str().unwrap().to_owned();
        let index = Index::new(&[Game {
            application: Some(9),
            name: "Prueba".into(),
            executables: vec![name],
        }]);
        assert!(processes().iter().any(|path| index.find(path).is_some()));
    }
}
