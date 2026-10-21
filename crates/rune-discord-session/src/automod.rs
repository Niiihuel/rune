//! A server's AutoMod on the personal session (automod, 2026-10-19). The
//! routes and bodies are the ones Discord documents and the official web
//! client's bundle shows (docs/DISCORD.md, «AutoMod»): `GET` and `POST
//! /guilds/{g}/auto-moderation/rules`, `PATCH` and `DELETE
//! /guilds/{g}/auto-moderation/rules/{id}`, and the Gateway's
//! `AUTO_MODERATION_RULE_*` and `AUTO_MODERATION_ACTION_EXECUTION`. Serein
//! has none of this. Fixtures only: none was tried against Discord.

use rune_i18n::es;
use rune_model::automod::{
    MAX_ALLOWED_PRESETS, MAX_EXEMPT_CHANNELS, MAX_EXEMPT_ROLES, MAX_KEYWORDS, MAX_NAME, MAX_NOTICE,
    MAX_PATTERNS, MAX_PATTERN_CHARS,
};
use rune_model::{
    permissions, AutoModAction, AutoModExecution, AutoModPreset, AutoModRule, AutoModRuleId,
    AutoModTrigger, ChannelId, Permissions, RoleId, ServerId, UserId, WordFilter,
};
use rune_provider::{Command, Event, Snapshot};
use serde_json::{json, Map, Value};

use crate::messages::wall_now;
use crate::profiles::text;
use crate::{wire, LoginError};

/// The most a list of rules may weigh: a few rules of a thousand words.
pub(crate) const MAX_RULES_BODY: usize = 2 * 1024 * 1024;
/// Rules of a server read, at most: far above what Discord allows.
const MAX_RULES_READ: usize = 64;
/// Characters of what matched, as an execution reports it, kept.
const MAX_MATCHED: usize = 100;

const NOT_ALLOWED: &str = es("No tenés permiso para administrar AutoMod en este servidor.");

/// Whether `command` is about AutoMod.
pub(crate) fn is_automod(command: &Command) -> bool {
    matches!(
        command,
        Command::LoadAutoModRules { .. }
            | Command::SaveAutoModRule { .. }
            | Command::SetAutoModRuleEnabled { .. }
            | Command::DeleteAutoModRule { .. }
    )
}

/// Whether `command` only reads: a lost answer is never ambiguous.
pub(crate) fn is_read(command: &Command) -> bool {
    matches!(command, Command::LoadAutoModRules { .. })
}

fn server_of(command: &Command) -> Option<ServerId> {
    match command {
        Command::LoadAutoModRules { server, .. }
        | Command::SaveAutoModRule { server, .. }
        | Command::SetAutoModRuleEnabled { server, .. }
        | Command::DeleteAutoModRule { server, .. } => Some(*server),
        _ => None,
    }
}

/// Whether the account may send `command`, by what the session knows:
/// `MANAGE_GUILD` in the server, and a rule Discord would take.
pub(crate) fn allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    let me = snapshot.me.as_ref().ok_or_else(wire::invalid)?.id;
    let server = server_of(command)
        .and_then(|id| snapshot.servers.iter().find(|known| known.id == id))
        .ok_or(LoginError::new(es("Ese servidor ya no está disponible.")))?;
    let owner = server.owner == Some(me);
    let mine = snapshot
        .members
        .iter()
        .find(|known| known.server == server.id && known.user == me);
    if mine.is_none() && !owner {
        return Err(LoginError::new(es(
            "Tu membresía en este servidor todavía no cargó.",
        )));
    }
    if mine.is_some_and(|mine| mine.restrictions.at(wall_now()).is_some()) {
        return Err(LoginError::new(es(
            "Tu cuenta está restringida en este servidor: podés leer, pero no administrar.",
        )));
    }
    let permissions = match mine {
        _ if owner => Permissions::ALL,
        Some(mine) => permissions::in_server(server, mine),
        None => Permissions::NONE,
    };
    if !permissions.contains(Permissions::MANAGE_SERVER) {
        return Err(LoginError::new(NOT_ALLOWED));
    }
    if let Command::SaveAutoModRule { rule, .. } = command {
        if rule.problem().is_some() {
            return Err(LoginError::new(es(
                "La regla no está completa: revisala antes de guardarla.",
            )));
        }
        for action in &rule.actions {
            match action {
                AutoModAction::Timeout { .. }
                    if !permissions.contains(Permissions::TIMEOUT_MEMBERS) =>
                {
                    return Err(LoginError::new(es(
                        "Aislar a alguien pide el permiso de aislar miembros.",
                    )));
                }
                AutoModAction::Alert { channel }
                    if !snapshot
                        .channels
                        .iter()
                        .any(|known| known.id == *channel && known.server == Some(server.id)) =>
                {
                    return Err(LoginError::new(es(
                        "El canal de la alerta ya no está en el servidor.",
                    )));
                }
                _ => {}
            }
        }
    }
    Ok(())
}

/// Up to `max` strings of the array `value`, each cut to `chars`.
fn strings(value: &Value, max: usize, chars: usize) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| text(item, chars))
                .take(max)
                .collect()
        })
        .unwrap_or_default()
}

/// The ids of the array `value`, up to `max`; one not an id is dropped.
fn ids(value: &Value, max: usize) -> Vec<u64> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| wire::id(item).ok())
                .take(max)
                .collect()
        })
        .unwrap_or_default()
}

fn filter(metadata: &Value) -> WordFilter {
    WordFilter {
        keywords: strings(&metadata["keyword_filter"], MAX_KEYWORDS, MAX_PATTERN_CHARS),
        patterns: strings(&metadata["regex_patterns"], MAX_PATTERNS, MAX_PATTERN_CHARS),
        allowed: strings(
            &metadata["allow_list"],
            MAX_ALLOWED_PRESETS,
            MAX_PATTERN_CHARS,
        ),
    }
}

/// Discord's number of a list of words.
fn preset_number(preset: AutoModPreset) -> u64 {
    match preset {
        AutoModPreset::Profanity => 1,
        AutoModPreset::Sexual => 2,
        AutoModPreset::Slurs => 3,
    }
}

fn trigger(kind: u64, metadata: &Value) -> AutoModTrigger {
    match kind {
        1 => AutoModTrigger::Keywords(filter(metadata)),
        3 => AutoModTrigger::Spam,
        4 => AutoModTrigger::Presets {
            lists: AutoModPreset::ALL
                .into_iter()
                .filter(|preset| {
                    metadata["presets"].as_array().is_some_and(|numbers| {
                        numbers
                            .iter()
                            .any(|number| number.as_u64() == Some(preset_number(*preset)))
                    })
                })
                .collect(),
            allowed: strings(
                &metadata["allow_list"],
                MAX_ALLOWED_PRESETS,
                MAX_PATTERN_CHARS,
            ),
        },
        5 => AutoModTrigger::Mentions {
            limit: metadata["mention_total_limit"]
                .as_u64()
                .and_then(|limit| u32::try_from(limit).ok())
                .unwrap_or(0),
            raids: metadata["mention_raid_protection_enabled"]
                .as_bool()
                .unwrap_or(false),
        },
        6 => AutoModTrigger::Profile(filter(metadata)),
        other => AutoModTrigger::Other(u32::try_from(other).unwrap_or(u32::MAX)),
    }
}

/// One action of a rule; one RUNE cannot read as Discord sent it is kept
/// as unknown, so the rule is never saved without it.
fn action(value: &Value) -> AutoModAction {
    let metadata = &value["metadata"];
    let kind = value["type"].as_u64().unwrap_or(0);
    let unknown = AutoModAction::Other(u32::try_from(kind).unwrap_or(u32::MAX));
    match kind {
        1 => AutoModAction::Block {
            notice: text(&metadata["custom_message"], MAX_NOTICE),
        },
        2 => wire::id(&metadata["channel_id"]).map_or(unknown, |channel| AutoModAction::Alert {
            channel: ChannelId(channel),
        }),
        3 => metadata["duration_seconds"]
            .as_u64()
            .and_then(|seconds| u32::try_from(seconds).ok())
            .map_or(unknown, |seconds| AutoModAction::Timeout { seconds }),
        4 => AutoModAction::Quarantine,
        _ => unknown,
    }
}

/// A rule as Discord sends it.
pub(crate) fn rule(value: &Value) -> Result<AutoModRule, LoginError> {
    let id = wire::id(&value["id"])?;
    let kind = value["trigger_type"].as_u64().ok_or_else(wire::invalid)?;
    Ok(AutoModRule {
        id: Some(AutoModRuleId(id)),
        name: text(&value["name"], MAX_NAME).unwrap_or_default(),
        trigger: trigger(kind, &value["trigger_metadata"]),
        actions: value["actions"]
            .as_array()
            .map(|actions| actions.iter().take(16).map(action).collect())
            .unwrap_or_default(),
        enabled: value["enabled"].as_bool().unwrap_or(false),
        exempt_roles: ids(&value["exempt_roles"], 2 * MAX_EXEMPT_ROLES)
            .into_iter()
            .map(RoleId)
            .collect(),
        exempt_channels: ids(&value["exempt_channels"], 2 * MAX_EXEMPT_CHANNELS)
            .into_iter()
            .map(ChannelId)
            .collect(),
        creator: wire::id(&value["creator_id"]).ok().map(UserId),
    })
}

/// Every rule of a server's list, as Discord sends it.
pub(crate) fn rules(value: &Value) -> Result<Vec<AutoModRule>, LoginError> {
    value
        .as_array()
        .ok_or_else(wire::invalid)?
        .iter()
        .take(MAX_RULES_READ)
        .map(rule)
        .collect()
}

/// What `rule` is sent as. A new one names its kind; a change never does,
/// as Discord documents the edit (a rule's kind does not change).
pub(crate) fn body(rule: &AutoModRule) -> Value {
    let words = |filter: &WordFilter| {
        json!({
            "keyword_filter": filter.keywords,
            "regex_patterns": filter.patterns,
            "allow_list": filter.allowed,
        })
    };
    let (kind, event, metadata) = match &rule.trigger {
        AutoModTrigger::Keywords(filter) => (1, 1, words(filter)),
        AutoModTrigger::Spam => (3, 1, json!({})),
        AutoModTrigger::Presets { lists, allowed } => (
            4,
            1,
            json!({
                "presets": lists.iter().map(|preset| preset_number(*preset)).collect::<Vec<_>>(),
                "allow_list": allowed,
            }),
        ),
        AutoModTrigger::Mentions { limit, raids } => (
            5,
            1,
            json!({
                "mention_total_limit": limit,
                "mention_raid_protection_enabled": raids,
            }),
        ),
        // Names and profiles change when a member joins or updates them.
        AutoModTrigger::Profile(filter) => (6, 2, words(filter)),
        AutoModTrigger::Other(kind) => (*kind, 1, json!({})),
    };
    let actions: Vec<Value> = rule
        .actions
        .iter()
        .filter_map(|action| {
            Some(match action {
                AutoModAction::Block { notice } => json!({
                    "type": 1,
                    "metadata": notice.as_ref().map_or_else(
                        || json!({}),
                        |notice| json!({ "custom_message": notice }),
                    ),
                }),
                AutoModAction::Alert { channel } => json!({
                    "type": 2,
                    "metadata": { "channel_id": channel.0.to_string() },
                }),
                AutoModAction::Timeout { seconds } => json!({
                    "type": 3,
                    "metadata": { "duration_seconds": seconds },
                }),
                AutoModAction::Quarantine => json!({ "type": 4, "metadata": {} }),
                // `AutoModRule::problem` keeps one with these from leaving.
                AutoModAction::Other(_) => return None,
            })
        })
        .collect();
    let mut body = Map::new();
    body.insert("name".into(), json!(rule.name.trim()));
    body.insert("event_type".into(), json!(event));
    if rule.id.is_none() {
        body.insert("trigger_type".into(), json!(kind));
    }
    body.insert("trigger_metadata".into(), metadata);
    body.insert("actions".into(), json!(actions));
    body.insert("enabled".into(), json!(rule.enabled));
    body.insert(
        "exempt_roles".into(),
        json!(rule
            .exempt_roles
            .iter()
            .map(|role| role.0.to_string())
            .collect::<Vec<_>>()),
    );
    body.insert(
        "exempt_channels".into(),
        json!(rule
            .exempt_channels
            .iter()
            .map(|channel| channel.0.to_string())
            .collect::<Vec<_>>()),
    );
    Value::Object(body)
}

/// What a Gateway event says of AutoMod in a server the session knows;
/// `None` for any other event. A malformed one is dropped, never a reason
/// to disconnect.
pub(crate) fn dispatch(snapshot: &Snapshot, kind: &str, data: &Value) -> Option<Vec<Event>> {
    if !kind.starts_with("AUTO_MODERATION_") {
        return None;
    }
    let known = wire::id(&data["guild_id"])
        .ok()
        .map(ServerId)
        .filter(|server| snapshot.servers.iter().any(|known| known.id == *server));
    let Some(server) = known else {
        return Some(Vec::new());
    };
    let event = match kind {
        "AUTO_MODERATION_RULE_CREATE" | "AUTO_MODERATION_RULE_UPDATE" => {
            rule(data).ok().map(|rule| Event::AutoModRuleChanged {
                server,
                rule,
                request: None,
            })
        }
        "AUTO_MODERATION_RULE_DELETE" => {
            wire::id(&data["id"])
                .ok()
                .map(|rule| Event::AutoModRuleRemoved {
                    server,
                    rule: AutoModRuleId(rule),
                })
        }
        "AUTO_MODERATION_ACTION_EXECUTION" => {
            let rule = wire::id(&data["rule_id"]).ok();
            let user = wire::id(&data["user_id"]).ok();
            rule.zip(user).map(|(rule, user)| {
                Event::AutoModExecuted(AutoModExecution {
                    server,
                    rule: AutoModRuleId(rule),
                    action: action(&data["action"]),
                    user: UserId(user),
                    channel: wire::id(&data["channel_id"]).ok().map(ChannelId),
                    matched: text(&data["matched_keyword"], MAX_MATCHED)
                        .or_else(|| text(&data["matched_content"], MAX_MATCHED)),
                })
            })
        }
        _ => None,
    };
    Some(event.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire_rule() -> Value {
        json!({
            "id": "969707018069872670",
            "guild_id": "613425648685547541",
            "name": "Palabras del taller",
            "creator_id": "423457898095789043",
            "event_type": 1,
            "trigger_type": 1,
            "trigger_metadata": {
                "keyword_filter": ["gato*", "*perro", "ñandú"],
                "regex_patterns": ["(b|c)at", "a b"],
                "allow_list": ["gatito"]
            },
            "actions": [
                { "type": 1, "metadata": { "custom_message": "Acá no." } },
                { "type": 2, "metadata": { "channel_id": "123456789123456789" } },
                { "type": 3, "metadata": { "duration_seconds": 60 } }
            ],
            "enabled": true,
            "exempt_roles": ["323456789123456789", "nope"],
            "exempt_channels": ["523456789123456789"]
        })
    }

    #[test]
    fn a_rule_reads_as_discord_documents_it_and_goes_back_the_same() {
        let read = rule(&wire_rule()).unwrap();
        assert_eq!(read.id, Some(AutoModRuleId(969_707_018_069_872_670)));
        assert_eq!(read.creator, Some(UserId(423_457_898_095_789_043)));
        let AutoModTrigger::Keywords(filter) = &read.trigger else {
            panic!("words: {:?}", read.trigger);
        };
        assert_eq!(filter.keywords, ["gato*", "*perro", "ñandú"]);
        assert_eq!(filter.patterns, ["(b|c)at", "a b"]);
        assert_eq!(
            read.actions[0],
            AutoModAction::Block {
                notice: Some("Acá no.".into())
            }
        );
        assert_eq!(read.actions[2], AutoModAction::Timeout { seconds: 60 });
        assert_eq!(read.exempt_roles, [RoleId(323_456_789_123_456_789)]);
        assert!(read.editable() && read.problem().is_none());
        // A change sends what Discord documents, never the kind.
        let sent = body(&read);
        assert!(sent.get("trigger_type").is_none());
        assert_eq!(sent["event_type"], 1);
        assert_eq!(sent["trigger_metadata"]["regex_patterns"][1], "a b");
        assert_eq!(
            sent["actions"][1]["metadata"]["channel_id"],
            "123456789123456789"
        );
        assert_eq!(sent["exempt_channels"][0], "523456789123456789");
        // A new one names it; a profile rule watches members, not messages.
        let fresh = AutoModRule {
            id: None,
            trigger: AutoModTrigger::Profile(WordFilter {
                keywords: vec!["admin".into()],
                ..WordFilter::default()
            }),
            actions: vec![AutoModAction::Quarantine],
            ..read
        };
        let sent = body(&fresh);
        assert_eq!(
            (sent["trigger_type"].clone(), sent["event_type"].clone()),
            (json!(6), json!(2))
        );
        assert_eq!(sent["actions"], json!([{ "type": 4, "metadata": {} }]));
    }

    #[test]
    fn what_rune_does_not_know_stays_unknown_and_is_never_edited() {
        let mut value = wire_rule();
        value["actions"] = json!([{ "type": 9 }, { "type": 2, "metadata": {} }]);
        let read = rule(&value).unwrap();
        assert_eq!(
            read.actions,
            [AutoModAction::Other(9), AutoModAction::Other(2)]
        );
        assert!(!read.editable());
        value["trigger_type"] = json!(8);
        assert_eq!(rule(&value).unwrap().trigger, AutoModTrigger::Other(8));
        // Lists and mentions.
        let presets = rule(&json!({
            "id": "1", "name": "Listas", "trigger_type": 4, "enabled": false,
            "trigger_metadata": { "presets": [3, 1, 7], "allow_list": [] },
            "actions": [{ "type": 1, "metadata": {} }]
        }))
        .unwrap();
        assert_eq!(
            presets.trigger,
            AutoModTrigger::Presets {
                lists: vec![AutoModPreset::Profanity, AutoModPreset::Slurs],
                allowed: Vec::new()
            }
        );
        assert_eq!(body(&presets)["trigger_metadata"]["presets"], json!([1, 3]));
        let mentions = rule(&json!({
            "id": "2", "name": "Menciones", "trigger_type": 5, "enabled": true,
            "trigger_metadata": { "mention_total_limit": 8, "mention_raid_protection_enabled": true },
            "actions": []
        }))
        .unwrap();
        assert_eq!(
            mentions.trigger,
            AutoModTrigger::Mentions {
                limit: 8,
                raids: true
            }
        );
        assert!(rule(&json!({ "name": "sin id", "trigger_type": 1 })).is_err());
        assert!(rules(&json!({})).is_err());
    }
}
