//! What the account may attach, and the checks a file send passes before
//! anything leaves: the transport is in `http::uploads`.
//!
//! The account's plan sets its limit as Serein reads it (`premium_type`:
//! 500 MiB with Nitro, 50 MiB with Classic or Basic, 20 MiB otherwise); a
//! server's boosts raise it for its conversations (`premium_tier` 2: 50 MiB,
//! 3: 100 MiB), as the official client shows. Discord still decides: a file
//! it refuses is refused before any byte is sent. Fixtures only.

use std::collections::HashMap;

use rune_model::{Permissions, ServerId};
use rune_provider::{Command, Event, Snapshot, UploadError, UploadLimits};
use serde_json::Value;

use crate::{gateway::GatewayEvent, messages::Messages, wire};

const MIB: u64 = 1024 * 1024;
/// Discord's attachments per message.
const MAX_FILES: usize = 10;
/// The most any account may attach: where Core and the interface start.
pub(crate) const CEILING: UploadLimits = UploadLimits {
    max_files: MAX_FILES,
    max_file_bytes: 500 * MIB,
    max_message_bytes: 500 * MIB,
};

/// The account's plan and each server's boost tier, as READY and the guild
/// dispatches tell them.
#[derive(Default)]
pub(crate) struct Limits {
    premium: u64,
    tiers: HashMap<ServerId, u64>,
}

impl Limits {
    fn account(&self) -> u64 {
        match self.premium {
            2 => 500 * MIB,
            1 | 3 => 50 * MIB,
            _ => 20 * MIB,
        }
    }

    fn boosted(tier: u64) -> u64 {
        match tier {
            3 => 100 * MIB,
            2 => 50 * MIB,
            _ => 0,
        }
    }

    /// The largest file the account may attach in `server`, or outside servers.
    pub fn file_bytes(&self, server: Option<ServerId>) -> u64 {
        let boost = server
            .and_then(|server| self.tiers.get(&server))
            .map_or(0, |tier| Self::boosted(*tier));
        self.account().max(boost)
    }

    /// Read what `event` says of the plan or a server's boosts; the limits
    /// the interface must learn, after the snapshot that carries them.
    pub fn observe(&mut self, event: &GatewayEvent) -> Vec<Event> {
        let limit = |limits: &Self, server: ServerId| Event::UploadLimit {
            server: Some(server),
            max_file_bytes: limits.file_bytes(Some(server)),
        };
        match event {
            GatewayEvent::Ready(data) => {
                self.premium = data["user"]["premium_type"].as_u64().unwrap_or(0);
                self.tiers = data["guilds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(tier)
                    .collect();
                let mut events = vec![Event::UploadLimit {
                    server: None,
                    max_file_bytes: self.account(),
                }];
                let mut boosted: Vec<_> = self
                    .tiers
                    .iter()
                    .filter(|(_, tier)| Self::boosted(**tier) > self.account())
                    .map(|(server, _)| *server)
                    .collect();
                boosted.sort_unstable();
                events.extend(boosted.into_iter().map(|server| limit(self, server)));
                events
            }
            GatewayEvent::Dispatch { kind, data }
                if matches!(kind.as_str(), "GUILD_CREATE" | "GUILD_UPDATE") =>
            {
                let Some((server, tier)) = self::tier(data) else {
                    return Vec::new();
                };
                let before = self.file_bytes(Some(server));
                self.tiers.insert(server, tier);
                if before == self.file_bytes(Some(server)) {
                    Vec::new()
                } else {
                    vec![limit(self, server)]
                }
            }
            GatewayEvent::Dispatch { kind, data } if kind.as_str() == "GUILD_DELETE" => {
                if let Ok(server) = wire::id(&data["id"]) {
                    self.tiers.remove(&ServerId(server));
                }
                Vec::new()
            }
            _ => Vec::new(),
        }
    }
}

/// A guild's id and boost tier, where READY's guilds keep it in `properties`.
fn tier(data: &Value) -> Option<(ServerId, u64)> {
    let id = ServerId(wire::id(&data["id"]).ok()?);
    let tier = data
        .get("properties")
        .and_then(|properties| properties.get("premium_tier"))
        .unwrap_or(&data["premium_tier"])
        .as_u64()?;
    Some((id, tier))
}

/// Whether `command`, a `SendFiles`, may start now in this session.
pub(crate) fn admit(
    snapshot: &Snapshot,
    messages: &Messages,
    limits: &Limits,
    generation: u64,
    command: &Command,
) -> Result<(), UploadError> {
    let Command::SendFiles {
        scope,
        reply_to,
        files,
        fence,
        ..
    } = command
    else {
        return Err(UploadError::InvalidFile);
    };
    if fence.is_cancelled() {
        return Err(UploadError::Cancelled);
    }
    if std::time::Instant::now() >= scope.expires_at {
        return Err(UploadError::Expired);
    }
    if scope.request == 0
        || scope.nonce == 0
        || scope.generation != generation
        || snapshot.me.as_ref().map(|me| me.id) != Some(scope.account)
    {
        return Err(UploadError::Denied);
    }
    if files.is_empty() {
        return Err(UploadError::InvalidFile);
    }
    if files.len() > MAX_FILES {
        return Err(UploadError::TooMany);
    }
    if files.iter().enumerate().any(|(index, file)| {
        files[..index]
            .iter()
            .any(|other| other.metadata().token == file.metadata().token)
    }) {
        return Err(UploadError::InvalidFile);
    }
    messages
        .allowed(snapshot, command)
        .map_err(|_| UploadError::Denied)?;
    let channel = snapshot
        .channels
        .iter()
        .find(|channel| channel.id == scope.channel)
        .ok_or(UploadError::Denied)?;
    if reply_to.is_some() && !channel.permissions.contains(Permissions::READ_HISTORY) {
        return Err(UploadError::Denied);
    }
    let largest = limits.file_bytes(channel.server);
    if files
        .iter()
        .any(|file| file.metadata().size_bytes > largest)
    {
        return Err(UploadError::TooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::DispatchKind;
    use serde_json::json;

    #[test]
    fn the_plan_and_the_server_boosts_set_the_largest_file() {
        let mut limits = Limits::default();
        let ready = GatewayEvent::Ready(json!({
            "user": {"id": "42", "premium_type": 0},
            "guilds": [
                {"id": "1", "properties": {"premium_tier": 3}},
                {"id": "2", "premium_tier": 2},
                {"id": "3", "properties": {"premium_tier": 1}},
            ],
        }));
        assert_eq!(
            limits.observe(&ready),
            vec![
                Event::UploadLimit {
                    server: None,
                    max_file_bytes: 20 * MIB
                },
                Event::UploadLimit {
                    server: Some(ServerId(1)),
                    max_file_bytes: 100 * MIB
                },
                Event::UploadLimit {
                    server: Some(ServerId(2)),
                    max_file_bytes: 50 * MIB
                },
            ]
        );
        assert_eq!(limits.file_bytes(Some(ServerId(3))), 20 * MIB);
        assert_eq!(limits.file_bytes(None), 20 * MIB);
        // A boost lost later lowers it back to the account's.
        let update = GatewayEvent::Dispatch {
            kind: DispatchKind::GuildUpdate,
            data: json!({"id": "1", "premium_tier": 0}),
        };
        assert_eq!(
            limits.observe(&update),
            vec![Event::UploadLimit {
                server: Some(ServerId(1)),
                max_file_bytes: 20 * MIB
            }]
        );
        // Nitro outranks every boost.
        let nitro = GatewayEvent::Ready(json!({
            "user": {"id": "42", "premium_type": 2},
            "guilds": [{"id": "1", "premium_tier": 3}],
        }));
        assert_eq!(
            limits.observe(&nitro),
            vec![Event::UploadLimit {
                server: None,
                max_file_bytes: 500 * MIB
            }]
        );
        assert_eq!(limits.file_bytes(Some(ServerId(1))), 500 * MIB);
    }
}
