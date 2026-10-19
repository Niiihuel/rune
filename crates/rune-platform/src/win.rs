//! What the Windows services share (windows-base): wide strings, the
//! user's known folders and the shell's «open».

use std::ffi::OsStr;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

use windows::core::PCWSTR;
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Downloads, FOLDERID_LocalAppData, FOLDERID_Profile, FOLDERID_RoamingAppData,
    SHGetKnownFolderPath, ShellExecuteW, KF_FLAG_DEFAULT,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

/// `text` as the system reads it: UTF-16, ending in NUL.
pub(crate) fn wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain([0]).collect()
}

/// The known folders RUNE uses.
#[derive(Clone, Copy)]
pub(crate) enum Known {
    LocalAppData,
    RoamingAppData,
    Profile,
    Downloads,
}

/// Where the user's `folder` is, as the shell says, wherever they moved it.
pub(crate) fn known_folder(folder: Known) -> Option<PathBuf> {
    let id = match folder {
        Known::LocalAppData => FOLDERID_LocalAppData,
        Known::RoamingAppData => FOLDERID_RoamingAppData,
        Known::Profile => FOLDERID_Profile,
        Known::Downloads => FOLDERID_Downloads,
    };
    // SAFETY: a valid folder ID; the string the shell allocates is copied
    // and freed once.
    unsafe {
        let path = SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None).ok()?;
        let folder = PathBuf::from(std::ffi::OsString::from_wide(path.as_wide()));
        CoTaskMemFree(Some(path.0 as *const _));
        Some(folder).filter(|folder| folder.is_absolute())
    }
}

/// Open `target`, a web address or a folder, with the handler the person
/// chose, on a thread of its own: the shell may take a while to answer.
pub(crate) fn shell_open(target: &OsStr) {
    let target = wide(target);
    let _ = std::thread::Builder::new()
        .name("rune-open".into())
        .spawn(move || {
            // SAFETY: the shell asks for COM on the calling thread, single
            // threaded and without OLE1; `target` ends in NUL and lives for
            // the call.
            unsafe {
                let com = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
                let opened = ShellExecuteW(
                    None,
                    windows::core::w!("open"),
                    PCWSTR(target.as_ptr()),
                    None,
                    None,
                    SW_SHOWNORMAL,
                );
                // Above 32 is success, as the function documents.
                if opened.0 as usize <= 32 {
                    eprintln!("rune: the shell could not open it ({})", opened.0 as usize);
                }
                if com.is_ok() {
                    CoUninitialize();
                }
            }
        });
}

/// The current user's registry: what RUNE reads and writes there.
pub(crate) mod registry {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
    use windows::Win32::System::Registry::{
        RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_ROUTINE_FLAGS,
        REG_SZ, RRF_RT_REG_BINARY, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };

    use super::wide;

    /// The largest value read: more is not one of the values RUNE reads.
    const MAX_VALUE: u32 = 64 * 1024;

    fn raw(key: &str, name: &str, kind: REG_ROUTINE_FLAGS) -> Option<Vec<u8>> {
        let (key, name) = (wide(key), wide(name));
        let mut size = 0u32;
        // SAFETY: both names end in NUL; the first call only sizes the value.
        let sized = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
                kind,
                None,
                None,
                Some(&mut size),
            )
        };
        if sized != ERROR_SUCCESS || size > MAX_VALUE {
            return None;
        }
        let mut bytes = vec![0u8; size as usize];
        // SAFETY: the buffer holds `size` bytes, as the call is told.
        let read = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
                kind,
                None,
                Some(bytes.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        (read == ERROR_SUCCESS).then(|| {
            bytes.truncate(size as usize);
            bytes
        })
    }

    /// The text value `name` of `key`, if there is one.
    pub(crate) fn text(key: &str, name: &str) -> Option<String> {
        let bytes = raw(key, name, RRF_RT_REG_SZ)?;
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|unit| *unit != 0)
            .collect();
        String::from_utf16(&units).ok()
    }

    /// The number value `name` of `key`, if there is one.
    pub(crate) fn number(key: &str, name: &str) -> Option<u32> {
        let bytes = raw(key, name, RRF_RT_REG_DWORD)?;
        Some(u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?))
    }

    /// The binary value `name` of `key`, if there is one.
    pub(crate) fn binary(key: &str, name: &str) -> Option<Vec<u8>> {
        raw(key, name, RRF_RT_REG_BINARY)
    }

    /// Write the text value `name` of `key`, making the key if it is missing.
    pub(crate) fn set_text(key: &str, name: &str, value: &str) -> std::io::Result<()> {
        let (key, name, value) = (wide(key), wide(name), wide(value));
        // SAFETY: names and value end in NUL; the size counts the NUL.
        let written = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
                REG_SZ.0,
                Some(value.as_ptr().cast()),
                (value.len() * 2) as u32,
            )
        };
        match written {
            ERROR_SUCCESS => Ok(()),
            error => Err(std::io::Error::from_raw_os_error(error.0 as i32)),
        }
    }

    /// Remove the value `name` of `key`; a missing one is not an error.
    pub(crate) fn remove(key: &str, name: &str) -> std::io::Result<()> {
        let (key, name) = (wide(key), wide(name));
        // SAFETY: both names end in NUL.
        let removed = unsafe {
            RegDeleteKeyValueW(
                HKEY_CURRENT_USER,
                PCWSTR(key.as_ptr()),
                PCWSTR(name.as_ptr()),
            )
        };
        match removed {
            ERROR_SUCCESS | ERROR_FILE_NOT_FOUND => Ok(()),
            error => Err(std::io::Error::from_raw_os_error(error.0 as i32)),
        }
    }
}
