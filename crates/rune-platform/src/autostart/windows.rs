//! Starting with Windows (windows-base): the value `RUNE` of the user's
//! `Run` key, the command Windows runs at sign-in. Task Manager's «Startup
//! apps» may turn it off without removing it (`StartupApproved\Run`): then
//! it counts as off, and turning it on here clears that. What touches the
//! registry is Windows only; the rest is tested on every system.

#[cfg(windows)]
use std::path::Path;

use super::MINIMIZED;
#[cfg(windows)]
use crate::win::registry;

#[cfg(windows)]
const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
#[cfg(windows)]
const NAME: &str = "RUNE";

/// Whether RUNE starts with Windows, and in the tray.
#[cfg(windows)]
pub(super) fn state() -> (bool, bool) {
    let Some(command) = registry::text(RUN, NAME) else {
        return (false, false);
    };
    let approved = registry::binary(APPROVED, NAME).is_none_or(|flags| approved(&flags));
    (approved, approved && minimized(&command))
}

/// Start `program` with Windows, in the tray when `minimized`, or no longer.
#[cfg(windows)]
pub(super) fn set(on: bool, minimized: bool, program: &Path) -> std::io::Result<()> {
    if !on {
        return registry::remove(RUN, NAME);
    }
    let program = program.to_str().filter(|_| program.is_absolute());
    let command = program
        .and_then(|program| command(program, minimized))
        .ok_or(std::io::ErrorKind::InvalidInput)?;
    registry::set_text(RUN, NAME, &command)?;
    // Turned off in Task Manager before: the person turns it on again here.
    registry::remove(APPROVED, NAME)
}

/// The command for `program`: quoted, as a path with spaces needs, and the
/// argument that starts RUNE in the tray. `None` for a path Windows cannot
/// quote.
fn command(program: &str, minimized: bool) -> Option<String> {
    if program.is_empty() || program.contains('"') || program.chars().any(char::is_control) {
        return None;
    }
    Some(if minimized {
        format!("\"{program}\" {MINIMIZED}")
    } else {
        format!("\"{program}\"")
    })
}

fn minimized(command: &str) -> bool {
    command.ends_with(&format!("\" {MINIMIZED}"))
}

/// Task Manager's flags: an odd first byte is «Disabled».
fn approved(flags: &[u8]) -> bool {
    flags.first().is_none_or(|first| first & 1 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_is_quoted_and_says_when_it_starts_in_the_tray() {
        let program = r"C:\Users\Ada Lovelace\AppData\Local\Programs\RUNE\rune.exe";
        let plain = command(program, false).unwrap();
        assert_eq!(plain, format!("\"{program}\""));
        assert!(!minimized(&plain));
        let tray = command(program, true).unwrap();
        assert_eq!(tray, format!("\"{program}\" --start-minimized"));
        assert!(minimized(&tray));
        assert!(command("", false).is_none());
        assert!(command("C:\\a\"b.exe", false).is_none());
        assert!(command("C:\\a\nb.exe", true).is_none());
    }

    #[test]
    fn task_manager_can_turn_it_off() {
        assert!(approved(&[2, 0, 0, 0]));
        assert!(approved(&[6]));
        assert!(!approved(&[3, 0, 0, 0]));
        assert!(approved(&[]));
    }

    // The runner's own registry (CI, windows-latest).
    #[cfg(windows)]
    #[test]
    #[ignore = "writes the Run key of this user"]
    fn the_run_value_is_written_read_and_removed() {
        let program = std::env::current_exe().unwrap();
        set(true, true, &program).unwrap();
        assert_eq!(state(), (true, true));
        set(true, false, &program).unwrap();
        assert_eq!(state(), (true, false));
        set(false, false, &program).unwrap();
        assert_eq!(state(), (false, false));
    }
}
