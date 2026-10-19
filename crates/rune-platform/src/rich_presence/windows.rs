//! The `discord-ipc-N` named pipe on Windows (windows-medios), where the
//! official client listens and programs look for it: `\\?\pipe\discord-ipc-N`,
//! the first free of 0 to 9. The first instance of the name is made first,
//! so another program's pipe is never shared; the pipe's default security
//! lets only this user (and the administrators and the system) write to it,
//! remote clients are refused, and a program of another logon session is
//! not heard. Each connected program gets the next instance made before it
//! is served, so the name never goes free.

use std::os::windows::io::AsRawHandle;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};
use tokio::sync::{mpsc, oneshot};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Pipes::GetNamedPipeClientProcessId;
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;

use super::{protocol, Account, PresenceEvent};

/// Where programs look for the pipes: this, then the slot's number.
pub(super) const PIPES: &str = r"\\?\pipe\discord-ipc-";

/// The pipe of `slot` under `prefix`.
fn name(prefix: &str, slot: u8) -> String {
    format!("{prefix}{slot}")
}

fn instance(name: &str, first: bool) -> std::io::Result<NamedPipeServer> {
    ServerOptions::new()
        .first_pipe_instance(first)
        .reject_remote_clients(true)
        .create(name)
}

pub(super) fn spawn(
    prefix: String,
    account: Arc<Mutex<Option<Account>>>,
    report: impl Fn(PresenceEvent) + Send + Sync + 'static,
    stopped: oneshot::Receiver<()>,
) {
    let report = Arc::new(report);
    let failed = report.clone();
    let started = std::thread::Builder::new()
        .name("rune-rich-presence".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                report(PresenceEvent::Unavailable);
                return;
            };
            runtime.block_on(async {
                // The first free name, as on Linux: the official client's is kept.
                let first = (0..10).find_map(|slot| {
                    let name = name(&prefix, slot);
                    Some((instance(&name, true).ok()?, name))
                });
                let Some((server, name)) = first else {
                    report(PresenceEvent::Unavailable);
                    return;
                };
                let (programs, heard) = mpsc::channel(1);
                tokio::select! {
                    _ = accept(server, &name, programs) => {}
                    _ = protocol::serve(heard, account, report) => {}
                    _ = stopped => {}
                }
            });
        });
    if started.is_err() {
        failed(PresenceEvent::Unavailable);
    }
}

fn session(pid: u32) -> Option<u32> {
    let mut session = 0;
    // SAFETY: a valid out-pointer; an unknown process is an error.
    unsafe { ProcessIdToSessionId(pid, &mut session) }
        .ok()
        .map(|()| session)
}

/// The program on `pipe` runs in this logon session.
fn same_session(pipe: &NamedPipeServer) -> bool {
    let mut pid = 0;
    // SAFETY: a connected pipe this server owns, and a valid out-pointer.
    let asked = unsafe { GetNamedPipeClientProcessId(HANDLE(pipe.as_raw_handle()), &mut pid) };
    asked.is_ok() && session(pid).is_some() && session(pid) == session(std::process::id())
}

/// Hand each program of this session that connects to `programs`.
async fn accept(mut server: NamedPipeServer, name: &str, programs: mpsc::Sender<NamedPipeServer>) {
    loop {
        // A program that connected and left before this is not one.
        let connected = server.connect().await.is_ok();
        let next = loop {
            match instance(name, false) {
                Ok(next) => break next,
                // Out of handles, say: try again later rather than spin.
                Err(_) => tokio::time::sleep(Duration::from_secs(1)).await,
            }
        };
        let program = std::mem::replace(&mut server, next);
        if connected && same_session(&program) && programs.send(program).await.is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use serde_json::{json, Value};

    use super::super::protocol::{FRAME, HANDSHAKE};
    use super::*;
    use crate::rich_presence::{Activity, RichPresence};

    #[test]
    fn programs_find_the_pipes_where_the_official_client_has_them() {
        assert_eq!(name(PIPES, 0), r"\\?\pipe\discord-ipc-0");
        assert_eq!(name(PIPES, 9), r"\\?\pipe\discord-ipc-9");
    }

    fn send(pipe: &mut std::fs::File, opcode: u32, body: &Value) {
        let body = serde_json::to_vec(body).unwrap();
        let mut frame = opcode.to_le_bytes().to_vec();
        frame.extend((body.len() as u32).to_le_bytes());
        frame.extend(body);
        pipe.write_all(&frame).unwrap();
    }

    fn receive(pipe: &mut std::fs::File) -> (u32, Value) {
        let mut header = [0; 8];
        pipe.read_exact(&mut header).unwrap();
        let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
        let mut body = vec![0; u32::from_le_bytes(header[4..].try_into().unwrap()) as usize];
        pipe.read_exact(&mut body).unwrap();
        (opcode, serde_json::from_slice(&body).unwrap())
    }

    fn next(presence: &RichPresence) -> PresenceEvent {
        for _ in 0..500 {
            if let Some(event) = presence.poll() {
                return event;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no event arrived");
    }

    // A pipe of this session under a test's own name, so a running client
    // is never touched.
    #[test]
    fn a_program_on_the_pipe_sets_its_activity_and_leaving_clears_it() {
        let prefix = format!(r"\\.\pipe\rune-test-{}-discord-ipc-", std::process::id());
        let presence = RichPresence::spawn(prefix.clone(), Arc::new(|| {}));
        let open = || {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(name(&prefix, 0))
        };
        let mut pipe = (0..200)
            .find_map(|_| {
                open().ok().or_else(|| {
                    std::thread::sleep(Duration::from_millis(10));
                    None
                })
            })
            .expect("the server never listened");
        send(&mut pipe, HANDSHAKE, &json!({"v": 1, "client_id": "1234"}));
        let (opcode, ready) = receive(&mut pipe);
        assert_eq!((opcode, ready["evt"].as_str()), (FRAME, Some("READY")));
        send(
            &mut pipe,
            FRAME,
            &json!({"cmd": "SET_ACTIVITY", "nonce": "n1", "args": {"pid": 1, "activity": {"details": "Nivel 3"}}}),
        );
        let (_, reply) = receive(&mut pipe);
        assert_eq!(reply["nonce"], "n1");
        let PresenceEvent::Set { client, activity } = next(&presence) else {
            panic!("not set");
        };
        assert_eq!(
            activity,
            Activity {
                application: 1234,
                details: Some("Nivel 3".into()),
                ..Activity::default()
            }
        );
        drop(pipe);
        assert_eq!(next(&presence), PresenceEvent::Cleared { client });
    }
}
