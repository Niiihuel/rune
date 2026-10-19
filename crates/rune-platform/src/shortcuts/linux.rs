//! The `org.freedesktop.portal.GlobalShortcuts` session, on its own thread.
//!
//! CreateSession, then BindShortcuts (which may show the desktop's dialog),
//! then Activated/Deactivated until the handle drops, the desktop closes the
//! session or the portal goes away. Every signal must come from the portal's
//! unique name and name this session.

use std::{collections::HashMap, time::Duration};

use futures_util::StreamExt;
use tokio::{sync::oneshot, time::timeout};
use zbus::{
    message::Type,
    zvariant::{DynamicType, OwnedObjectPath, OwnedValue, Value},
    Connection, MatchRule, Message, MessageStream,
};

use super::{Shortcut, ShortcutError, ShortcutEvent};
use crate::portal::{token, DESKTOP, DESTINATION, REQUEST};

const GLOBAL_SHORTCUTS: &str = "org.freedesktop.portal.GlobalShortcuts";
const SESSION: &str = "org.freedesktop.portal.Session";
const RPC_TIMEOUT: Duration = Duration::from_secs(2);
/// BindShortcuts can wait on the person in the desktop's dialog.
const DIALOG_TIMEOUT: Duration = Duration::from_secs(300);
const MAX_BODY_BYTES: usize = 64 * 1024;

type Described = Vec<(String, HashMap<String, OwnedValue>)>;

pub(super) fn spawn(
    address: Option<String>,
    shortcuts: Vec<Shortcut>,
    report: impl Fn(ShortcutEvent) + Send + 'static,
    stopped: oneshot::Receiver<()>,
) {
    // A thread that cannot start drops `report`: the handle then reads as
    // ended through its disconnected channel.
    let _ = std::thread::Builder::new()
        .name("rune-shortcuts".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                report(ShortcutEvent::Ended(ShortcutError::FAILED));
                return;
            };
            runtime.block_on(async {
                if let Err(error) = run(address, &shortcuts, &report, stopped).await {
                    report(ShortcutEvent::Ended(error));
                }
            });
        });
}

/// `Ok` only when stopped by the handle; every other end is its reason.
async fn run(
    address: Option<String>,
    shortcuts: &[Shortcut],
    report: &impl Fn(ShortcutEvent),
    mut stopped: oneshot::Receiver<()>,
) -> Result<(), ShortcutError> {
    let builder = match &address {
        Some(address) => zbus::connection::Builder::address(address.as_str()),
        None => zbus::connection::Builder::session(),
    }
    .map_err(|_| ShortcutError::NO_PORTAL)?;
    let connection = timeout(
        RPC_TIMEOUT,
        builder.method_timeout(RPC_TIMEOUT).max_queued(16).build(),
    )
    .await
    .map_err(|_| ShortcutError::NO_PORTAL)?
    .map_err(|_| ShortcutError::NO_PORTAL)?;
    // Activates the portal; a portal without the interface has no version.
    connection
        .call_method(
            Some(DESTINATION),
            DESKTOP,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(GLOBAL_SHORTCUTS, "version"),
        )
        .await
        .map_err(|error| match error {
            zbus::Error::MethodError(name, _, _)
                if name.as_str().ends_with("ServiceUnknown")
                    || name.as_str().ends_with("NameHasNoOwner") =>
            {
                ShortcutError::NO_PORTAL
            }
            zbus::Error::MethodError(..) => ShortcutError::UNSUPPORTED,
            _ => ShortcutError::FAILED,
        })?;
    let owners = stream(
        &connection,
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.DBus")
            .unwrap()
            .interface("org.freedesktop.DBus")
            .unwrap()
            .member("NameOwnerChanged")
            .unwrap()
            .add_arg(DESTINATION)
            .unwrap()
            .build(),
    )
    .await?;
    let owner = zbus::fdo::DBusProxy::new(&connection)
        .await
        .map_err(|_| ShortcutError::FAILED)?
        .get_name_owner(DESTINATION.try_into().unwrap())
        .await
        .map_err(|_| ShortcutError::NO_PORTAL)?;
    let sender = connection
        .unique_name()
        .ok_or(ShortcutError::FAILED)?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let session_token = token().map_err(|_| ShortcutError::FAILED)?;
    let session = OwnedObjectPath::try_from(format!("{DESKTOP}/session/{sender}/{session_token}"))
        .map_err(|_| ShortcutError::FAILED)?;
    let signals = |path: &str, interface: &'static str| {
        MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(owner.as_str())
            .unwrap()
            .path(path.to_owned())
            .unwrap()
            .interface(interface)
            .unwrap()
            .build()
    };
    let mut keys = stream(&connection, signals(DESKTOP, GLOBAL_SHORTCUTS)).await?;
    let mut closed = stream(&connection, signals(session.as_str(), SESSION)).await?;
    let mut portal = Portal {
        connection: &connection,
        owner: owner.as_str(),
        sender: &sender,
        owners,
        stopped: &mut stopped,
    };

    let created = portal
        .request("CreateSession", RPC_TIMEOUT, |token| {
            (HashMap::from([
                ("handle_token", Value::from(token.to_owned())),
                ("session_handle_token", Value::from(session_token.as_str())),
            ]),)
        })
        .await;
    // From here on the session may exist: close it on every way out.
    let result = async {
        let mut fields = created?;
        let handle = fields
            .remove("session_handle")
            .ok_or(ShortcutError::FAILED)?;
        // The specification types it as a string; some portals send a path.
        let handle = match String::try_from(handle.try_clone().map_err(|_| ShortcutError::FAILED)?)
        {
            Ok(handle) => handle,
            Err(_) => OwnedObjectPath::try_from(handle)
                .map_err(|_| ShortcutError::FAILED)?
                .to_string(),
        };
        if handle != session.as_str() {
            return Err(ShortcutError::FAILED);
        }
        let wanted: Vec<(&str, HashMap<&str, Value>)> = shortcuts
            .iter()
            .map(|shortcut| {
                let mut properties =
                    HashMap::from([("description", Value::from(shortcut.description.as_str()))]);
                if let Some(trigger) = &shortcut.preferred_trigger {
                    properties.insert("preferred_trigger", Value::from(trigger.as_str()));
                }
                (shortcut.id.as_str(), properties)
            })
            .collect();
        let mut fields = portal
            .request("BindShortcuts", DIALOG_TIMEOUT, |token| {
                (
                    session.clone(),
                    wanted,
                    "",
                    HashMap::from([("handle_token", Value::from(token.to_owned()))]),
                )
            })
            .await?;
        let bound = described(
            Described::try_from(fields.remove("shortcuts").ok_or(ShortcutError::DENIED)?)
                .map_err(|_| ShortcutError::FAILED)?,
        );
        if !shortcuts
            .iter()
            .all(|shortcut| bound.iter().any(|(id, _)| *id == shortcut.id))
        {
            return Err(ShortcutError::DENIED);
        }
        report(ShortcutEvent::Bound(bound));
        loop {
            tokio::select! {
                biased;
                _ = &mut *portal.stopped => return Ok(()),
                _ = portal.owners.next() => return Err(ShortcutError::CLOSED),
                message = closed.next() => match message {
                    Some(Ok(message)) if member(&message).as_deref() != Some("Closed") => {}
                    _ => return Err(ShortcutError::CLOSED),
                },
                message = keys.next() => {
                    let Some(Ok(message)) = message else { return Err(ShortcutError::CLOSED) };
                    if let Some(event) = key_event(&message, &session, shortcuts) {
                        report(event);
                    }
                }
            }
        }
    }
    .await;
    let _ = timeout(
        RPC_TIMEOUT,
        connection.call_method(
            Some(owner.as_str()),
            session.as_str(),
            Some(SESSION),
            "Close",
            &(),
        ),
    )
    .await;
    result
}

async fn stream(
    connection: &Connection,
    rule: MatchRule<'_>,
) -> Result<MessageStream, ShortcutError> {
    timeout(
        RPC_TIMEOUT,
        MessageStream::for_match_rule(rule, connection, Some(16)),
    )
    .await
    .map_err(|_| ShortcutError::FAILED)?
    .map_err(|_| ShortcutError::FAILED)
}

struct Portal<'a> {
    connection: &'a Connection,
    owner: &'a str,
    sender: &'a str,
    owners: MessageStream,
    stopped: &'a mut oneshot::Receiver<()>,
}

impl Portal<'_> {
    /// Call `method` with the body `body` builds for a fresh request token, and
    /// wait for that request's Response. Subscribed before calling, so an
    /// early Response is queued rather than lost. A stop or a vanished portal
    /// reads as closed.
    async fn request<B>(
        &mut self,
        method: &str,
        wait: Duration,
        body: impl FnOnce(&str) -> B,
    ) -> Result<HashMap<String, OwnedValue>, ShortcutError>
    where
        B: zbus::export::serde::Serialize + DynamicType,
    {
        let token = token().map_err(|_| ShortcutError::FAILED)?;
        let path = OwnedObjectPath::try_from(format!("{DESKTOP}/request/{}/{token}", self.sender))
            .map_err(|_| ShortcutError::FAILED)?;
        let mut responses = stream(
            self.connection,
            MatchRule::builder()
                .msg_type(Type::Signal)
                .sender(self.owner)
                .unwrap()
                .path(path.as_str())
                .unwrap()
                .interface(REQUEST)
                .unwrap()
                .member("Response")
                .unwrap()
                .build(),
        )
        .await?;
        let body = body(&token);
        let reply = tokio::select! {
            biased;
            _ = &mut *self.stopped => return Err(ShortcutError::CLOSED),
            _ = self.owners.next() => return Err(ShortcutError::CLOSED),
            reply = self.connection.call_method(
                Some(self.owner),
                DESKTOP,
                Some(GLOBAL_SHORTCUTS),
                method,
                &body,
            ) => reply.map_err(|_| ShortcutError::FAILED)?,
        };
        let handle: OwnedObjectPath = reply
            .body()
            .deserialize()
            .map_err(|_| ShortcutError::FAILED)?;
        if handle != path {
            return Err(ShortcutError::FAILED);
        }
        let response = tokio::select! {
            biased;
            _ = &mut *self.stopped => Err(ShortcutError::CLOSED),
            _ = self.owners.next() => Err(ShortcutError::CLOSED),
            _ = tokio::time::sleep(wait) => Err(ShortcutError::FAILED),
            response = responses.next() => match response {
                Some(Ok(message)) => Ok(message),
                _ => Err(ShortcutError::FAILED),
            },
        };
        if response.is_err() {
            // The dialog may still be up; nothing else will answer it.
            let _ = timeout(
                RPC_TIMEOUT,
                self.connection.call_method(
                    Some(self.owner),
                    path.as_str(),
                    Some(REQUEST),
                    "Close",
                    &(),
                ),
            )
            .await;
        }
        let response = response?;
        if response.body().len() > MAX_BODY_BYTES {
            return Err(ShortcutError::FAILED);
        }
        let (status, fields): (u32, HashMap<String, OwnedValue>) = response
            .body()
            .deserialize()
            .map_err(|_| ShortcutError::FAILED)?;
        match status {
            0 => Ok(fields),
            1 => Err(ShortcutError::DENIED),
            _ => Err(ShortcutError::FAILED),
        }
    }
}

fn member(message: &Message) -> Option<String> {
    message.header().member().map(|member| member.to_string())
}

/// Each bound shortcut's id and the desktop's description of its trigger.
fn described(list: Described) -> Vec<(String, String)> {
    list.into_iter()
        .map(|(id, mut properties)| {
            let trigger = properties
                .remove("trigger_description")
                .and_then(|value| String::try_from(value).ok())
                .unwrap_or_default();
            (id, trigger)
        })
        .collect()
}

/// A press or release of one of `shortcuts` in `session`; nothing else.
fn key_event(
    message: &Message,
    session: &OwnedObjectPath,
    shortcuts: &[Shortcut],
) -> Option<ShortcutEvent> {
    if message.body().len() > MAX_BODY_BYTES {
        return None;
    }
    let pressed = match member(message)?.as_str() {
        "Activated" => true,
        "Deactivated" => false,
        "ShortcutsChanged" => {
            let (handle, changed): (OwnedObjectPath, Described) =
                message.body().deserialize().ok()?;
            return (handle == *session).then(|| ShortcutEvent::Bound(described(changed)));
        }
        _ => return None,
    };
    let (handle, id, _, _): (OwnedObjectPath, String, u64, HashMap<String, OwnedValue>) =
        message.body().deserialize().ok()?;
    if handle != *session || !shortcuts.iter().any(|shortcut| shortcut.id == id) {
        return None;
    }
    Some(if pressed {
        ShortcutEvent::Pressed(id)
    } else {
        ShortcutEvent::Released(id)
    })
}

#[cfg(test)]
mod tests;
