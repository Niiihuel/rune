//! Hiding the window in the tray on Hyprland, which neither hides a Wayland
//! surface nor honors minimizing: the window goes to a special workspace of
//! its own and comes back to the one in view, through Hyprland's command
//! socket, as Serein does. Elsewhere the caller hides or minimizes.
//!
//! Blocking, briefly: call it off the interface's thread.

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(1);
const WORKSPACE: &str = "special:rune";

/// Hyprland's command socket for this session, when RUNE runs under it.
pub fn socket() -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok()?;
    if signature.is_empty()
        || !signature
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return None;
    }
    let dir = crate::local_socket::runtime_dir()?;
    Some(dir.join("hypr").join(signature).join(".socket.sock"))
}

/// Send the window of process `pid` out of sight.
pub fn hide(socket: &PathBuf, pid: u32) -> std::io::Result<()> {
    ok(request(
        socket,
        &format!("dispatch movetoworkspacesilent {WORKSPACE},pid:{pid}"),
    )?)
}

/// Bring it back to the workspace in view, and focus it.
pub fn show(socket: &PathBuf, pid: u32) -> std::io::Result<()> {
    let active = request(socket, "j/activeworkspace")?;
    let id = serde_json::from_str::<serde_json::Value>(&active)
        .ok()
        .and_then(|workspace| workspace["id"].as_i64())
        .ok_or_else(|| std::io::Error::from(std::io::ErrorKind::InvalidData))?;
    ok(request(
        socket,
        &format!("dispatch movetoworkspace {id},pid:{pid}"),
    )?)?;
    ok(request(socket, &format!("dispatch focuswindow pid:{pid}"))?)
}

fn ok(answer: String) -> std::io::Result<()> {
    if answer.trim() == "ok" {
        Ok(())
    } else {
        Err(std::io::Error::other("Hyprland refused"))
    }
}

fn request(socket: &PathBuf, command: &str) -> std::io::Result<String> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;
    stream.write_all(command.as_bytes())?;
    let mut answer = String::new();
    stream.take(64 * 1024).read_to_string(&mut answer)?;
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn the_window_goes_to_a_special_workspace_and_back_to_the_one_in_view() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".socket.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let mut heard = Vec::new();
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut command = [0u8; 256];
                let read = stream.read(&mut command).unwrap();
                let command = String::from_utf8_lossy(&command[..read]).into_owned();
                let answer = if command == "j/activeworkspace" {
                    r#"{"id": 3, "name": "3"}"#
                } else {
                    "ok"
                };
                stream.write_all(answer.as_bytes()).unwrap();
                heard.push(command);
            }
            heard
        });
        hide(&path, 42).unwrap();
        show(&path, 42).unwrap();
        assert_eq!(
            server.join().unwrap(),
            [
                "dispatch movetoworkspacesilent special:rune,pid:42",
                "j/activeworkspace",
                "dispatch movetoworkspace 3,pid:42",
                "dispatch focuswindow pid:42",
            ]
        );
    }
}
