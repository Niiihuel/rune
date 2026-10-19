//! Starting RUNE with the desktop session, through an XDG autostart entry
//! (`$XDG_CONFIG_HOME/autostart/rune.desktop`), which every freedesktop
//! session reads. The entry is the only state: it exists or it does not.
//! The portal's `Background` interface would need an installed application
//! ID that RUNE does not have yet. On Windows it is the user's `Run` key
//! (windows-base): [`state`] and [`change`] are the same on both.
//!
//! Everything here touches the disk: call it off the interface's thread.

use std::io;
use std::path::{Path, PathBuf};

use rune_i18n::es;

#[cfg(any(windows, test))]
mod windows;

const FILE: &str = "rune.desktop";
/// The argument that starts RUNE in the tray, without its window.
pub const MINIMIZED: &str = "--start-minimized";

/// Whether RUNE starts with the session, and in the tray, as the system
/// will read it; `None` where it has no place for it.
pub fn state() -> Option<(bool, bool)> {
    #[cfg(windows)]
    {
        Some(self::windows::state())
    }
    #[cfg(not(windows))]
    {
        entry_path().map(|path| (enabled(&path), minimized(&path)))
    }
}

/// Start `program` with the session, in the tray when `minimized`, or no
/// longer; why not, as a text for [`rune_i18n::t`], when it could not.
pub fn change(on: bool, minimized: bool, program: &Path) -> Result<(), &'static str> {
    const FAILED: &str = es("No se pudo cambiar el inicio con la sesión.");
    #[cfg(windows)]
    {
        self::windows::set(on, minimized, program).map_err(|_| FAILED)
    }
    #[cfg(not(windows))]
    {
        let path = entry_path().ok_or(es("No se encontró la carpeta de configuración."))?;
        set(&path, on, minimized, program).map_err(|_| FAILED)
    }
}

/// Where the entry lives: `$XDG_CONFIG_HOME/autostart`, or `~/.config/autostart`.
pub fn entry_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|base| base.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("autostart").join(FILE))
}

/// Whether the entry at `path` starts RUNE: it exists and nothing in it
/// turns it off, as the session would read it.
pub fn enabled(path: &Path) -> bool {
    let Ok(entry) = std::fs::read_to_string(path) else {
        return false;
    };
    !entry.lines().map(str::trim).any(|line| {
        matches!(
            line,
            "Hidden=true" | "X-GNOME-Autostart-enabled=false" | "X-KDE-autostart-enabled=false"
        )
    })
}

/// Whether the entry at `path` starts RUNE in the tray.
pub fn minimized(path: &Path) -> bool {
    std::fs::read_to_string(path).is_ok_and(|entry| {
        entry.lines().any(|line| {
            line.trim()
                .strip_prefix("Exec=")
                // After the quoted program, as [`set`] writes it.
                .is_some_and(|exec| exec.ends_with(&format!("\" {MINIMIZED}")))
        })
    })
}

/// Write the entry that starts `program` at `path`, in the tray when
/// `minimized`, or remove it.
pub fn set(path: &Path, on: bool, minimized: bool, program: &Path) -> io::Result<()> {
    if !on {
        return match std::fs::remove_file(path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            other => other,
        };
    }
    write(path, &entry(program, minimized, AUTOSTART)?)
}

/// What only the autostart entry says.
const AUTOSTART: &str = "X-GNOME-Autostart-enabled=true\n";

// RUNE's own updates (actualizaciones).
/// What only the applications menu's entry says.
const MENU: &str = "Categories=Network;InstantMessaging;Chat;\nStartupWMClass=rune\n";

/// Write RUNE's entry in the applications menu at `path`
/// (`$XDG_DATA_HOME/applications/rune.desktop`), starting `program`: an
/// installation's, which stays the same across updates.
pub fn set_menu_entry(path: &Path, program: &Path) -> io::Result<()> {
    write(path, &entry(program, false, MENU)?)
}

/// Write `entry` at `path` aside and rename it, so the session never
/// reads half an entry.
fn write(path: &Path, entry: &str) -> io::Result<()> {
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder)?;
    }
    let partial = path.with_extension("desktop.partial");
    std::fs::write(&partial, entry)?;
    std::fs::rename(&partial, path)
}

/// The desktop entry for `program`, an absolute path quoted as the
/// Desktop Entry specification asks for an `Exec` argument, ending in
/// `last`.
fn entry(program: &Path, minimized: bool, last: &str) -> io::Result<String> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "unusable program path");
    let program = program.to_str().ok_or_else(invalid)?;
    if !program.starts_with('/') || program.chars().any(char::is_control) {
        return Err(invalid());
    }
    let mut quoted = String::with_capacity(program.len() + 2);
    quoted.push('"');
    for character in program.chars() {
        match character {
            // Quoting escapes these with a backslash, and a string value
            // escapes that backslash again.
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(character);
            }
            '\\' => quoted.push_str("\\\\\\\\"),
            '%' => quoted.push_str("%%"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    if minimized {
        quoted.push(' ');
        quoted.push_str(MINIMIZED);
    }
    Ok(format!(
        "[Desktop Entry]\nType=Application\nName=RUNE\nComment={comment}\nExec={quoted}\nIcon=rune\nTerminal=false\n{last}",
        comment = rune_i18n::t("Cliente de chat nativo"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_entry_starts_the_program_and_is_removed_when_turned_off() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("autostart").join(FILE);
        assert!(!enabled(&path));
        set(&path, true, false, Path::new("/opt/rune/bin/rune")).unwrap();
        assert!(enabled(&path) && !minimized(&path));
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.contains("Exec=\"/opt/rune/bin/rune\"\n"),
            "{written}"
        );
        assert!(written.contains("Icon=rune\n"), "{written}");
        assert!(!folder
            .path()
            .join("autostart/rune.desktop.partial")
            .exists());
        // In the tray: the same entry with the argument.
        set(&path, true, true, Path::new("/opt/rune/bin/rune")).unwrap();
        assert!(enabled(&path) && minimized(&path));
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.contains("Exec=\"/opt/rune/bin/rune\" --start-minimized\n"),
            "{written}"
        );
        set(&path, false, true, Path::new("/opt/rune/bin/rune")).unwrap();
        assert!(!enabled(&path) && !minimized(&path));
        // Turning off what is not there is not an error.
        set(&path, false, false, Path::new("/opt/rune/bin/rune")).unwrap();
    }

    #[test]
    fn the_menu_entry_starts_the_installed_program() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("applications").join(FILE);
        let program = Path::new("/home/ada/.local/share/rune/app/current/rune");
        set_menu_entry(&path, program).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        assert!(
            written.contains("Exec=\"/home/ada/.local/share/rune/app/current/rune\"\n"),
            "{written}"
        );
        assert!(written.contains("Categories=Network;InstantMessaging;Chat;\n"));
        assert!(!written.contains("Autostart"), "{written}");
    }

    #[test]
    fn an_entry_the_session_was_told_to_skip_is_off() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join(FILE);
        std::fs::write(&path, "[Desktop Entry]\nExec=rune\nHidden=true\n").unwrap();
        assert!(!enabled(&path));
        std::fs::write(
            &path,
            "[Desktop Entry]\nExec=rune\nX-GNOME-Autostart-enabled=false\n",
        )
        .unwrap();
        assert!(!enabled(&path));
    }

    #[test]
    fn the_program_is_quoted_and_odd_paths_are_refused() {
        let entry = entry(Path::new("/home/a b/100%/$x\"y\\z"), false, AUTOSTART).unwrap();
        assert!(
            entry.contains(r#"Exec="/home/a b/100%%/\\$x\\"y\\\\z""#),
            "{entry}"
        );
        assert!(
            super::entry(Path::new("rune"), false, AUTOSTART).is_err(),
            "relative"
        );
        assert!(
            super::entry(Path::new("/opt/ru\nne"), true, AUTOSTART).is_err(),
            "newline"
        );
        // A folder named like the argument is not the argument.
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join(FILE);
        std::fs::write(&path, "[Desktop Entry]\nExec=\"/a b/ --start-minimized\"\n").unwrap();
        assert!(!minimized(&path));
    }
}
