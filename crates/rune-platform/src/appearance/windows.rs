//! The light or dark preference and the accent color on Windows
//! (windows-base): `AppsUseLightTheme` of the user's `Personalize` key and
//! `AccentColor` of their `DWM` key, read at once and again each time the
//! registry says either key changed, on a thread of its own.

use super::{Accent, ColorScheme};

/// What `AppsUseLightTheme` says: 1 light, 0 dark, missing no preference.
fn scheme(light: Option<u32>) -> ColorScheme {
    match light {
        Some(0) => ColorScheme::Dark,
        Some(_) => ColorScheme::Light,
        None => ColorScheme::NoPreference,
    }
}

/// `AccentColor` is 0xAABBGGRR: the alpha is always opaque, and dropped.
fn accent(abgr: u32) -> Accent {
    let [red, green, blue, _] = abgr.to_le_bytes();
    [red, green, blue]
}

#[cfg(windows)]
pub(super) use watch::spawn;

#[cfg(windows)]
mod watch {
    use std::time::Duration;

    use tokio::sync::oneshot::{self, error::TryRecvError};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        CloseHandle, ERROR_SUCCESS, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows::Win32::System::Registry::{
        RegCloseKey, RegNotifyChangeKeyValue, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, KEY_NOTIFY,
        REG_NOTIFY_CHANGE_LAST_SET,
    };
    use windows::Win32::System::Threading::{CreateEventW, WaitForMultipleObjects};

    use super::{accent, scheme, Accent, ColorScheme};
    use crate::win::{registry, wide};

    const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";
    const DWM: &str = r"Software\Microsoft\Windows\DWM";
    /// How often the thread looks whether its handle is gone.
    const STOP_CHECK: Duration = Duration::from_millis(500);

    /// A key watched for changes of its values, with the event that says so.
    struct Watched {
        key: HKEY,
        event: HANDLE,
    }

    impl Watched {
        fn open(path: &str) -> Option<Self> {
            let path = wide(path);
            let mut key = HKEY::default();
            // SAFETY: the path ends in NUL; the key and the event are owned
            // here and closed on drop.
            unsafe {
                if RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(path.as_ptr()),
                    None,
                    KEY_NOTIFY,
                    &mut key,
                ) != ERROR_SUCCESS
                {
                    return None;
                }
                let Ok(event) = CreateEventW(None, false, false, PCWSTR::null()) else {
                    let _ = RegCloseKey(key);
                    return None;
                };
                let watched = Self { key, event };
                watched.arm().then_some(watched)
            }
        }

        /// Ask for the next change; the registry asks again each time.
        fn arm(&self) -> bool {
            // SAFETY: an open key and event of this value.
            unsafe {
                RegNotifyChangeKeyValue(
                    self.key,
                    false,
                    REG_NOTIFY_CHANGE_LAST_SET,
                    Some(self.event),
                    true,
                ) == ERROR_SUCCESS
            }
        }
    }

    impl Drop for Watched {
        fn drop(&mut self) {
            // SAFETY: both were opened by `open` and are closed once.
            unsafe {
                let _ = RegCloseKey(self.key);
                let _ = CloseHandle(self.event);
            }
        }
    }

    fn read() -> (ColorScheme, Option<Accent>) {
        (
            scheme(registry::number(PERSONALIZE, "AppsUseLightTheme")),
            registry::number(DWM, "AccentColor").map(accent),
        )
    }

    pub(in crate::appearance) fn spawn(
        report: impl Fn(ColorScheme) + Send + 'static,
        tint: impl Fn(Option<Accent>) + Send + 'static,
        mut stopped: oneshot::Receiver<()>,
    ) {
        // A thread that cannot start leaves the preference unknown: RUNE's own.
        let _ = std::thread::Builder::new()
            .name("rune-appearance".into())
            .spawn(move || {
                let (mut shown, mut tinted) = read();
                report(shown);
                tint(tinted);
                // Without a key to watch, the first reading stays.
                let watched: Vec<Watched> = [PERSONALIZE, DWM]
                    .into_iter()
                    .filter_map(Watched::open)
                    .collect();
                let events: Vec<HANDLE> = watched.iter().map(|key| key.event).collect();
                while matches!(stopped.try_recv(), Err(TryRecvError::Empty)) {
                    if events.is_empty() {
                        std::thread::sleep(STOP_CHECK);
                        continue;
                    }
                    // SAFETY: the events stay open while `watched` lives.
                    let woken = unsafe {
                        WaitForMultipleObjects(&events, false, STOP_CHECK.as_millis() as u32)
                    };
                    let changed = woken.0.wrapping_sub(WAIT_OBJECT_0.0) as usize;
                    let Some(key) = watched.get(changed) else {
                        // A wait that failed must not spin.
                        if woken != WAIT_TIMEOUT {
                            std::thread::sleep(STOP_CHECK);
                        }
                        continue;
                    };
                    key.arm();
                    let (scheme, accent) = read();
                    if scheme != shown {
                        shown = scheme;
                        report(scheme);
                    }
                    if accent != tinted {
                        tinted = accent;
                        tint(accent);
                    }
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_registry_s_values_read_as_scheme_and_accent() {
        assert_eq!(scheme(Some(0)), ColorScheme::Dark);
        assert_eq!(scheme(Some(1)), ColorScheme::Light);
        assert_eq!(scheme(None), ColorScheme::NoPreference);
        // Windows' default blue, #0078D7.
        assert_eq!(accent(0xffd7_7800), [0x00, 0x78, 0xd7]);
    }
}
