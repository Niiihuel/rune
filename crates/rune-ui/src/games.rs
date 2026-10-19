//! The person's own list of games on this device (escritorio-sesion-y-juegos),
//! as Discord's «Juegos registrados» page keeps it and Serein copies it
//! (`ui/src/registered_games.rs`, `model/src/registered_games.rs`, read
//! 2026-10-18): programs added by hand, detections renamed or hidden, and
//! when each one was last played. A device setting; the host matches the
//! programs open against it and the provider's list.

use rune_i18n::{es, t};

use crate::{Intent, Ui};

/// The setting the list is kept in.
pub const SETTING: &str = "registered_games";
/// Games kept, at most, as Serein bounds them.
pub const MAX_GAMES: usize = 128;
/// The longest name, in bytes.
pub const MAX_NAME: usize = 128;
const MAX_EXECUTABLE: usize = 256;
/// The field a game's new name is written in.
pub(crate) const NAME: &str = "us:game-name";
/// Between a game's fields and between games, as kept: neither is in a
/// name or a path, which have no control characters.
const FIELD: char = '\u{1f}';
const RECORD: char = '\u{1e}';

/// A game of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegisteredGame {
    /// The program's last path components, as the host normalized them.
    pub executable: String,
    pub name: String,
    /// The provider's game it was detected as; `None` when added by hand.
    pub application: Option<u64>,
    /// Never detected again.
    pub hidden: bool,
    /// Unix milliseconds when it was last seen starting or stopping.
    pub last_played: Option<i64>,
}

impl RegisteredGame {
    /// Found by the provider's list, not added by hand.
    pub fn detected(&self) -> bool {
        self.application.is_some()
    }
}

/// The game the host sees running now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Running {
    /// As [`RegisteredGame::executable`].
    pub executable: String,
    pub application: Option<u64>,
    pub name: String,
}

/// What the settings show of the list besides it.
#[derive(Debug, Default)]
pub struct Games {
    /// The game running now, as the host last said.
    pub running: Option<Running>,
    /// The programs open, by their key and their path, once listed.
    pub programs: Option<Vec<(String, String)>>,
    /// The game being renamed, by its executable, and its new name so far.
    pub renaming: Option<(String, String)>,
    /// Grows with each change of the list, for the host to match again.
    pub revision: u64,
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty()
        && name.trim() == name
        && name.len() <= MAX_NAME
        && !name.chars().any(char::is_control)
}

fn valid_executable(executable: &str) -> bool {
    !executable.is_empty()
        && executable.len() <= MAX_EXECUTABLE
        && !executable.chars().any(char::is_control)
}

/// The list as kept.
pub(crate) fn kept(games: &[RegisteredGame]) -> Option<String> {
    (!games.is_empty()).then(|| {
        games
            .iter()
            .map(|game| {
                let number = |value: Option<i64>| value.map(|value| value.to_string());
                [
                    game.executable.clone(),
                    game.name.clone(),
                    game.application
                        .map(|id| id.to_string())
                        .unwrap_or_default(),
                    u8::from(game.hidden).to_string(),
                    number(game.last_played).unwrap_or_default(),
                ]
                .join(&FIELD.to_string())
            })
            .collect::<Vec<_>>()
            .join(&RECORD.to_string())
    })
}

/// What [`kept`] wrote; a game that does not read, or one named twice, is
/// left out.
pub(crate) fn restore(kept: Option<String>) -> Vec<RegisteredGame> {
    let mut games: Vec<RegisteredGame> = Vec::new();
    for record in kept.unwrap_or_default().split(RECORD) {
        let fields: Vec<&str> = record.split(FIELD).collect();
        let [executable, name, application, hidden, last] = fields[..] else {
            continue;
        };
        let game = RegisteredGame {
            executable: executable.to_owned(),
            name: name.to_owned(),
            application: match application {
                "" => None,
                id => match id.parse() {
                    Ok(id) => Some(id),
                    Err(_) => continue,
                },
            },
            hidden: match hidden {
                "0" => false,
                "1" => true,
                _ => continue,
            },
            last_played: match last {
                "" => None,
                at => match at.parse() {
                    Ok(at) => Some(at),
                    Err(_) => continue,
                },
            },
        };
        if valid_executable(&game.executable)
            && valid_name(&game.name)
            && !games
                .iter()
                .any(|known| known.executable == game.executable)
            && games.len() < MAX_GAMES
        {
            games.push(game);
        }
    }
    games
}

/// The name a program added by hand starts with: its file's, without the
/// extension.
fn default_name(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or(path).trim();
    let stem = file
        .rsplit_once('.')
        .filter(|(stem, extension)| !stem.is_empty() && extension.len() <= 6)
        .map_or(file, |(stem, _)| stem);
    let mut name: String = stem.trim().to_owned();
    while name.len() > MAX_NAME {
        name.pop();
    }
    let name = name.trim().to_owned();
    if name.is_empty() {
        t("Juego sin nombre").to_owned()
    } else {
        name
    }
}

fn changed(ui: &mut Ui) -> Vec<Intent> {
    ui.desktop.games.revision += 1;
    vec![Intent::SaveSetting {
        key: SETTING.into(),
        value: kept(&ui.prefs.registered_games),
    }]
}

/// What the host saw running, at `now` (Unix milliseconds): a game that
/// starts or stops was played now, and a new detection joins the list so
/// it can be renamed or hidden later. When the list is full, the detection
/// played longest ago gives way; what the person chose never does.
pub fn running(ui: &mut Ui, running: Option<Running>, now: i64) -> Vec<Intent> {
    let before = std::mem::replace(&mut ui.desktop.games.running, running.clone());
    if before == running {
        return Vec::new();
    }
    let games = &mut ui.prefs.registered_games;
    let mut touched = false;
    for executable in [&before, &running]
        .into_iter()
        .flatten()
        .map(|game| &game.executable)
    {
        if let Some(game) = games.iter_mut().find(|game| game.executable == *executable) {
            game.last_played = Some(now);
            touched = true;
        }
    }
    if let Some(running) = running.filter(|running| {
        running.application.is_some()
            && valid_executable(&running.executable)
            && valid_name(&running.name)
    }) {
        if !games
            .iter()
            .any(|game| game.executable == running.executable)
        {
            if games.len() >= MAX_GAMES {
                let oldest = games
                    .iter()
                    .enumerate()
                    .filter(|(_, game)| game.detected() && !game.hidden)
                    .min_by_key(|(_, game)| game.last_played)
                    .map(|(at, _)| at);
                if let Some(oldest) = oldest {
                    games.remove(oldest);
                }
            }
            if games.len() < MAX_GAMES {
                games.push(RegisteredGame {
                    executable: running.executable,
                    name: running.name,
                    application: running.application,
                    hidden: false,
                    last_played: Some(now),
                });
                touched = true;
            }
        }
    }
    if touched {
        changed(ui)
    } else {
        Vec::new()
    }
}

/// The programs open, by key and path, as the host listed them.
pub fn programs_listed(ui: &mut Ui, programs: Vec<(String, String)>) {
    ui.desktop.games.programs = Some(
        programs
            .into_iter()
            .filter(|(key, path)| valid_executable(key) && valid_executable(path))
            .collect(),
    );
}

/// How many times the list changed, for the host to match it again.
pub fn revision(ui: &Ui) -> u64 {
    ui.desktop.games.revision
}

/// The name being written, as typed.
pub(crate) fn typed(ui: &mut Ui, value: &str) {
    if let Some((_, draft)) = &mut ui.desktop.games.renaming {
        *draft = value.chars().filter(|c| !c.is_control()).collect();
    }
}

/// Keep the name written, if it is one.
pub(crate) fn save_name(ui: &mut Ui) -> Vec<Intent> {
    let Some((executable, draft)) = ui.desktop.games.renaming.take() else {
        return Vec::new();
    };
    let mut name = draft.trim().to_owned();
    while name.len() > MAX_NAME {
        name.pop();
    }
    let name = name.trim_end().to_owned();
    if !valid_name(&name) {
        return Vec::new();
    }
    let games = &mut ui.prefs.registered_games;
    match games.iter_mut().find(|game| game.executable == executable) {
        Some(game) if game.name == name => return Vec::new(),
        Some(game) => game.name = name,
        // The game running, renamed before it was ever listed.
        None => {
            let Some(running) = ui
                .desktop
                .games
                .running
                .clone()
                .filter(|running| running.executable == executable)
            else {
                return Vec::new();
            };
            if games.len() >= MAX_GAMES {
                return Vec::new();
            }
            games.push(RegisteredGame {
                executable,
                name,
                application: running.application,
                hidden: false,
                last_played: None,
            });
        }
    }
    changed(ui)
}

/// A click of the list in the settings, `rest` after `us:`.
pub(crate) fn clicked(ui: &mut Ui, rest: &str) -> Option<Vec<Intent>> {
    if let Some(at) = rest.strip_prefix("game-add=") {
        let (key, path) = ui
            .desktop
            .games
            .programs
            .as_ref()?
            .get(at.parse::<usize>().ok()?)?
            .clone();
        ui.settings_state.select = None;
        let games = &mut ui.prefs.registered_games;
        // Added again, a hidden detection or an older entry is replaced.
        games.retain(|game| game.executable != key);
        if games.len() >= MAX_GAMES {
            return Some(Vec::new());
        }
        games.push(RegisteredGame {
            executable: key,
            name: default_name(&path),
            application: None,
            hidden: false,
            last_played: None,
        });
        return Some(changed(ui));
    }
    if let Some(executable) = rest.strip_prefix("game-rename=") {
        let name = ui
            .prefs
            .registered_games
            .iter()
            .find(|game| game.executable == executable)
            .map(|game| game.name.clone())
            .or_else(|| {
                let running = ui.desktop.games.running.as_ref()?;
                (running.executable == executable).then(|| running.name.clone())
            })?;
        ui.desktop.games.renaming = Some((executable.to_owned(), name.clone()));
        return Some(vec![
            Intent::SetInput {
                id: NAME.into(),
                value: name,
            },
            Intent::Focus(NAME.into()),
        ]);
    }
    if let Some(executable) = rest.strip_prefix("game-remove=") {
        let games = &mut ui.prefs.registered_games;
        let at = games
            .iter()
            .position(|game| game.executable == executable)?;
        // One the provider knows would be detected right back: it stays, hidden.
        if games[at].detected() {
            games[at].hidden = true;
        } else {
            games.remove(at);
        }
        return Some(changed(ui));
    }
    if let Some(executable) = rest.strip_prefix("game-restore=") {
        let game = ui
            .prefs
            .registered_games
            .iter_mut()
            .find(|game| game.executable == executable)?;
        game.hidden = false;
        return Some(changed(ui));
    }
    Some(match rest {
        "games-add" | "select:programs" => {
            let open = ui.settings_state.select == Some("programs");
            ui.settings_state.select = (!open).then_some("programs");
            if open {
                Vec::new()
            } else {
                ui.desktop.games.programs = None;
                vec![Intent::ListPrograms]
            }
        }
        "game-name:save" => save_name(ui),
        "game-name:cancel" => {
            ui.desktop.games.renaming = None;
            Vec::new()
        }
        // Not this game: never detect it again.
        "game-hide-running" => {
            let running = ui.desktop.games.running.clone()?;
            let games = &mut ui.prefs.registered_games;
            let known = games
                .iter()
                .position(|game| game.executable == running.executable);
            match known {
                Some(at) => {
                    games[at].hidden = true;
                    games[at].application = games[at].application.or(running.application);
                }
                None if games.len() < MAX_GAMES => games.push(RegisteredGame {
                    executable: running.executable,
                    name: running.name,
                    application: running.application,
                    hidden: true,
                    last_played: None,
                }),
                None => return Some(Vec::new()),
            }
            changed(ui)
        }
        _ => return None,
    })
}

/// The detail under a game: how it got here and when it was last played.
pub(crate) fn detail(game: &RegisteredGame, now: rune_model::Timestamp) -> String {
    if game.hidden {
        return t("Oculto: no se detecta.").to_owned();
    }
    let source = if game.detected() {
        t("Detectado")
    } else {
        t("Agregado a mano")
    };
    match game.last_played {
        Some(at) => format!(
            "{source} · {}",
            rune_i18n::tr!(
                "Última vez: {day}",
                day = rune_components::time::day_label(rune_model::Timestamp(at), now)
            )
        ),
        None => source.to_owned(),
    }
}

/// What the page says while the programs are read.
pub(crate) const READING: &str = es("Leyendo los programas abiertos…");

#[cfg(test)]
mod tests {
    use super::*;

    fn game(executable: &str, application: Option<u64>) -> RegisteredGame {
        RegisteredGame {
            executable: executable.into(),
            name: "Juego".into(),
            application,
            hidden: false,
            last_played: None,
        }
    }

    #[test]
    fn the_list_is_kept_and_read_back_and_what_does_not_read_is_left_out() {
        let games = vec![
            RegisteredGame {
                last_played: Some(1_700_000_000_000),
                ..game("bin/juego", Some(7))
            },
            RegisteredGame {
                name: "Mi juego · 2".into(),
                hidden: true,
                ..game("z:/games/otro.exe", None)
            },
        ];
        assert_eq!(restore(kept(&games)), games);
        assert_eq!(kept(&[]), None);
        let broken = format!(
            "a{FIELD}A{FIELD}x{FIELD}0{FIELD}{RECORD}b{FIELD} {FIELD}{FIELD}0{FIELD}{RECORD}c{FIELD}C{FIELD}{FIELD}2{FIELD}{RECORD}d{FIELD}D{FIELD}{FIELD}0{FIELD}{RECORD}d{FIELD}Otra{FIELD}{FIELD}0{FIELD}"
        );
        assert_eq!(
            restore(Some(broken)),
            [RegisteredGame {
                name: "D".into(),
                ..game("d", None)
            }]
        );
    }

    #[test]
    fn a_detection_joins_the_list_and_starting_or_stopping_marks_it_played() {
        let mut ui = Ui::new();
        let juego = Running {
            executable: "bin/juego".into(),
            application: Some(7),
            name: "Juego".into(),
        };
        let saved = running(&mut ui, Some(juego.clone()), 10);
        assert!(matches!(&saved[..], [Intent::SaveSetting { key, .. }] if key == SETTING));
        assert_eq!(ui.prefs.registered_games[0].last_played, Some(10));
        assert!(running(&mut ui, Some(juego), 11).is_empty(), "no change");
        running(&mut ui, None, 20);
        assert_eq!(ui.prefs.registered_games[0].last_played, Some(20));
        // A program the provider does not know never joins by itself.
        let unknown = Running {
            executable: "bin/otro".into(),
            application: None,
            name: "Otro".into(),
        };
        assert!(running(&mut ui, Some(unknown), 30).is_empty());
        assert_eq!(ui.prefs.registered_games.len(), 1);
        assert_eq!(revision(&ui), 2);
    }

    #[test]
    fn a_full_list_lets_the_oldest_detection_go_never_a_choice() {
        let mut ui = Ui::new();
        ui.prefs.registered_games = (0..MAX_GAMES)
            .map(|n| RegisteredGame {
                last_played: Some(n as i64),
                hidden: n == 0,
                ..game(&format!("g{n}"), (n % 2 == 0).then_some(n as u64))
            })
            .collect();
        running(
            &mut ui,
            Some(Running {
                executable: "nuevo".into(),
                application: Some(999),
                name: "Nuevo".into(),
            }),
            500,
        );
        let games = &ui.prefs.registered_games;
        assert_eq!(games.len(), MAX_GAMES);
        assert!(
            games.iter().any(|game| game.executable == "g0"),
            "hidden stays"
        );
        assert!(
            games.iter().any(|game| game.executable == "g1"),
            "added by hand stays"
        );
        assert!(
            !games.iter().any(|game| game.executable == "g2"),
            "oldest detection goes"
        );
        assert!(games.iter().any(|game| game.executable == "nuevo"));
    }

    #[test]
    fn adding_renaming_hiding_and_restoring() {
        let mut ui = Ui::new();
        programs_listed(
            &mut ui,
            vec![
                (
                    "opt/juego/bin/juego.x86_64".into(),
                    "opt/Juego/bin/Juego.x86_64".into(),
                ),
                ("bad\u{7}".into(), "bad".into()),
            ],
        );
        assert_eq!(ui.desktop.games.programs.as_ref().unwrap().len(), 1);
        clicked(&mut ui, "game-add=0").unwrap();
        let added = &ui.prefs.registered_games[0];
        assert_eq!(
            (
                added.executable.as_str(),
                added.name.as_str(),
                added.detected()
            ),
            ("opt/juego/bin/juego.x86_64", "Juego", false)
        );
        let opened = clicked(&mut ui, "game-rename=opt/juego/bin/juego.x86_64").unwrap();
        assert!(opened.contains(&Intent::Focus(NAME.into())));
        typed(&mut ui, "  Mi juego  ");
        clicked(&mut ui, "game-name:save").unwrap();
        assert_eq!(ui.prefs.registered_games[0].name, "Mi juego");
        // Removed by hand, it goes; a detection is hidden instead.
        clicked(&mut ui, "game-remove=opt/juego/bin/juego.x86_64").unwrap();
        assert!(ui.prefs.registered_games.is_empty());
        let detected = Running {
            executable: "juego.exe".into(),
            application: Some(4),
            name: "Juego".into(),
        };
        running(&mut ui, Some(detected), 1);
        clicked(&mut ui, "game-hide-running").unwrap();
        assert!(ui.prefs.registered_games[0].hidden);
        clicked(&mut ui, "game-restore=juego.exe").unwrap();
        assert!(!ui.prefs.registered_games[0].hidden);
        clicked(&mut ui, "game-remove=juego.exe").unwrap();
        assert!(ui.prefs.registered_games[0].hidden, "detected: hidden");
        // An empty name is not one.
        clicked(&mut ui, "game-rename=juego.exe").unwrap();
        typed(&mut ui, "   ");
        assert!(clicked(&mut ui, "game-name:save").unwrap().is_empty());
        assert_eq!(ui.prefs.registered_games[0].name, "Juego");
    }

    #[test]
    fn default_names_come_from_the_file() {
        assert_eq!(default_name("Games/Juego.x86_64"), "Juego");
        assert_eq!(default_name("z:/games/lms.exe"), "lms");
        assert_eq!(default_name("bin/.oculto"), ".oculto");
        assert_eq!(default_name("bin/"), t("Juego sin nombre"));
    }
}
