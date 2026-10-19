//! The local Rich Presence protocol, over any stream: one program's
//! session, and the server that serves each program on a task of its own.
//! Linux listens on a socket ([`super::linux`]), Windows on a named pipe
//! ([`super::windows`]); both hand each program of this user here.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::sync::{mpsc, Semaphore};
use tokio::time::timeout;

use super::{Account, Activity, PresenceEvent};

/// Programs heard at once.
const MAX_CLIENTS: usize = 8;
/// The largest frame a program may send.
pub(super) const MAX_FRAME: u32 = 16 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);
/// Once a frame starts, the rest of it must follow this soon.
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);
/// A pause after each command, so a busy program cannot flood the account.
const COMMAND_PAUSE: Duration = Duration::from_millis(100);
const MAX_TEXT: usize = 128;
const MAX_IMAGE: usize = 512;

pub(super) const HANDSHAKE: u32 = 0;
pub(super) const FRAME: u32 = 1;
pub(super) const CLOSE: u32 = 2;
pub(super) const PING: u32 = 3;
pub(super) const PONG: u32 = 4;

/// Serve each program `programs` hands over (already known to be this
/// user's), at most [`MAX_CLIENTS`] at once; what they do goes to `report`
/// in the order they do it.
pub(super) async fn serve<S>(
    mut programs: mpsc::Receiver<S>,
    account: Arc<Mutex<Option<Account>>>,
    report: Arc<impl Fn(PresenceEvent) + Send + Sync + 'static>,
) where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let clients = Arc::new(Semaphore::new(MAX_CLIENTS));
    let next = AtomicU64::new(1);
    // Events leave in the order the clients produce them.
    let (events, mut received) = mpsc::unbounded_channel::<PresenceEvent>();
    let forward = async {
        while let Some(event) = received.recv().await {
            report(event);
        }
    };
    let accept = async {
        while let Some(stream) = programs.recv().await {
            let Ok(permit) = clients.clone().try_acquire_owned() else {
                continue;
            };
            let client = next.fetch_add(1, Ordering::Relaxed);
            let account = account.lock().ok().and_then(|account| account.clone());
            let events = events.clone();
            tokio::spawn(async move {
                let set = client_session(stream, client, account, &events).await;
                if set {
                    let _ = events.send(PresenceEvent::Cleared { client });
                }
                drop(permit);
            });
        }
    };
    tokio::join!(accept, forward);
}

/// Serve one program until it leaves or breaks the protocol. True when it
/// had set an activity, which its leaving clears.
async fn client_session(
    mut stream: impl AsyncRead + AsyncWrite + Unpin,
    client: u64,
    account: Option<Account>,
    events: &mpsc::UnboundedSender<PresenceEvent>,
) -> bool {
    let Ok(Ok((HANDSHAKE, greeting))) = timeout(HANDSHAKE_TIMEOUT, read_frame(&mut stream)).await
    else {
        return false;
    };
    let Some(application) = greeting_application(&greeting) else {
        let _ = write_frame(
            &mut stream,
            CLOSE,
            &json!({"code": 4000, "message": "Invalid client ID"}),
        )
        .await;
        return false;
    };
    if write_frame(&mut stream, FRAME, &ready(account.as_ref()))
        .await
        .is_err()
    {
        return false;
    }
    let mut set = false;
    loop {
        let Ok((opcode, body)) = read_frame(&mut stream).await else {
            return set;
        };
        match opcode {
            FRAME => {
                let (reply, event) = command(application, &body);
                if let Some(event) = event {
                    let event = match event {
                        PresenceEvent::Set { activity, .. } => {
                            set = true;
                            PresenceEvent::Set { client, activity }
                        }
                        PresenceEvent::Cleared { .. } => {
                            set = false;
                            PresenceEvent::Cleared { client }
                        }
                        other => other,
                    };
                    let _ = events.send(event);
                }
                if write_frame(&mut stream, FRAME, &reply).await.is_err() {
                    return set;
                }
                tokio::time::sleep(COMMAND_PAUSE).await;
            }
            PING => {
                if write_frame(&mut stream, PONG, &body).await.is_err() {
                    return set;
                }
            }
            PONG => {}
            // A close, or an opcode this protocol does not have.
            _ => return set,
        }
    }
}

/// The application ID of a greeting `{"v":1,"client_id":"…"}`.
fn greeting_application(greeting: &Value) -> Option<u64> {
    let version = &greeting["v"];
    if version.as_u64() != Some(1) && version.as_str() != Some("1") {
        return None;
    }
    let id = greeting["client_id"].as_str()?;
    (id.len() <= 20 && id.bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| id.parse().ok())
        .flatten()
        .filter(|id| *id != 0)
}

fn ready(account: Option<&Account>) -> Value {
    let user = match account {
        Some(account) => json!({
            "id": account.id.to_string(),
            "username": account.username,
            "discriminator": "0",
            "avatar": null,
        }),
        None => json!({"id": "0", "username": "RUNE", "discriminator": "0", "avatar": null}),
    };
    json!({
        "cmd": "DISPATCH",
        "evt": "READY",
        "data": {
            "v": 1,
            "config": {
                "cdn_host": "cdn.discordapp.com",
                "api_endpoint": "//discord.com/api",
                "environment": "production",
            },
            "user": user,
        },
    })
}

/// The answer to one command, and what it changed. `Set`'s and
/// `Cleared`'s client is filled in by the caller.
fn command(application: u64, body: &Value) -> (Value, Option<PresenceEvent>) {
    let cmd = body["cmd"].as_str().unwrap_or_default();
    let nonce = body["nonce"]
        .as_str()
        .filter(|nonce| (1..=128).contains(&nonce.len()));
    let refuse = |message: &str| {
        (
            json!({
                "cmd": body["cmd"],
                "evt": "ERROR",
                "nonce": nonce,
                "data": {"code": 4000, "message": message},
            }),
            None,
        )
    };
    let Some(nonce) = nonce else {
        return refuse("Missing nonce");
    };
    match cmd {
        "SET_ACTIVITY" => {
            let args = &body["args"];
            if !args["pid"]
                .as_u64()
                .is_some_and(|pid| pid != 0 && pid <= u64::from(u32::MAX))
            {
                return refuse("Invalid pid");
            }
            match &args["activity"] {
                Value::Null => (
                    json!({"cmd": "SET_ACTIVITY", "evt": null, "nonce": nonce, "data": null}),
                    Some(PresenceEvent::Cleared { client: 0 }),
                ),
                given => match activity(application, given) {
                    Some(activity) => (
                        json!({
                            "cmd": "SET_ACTIVITY",
                            "evt": null,
                            "nonce": nonce,
                            "data": echo(&activity),
                        }),
                        Some(PresenceEvent::Set {
                            client: 0,
                            activity,
                        }),
                    ),
                    None => refuse("Unsupported or invalid activity payload"),
                },
            }
        }
        "INVITE_BROWSER" => {
            let code = body["args"]["code"].as_str().unwrap_or_default();
            let valid = (1..=64).contains(&code.len())
                && code
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
            if !valid {
                return refuse("Invalid invite code");
            }
            (
                json!({"cmd": "INVITE_BROWSER", "evt": null, "nonce": nonce, "data": {"code": code}}),
                Some(PresenceEvent::Invite {
                    code: code.to_owned(),
                }),
            )
        }
        _ => refuse("Unsupported command"),
    }
}

/// What RUNE keeps of an activity a program sent; `None` when it is unusable.
fn activity(application: u64, given: &Value) -> Option<Activity> {
    if !given.is_object() {
        return None;
    }
    let kind = match given.get("type") {
        None | Some(Value::Null) => 0,
        Some(kind) => kind.as_u64().filter(|kind| matches!(kind, 0 | 2 | 3 | 5))? as u8,
    };
    let text = |value: &Value, max: usize| -> Result<Option<String>, ()> {
        match value {
            Value::Null => Ok(None),
            Value::String(text) => {
                let text = text.trim();
                if text.chars().count() > max || text.chars().any(char::is_control) {
                    Err(())
                } else {
                    Ok((!text.is_empty()).then(|| text.to_owned()))
                }
            }
            _ => Err(()),
        }
    };
    // Seconds, as some programs send them, become milliseconds.
    let time = |value: &Value| {
        value.as_i64().filter(|time| *time > 0).map(|time| {
            if time < 10_000_000_000 {
                time * 1000
            } else {
                time
            }
        })
    };
    let assets = &given["assets"];
    Some(Activity {
        application,
        kind,
        details: text(&given["details"], MAX_TEXT).ok()?,
        state: text(&given["state"], MAX_TEXT).ok()?,
        start: time(&given["timestamps"]["start"]),
        end: time(&given["timestamps"]["end"]),
        large_image: text(&assets["large_image"], MAX_IMAGE).ok()?,
        large_text: text(&assets["large_text"], MAX_TEXT).ok()?,
        small_image: text(&assets["small_image"], MAX_IMAGE).ok()?,
        small_text: text(&assets["small_text"], MAX_TEXT).ok()?,
    })
}

/// The activity as RUNE kept it, in the shape the program sent.
fn echo(activity: &Activity) -> Value {
    json!({
        "type": activity.kind,
        "details": activity.details,
        "state": activity.state,
        "timestamps": {"start": activity.start, "end": activity.end},
        "assets": {
            "large_image": activity.large_image,
            "large_text": activity.large_text,
            "small_image": activity.small_image,
            "small_text": activity.small_text,
        },
        "application_id": activity.application.to_string(),
    })
}

async fn read_frame(stream: &mut (impl AsyncRead + Unpin)) -> std::io::Result<(u32, Value)> {
    let mut header = [0; 8];
    // Waiting for a frame is idle time; once it starts it must finish.
    stream.read_exact(&mut header[..1]).await?;
    let invalid = || std::io::Error::from(std::io::ErrorKind::InvalidData);
    timeout(FRAME_TIMEOUT, async {
        stream.read_exact(&mut header[1..]).await?;
        let opcode = u32::from_le_bytes(header[..4].try_into().unwrap());
        let length = u32::from_le_bytes(header[4..].try_into().unwrap());
        if length > MAX_FRAME {
            return Err(invalid());
        }
        let mut body = vec![0; length as usize];
        stream.read_exact(&mut body).await?;
        let body = if body.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&body).map_err(|_| invalid())?
        };
        Ok((opcode, body))
    })
    .await
    .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?
}

/// One frame in one write: some programs' libraries break on split frames.
async fn write_frame(
    stream: &mut (impl AsyncWrite + Unpin),
    opcode: u32,
    body: &Value,
) -> std::io::Result<()> {
    let body = serde_json::to_vec(body)?;
    let mut frame = Vec::with_capacity(8 + body.len());
    frame.extend(opcode.to_le_bytes());
    frame.extend((body.len() as u32).to_le_bytes());
    frame.extend(body);
    timeout(FRAME_TIMEOUT, stream.write_all(&frame))
        .await
        .map_err(|_| std::io::Error::from(std::io::ErrorKind::TimedOut))?
}
