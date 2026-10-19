//! Where RUNE keeps its files on this system (windows-base): the XDG base
//! directories on Linux, the user's known folders on Windows. Each is
//! RUNE's own folder, not yet made; `None` when the system names none.

use std::path::PathBuf;

/// The local store and what belongs with it: `$XDG_DATA_HOME/rune`, or
/// `%LOCALAPPDATA%\RUNE` (this computer only: the store is sealed with keys
/// of this computer's keyring).
pub fn data() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        local().map(|local| local.join(WINDOWS_NAME))
    }
    #[cfg(not(windows))]
    {
        xdg("XDG_DATA_HOME", ".local/share")
    }
}

/// The small files read before the window opens: `$XDG_CONFIG_HOME/rune`,
/// or `%APPDATA%\RUNE`.
pub fn config() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        crate::win::known_folder(crate::win::Known::RoamingAppData)
            .map(|roaming| roaming.join(WINDOWS_NAME))
    }
    #[cfg(not(windows))]
    {
        xdg("XDG_CONFIG_HOME", ".config")
    }
}

/// Pictures kept between runs: `$XDG_CACHE_HOME/rune`, or
/// `%LOCALAPPDATA%\RUNE\cache`.
pub fn cache() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        data().map(|data| data.join("cache"))
    }
    #[cfg(not(windows))]
    {
        xdg("XDG_CACHE_HOME", ".cache")
    }
}

/// The log: `$XDG_STATE_HOME/rune`, or `%LOCALAPPDATA%\RUNE\logs`.
pub fn state() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        data().map(|data| data.join("logs"))
    }
    #[cfg(not(windows))]
    {
        xdg("XDG_STATE_HOME", ".local/state")
    }
}

/// The person's home folder: `$HOME`, or the profile folder on Windows.
pub fn home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        crate::win::known_folder(crate::win::Known::Profile)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| home.is_absolute())
    }
}

/// The person's Downloads folder on Windows, wherever they moved it.
#[cfg(windows)]
pub fn downloads() -> Option<PathBuf> {
    crate::win::known_folder(crate::win::Known::Downloads)
}

#[cfg(windows)]
const WINDOWS_NAME: &str = "RUNE";

#[cfg(windows)]
fn local() -> Option<PathBuf> {
    crate::win::known_folder(crate::win::Known::LocalAppData)
}

/// `$variable/rune`, or `~/{fallback}/rune`: as the specification says, a
/// variable that is empty or not absolute is ignored.
#[cfg(not(windows))]
fn xdg(variable: &str, fallback: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home().map(|home| home.join(fallback)))
        .map(|base| base.join("rune"))
}

#[cfg(test)]
mod tests {
    // The system's folders: absolute, and RUNE's own.
    #[test]
    fn each_folder_is_absolute_and_rune_s_own() {
        for folder in [
            super::data(),
            super::config(),
            super::cache(),
            super::state(),
        ]
        .into_iter()
        .flatten()
        {
            assert!(folder.is_absolute(), "{folder:?}");
            assert!(
                folder
                    .components()
                    .any(|part| part.as_os_str().eq_ignore_ascii_case("rune")),
                "{folder:?}"
            );
        }
    }
}
