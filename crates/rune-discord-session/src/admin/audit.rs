//! The audit log as Discord documents it (`GET /guilds/{g}/audit-logs`,
//! read the way Serein's `server_audit_log.rs` reads it), told in RUNE's
//! words. Only the keys an action is described by are read: a change's
//! other values (a webhook's token, a secret) never leave this module.

use rune_i18n::{es, t, tr};
use rune_model::{
    AuditAction, AuditActionKind, AuditEntry, ChannelId, RoleId, ServerId, User, UserId,
};
use rune_provider::Snapshot;
use serde_json::Value;

use crate::{wire, LoginError};

/// Changes one entry may list, at most.
const MAX_CHANGES: usize = 256;
/// Users and entries one answer may carry, at most.
const MAX_USERS: usize = 1000;
/// What a name reads when neither the entry nor the session knows it.
const UNNAMED: &str = es("(sin nombre)");

/// The `action_type`s Discord filters by for `kind`; none for one it has
/// not. The route takes one at a time: a kind of several (an overwrite
/// made, changed or removed: 13, 14 and 15) is asked for once per type and
/// the pages merged. Its type 24 holds both a nickname and a timeout, so a
/// page filtered by either is filtered again here.
pub(crate) fn action_types(kind: AuditActionKind) -> &'static [u64] {
    use AuditActionKind as Kind;
    match kind {
        Kind::ServerUpdated => &[1],
        Kind::ChannelCreated => &[10],
        Kind::ChannelUpdated => &[11],
        Kind::ChannelDeleted => &[12],
        Kind::PermissionsChanged => &[13, 14, 15],
        Kind::MemberKicked => &[20],
        Kind::MemberBanned => &[22],
        Kind::MemberUnbanned => &[23],
        Kind::NickChanged | Kind::MemberTimedOut => &[24],
        Kind::MemberRolesChanged => &[25],
        Kind::RoleCreated => &[30],
        Kind::RoleUpdated => &[31],
        Kind::RoleDeleted => &[32],
        Kind::InviteCreated => &[40],
        Kind::InviteDeleted => &[42],
        Kind::EmojiCreated => &[60],
        Kind::EmojiUpdated => &[61],
        Kind::EmojiDeleted => &[62],
        Kind::StickerCreated => &[90],
        Kind::StickerUpdated => &[91],
        Kind::StickerDeleted => &[92],
        Kind::Other => &[],
    }
}

/// Pages of one stretch of the log, one per type asked for, merged newest
/// first: up to `limit` entries, and where the next stretch starts when
/// there may be more. Past the oldest entry of a full page, that type may
/// have more not read yet, so nothing older than it is kept now.
pub(crate) fn merge(pages: Vec<Page>, limit: usize) -> (Vec<AuditEntry>, Option<u64>) {
    let mut stop = pages
        .iter()
        .filter(|page| page.count >= limit)
        .filter_map(|page| page.oldest)
        .max();
    let mut entries: Vec<AuditEntry> = pages.into_iter().flat_map(|page| page.entries).collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.id));
    entries.dedup_by_key(|entry| entry.id);
    if let Some(stop) = stop {
        entries.retain(|entry| entry.id >= stop);
    }
    if entries.len() > limit {
        entries.truncate(limit);
        stop = entries.last().map(|entry| entry.id);
    }
    (entries, stop)
}

/// One answer of the audit log.
pub(crate) struct Page {
    pub entries: Vec<AuditEntry>,
    pub users: Vec<User>,
    /// Entries Discord sent, described or not; a full page may have more.
    pub count: usize,
    /// The oldest of them, where the next page starts.
    pub oldest: Option<u64>,
}

/// Up to `limit` entries, newest first and older than `before`; anything
/// out of that order makes the whole page unreadable.
pub(crate) fn page(value: &Value, before: Option<u64>, limit: usize) -> Result<Page, LoginError> {
    let rows = value["audit_log_entries"]
        .as_array()
        .filter(|rows| rows.len() <= limit)
        .ok_or_else(wire::invalid)?;
    let users = match &value["users"] {
        Value::Null => Vec::new(),
        users => users
            .as_array()
            .filter(|users| users.len() <= MAX_USERS)
            .ok_or_else(wire::invalid)?
            .iter()
            // A name that cannot be shown costs the name, not the page.
            .filter_map(|user| wire::user(user).ok())
            .collect(),
    };
    let mut entries = Vec::new();
    let mut last = before;
    for row in rows {
        let id = wire::id(&row["id"])?;
        if last.is_some_and(|last| id >= last) {
            return Err(wire::invalid());
        }
        last = Some(id);
        entries.extend(entry(row, id)?);
    }
    Ok(Page {
        entries,
        users,
        count: rows.len(),
        oldest: last.filter(|_| !rows.is_empty()),
    })
}

/// One entry; `None` for one nobody made (Discord's own).
fn entry(row: &Value, id: u64) -> Result<Option<AuditEntry>, LoginError> {
    let Some(actor) = optional(&row["user_id"])? else {
        return Ok(None);
    };
    let changes = match &row["changes"] {
        Value::Null => &[][..],
        changes => changes
            .as_array()
            .filter(|changes| changes.len() <= MAX_CHANGES)
            .ok_or_else(wire::invalid)?,
    };
    let reason = match &row["reason"] {
        Value::Null => None,
        reason => Some(
            reason
                .as_str()
                .filter(|reason| reason.chars().count() <= 512)
                .ok_or_else(wire::invalid)?
                .replace(|c: char| c.is_control(), " "),
        ),
    };
    Ok(Some(AuditEntry {
        id,
        actor: UserId(actor),
        action: action(
            row["action_type"].as_u64().ok_or_else(wire::invalid)?,
            optional(&row["target_id"])?,
            changes,
            &row["options"],
        ),
        // Every id carries when it was made.
        when: wire::created(UserId(id)).unwrap_or_default(),
        reason,
    }))
}

fn optional(value: &Value) -> Result<Option<u64>, LoginError> {
    if value.is_null() {
        Ok(None)
    } else {
        wire::id(value).map(Some)
    }
}

/// What entry `kind` records, from its target and the changes it lists.
fn action(kind: u64, target: Option<u64>, changes: &[Value], options: &Value) -> AuditAction {
    let change = |key: &str| changes.iter().find(|change| change["key"] == key);
    // A name as it was set (`new`) or as it was (`old`), bounded.
    let named = |key: &str, side: &str| {
        change(key)
            .and_then(|change| change[side].as_str())
            .filter(|name| name.chars().count() <= 200 && !name.chars().any(char::is_control))
            .map(str::to_owned)
    };
    let name = |side: &str| named("name", side);
    let either = || {
        name("new_value")
            .or_else(|| name("old_value"))
            .unwrap_or_default()
    };
    let roles = |key: &str| -> Vec<RoleId> {
        change(key)
            .and_then(|change| change["new_value"].as_array())
            .into_iter()
            .flatten()
            .filter_map(|role| wire::id(&role["id"]).ok().map(RoleId))
            .take(MAX_CHANGES)
            .collect()
    };
    let other = |what: &str| AuditAction::Other { what: what.into() };
    let (user, channel, role) = (
        target.map(UserId),
        target.map(ChannelId),
        target.map(RoleId),
    );
    match (kind, user, channel, role) {
        (1, ..) => AuditAction::ServerUpdated,
        (10, _, Some(channel), _) => AuditAction::ChannelCreated {
            channel,
            name: name("new_value").unwrap_or_default(),
        },
        (11, _, Some(channel), _) => AuditAction::ChannelUpdated {
            channel,
            name: either(),
        },
        (12, ..) => AuditAction::ChannelDeleted {
            name: name("old_value").unwrap_or_default(),
        },
        // The channel is the entry's target; the overwrite's is in its options.
        (13..=15, _, Some(channel), _) => AuditAction::PermissionsChanged {
            channel,
            name: String::new(),
        },
        (20, Some(user), ..) => AuditAction::MemberKicked { user },
        (21, ..) => match options["members_removed"].as_str() {
            Some(count) if count.len() <= 10 && count.bytes().all(|b| b.is_ascii_digit()) => {
                other(&tr!("podó {count} miembros inactivos", count))
            }
            _ => other(t("podó miembros inactivos")),
        },
        (22, Some(user), ..) => AuditAction::MemberBanned { user },
        (23, Some(user), ..) => AuditAction::MemberUnbanned { user },
        (24, Some(user), ..) => {
            if let Some(change) = change("communication_disabled_until") {
                AuditAction::MemberTimedOut {
                    user,
                    until: wire::timestamp(&change["new_value"]).ok(),
                }
            } else if change("nick").is_some() {
                AuditAction::NickChanged {
                    user,
                    nick: named("nick", "new_value"),
                }
            } else {
                other(t("actualizó a un miembro"))
            }
        }
        (25, Some(user), ..) => AuditAction::MemberRolesChanged {
            user,
            added: roles("$add"),
            removed: roles("$remove"),
        },
        (26, ..) => other(t("movió a alguien de canal de voz")),
        (27, ..) => other(t("desconectó a alguien de un canal de voz")),
        (28, ..) => other(t("agregó un bot")),
        (30, _, _, Some(role)) => AuditAction::RoleCreated {
            role,
            name: name("new_value").unwrap_or_default(),
        },
        (31, _, _, Some(role)) => AuditAction::RoleUpdated {
            role,
            name: either(),
        },
        (32, ..) => AuditAction::RoleDeleted {
            name: name("old_value").unwrap_or_default(),
        },
        (40, ..) => AuditAction::InviteCreated {
            code: named("code", "new_value").unwrap_or_default(),
        },
        (41, ..) => other(t("actualizó una invitación")),
        (42, ..) => AuditAction::InviteDeleted {
            code: named("code", "old_value").unwrap_or_default(),
        },
        (50, ..) => other(t("creó un webhook")),
        (51, ..) => other(t("actualizó un webhook")),
        (52, ..) => other(t("eliminó un webhook")),
        (60, ..) => AuditAction::EmojiCreated {
            name: name("new_value").unwrap_or_default(),
        },
        (61, ..) => AuditAction::EmojiUpdated { name: either() },
        (62, ..) => AuditAction::EmojiDeleted {
            name: name("old_value").unwrap_or_default(),
        },
        (72, ..) => other(t("eliminó un mensaje")),
        (73, ..) => other(t("eliminó mensajes en bloque")),
        (74, ..) => other(t("fijó un mensaje")),
        (75, ..) => other(t("dejó de fijar un mensaje")),
        (80, ..) => other(t("agregó una integración")),
        (81, ..) => other(t("actualizó una integración")),
        (82, ..) => other(t("quitó una integración")),
        (83, ..) => other(t("abrió un escenario")),
        (84, ..) => other(t("actualizó un escenario")),
        (85, ..) => other(t("cerró un escenario")),
        (90, ..) => AuditAction::StickerCreated {
            name: name("new_value").unwrap_or_default(),
        },
        (91, ..) => AuditAction::StickerUpdated { name: either() },
        (92, ..) => AuditAction::StickerDeleted {
            name: name("old_value").unwrap_or_default(),
        },
        (100, ..) => other(t("creó un evento")),
        (101, ..) => other(t("actualizó un evento")),
        (102, ..) => other(t("eliminó un evento")),
        (110, ..) => other(t("creó un hilo")),
        (111, ..) => other(t("actualizó un hilo")),
        (112, ..) => other(t("eliminó un hilo")),
        (121, ..) => other(t("actualizó los permisos de los comandos")),
        (140, ..) => other(t("creó una regla de AutoMod")),
        (141, ..) => other(t("actualizó una regla de AutoMod")),
        (142, ..) => other(t("eliminó una regla de AutoMod")),
        (143..=145, ..) => other(t("activó una regla de AutoMod")),
        _ => other(t("hizo un cambio en el servidor")),
    }
}

/// Fill the names an entry left out with the ones the session knows now.
pub(crate) fn names(snapshot: &Snapshot, server: ServerId, entries: &mut [AuditEntry]) {
    let channel = |id: ChannelId| {
        snapshot
            .channels
            .iter()
            .find(|known| known.id == id && known.server == Some(server))
            .map(|known| known.name.clone())
    };
    let role = |id: RoleId| {
        snapshot
            .servers
            .iter()
            .find(|known| known.id == server)
            .and_then(|known| known.roles.iter().find(|role| role.id == id))
            .map(|role| role.name.clone())
    };
    for entry in entries {
        let (name, known) = match &mut entry.action {
            AuditAction::ChannelCreated { channel: id, name }
            | AuditAction::ChannelUpdated { channel: id, name }
            | AuditAction::PermissionsChanged { channel: id, name } => (name, channel(*id)),
            AuditAction::RoleCreated { role: id, name }
            | AuditAction::RoleUpdated { role: id, name } => (name, role(*id)),
            AuditAction::ChannelDeleted { name }
            | AuditAction::RoleDeleted { name }
            | AuditAction::EmojiCreated { name }
            | AuditAction::EmojiUpdated { name }
            | AuditAction::EmojiDeleted { name }
            | AuditAction::StickerCreated { name }
            | AuditAction::StickerUpdated { name }
            | AuditAction::StickerDeleted { name } => (name, None),
            _ => continue,
        };
        if name.is_empty() {
            *name = known.unwrap_or_else(|| t(UNNAMED).into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rune_model::AuditQuery;
    use serde_json::json;

    fn row(id: u64, kind: u64, target: &str, changes: Value) -> Value {
        json!({"id": id.to_string(), "user_id": "42", "target_id": target,
            "action_type": kind, "changes": changes, "reason": null})
    }

    #[test]
    fn entries_say_what_changed_and_never_carry_other_values() {
        let value = json!({"users":[{"id":"42","username":"Owner"}], "audit_log_entries":[
            row(90, 24, "43", json!([{"key":"communication_disabled_until","new_value":"2026-10-20T00:00:00Z"}])),
            row(80, 24, "43", json!([{"key":"nick","old_value":"a","new_value":"b"}])),
            row(70, 25, "43", json!([{"key":"$add","new_value":[{"id":"11","name":"Mod"}]}])),
            row(60, 50, "5", json!([{"key":"token","new_value":"SYNTHETIC_WEBHOOK_TOKEN"}])),
            row(50, 32, "11", json!([{"key":"name","old_value":"Mod"}])),
            row(40, 999, "1", Value::Null),
            {"id":"30","user_id":null,"action_type":1},
        ]});
        let page = page(&value, Some(100), 50).unwrap();
        assert_eq!(
            (page.count, page.oldest, page.users.len()),
            (7, Some(30), 1)
        );
        assert_eq!(page.entries.len(), 6, "an entry nobody made is skipped");
        assert!(matches!(
            page.entries[0].action,
            AuditAction::MemberTimedOut { until: Some(_), .. }
        ));
        assert_eq!(
            page.entries[1].action,
            AuditAction::NickChanged {
                user: UserId(43),
                nick: Some("b".into())
            }
        );
        assert_eq!(
            page.entries[2].action,
            AuditAction::MemberRolesChanged {
                user: UserId(43),
                added: vec![RoleId(11)],
                removed: vec![]
            }
        );
        assert_eq!(
            page.entries[3].action,
            AuditAction::Other {
                what: "creó un webhook".into()
            }
        );
        assert_eq!(
            page.entries[4].action,
            AuditAction::RoleDeleted { name: "Mod".into() }
        );
        assert!(!format!("{:?}", page.entries).contains("SYNTHETIC_WEBHOOK_TOKEN"));
        // One kind of Discord's (24) is two of RUNE's: a filter keeps one.
        let nick = AuditQuery {
            actor: None,
            action: Some(AuditActionKind::NickChanged),
        };
        assert_eq!(
            page.entries
                .iter()
                .filter(|entry| nick.matches(entry))
                .count(),
            1
        );
        assert_eq!(action_types(AuditActionKind::MemberTimedOut), [24]);
        assert!(action_types(AuditActionKind::Other).is_empty());
    }

    #[test]
    fn a_page_out_of_order_or_past_its_cursor_is_unreadable() {
        let page_of = |ids: &[u64]| json!({"audit_log_entries": ids.iter().map(|id| row(*id, 1, "10", Value::Null)).collect::<Vec<_>>()});
        assert!(page(&page_of(&[5, 6]), None, 50).is_err());
        assert!(page(&page_of(&[5, 5]), None, 50).is_err());
        assert!(page(&page_of(&[100]), Some(100), 50).is_err());
        assert!(page(&page_of(&[3, 2, 1]), None, 2).is_err());
        let empty = page(&page_of(&[]), Some(9), 50).unwrap();
        assert_eq!((empty.count, empty.oldest), (0, None));
    }
}
