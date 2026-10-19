//! The portal's `Settings` interface, on its own thread: read the color
//! scheme once, then each `SettingChanged` for it from the portal's owner.

use std::time::Duration;

use futures_util::StreamExt;
use tokio::{sync::oneshot, time::timeout};
use zbus::{
    message::Type,
    zvariant::{OwnedValue, Value},
    Connection, MatchRule, MessageStream,
};

use super::ColorScheme;
use crate::portal::{DESKTOP, DESTINATION};

const SETTINGS: &str = "org.freedesktop.portal.Settings";
const NAMESPACE: &str = "org.freedesktop.appearance";
const KEY: &str = "color-scheme";
const RPC_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) fn spawn(
    address: Option<String>,
    report: impl Fn(ColorScheme) + Send + 'static,
    stopped: oneshot::Receiver<()>,
) {
    // A thread that cannot start leaves the preference unknown: RUNE's own.
    let _ = std::thread::Builder::new()
        .name("rune-appearance".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async {
                tokio::select! {
                    _ = run(address, &report) => {}
                    _ = stopped => {}
                }
            });
        });
}

/// Report the scheme, then every change, until the bus goes away. A portal
/// that restarts (a new owner of its name) is read and followed again; one
/// that goes away leaves the last scheme until another comes.
async fn run(address: Option<String>, report: &impl Fn(ColorScheme)) -> Option<()> {
    let builder = match &address {
        Some(address) => zbus::connection::Builder::address(address.as_str()),
        None => zbus::connection::Builder::session(),
    }
    .ok()?;
    let connection = timeout(
        RPC_TIMEOUT,
        builder.method_timeout(RPC_TIMEOUT).max_queued(16).build(),
    )
    .await
    .ok()?
    .ok()?;
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender("org.freedesktop.DBus")
        .ok()?
        .path("/org/freedesktop/DBus")
        .ok()?
        .interface("org.freedesktop.DBus")
        .ok()?
        .member("NameOwnerChanged")
        .ok()?
        .add_arg(DESTINATION)
        .ok()?
        .build();
    // Subscribed before asking who owns it, so a restart in between is seen.
    let mut owners = timeout(
        RPC_TIMEOUT,
        MessageStream::for_match_rule(rule, &connection, Some(4)),
    )
    .await
    .ok()?
    .ok()?;
    let owner = timeout(RPC_TIMEOUT, async {
        zbus::fdo::DBusProxy::new(&connection)
            .await?
            .get_name_owner(DESTINATION.try_into().unwrap())
            .await
    })
    .await
    .ok()?;
    let mut owner = match owner {
        Ok(owner) => Some(owner.to_string()),
        // No portal yet: the preference stays unknown until one comes.
        Err(zbus::fdo::Error::NameHasNoOwner(_)) => None,
        Err(_) => return None,
    };
    let mut starts = 0;
    loop {
        owner = match owner {
            Some(current) => follow(&connection, &current, report, &mut owners).await?,
            None => {
                // The bus starts the portal on demand and nothing restarts
                // one that went away: ask, as the dialogs do, a few times
                // only, so one that keeps failing is not started forever.
                if starts < STARTS {
                    starts += 1;
                    start(&connection).await;
                }
                next_owner(&mut owners).await?
            }
        };
    }
}

/// Times the portal is started by this watcher, at most.
const STARTS: u32 = 3;

/// Ask the bus to start the portal; its new owner comes as
/// `NameOwnerChanged`, so the answer itself does not matter.
async fn start(connection: &Connection) {
    let _ = timeout(RPC_TIMEOUT, async {
        zbus::fdo::DBusProxy::new(connection)
            .await?
            .start_service_by_name(DESTINATION.try_into().unwrap(), 0)
            .await
    })
    .await;
}

/// Follow the portal owned by `owner` until its name changes hands: the new
/// owner, `None` when nobody has it. Ends (`None` outer) with the bus.
async fn follow(
    connection: &Connection,
    owner: &str,
    report: &impl Fn(ColorScheme),
    owners: &mut MessageStream,
) -> Option<Option<String>> {
    let rule = MatchRule::builder()
        .msg_type(Type::Signal)
        .sender(owner)
        .ok()?
        .path(DESKTOP)
        .ok()?
        .interface(SETTINGS)
        .ok()?
        .member("SettingChanged")
        .ok()?
        .add_arg(NAMESPACE)
        .ok()?
        .build();
    // Subscribed before reading, so a switch in between is not lost.
    let mut changes = timeout(
        RPC_TIMEOUT,
        MessageStream::for_match_rule(rule, connection, Some(16)),
    )
    .await
    .ok()?
    .ok()?;
    if let Some(scheme) = read(connection, owner).await {
        report(scheme);
    }
    loop {
        tokio::select! {
            message = changes.next() => {
                let Some(Ok(message)) = message else { return None };
                let Ok((namespace, key, value)) =
                    message.body().deserialize::<(String, String, OwnedValue)>()
                else {
                    continue;
                };
                if namespace == NAMESPACE && key == KEY {
                    if let Some(scheme) = scheme(&value) {
                        report(scheme);
                    }
                }
            }
            changed = owner_changed(owners) => match changed? {
                Some(new) if new == owner => {}
                new => return Some(new),
            },
        }
    }
}

/// The next owner the portal's name has; ends (`None`) with the bus.
async fn next_owner(owners: &mut MessageStream) -> Option<Option<String>> {
    loop {
        if let Some(owner) = owner_changed(owners).await? {
            return Some(Some(owner));
        }
    }
}

/// The new owner in the next `NameOwnerChanged` of the portal's name, from
/// the bus itself: `Some(None)` when nobody has it now.
async fn owner_changed(owners: &mut MessageStream) -> Option<Option<String>> {
    loop {
        let message = owners.next().await?.ok()?;
        if message
            .header()
            .sender()
            .is_none_or(|sender| sender.as_str() != "org.freedesktop.DBus")
        {
            continue;
        }
        let Ok((name, _, new)) = message.body().deserialize::<(String, String, String)>() else {
            continue;
        };
        if name == DESTINATION {
            return Some((!new.is_empty()).then_some(new));
        }
    }
}

/// `ReadOne` (portal version 2), or the older `Read`, which wraps the
/// value in one more variant.
async fn read(connection: &Connection, owner: &str) -> Option<ColorScheme> {
    for method in ["ReadOne", "Read"] {
        let reply = timeout(
            RPC_TIMEOUT,
            connection.call_method(
                Some(owner),
                DESKTOP,
                Some(SETTINGS),
                method,
                &(NAMESPACE, KEY),
            ),
        )
        .await
        .ok()?;
        if let Ok(reply) = reply {
            let value: OwnedValue = reply.body().deserialize().ok()?;
            return scheme(&value);
        }
    }
    None
}

/// The scheme in `value`, unwrapping variants the way `Read` nests them.
fn scheme(value: &Value<'_>) -> Option<ColorScheme> {
    let mut value = value;
    // Bounded: a portal never nests more than twice.
    for _ in 0..3 {
        match value {
            Value::U32(number) => return Some(ColorScheme::from_portal(*number)),
            Value::Value(inner) => value = inner,
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
