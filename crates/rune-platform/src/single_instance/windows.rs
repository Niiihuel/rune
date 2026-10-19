//! The single instance on Windows (windows-base): a named pipe of this
//! logon session, `\\.\pipe\rune-instance-{session}`. The first run makes
//! its first instance (so nobody else holds the name); the pipe's default
//! security lets only this user, the administrators and the system write to
//! it, and remote clients are refused. A later run checks that the pipe's
//! server is of its own session before it hands over its links, and lets it
//! come to the front.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::sync::{mpsc, Arc};

use windows::core::PCWSTR;
use windows::Win32::Foundation::{ERROR_NO_DATA, ERROR_PIPE_CONNECTED, HANDLE};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeServerProcessId, GetNamedPipeServerSessionId,
    WaitNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow;

use super::{heard, message, parse, Launch, Listener, MAX_REQUEST, TIMEOUT};

/// The claim on the pipe: its first instance, before anyone connects.
pub struct Instance {
    first: OwnedHandle,
    name: Vec<u16>,
}

fn session() -> Option<u32> {
    let mut session = 0;
    // SAFETY: a valid out-pointer for this process's own session.
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) }
        .ok()
        .map(|()| session)
}

fn name(session: u32) -> String {
    format!(r"\\.\pipe\rune-instance-{session}")
}

/// One instance of the pipe; the first one only when `first`.
fn instance(name: &[u16], first: bool) -> Option<OwnedHandle> {
    let mut open = PIPE_ACCESS_INBOUND;
    if first {
        open |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    // SAFETY: `name` ends in NUL; the handle returned is owned here.
    let pipe = unsafe {
        CreateNamedPipeW(
            PCWSTR(name.as_ptr()),
            open,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            0,
            MAX_REQUEST as u32,
            TIMEOUT.as_millis() as u32,
            None,
        )
    };
    (!pipe.is_invalid()).then(|| unsafe { OwnedHandle::from_raw_handle(pipe.0) })
}

pub(super) fn claim(links: &[String]) -> Launch {
    let Some(session) = session() else {
        return Launch::Alone;
    };
    let name = name(session);
    let wide = crate::win::wide(&name);
    // A run that is there takes the links; only without one is the name
    // claimed, and a start at the same moment then finds this one.
    if forward(&name, &wide, session, links).is_ok() {
        return Launch::Forwarded;
    }
    if let Some(first) = instance(&wide, true) {
        return Launch::First(Instance { first, name: wide });
    }
    match forward(&name, &wide, session, links) {
        Ok(()) => Launch::Forwarded,
        Err(_) => Launch::Alone,
    }
}

fn forward(name: &str, wide: &[u16], session: u32, links: &[String]) -> std::io::Result<()> {
    let open = || OpenOptions::new().write(true).open(name);
    let mut pipe = match open() {
        Ok(pipe) => pipe,
        // Every instance is busy with another start: wait for one, once.
        // Without any, the wait ends at once.
        Err(_) => {
            // SAFETY: `wide` ends in NUL.
            let _ = unsafe { WaitNamedPipeW(PCWSTR(wide.as_ptr()), TIMEOUT.as_millis() as u32) };
            open()?
        }
    };
    let handle = HANDLE(pipe.as_raw_handle());
    let (mut server, mut theirs) = (0, 0);
    // SAFETY: an open pipe handle and valid out-pointers.
    unsafe {
        GetNamedPipeServerSessionId(handle, &mut theirs)?;
        GetNamedPipeServerProcessId(handle, &mut server)?;
    }
    // Someone else's pipe under RUNE's name never hears the links.
    if theirs != session {
        return Err(std::io::ErrorKind::PermissionDenied.into());
    }
    // The first run may come to the front: this one was just started by
    // the person, so it may pass that on.
    // SAFETY: a plain call with the server's process ID.
    let _ = unsafe { AllowSetForegroundWindow(server) };
    pipe.write_all(message(links).as_bytes())
}

impl Instance {
    /// Hear later runs on a thread of its own; `wake` runs after each request.
    pub fn listen(self, wake: Arc<dyn Fn() + Send + Sync>) -> Listener {
        let (sender, requests) = mpsc::channel();
        let Instance { first, name } = self;
        // A thread that cannot start only means later runs are not heard.
        let _ = std::thread::Builder::new()
            .name("rune-instance".into())
            .spawn(move || {
                let mut current = first;
                loop {
                    let handle = HANDLE(current.as_raw_handle());
                    // SAFETY: an instance this thread owns, waited on once.
                    // A later run may have connected before this wait, or
                    // even written and gone: what it wrote is still there.
                    let connected = match unsafe { ConnectNamedPipe(handle, None) } {
                        Ok(()) => true,
                        Err(error) => [ERROR_PIPE_CONNECTED, ERROR_NO_DATA]
                            .iter()
                            .any(|code| error.code() == code.to_hresult()),
                    };
                    // The next instance before this one goes, so the name is
                    // never free for another first run.
                    let Some(next) = instance(&name, false) else {
                        return;
                    };
                    if connected {
                        // shortcut: a client of this user that never closes
                        // holds the listener; RUNE's own later runs write
                        // and close at once.
                        let requests = parse(File::from(current));
                        if !heard(requests, &sender, &wake) {
                            return;
                        }
                    }
                    current = next;
                }
            });
        Listener {
            requests,
            path: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_session_has_its_own_pipe() {
        assert_eq!(name(1), r"\\.\pipe\rune-instance-1");
        assert_ne!(name(1), name(2));
    }

    // Real pipes of the runner (CI, windows-latest), under a test's own name.
    #[test]
    #[ignore = "opens a named pipe of this session"]
    fn a_second_run_hands_its_links_to_the_first() {
        let session = session().unwrap();
        let name = format!(r"\\.\pipe\rune-instance-test-{}", std::process::id());
        let wide = crate::win::wide(&name);
        assert!(forward(&name, &wide, session, &[]).is_err(), "nobody there");
        let first = instance(&wide, true).expect("the first run claims");
        let listener = Instance {
            first,
            name: wide.clone(),
        }
        .listen(Arc::new(|| {}));
        let link = "https://discord.com/channels/1/2/3".to_owned();
        forward(&name, &wide, session, std::slice::from_ref(&link)).unwrap();
        let mut heard = Vec::new();
        for _ in 0..200 {
            heard.extend(std::iter::from_fn(|| listener.poll()));
            if heard.len() == 2 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(
            heard,
            [
                super::super::Request::Show,
                super::super::Request::Open(link)
            ]
        );
        // Another session's pipe is never told.
        assert!(forward(&name, &wide, session + 1, &[]).is_err());
        // Last: Windows refuses a second first instance; Wine does not.
        assert!(instance(&wide, true).is_none(), "the name is taken");
    }
}
