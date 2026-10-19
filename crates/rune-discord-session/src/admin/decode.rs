//! Bounded readings of Discord's administration answers into RUNE's types.
//! A webhook's `token` and `url` are a credential: never read, never kept.

use rune_i18n::t;
use std::collections::HashSet;

use rune_model::{
    Ban, ChannelId, Integration, IntegrationId, Invite, Member, ServerId, Timestamp, User, UserId,
    Webhook, WebhookId, WebhookKind,
};
use serde_json::{json, Value};

use crate::{bootstrap, wire, LoginError};

/// Rows one list answer may hold.
pub(crate) const MAX_ROWS: usize = 1000;
/// Members of one search page, as Serein asks for them.
pub(crate) const MEMBERS_PAGE: usize = 100;

fn rows(value: &Value, max: usize) -> Result<&[Value], LoginError> {
    value
        .as_array()
        .filter(|rows| rows.len() <= max)
        .map(Vec::as_slice)
        .ok_or_else(wire::invalid)
}

/// Text without control characters (a paragraph keeps its line breaks),
/// bounded; `null` is none.
fn text(value: &Value, max: usize, lines: bool) -> Result<Option<String>, LoginError> {
    match value {
        Value::Null => Ok(None),
        Value::String(text)
            if text.chars().count() <= max
                && !text
                    .chars()
                    .any(|c| c.is_control() && !(lines && c == '\n')) =>
        {
            Ok(Some(text.clone()))
        }
        _ => Err(wire::invalid()),
    }
}

fn optional_user(value: Option<&Value>) -> Result<Option<User>, LoginError> {
    value
        .filter(|value| !value.is_null())
        .map(wire::user)
        .transpose()
}

/// A count Discord may leave out: `None`, never zero.
fn count(value: &Value) -> Result<Option<u32>, LoginError> {
    match value {
        Value::Null => Ok(None),
        value => value
            .as_u64()
            .and_then(|count| u32::try_from(count).ok())
            .map(Some)
            .ok_or_else(wire::invalid),
    }
}

/// An invitation code fit for a path segment.
pub(crate) fn valid_code(code: &str) -> bool {
    (1..=64).contains(&code.len())
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

/// One invitation and whoever made it.
pub(crate) fn invite(value: &Value) -> Result<(Invite, Option<User>), LoginError> {
    let code = value["code"]
        .as_str()
        .filter(|code| valid_code(code))
        .ok_or_else(wire::invalid)?;
    let channel = match value["channel"].get("id").or(value.get("channel_id")) {
        Some(id) if !id.is_null() => Some(ChannelId(wire::id(id)?)),
        _ => None,
    };
    let inviter = optional_user(value.get("inviter"))?;
    let max_age = count(&value["max_age"])?.unwrap_or(0);
    let expires = match &value["expires_at"] {
        Value::Null if max_age > 0 => match &value["created_at"] {
            Value::Null => None,
            created => {
                let created = wire::timestamp(created)?;
                Some(Timestamp(created.0 + i64::from(max_age) * 1000))
            }
        },
        Value::Null => None,
        expires => Some(wire::timestamp(expires)?),
    };
    Ok((
        Invite {
            code: code.to_owned(),
            channel,
            inviter: inviter.as_ref().map(|user| user.id),
            uses: count(&value["uses"])?,
            max_uses: count(&value["max_uses"])?.filter(|uses| *uses > 0),
            expires,
            temporary: value["temporary"].as_bool().unwrap_or(false),
        },
        inviter,
    ))
}

/// One webhook of `server`, and who made it. Its token is never read.
pub(crate) fn webhook(
    value: &Value,
    server: ServerId,
) -> Result<(Webhook, Option<User>), LoginError> {
    let id = WebhookId(wire::id(&value["id"])?);
    if let Some(guild) = value.get("guild_id").filter(|guild| !guild.is_null()) {
        if wire::id(guild)? != server.0 {
            return Err(wire::invalid());
        }
    }
    let kind = match value["type"].as_u64() {
        Some(1) => WebhookKind::Incoming,
        Some(2) => WebhookKind::Follower,
        Some(3) => WebhookKind::Application,
        _ => return Err(wire::invalid()),
    };
    let creator = optional_user(value.get("user"))?;
    Ok((
        Webhook {
            id,
            channel: ChannelId(wire::id(&value["channel_id"])?),
            name: text(&value["name"], 80, false)?.unwrap_or_else(|| "Webhook".into()),
            kind,
            creator: creator.as_ref().map(|user| user.id),
            avatar: wire::image("avatars", id.0, &value["avatar"]),
        },
        creator,
    ))
}

/// One integration, with who added it and its application's bot.
pub(crate) fn integration(value: &Value) -> Result<(Integration, Vec<User>), LoginError> {
    let kind = value["type"]
        .as_str()
        .filter(|kind| {
            (1..=32).contains(&kind.len())
                && kind
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
        })
        .ok_or_else(wire::invalid)?;
    let service = match kind {
        "discord" => t("Aplicación"),
        "twitch" => "Twitch",
        "youtube" => "YouTube",
        "guild_subscription" => t("Suscripciones del servidor"),
        other => other,
    };
    let user = optional_user(value.get("user"))?;
    let bot = optional_user(value["application"].get("bot"))?;
    Ok((
        Integration {
            id: IntegrationId(wire::id(&value["id"])?),
            name: text(&value["name"], 100, false)?.ok_or_else(wire::invalid)?,
            service: service.to_owned(),
            enabled: value["enabled"].as_bool().unwrap_or(true),
            user: user.as_ref().map(|user| user.id),
            bot: bot.as_ref().map(|bot| bot.id),
        },
        user.into_iter().chain(bot).collect(),
    ))
}

/// A page of bans, by ascending user, all after `after` and at most `limit`.
pub(crate) fn bans(
    value: &Value,
    after: Option<UserId>,
    limit: usize,
) -> Result<(Vec<Ban>, Vec<User>), LoginError> {
    let mut bans = Vec::new();
    let mut users = Vec::new();
    for row in rows(value, limit)? {
        let user = wire::user(&row["user"])?;
        bans.push(Ban {
            user: user.id,
            reason: text(&row["reason"], 512, true)?,
        });
        users.push(user);
    }
    bans.sort_by_key(|ban| ban.user);
    let ordered = bans.windows(2).all(|pair| pair[0].user < pair[1].user)
        && bans
            .first()
            .is_none_or(|first| after.is_none_or(|after| first.user > after));
    if !ordered {
        return Err(wire::invalid());
    }
    Ok((bans, users))
}

/// A page of the member search, as the provider reports and keeps it.
pub(crate) struct MemberPage {
    pub members: Vec<Member>,
    pub users: Vec<User>,
    /// Each member as `GUILD_MEMBER_UPDATE` carries it, for the session.
    pub updates: Vec<Value>,
    pub next: Option<String>,
    pub total: Option<u32>,
    // Server administration, rest (P7).
    /// How each of them joined, when the search says.
    pub joins: Vec<rune_model::JoinedBy>,
}

/// The answer to `POST /guilds/{g}/members-search`, as Serein checks it:
/// of this server, its count matching, nobody twice.
pub(crate) fn member_page(value: &Value, server: ServerId) -> Result<MemberPage, LoginError> {
    if wire::id(&value["guild_id"])? != server.0 {
        return Err(wire::invalid());
    }
    let rows = rows(&value["members"], MEMBERS_PAGE)?;
    if value["page_result_count"].as_u64() != Some(rows.len() as u64) {
        return Err(wire::invalid());
    }
    let mut page = MemberPage {
        members: Vec::new(),
        users: Vec::new(),
        updates: Vec::new(),
        next: None,
        total: None,
        joins: Vec::new(),
    };
    let mut seen = HashSet::new();
    for row in rows {
        let mut data = row["member"].clone();
        if !data.is_object() {
            return Err(wire::invalid());
        }
        data["guild_id"] = json!(server.to_string());
        let member = bootstrap::list_member(&data, server)?;
        let user = wire::user(&data["user"])?;
        if user.id != member.user || !seen.insert(member.user) {
            return Err(wire::invalid());
        }
        page.users.push(user);
        page.joins
            .extend(crate::admin::rest::join_method(row).map(|method| (member.user, method)));
        page.members.push(member);
        page.updates.push(data);
    }
    let total = value["total_result_count"].as_u64();
    if total.is_some_and(|total| total < rows.len() as u64) {
        return Err(wire::invalid());
    }
    page.total = total.map(|total| u32::try_from(total).unwrap_or(u32::MAX));
    // Discord's search goes on from the last one's join date; a member
    // without one ends the paging, as in Serein.
    page.next = (rows.len() == MEMBERS_PAGE)
        .then(|| page.members.last())
        .flatten()
        .and_then(|last| Some(format!("{}:{}", last.user, last.joined?.0)));
    Ok(page)
}

/// The member and join date (milliseconds) a page cursor names.
pub(crate) fn parse_cursor(cursor: &str) -> Option<(UserId, i64)> {
    let (user, joined) = cursor.split_once(':')?;
    let user = user.parse::<u64>().ok().filter(|user| *user != 0)?;
    let joined = joined.parse::<i64>().ok().filter(|joined| *joined >= 0)?;
    (cursor.len() <= 48 && cursor == format!("{user}:{joined}")).then_some((UserId(user), joined))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invites_keep_unknowns_unknown_and_reach_their_expiry() {
        let (found, inviter) = invite(&json!({"code":"abc-123","channel":{"id":"30"},
            "inviter":{"id":"42","username":"Owner"},"uses":3,"max_uses":0,"max_age":3600,
            "created_at":"2026-10-17T00:00:00Z","expires_at":null,"temporary":true}))
        .unwrap();
        assert_eq!(found.channel, Some(ChannelId(30)));
        assert_eq!(inviter.unwrap().id, UserId(42));
        assert_eq!((found.uses, found.max_uses), (Some(3), None));
        assert!(found.temporary);
        let created = wire::timestamp(&json!("2026-10-17T00:00:00Z")).unwrap();
        assert_eq!(found.expires, Some(Timestamp(created.0 + 3_600_000)));
        let (bare, _) = invite(&json!({"code":"x"})).unwrap();
        assert_eq!((bare.uses, bare.expires, bare.channel), (None, None, None));
        for code in ["", "../x", "a b"] {
            assert!(invite(&json!({ "code": code })).is_err());
        }
    }

    #[test]
    fn a_webhook_never_keeps_its_token() {
        let (hook, creator) = webhook(
            &json!({"id":"5","guild_id":"10","channel_id":"30","type":1,"name":"CI",
                "token":"SYNTHETIC_WEBHOOK_TOKEN","url":"https://discord.com/api/webhooks/5/SYNTHETIC_WEBHOOK_TOKEN",
                "user":{"id":"42","username":"Owner"}}),
            ServerId(10),
        )
        .unwrap();
        assert_eq!(hook.kind, WebhookKind::Incoming);
        assert_eq!(hook.creator, Some(UserId(42)));
        assert!(creator.is_some());
        assert!(!format!("{hook:?}").contains("SYNTHETIC_WEBHOOK_TOKEN"));
        // Another server's, or of an unknown kind: refused.
        assert!(webhook(
            &json!({"id":"5","guild_id":"11","channel_id":"30","type":1}),
            ServerId(10)
        )
        .is_err());
        assert!(webhook(&json!({"id":"5","channel_id":"30","type":9}), ServerId(10)).is_err());
    }

    #[test]
    fn integrations_name_their_service_and_people() {
        let (found, users) = integration(&json!({"id":"7","name":"Bot","type":"discord",
            "enabled":true,"user":{"id":"42","username":"Owner"},
            "application":{"id":"8","bot":{"id":"44","username":"bot","bot":true}}}))
        .unwrap();
        assert_eq!(found.service, "Aplicación");
        assert_eq!(
            (found.user, found.bot),
            (Some(UserId(42)), Some(UserId(44)))
        );
        assert_eq!(users.len(), 2);
        assert!(integration(&json!({"id":"7","name":"x","type":"../y"})).is_err());
    }

    #[test]
    fn bans_come_ascending_and_after_the_cursor() {
        let row = |id: &str| json!({"user":{"id":id,"username":"x"},"reason":null});
        let (bans, users) = bans(&json!([row("9"), row("7")]), Some(UserId(5)), 10).unwrap();
        assert_eq!(
            bans.iter().map(|ban| ban.user.0).collect::<Vec<_>>(),
            [7, 9]
        );
        assert_eq!(users.len(), 2);
        assert!(super::bans(&json!([row("4")]), Some(UserId(5)), 10).is_err());
        assert!(super::bans(&json!([row("7"), row("7")]), None, 10).is_err());
        assert!(super::bans(&json!([row("7"), row("8")]), None, 1).is_err());
    }

    #[test]
    fn a_member_page_names_the_next_from_its_last_join() {
        let member = |id: u64| {
            json!({"member":{"user":{"id":id.to_string(),"username":format!("u{id}")},
                "roles":[],"joined_at":"2026-10-17T00:00:00Z","communication_disabled_until":null}})
        };
        let full: Vec<Value> = (1..=MEMBERS_PAGE as u64).map(member).collect();
        let page = member_page(
            &json!({"guild_id":"10","members":full,"page_result_count":MEMBERS_PAGE,"total_result_count":250}),
            ServerId(10),
        )
        .unwrap();
        let joined = wire::timestamp(&json!("2026-10-17T00:00:00Z")).unwrap().0;
        assert_eq!(page.next, Some(format!("{}:{joined}", MEMBERS_PAGE)));
        assert_eq!(page.total, Some(250));
        assert_eq!(page.updates[0]["guild_id"], "10");
        assert_eq!(
            parse_cursor(page.next.as_deref().unwrap()),
            Some((UserId(100), joined))
        );
        let last = member_page(
            &json!({"guild_id":"10","members":[member(1)],"page_result_count":1,"total_result_count":1}),
            ServerId(10),
        )
        .unwrap();
        assert_eq!(last.next, None);
        // Another server's page, a wrong count or someone twice: refused.
        for bad in [
            json!({"guild_id":"11","members":[member(1)],"page_result_count":1}),
            json!({"guild_id":"10","members":[member(1)],"page_result_count":2}),
            json!({"guild_id":"10","members":[member(1), member(1)],"page_result_count":2}),
        ] {
            assert!(member_page(&bad, ServerId(10)).is_err());
        }
        for cursor in ["", "0:5", "1:-1", "x:1", "1:01", "1"] {
            assert_eq!(parse_cursor(cursor), None, "{cursor}");
        }
    }
}
