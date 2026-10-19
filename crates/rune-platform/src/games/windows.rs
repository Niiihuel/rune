//! The processes of this Windows session (windows-medios): a snapshot of
//! the system's process list (`CreateToolhelp32Snapshot`), and for each
//! process of this logon session only the full path of its program
//! (`QueryFullProcessImageNameW`, asked with the least access Windows has),
//! or its file name from the snapshot when that path cannot be read. Never
//! a command line, a process's memory or any of its files.

use std::os::windows::io::{FromRawHandle, OwnedHandle};

use windows::core::PWSTR;
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};

use super::MAX_PROCESSES;

fn session(pid: u32) -> Option<u32> {
    let mut session = 0;
    // SAFETY: a valid out-pointer; an unknown process is an error.
    unsafe { ProcessIdToSessionId(pid, &mut session) }
        .ok()
        .map(|()| session)
}

/// Where the program of the process `pid` lives, if Windows lets this user
/// ask.
fn image(pid: u32) -> Option<String> {
    // SAFETY: the handle returned is owned (and closed) here.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let _owned = unsafe { OwnedHandle::from_raw_handle(process.0) };
    let mut path = [0u16; 1024];
    let mut length = path.len() as u32;
    // SAFETY: the buffer holds `length` units, as the call is told.
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(path.as_mut_ptr()),
            &mut length,
        )
    }
    .ok()?;
    String::from_utf16(path.get(..length as usize)?).ok()
}

/// The text of a NUL-ended UTF-16 field.
fn text(field: &[u16]) -> Option<String> {
    let end = field
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(field.len());
    String::from_utf16(&field[..end])
        .ok()
        .filter(|text| !text.is_empty())
}

pub(super) fn processes() -> Vec<String> {
    let Some(mine) = session(std::process::id()) else {
        return Vec::new();
    };
    // SAFETY: a snapshot of every process; its handle is owned here.
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return Vec::new();
    };
    let _owned = unsafe { OwnedHandle::from_raw_handle(snapshot.0) };
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut paths = Vec::new();
    // SAFETY: the snapshot is open and `entry` says its own size.
    let mut more = unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok();
    while more && paths.len() < MAX_PROCESSES {
        let pid = entry.th32ProcessID;
        // Another user's session (or the services') is not this person's.
        if pid != 0 && session(pid) == Some(mine) {
            if let Some(path) = image(pid).or_else(|| text(&entry.szExeFile)) {
                paths.push(path);
            }
        }
        // SAFETY: as above.
        more = unsafe { Process32NextW(snapshot, &mut entry) }.is_ok();
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_snapshot_field_is_read_up_to_its_end() {
        let mut field = [0u16; 8];
        for (at, unit) in "a.exe".encode_utf16().enumerate() {
            field[at] = unit;
        }
        assert_eq!(text(&field).as_deref(), Some("a.exe"));
        assert_eq!(text(&[0; 4]), None);
        assert_eq!(text(&[0x61; 3]).as_deref(), Some("aaa"), "without its end");
    }

    #[test]
    fn this_process_is_in_its_own_session_with_its_path() {
        let me = std::process::id();
        assert!(session(me).is_some());
        let path = image(me).unwrap();
        let file = std::env::current_exe().unwrap();
        assert!(path
            .to_lowercase()
            .ends_with(&file.file_name().unwrap().to_str().unwrap().to_lowercase()));
    }
}
