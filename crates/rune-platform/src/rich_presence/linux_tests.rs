//! A program speaking the local protocol to a server in a private folder.

use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::time::Duration;

use serde_json::{json, Value};

use super::super::protocol::{CLOSE, FRAME, HANDSHAKE, MAX_FRAME, PING, PONG};
use super::*;
use crate::rich_presence::{Activity, RichPresence};

struct Program(UnixStream);

impl Program {
    fn connect(dir: &std::path::Path) -> Self {
        let path = dir.join("discord-ipc-0");
        for _ in 0..200 {
            if let Ok(stream) = UnixStream::connect(&path) {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                return Self(stream);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("the server never listened");
    }

    fn send(&mut self, opcode: u32, body: &Value) {
        let body = serde_json::to_vec(body).unwrap();
        let mut frame = opcode.to_le_bytes().to_vec();
        frame.extend((body.len() as u32).to_le_bytes());
        frame.extend(body);
        self.0.write_all(&frame).unwrap();
    }

    fn receive(&mut self) -> (u32, Value) {
        let mut header = [0; 8];
        self.0.read_exact(&mut header).unwrap();
        let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
        let mut body = vec![0; u32::from_le_bytes(header[4..].try_into().unwrap()) as usize];
        self.0.read_exact(&mut body).unwrap();
        (opcode, serde_json::from_slice(&body).unwrap())
    }

    fn closed(&mut self) -> bool {
        let mut byte = [0; 1];
        matches!(self.0.read(&mut byte), Ok(0) | Err(_))
    }
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

fn private_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

#[test]
fn a_program_greets_sets_its_activity_and_leaving_clears_it() {
    let dir = private_dir();
    let presence = RichPresence::spawn(Some(dir.path().to_owned()), Arc::new(|| {}));
    presence.set_account(Some(Account {
        id: 42,
        username: "ana".into(),
    }));
    let mut program = Program::connect(dir.path());
    let mode = std::fs::metadata(dir.path().join("discord-ipc-0"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
    program.send(HANDSHAKE, &json!({"v": 1, "client_id": "1234"}));
    let (opcode, ready) = program.receive();
    assert_eq!(opcode, FRAME);
    assert_eq!(ready["evt"], "READY");
    assert_eq!(ready["data"]["user"]["id"], "42");
    assert_eq!(ready["data"]["user"]["username"], "ana");
    program.send(
        FRAME,
        &json!({
            "cmd": "SET_ACTIVITY",
            "nonce": "n1",
            "args": {"pid": 77, "activity": {
                "details": "Nivel 3",
                "state": "En grupo",
                "timestamps": {"start": 1_700_000_000},
                "assets": {"large_image": "map", "large_text": "Mapa"},
                "buttons": [{"label": "x", "url": "https://example.com"}],
                "secrets": {"join": "s"},
            }},
        }),
    );
    let (_, ack) = program.receive();
    assert_eq!(ack["cmd"], "SET_ACTIVITY");
    assert_eq!(ack["nonce"], "n1");
    assert_eq!(ack["data"]["details"], "Nivel 3");
    let PresenceEvent::Set { client, activity } = next(&presence) else {
        panic!("an activity")
    };
    assert_eq!(
        activity,
        Activity {
            application: 1234,
            kind: 0,
            details: Some("Nivel 3".into()),
            state: Some("En grupo".into()),
            start: Some(1_700_000_000_000),
            end: None,
            large_image: Some("map".into()),
            large_text: Some("Mapa".into()),
            small_image: None,
            small_text: None,
        },
        "seconds become milliseconds; buttons and secrets are dropped"
    );
    // A ping is answered; anything but the two commands is refused.
    program.send(PING, &json!({"t": 1}));
    assert_eq!(program.receive(), (PONG, json!({"t": 1})));
    program.send(
        FRAME,
        &json!({"cmd": "AUTHORIZE", "nonce": "n2", "args": {}}),
    );
    let (_, refused) = program.receive();
    assert_eq!(refused["evt"], "ERROR");
    assert_eq!(refused["data"]["code"], 4000);
    program.send(
        FRAME,
        &json!({"cmd": "INVITE_BROWSER", "nonce": "n3", "args": {"code": "abc-DEF_1"}}),
    );
    let _ = program.receive();
    assert_eq!(
        next(&presence),
        PresenceEvent::Invite {
            code: "abc-DEF_1".into()
        }
    );
    drop(program);
    assert_eq!(next(&presence), PresenceEvent::Cleared { client });
}

#[test]
fn bad_greetings_and_oversized_frames_end_the_connection() {
    let dir = private_dir();
    let presence = RichPresence::spawn(Some(dir.path().to_owned()), Arc::new(|| {}));
    let mut program = Program::connect(dir.path());
    program.send(HANDSHAKE, &json!({"v": 1, "client_id": "not a number"}));
    let (opcode, _) = program.receive();
    assert_eq!(opcode, CLOSE);
    assert!(program.closed());
    let mut program = Program::connect(dir.path());
    program.send(HANDSHAKE, &json!({"v": 1, "client_id": "5"}));
    let _ = program.receive();
    let mut frame = FRAME.to_le_bytes().to_vec();
    frame.extend((MAX_FRAME + 1).to_le_bytes());
    program.0.write_all(&frame).unwrap();
    assert!(program.closed());
    // An activity that clears, then nothing more: nothing was set.
    let mut program = Program::connect(dir.path());
    program.send(HANDSHAKE, &json!({"v": "1", "client_id": "5"}));
    let _ = program.receive();
    program.send(
        FRAME,
        &json!({"cmd": "SET_ACTIVITY", "nonce": "n", "args": {"pid": 1, "activity": {"type": 1}}}),
    );
    let (_, refused) = program.receive();
    assert_eq!(
        refused["evt"], "ERROR",
        "streaming is not a program's to set"
    );
    drop(program);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(presence.poll(), None);
}

#[test]
fn a_shared_folder_is_never_used() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o777)).unwrap();
    let presence = RichPresence::spawn(Some(dir.path().to_owned()), Arc::new(|| {}));
    assert_eq!(next(&presence), PresenceEvent::Unavailable);
    assert!(!dir.path().join("discord-ipc-0").exists());
}
