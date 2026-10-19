//! Translate account navigation into provider-independent state. Personal READY
//! extensions are an unofficial compatibility surface; see DISCORD_SESSION.md.
//! Unknown permissions fail closed. Payloads never leave this adapter.

use rune_i18n::{es, t};
use std::collections::{HashMap, HashSet};

use rune_model::{
    permissions, Channel, ChannelId, ChannelKind, Member, MemberRestrictions, MessageId, Overwrite,
    OverwriteTarget, Permissions, Rgb, Role, RoleId, Server, ServerId, User, UserId,
};

mod threads;
use rune_provider::{Event, Snapshot};
use serde_json::Value;
pub(crate) use threads::{
    allowed as thread_allowed, confirmed as thread_confirmed, is_thread_command, thread_answer,
    valid_cursor,
};
// Threads, forums and stages (2026-10-18).
pub(crate) use threads::{keep_paged as keep_paged_threads, muted_until, ArchiveCursor, MAX_EDGE};

use crate::{wire, LoginError};

const MAX_ITEMS: usize = 20_000;
const MAX_GUILDS: usize = 500;
const MAX_ROLES: usize = 512;
const BAD: LoginError = LoginError::new(es("No se pudo interpretar el estado de la cuenta."));

fn boolean(value: &Value) -> Result<bool, LoginError> {
    if value.is_null() {
        Ok(false)
    } else {
        value.as_bool().ok_or(BAD)
    }
}

fn unsigned(value: &Value) -> Result<u64, LoginError> {
    if value.is_null() {
        Ok(0)
    } else {
        value.as_u64().ok_or(BAD)
    }
}

/// Documented guild member flag: held back by automatic moderation.
const AUTOMOD_QUARANTINED_USERNAME: u64 = 1 << 7;
/// Held back for the server tag shown (`AUTOMOD_QUARANTINED_SERVER_TAG`,
/// named so in the official client's bundle; see docs/DISCORD_SESSION.md).
const AUTOMOD_QUARANTINED_SERVER_TAG: u64 = 1 << 10;

/// Apply the restriction fields `data` carries onto `restrictions`; a field
/// it omits keeps what was known. Malformed values end the session instead
/// of granting access.
fn restrictions(data: &Value, restrictions: &mut MemberRestrictions) -> Result<(), LoginError> {
    if let Some(until) = data.get("communication_disabled_until") {
        restrictions.timeout_until = (!until.is_null())
            .then(|| wire::timestamp(until).map_err(|_| BAD))
            .transpose()?;
    }
    if let Some(pending) = data.get("pending") {
        restrictions.pending = boolean(pending)?;
    }
    if let Some(flags) = data.get("flags") {
        let flags = unsigned(flags)?;
        restrictions.quarantined = flags & AUTOMOD_QUARANTINED_USERNAME != 0;
        restrictions.tag_quarantined = flags & AUTOMOD_QUARANTINED_SERVER_TAG != 0;
        // Rules and onboarding (2026-10-18).
        restrictions.onboarded = flags & crate::onboarding::COMPLETED_ONBOARDING != 0;
        restrictions.guest = flags & crate::onboarding::IS_GUEST != 0;
    }
    Ok(())
}

fn rows(value: &Value, limit: usize) -> Result<&[Value], LoginError> {
    if value.is_null() {
        return Ok(&[]);
    }
    let values = value.as_array().ok_or(BAD)?;
    if values.len() > limit {
        return Err(BAD);
    }
    Ok(values)
}

fn label(value: &Value, fallback: &str, max: usize) -> Result<String, LoginError> {
    let text = if value.is_null() {
        fallback
    } else {
        value.as_str().ok_or(BAD)?
    };
    if text.len() > max * 4 || text.chars().count() > max || text.chars().any(char::is_control) {
        return Err(BAD);
    }
    Ok(text.to_owned())
}

fn optional_id(value: &Value) -> Result<Option<u64>, LoginError> {
    if value.is_null() {
        Ok(None)
    } else {
        wire::id(value).map(Some)
    }
}

fn paragraph(value: &Value, max: usize) -> Result<Option<String>, LoginError> {
    if value.is_null() {
        return Ok(None);
    }
    let text = value.as_str().ok_or(BAD)?;
    if text.len() > max * 4
        || text.chars().count() > max
        || text
            .chars()
            .any(|c| c.is_control() && c != '\n' && c != '\t')
    {
        return Err(BAD);
    }
    Ok(Some(text.to_owned()))
}

fn decode_overwrites(value: &Value) -> Result<Vec<Overwrite>, LoginError> {
    let mut overwrites = Vec::new();
    let mut seen = HashSet::new();
    if !value.is_array() {
        return Err(BAD);
    }
    for overwrite in rows(value, 1000)? {
        let id = wire::id(&overwrite["id"])?;
        let target = match overwrite["type"].as_u64() {
            Some(0) => OverwriteTarget::Role(RoleId(id)),
            Some(1) => OverwriteTarget::Member(UserId(id)),
            _ => return Err(BAD),
        };
        if !seen.insert(target) {
            return Err(BAD);
        }
        overwrites.push(Overwrite {
            target,
            allow: permission_bits(&overwrite["allow"])?,
            deny: permission_bits(&overwrite["deny"])?,
        });
    }
    Ok(overwrites)
}

/// Discord's bit positions are not RUNE's bit positions. Unknown high bits
/// must not alias local permissions or overflow into an administrator grant.
pub(crate) fn permission_bits(value: &Value) -> Result<Permissions, LoginError> {
    let bits = if let Some(text) = value.as_str() {
        if text.is_empty() || text.len() > 39 || !text.bytes().all(|b| b.is_ascii_digit()) {
            return Err(BAD);
        }
        text.parse::<u128>().map_err(|_| BAD)?
    } else {
        value.as_u64().ok_or(BAD)? as u128
    };
    let mut result = Permissions::NONE;
    for (bit, permission) in PERMISSION_BITS {
        if bits & (1_u128 << bit) != 0 {
            result = result | permission;
        }
    }
    Ok(result)
}

/// Each Discord permission bit RUNE models, with its RUNE permission; the
/// others are kept as they are when RUNE writes permissions (`admin`).
pub(crate) const PERMISSION_BITS: [(u32, Permissions); 41] = [
    (0, Permissions::CREATE_INVITE),
    (1, Permissions::KICK),
    (2, Permissions::BAN),
    (3, Permissions::ADMINISTRATOR),
    (4, Permissions::MANAGE_CHANNELS),
    (5, Permissions::MANAGE_SERVER),
    (6, Permissions::REACT),
    (7, Permissions::VIEW_AUDIT_LOG),
    (8, Permissions::PRIORITY_SPEAKER),
    (9, Permissions::STREAM),
    (10, Permissions::VIEW),
    (11, Permissions::SEND),
    (13, Permissions::MANAGE_MESSAGES),
    (14, Permissions::EMBED_LINKS),
    (15, Permissions::ATTACH),
    (16, Permissions::READ_HISTORY),
    (17, Permissions::MENTION_EVERYONE),
    (18, Permissions::EXTERNAL_EMOJI),
    (20, Permissions::CONNECT),
    (21, Permissions::SPEAK),
    (22, Permissions::MUTE_MEMBERS),
    (23, Permissions::DEAFEN_MEMBERS),
    (24, Permissions::MOVE_MEMBERS),
    (25, Permissions::USE_VOICE_ACTIVITY),
    (26, Permissions::CHANGE_NICK),
    (27, Permissions::MANAGE_NICKS),
    (28, Permissions::MANAGE_ROLES),
    (29, Permissions::MANAGE_WEBHOOKS),
    (30, Permissions::MANAGE_EMOJI),
    (31, Permissions::USE_APP_COMMANDS),
    (32, Permissions::REQUEST_TO_SPEAK),
    (34, Permissions::MANAGE_THREADS),
    (35, Permissions::CREATE_THREADS),
    (36, Permissions::CREATE_PRIVATE_THREADS),
    (38, Permissions::SEND_IN_THREADS),
    (40, Permissions::TIMEOUT_MEMBERS),
    // Message actions (P4).
    (49, Permissions::SEND_POLLS),
    // Reactions, pins and search: `PIN_MESSAGES` in the web client's table.
    (51, Permissions::PIN_MESSAGES),
    // Emoji and states (2026-10-18): `USE_EXTERNAL_STICKERS`.
    (37, Permissions::EXTERNAL_STICKERS),
    // Scheduled events (eventos-programados): `MANAGE_EVENTS`, `CREATE_EVENTS`.
    (33, Permissions::MANAGE_EVENTS),
    (44, Permissions::CREATE_EVENTS),
];

pub(crate) fn role(value: &Value) -> Result<Role, LoginError> {
    let color = value["colors"]["primary_color"]
        .as_u64()
        .or_else(|| value["color"].as_u64())
        .unwrap_or(0);
    if color > 0xffffff {
        return Err(BAD);
    }
    Ok(Role {
        // Presence, identity and expressions (P5).
        style: crate::identity::role_style(wire::id(&value["id"])?, value),
        id: RoleId(wire::id(&value["id"])?),
        name: label(&value["name"], t("Rol"), 100)?,
        color: (color != 0).then_some(Rgb(color as u32)),
        position: value["position"]
            .as_i64()
            .unwrap_or(0)
            .try_into()
            .map_err(|_| BAD)?,
        hoisted: boolean(&value["hoist"])?,
        mentionable: boolean(&value["mentionable"])?,
        permissions: permission_bits(&value["permissions"])?,
    })
}

fn member_id(value: &Value) -> Result<UserId, LoginError> {
    let embedded = optional_id(&value["user"]["id"])?;
    let separate = optional_id(&value["user_id"])?;
    if embedded.zip(separate).is_some_and(|(a, b)| a != b) {
        return Err(BAD);
    }
    Ok(UserId(embedded.or(separate).ok_or(BAD)?))
}

fn member(value: &Value, server: ServerId) -> Result<Member, LoginError> {
    let user = member_id(value)?;
    // Without a roles array this is an identity fragment, not permission evidence.
    let roles = value["roles"].as_array().ok_or(BAD)?;
    if roles.len() > MAX_ROLES {
        return Err(BAD);
    }
    let mut seen = HashSet::new();
    let roles = roles
        .iter()
        .map(|value| {
            let id = RoleId(wire::id(value)?);
            if !seen.insert(id) || id.0 == server.0 {
                return Err(BAD);
            }
            Ok(id)
        })
        .collect::<Result<_, LoginError>>()?;
    Ok(Member {
        user,
        server,
        roles,
        nick: (!value["nick"].is_null())
            .then(|| label(&value["nick"], "", 128))
            .transpose()?,
        // A malformed join date only loses the date: it grants nothing.
        joined: wire::timestamp(&value["joined_at"]).ok(),
        // The picture set for this server, on the CDN as the official
        // client builds it (`GUILD_MEMBER_AVATAR`); a bad hash shows theirs.
        avatar: wire::cdn(
            &format!("guilds/{server}/users/{user}/avatars"),
            false,
            &value["avatar"],
        ),
        // The marks worn in this server over the account's (P5).
        identity: crate::identity::identity(value),
        ..Default::default()
    })
}

/// A person of a member list: their membership and its restrictions.
pub(crate) fn list_member(value: &Value, server: ServerId) -> Result<Member, LoginError> {
    let mut listed = member(value, server)?;
    restrictions(value, &mut listed.restrictions)?;
    Ok(listed)
}

pub(crate) fn add_user(snapshot: &mut Snapshot, value: &Value) -> Result<(), LoginError> {
    let mut user = wire::user(value)?;
    if let Some(known) = snapshot.users.iter_mut().find(|known| known.id == user.id) {
        user.banner.inherit_unknown(&known.banner);
        *known = user;
    } else {
        if snapshot.users.len() >= MAX_ITEMS {
            return Err(BAD);
        }
        snapshot.users.push(user);
    }
    Ok(())
}

fn add_member(snapshot: &mut Snapshot, data: &Value, server: ServerId) -> Result<(), LoginError> {
    if data["user"]["username"].is_string() {
        add_user(snapshot, &data["user"])?;
    }
    let user = member_id(data)?;
    let previous = snapshot
        .members
        .iter()
        .find(|m| m.server == server && m.user == user);
    // Supplemental identity fragments often omit roles. They may enrich a
    // known member, but cannot create membership or erase role evidence.
    let mut member = if data.get("roles").is_some() {
        member(data, server)?
    } else {
        let Some(previous) = previous else {
            return Ok(());
        };
        let mut member = previous.clone();
        if let Some(nick) = data.get("nick") {
            member.nick = (!nick.is_null())
                .then(|| label(nick, "", 128))
                .transpose()?;
        }
        member
    };
    // Restrictions and the join date a row omits stay as last known, so a
    // partial update cannot lift a timeout. The domain carries them; the
    // state and the REST gate deny taking part while one lasts.
    if let Some(previous) = previous {
        member.restrictions = previous.restrictions.clone();
        member.joined = member.joined.or(previous.joined);
    }
    restrictions(data, &mut member.restrictions)?;
    snapshot
        .members
        .retain(|m| !(m.server == server && m.user == member.user));
    if snapshot.members.len() >= MAX_ITEMS {
        return Err(BAD);
    }
    snapshot.members.push(member);
    Ok(())
}

/// RUNE's kind for a documented channel type; `None` for one it does not show.
/// Media channels are drawn as forums.
pub(crate) fn kind_of(value: u64) -> Option<ChannelKind> {
    Some(match value {
        0 => ChannelKind::Text,
        1 => ChannelKind::Direct,
        2 => ChannelKind::Voice,
        13 => ChannelKind::Stage,
        3 => ChannelKind::Group,
        4 => ChannelKind::Category,
        5 => ChannelKind::Announcement,
        10..=12 => ChannelKind::Thread,
        15 | 16 => ChannelKind::Forum,
        _ => return None,
    })
}

fn decode_channel(
    data: &Value,
    server: Option<ServerId>,
    users: &[User],
) -> Result<Option<Channel>, LoginError> {
    let id = ChannelId(wire::id(&data["id"])?);
    if let Some(supplied) = optional_id(&data["guild_id"])? {
        if Some(ServerId(supplied)) != server {
            return Err(BAD);
        }
    }
    let Some(kind) = kind_of(data["type"].as_u64().ok_or(BAD)?) else {
        return Ok(None);
    };
    let thread = kind == ChannelKind::Thread;
    let parent = optional_id(&data["parent_id"])?.map(ChannelId);
    if thread && (parent.is_none() || parent == Some(id)) {
        return Ok(None);
    }
    let direct = matches!(kind, ChannelKind::Direct | ChannelKind::Group);
    if direct != server.is_none() {
        return Err(BAD);
    }
    // The reference's obfuscated channel flag is deliberately omitted, never
    // reconstructed as an accessible channel from its other fields.
    if unsigned(&data["flags"])? & (1 << 17) != 0 {
        return Ok(None);
    }
    let mut recipients = Vec::new();
    for user in rows(&data["recipients"], 64)? {
        recipients.push(UserId(wire::id(&user["id"])?));
    }
    for user in rows(&data["recipient_ids"], 64)? {
        let id = UserId(wire::id(user)?);
        if !recipients.contains(&id) {
            recipients.push(id);
        }
    }
    let fallback = if direct {
        let names = recipients
            .iter()
            .filter_map(|id| users.iter().find(|u| u.id == *id))
            .map(User::shown_name)
            .take(8)
            .collect::<Vec<_>>()
            .join(", ");
        if names.is_empty() {
            rune_model::Channel::UNNAMED.to_owned()
        } else {
            names
        }
    } else {
        "canal".to_owned()
    };
    // A thread has no overwrites of its own: it takes its parent's access.
    if !direct && !thread && !data["permission_overwrites"].is_array() {
        // Do not confuse missing overwrite evidence with an empty deny list.
        return Ok(None);
    }
    let overwrites = if direct || thread {
        Vec::new()
    } else {
        decode_overwrites(&data["permission_overwrites"])?
    };
    let permissions = if direct {
        // Anyone in a direct conversation may attach files, as in Discord.
        Permissions::VIEW
            | Permissions::READ_HISTORY
            | Permissions::SEND
            | Permissions::REACT
            | Permissions::ATTACH
    } else {
        Permissions::NONE
    };
    let topic = paragraph(&data["topic"], 4096)?;
    Ok(Some(Channel {
        id,
        server,
        kind,
        name: if data["name"].is_null() {
            fallback
        } else {
            label(&data["name"], "", 128)?
        },
        // A topic's server emoji, as the official client draws them.
        topic_pictures: crate::emoji::pictures(topic.as_deref()),
        topic,
        parent,
        position: data["position"]
            .as_i64()
            .unwrap_or(0)
            .try_into()
            .map_err(|_| BAD)?,
        recipients,
        // Ownership grants nothing here (group writes are not offered), so a
        // malformed one is dropped instead of failing the whole account.
        owner: (kind == ChannelKind::Group)
            .then(|| wire::id(&data["owner_id"]).ok().map(UserId))
            .flatten(),
        permissions,
        overwrites,
        slowmode_secs: unsigned(&data["rate_limit_per_user"])?
            .try_into()
            .map_err(|_| BAD)?,
        nsfw: boolean(&data["nsfw"])?,
        // A malformed picture is dropped, never a reason to lose the group.
        icon: (kind == ChannelKind::Group)
            .then(|| wire::image("channel-icons", id.0, &data["icon"]))
            .flatten(),
        thread: thread.then(|| threads::metadata(data, id, None)),
        tags: if kind == ChannelKind::Forum {
            threads::forum_tags(&data["available_tags"])
        } else {
            Vec::new()
        },
        forum: if kind == ChannelKind::Forum {
            threads::forum_defaults(data, &Default::default())
        } else {
            Default::default()
        },
    }))
}

/// A conversation outside servers as a REST answer names it, with the
/// people its `recipients` carry.
pub(crate) fn private_channel(data: &Value) -> Result<(Channel, Vec<User>), LoginError> {
    let users = rows(&data["recipients"], 64)?
        .iter()
        .map(wire::user)
        .collect::<Result<Vec<_>, _>>()?;
    let channel = decode_channel(data, None, &users)?.ok_or(BAD)?;
    Ok((channel, users))
}

fn add_channel(
    snapshot: &mut Snapshot,
    data: &Value,
    server: Option<ServerId>,
) -> Result<(), LoginError> {
    for user in rows(&data["recipients"], 64)? {
        add_user(snapshot, user)?;
    }
    let Some(channel) = decode_channel(data, server, &snapshot.users)? else {
        return Ok(());
    };
    if snapshot.channels.iter().any(|known| known.id == channel.id) {
        return Err(BAD);
    }
    if snapshot.channels.len() >= MAX_ITEMS {
        return Err(BAD);
    }
    if let Some(last) = optional_id(&data["last_message_id"])? {
        snapshot.last_messages.push((channel.id, MessageId(last)));
    }
    snapshot.channels.push(channel);
    Ok(())
}

fn conflicting_membership<'a>(
    data: impl Iterator<Item = &'a Value>,
    me: Option<UserId>,
    server: ServerId,
) -> Result<bool, LoginError> {
    let mut roles: Option<HashSet<RoleId>> = None;
    let mut conflict = false;
    for data in data {
        if Some(member_id(data)?) != me {
            continue;
        }
        if data.get("roles").is_some() {
            let incoming = member(data, server)?.roles.into_iter().collect();
            conflict |= roles.as_ref().is_some_and(|previous| *previous != incoming);
            roles = Some(incoming);
        }
        // Validated here so a malformed restriction fails before anything applies.
        restrictions(data, &mut MemberRestrictions::default())?;
    }
    Ok(conflict)
}

fn add_guild(snapshot: &mut Snapshot, data: &Value, merged: &[Value]) -> Result<(), LoginError> {
    let id = ServerId(wire::id(&data["id"])?);
    if snapshot.servers.iter().any(|s| s.id == id) || snapshot.servers.len() >= MAX_GUILDS {
        return Err(BAD);
    }
    let property = |key: &str| {
        data.get("properties")
            .and_then(|p| p.get(key))
            .unwrap_or(&data[key])
    };
    let mut roles = Vec::new();
    let mut ids = HashSet::new();
    for data in rows(&data["roles"], MAX_ROLES)? {
        let role = role(data)?;
        if !ids.insert(role.id) {
            return Err(BAD);
        }
        roles.push(role);
    }
    // RUNE's generic model has a permissive default for newly-created local
    // servers. Remote metadata must always install an explicit everyone role.
    if !roles.iter().any(|r| r.id.0 == id.0) {
        roles.push(Role::everyone(id, Permissions::NONE));
    }
    let server = Server {
        id,
        name: label(property("name"), Server::UNNAMED, 128)?,
        accent: crate::admin::accent(property("brand_color_primary")).unwrap_or(Rgb(0x5865f2)),
        icon: wire::image("icons", id.0, property("icon")),
        emojis: crate::emoji::catalog(&data["emojis"], id)?,
        stickers: crate::stickers::decode(&data["stickers"], true)?,
        roles,
        owner: optional_id(property("owner_id"))?.map(UserId),
        description: paragraph(property("description"), 4096)?,
        // Server administration, rest (P7).
        options: crate::admin::rest::options(
            |key| {
                data.get("properties")
                    .and_then(|p| p.get(key))
                    .or_else(|| data.get(key))
            },
            &Default::default(),
        ),
    };
    snapshot.servers.push(server);
    let members = rows(&data["members"], MAX_ITEMS)?;
    let own_conflict = conflicting_membership(
        members.iter().chain(merged),
        snapshot.me.as_ref().map(|me| me.id),
        id,
    )?;
    for data in members.iter().chain(merged) {
        add_member(snapshot, data, id)?;
    }
    if own_conflict {
        snapshot
            .members
            .retain(|m| m.server != id || snapshot.me.as_ref().is_none_or(|me| m.user != me.id));
    }
    for data in rows(&data["channels"], MAX_ITEMS)? {
        add_channel(snapshot, data, Some(id))?;
    }
    threads::add_active(snapshot, data, id);
    crate::voice::guild_states(snapshot, data);
    Ok(())
}

fn recompute(snapshot: &mut Snapshot) {
    let Some(me) = snapshot.me.as_ref().map(|u| u.id) else {
        return;
    };
    let own_members: HashMap<_, _> = snapshot
        .members
        .iter()
        .filter(|m| m.user == me)
        .map(|m| (m.server, m))
        .collect();
    let contexts: HashMap<_, _> = snapshot
        .servers
        .iter()
        .filter_map(|server| {
            let member = *own_members.get(&server.id)?;
            let roles: HashSet<_> = server.roles.iter().map(|r| r.id).collect();
            member
                .roles
                .iter()
                .all(|id| roles.contains(id))
                .then_some((server.id, (server, member)))
        })
        .collect();
    for channel in &mut snapshot.channels {
        let Some(id) = channel.server else {
            continue;
        };
        channel.permissions = contexts.get(&id).map_or(Permissions::NONE, |(s, m)| {
            permissions::in_channel(s, m, channel)
        });
    }
    // A thread can do what its parent can; without its parent, nothing.
    let parents: HashMap<_, _> = snapshot
        .channels
        .iter()
        .filter(|c| !c.is_thread())
        .map(|c| (c.id, c.permissions))
        .collect();
    for channel in snapshot.channels.iter_mut().filter(|c| c.is_thread()) {
        channel.permissions = channel
            .parent
            .and_then(|parent| parents.get(&parent).copied())
            .unwrap_or(Permissions::NONE);
    }
}

pub(crate) fn ready(data: &Value, me: &User) -> Result<Snapshot, LoginError> {
    if wire::id(&data["user"]["id"])? != me.id.0 {
        return Err(BAD);
    }
    let mut snapshot = Snapshot {
        me: Some(me.clone()),
        users: vec![me.clone()],
        ..Default::default()
    };
    for user in rows(&data["users"], MAX_ITEMS)? {
        add_user(&mut snapshot, user)?;
    }
    let guilds = rows(&data["guilds"], MAX_GUILDS)?;
    let merged = rows(&data["merged_members"], MAX_GUILDS)?;
    if !merged.is_empty() && merged.len() != guilds.len() {
        return Err(BAD);
    }
    for (index, guild) in guilds.iter().enumerate() {
        let members = merged
            .get(index)
            .map(|v| rows(v, MAX_ITEMS))
            .transpose()?
            .unwrap_or(&[]);
        add_guild(&mut snapshot, guild, members)?;
    }
    for channel in rows(&data["private_channels"], MAX_ITEMS)? {
        add_channel(&mut snapshot, channel, None)?;
    }
    crate::social::ready(&mut snapshot, &data["relationships"]);
    crate::social::ready_presences(&mut snapshot, &data["presences"]);
    let reads = data["read_state"]
        .get("entries")
        .unwrap_or(&data["read_state"]);
    for entry in rows(reads, MAX_ITEMS)? {
        if entry["type"].as_u64().unwrap_or(0) != 0 {
            continue;
        }
        let channel = ChannelId(wire::id(&entry["id"])?);
        if snapshot.channels.iter().any(|c| c.id == channel) {
            let cursor = &entry["last_message_id"];
            let cursor = if cursor == "0" || cursor.as_u64() == Some(0) {
                None
            } else {
                optional_id(cursor)?
            };
            snapshot.read_states.push((
                channel,
                cursor.map(MessageId),
                entry["mention_count"]
                    .as_u64()
                    .or_else(|| entry["badge_count"].as_u64())
                    .unwrap_or(0)
                    .try_into()
                    .map_err(|_| BAD)?,
            ));
        }
    }
    recompute(&mut snapshot);
    Ok(snapshot)
}

fn remove_channel(snapshot: &mut Snapshot, channel: ChannelId) {
    snapshot.channels.retain(|c| c.id != channel);
    snapshot.last_messages.retain(|(c, _)| *c != channel);
    snapshot.read_states.retain(|(c, _, _)| *c != channel);
}

fn remove_guild(snapshot: &mut Snapshot, server: ServerId) {
    let removed: HashSet<_> = snapshot
        .channels
        .iter()
        .filter(|c| c.server == Some(server))
        .map(|c| c.id)
        .collect();
    snapshot.channels.retain(|c| !removed.contains(&c.id));
    snapshot.last_messages.retain(|(c, _)| !removed.contains(c));
    snapshot
        .read_states
        .retain(|(c, _, _)| !removed.contains(c));
    snapshot.members.retain(|m| m.server != server);
    snapshot
        .voice
        .retain(|state| !removed.contains(&state.channel));
    snapshot.servers.retain(|s| s.id != server);
}

fn patch_channel(previous: &Channel, data: &Value) -> Result<Option<Channel>, LoginError> {
    let mut channel = previous.clone();
    if let Some(guild) = optional_id(&data["guild_id"])? {
        if channel.server != Some(ServerId(guild)) {
            return Err(BAD);
        }
    }
    if unsigned(&data["flags"])? & (1 << 17) != 0 {
        return Ok(None);
    }
    if let Some(value) = data.get("type") {
        let Some(kind) = value.as_u64().and_then(kind_of) else {
            return Ok(None);
        };
        // Threads change through THREAD_UPDATE; a channel is never turned into one.
        if (kind == ChannelKind::Thread) != channel.is_thread() {
            return Ok(None);
        }
        if matches!(kind, ChannelKind::Direct | ChannelKind::Group) != channel.server.is_none() {
            return Err(BAD);
        }
        channel.kind = kind;
    }
    if let Some(value) = data.get("name") {
        channel.name = label(value, &channel.name, 128)?;
    }
    if let Some(value) = data.get("topic") {
        channel.topic = paragraph(value, 4096)?;
        channel.topic_pictures = crate::emoji::pictures(channel.topic.as_deref());
    }
    if let Some(value) = data.get("parent_id") {
        channel.parent = optional_id(value)?.map(ChannelId);
    }
    if let Some(value) = data.get("position") {
        channel.position = value.as_i64().ok_or(BAD)?.try_into().map_err(|_| BAD)?;
    }
    if let Some(value) = data.get("rate_limit_per_user") {
        channel.slowmode_secs = value.as_u64().ok_or(BAD)?.try_into().map_err(|_| BAD)?;
    }
    if let Some(value) = data.get("nsfw") {
        channel.nsfw = value.as_bool().ok_or(BAD)?;
    }
    if let Some(value) = data.get("permission_overwrites") {
        channel.overwrites = decode_overwrites(value)?;
    }
    if let (ChannelKind::Group, Some(value)) = (channel.kind, data.get("owner_id")) {
        channel.owner = wire::id(value).ok().map(UserId);
    }
    if let (ChannelKind::Group, Some(value)) = (channel.kind, data.get("icon")) {
        channel.icon = wire::image("channel-icons", channel.id.0, value);
    }
    if let Some(value) = data.get("available_tags") {
        channel.tags = threads::forum_tags(value);
    }
    if channel.kind == ChannelKind::Forum {
        channel.forum = threads::forum_defaults(data, &channel.forum);
    }
    Ok(Some(channel))
}

/// Apply navigation changes and report only affected entities. On malformed
/// permission evidence the caller must stop this session, not retain grants.
pub(crate) fn update(
    snapshot: &mut Snapshot,
    kind: &str,
    data: &Value,
) -> Result<Vec<Event>, LoginError> {
    let before: HashMap<_, _> = snapshot
        .channels
        .iter()
        .map(|c| (c.id, c.permissions))
        .collect();
    let mut events = Vec::new();
    match kind {
        "GUILD_CREATE" => {
            let server = ServerId(wire::id(&data["id"])?);
            let old: Vec<_> = snapshot
                .channels
                .iter()
                .filter(|c| c.server == Some(server))
                .map(|c| c.id)
                .collect();
            let gone: HashSet<_> = old.iter().copied().collect();
            let old_voice: Vec<_> = snapshot
                .voice
                .iter()
                .filter(|state| gone.contains(&state.channel))
                .cloned()
                .collect();
            remove_guild(snapshot, server);
            add_guild(snapshot, data, &[])?;
            recompute(snapshot);
            // Who is in voice there now: a guild arriving after READY (lazy,
            // or back from an outage) says so only here.
            let channels: HashSet<_> = snapshot
                .channels
                .iter()
                .filter(|c| c.server == Some(server))
                .map(|c| c.id)
                .collect();
            for state in old_voice {
                if !snapshot.voice.iter().any(|known| known.user == state.user) {
                    events.push(Event::VoiceLeft {
                        user: state.user,
                        channel: state.channel,
                    });
                }
            }
            events.extend(
                snapshot
                    .voice
                    .iter()
                    .filter(|state| channels.contains(&state.channel))
                    .cloned()
                    .map(Event::VoiceChanged),
            );
            for id in old {
                if !snapshot.channels.iter().any(|c| c.id == id) {
                    events.push(Event::ChannelRemoved { channel: id });
                }
            }
            if let Some(s) = snapshot.servers.iter().find(|s| s.id == server) {
                events.push(Event::ServerChanged(s.clone()));
            }
            for data in rows(&data["members"], MAX_ITEMS)? {
                if data["user"]["username"].is_string() {
                    events.push(Event::UserChanged(wire::user(&data["user"])?));
                }
            }
            events.extend(
                snapshot
                    .members
                    .iter()
                    .filter(|m| m.server == server)
                    .cloned()
                    .map(Event::MemberChanged),
            );
            events.extend(
                snapshot
                    .channels
                    .iter()
                    .filter(|c| c.server == Some(server))
                    .cloned()
                    .map(Event::ChannelChanged),
            );
        }
        "GUILD_UPDATE" => {
            let id = ServerId(wire::id(&data["id"])?);
            let Some(server) = snapshot.servers.iter_mut().find(|s| s.id == id) else {
                return Ok(events);
            };
            if let Some(v) = data.get("name") {
                server.name = label(v, &server.name, 128)?;
            }
            if let Some(v) = data.get("description") {
                server.description = paragraph(v, 4096)?;
            }
            if let Some(value) = data.get("brand_color_primary") {
                server.accent = crate::admin::accent(value).unwrap_or(Rgb(0x5865f2));
            }
            if let Some(value) = data.get("icon") {
                server.icon = wire::image("icons", id.0, value);
            }
            if let Some(value) = data.get("emojis") {
                server.emojis = crate::emoji::catalog(value, id)?;
            }
            if let Some(value) = data.get("stickers") {
                server.stickers = crate::stickers::decode(value, true)?;
            }
            if let Some(v) = data.get("owner_id") {
                server.owner = optional_id(v)?.map(UserId);
            }
            // Server administration, rest (P7).
            server.options = crate::admin::rest::options(|key| data.get(key), &server.options);
            if let Some(value) = data.get("roles") {
                if !value.is_array() {
                    return Err(BAD);
                }
                let mut roles = Vec::new();
                let mut ids = HashSet::new();
                for data in rows(value, MAX_ROLES)? {
                    let role = role(data)?;
                    if !ids.insert(role.id) {
                        return Err(BAD);
                    }
                    roles.push(role);
                }
                if !roles.iter().any(|r| r.id.0 == id.0) {
                    roles.push(Role::everyone(id, Permissions::NONE));
                }
                server.roles = roles;
            }
            events.push(Event::ServerChanged(server.clone()));
        }
        "GUILD_STICKERS_UPDATE" => {
            let id = ServerId(wire::id(&data["guild_id"])?);
            if let Some(server) = snapshot.servers.iter_mut().find(|server| server.id == id) {
                server.stickers = crate::stickers::decode(&data["stickers"], true)?;
                events.push(Event::ServerChanged(server.clone()));
            }
        }
        "GUILD_EMOJIS_UPDATE" => {
            let id = ServerId(wire::id(&data["guild_id"])?);
            if let Some(server) = snapshot.servers.iter_mut().find(|server| server.id == id) {
                server.emojis = crate::emoji::catalog(&data["emojis"], id)?;
                events.push(Event::ServerChanged(server.clone()));
            }
        }
        "GUILD_DELETE" => {
            let server = ServerId(wire::id(&data["id"])?);
            remove_guild(snapshot, server);
            events.push(Event::ServerRemoved {
                server,
                reason: None,
            });
        }
        "CHANNEL_CREATE" => {
            let id = ChannelId(wire::id(&data["id"])?);
            let server = optional_id(&data["guild_id"])?.map(ServerId);
            if server.is_some_and(|id| !snapshot.servers.iter().any(|s| s.id == id)) {
                return Ok(events);
            }
            if snapshot.channels.iter().any(|c| c.id == id) {
                return update(snapshot, "CHANNEL_UPDATE", data);
            }
            add_channel(snapshot, data, server)?;
            recompute(snapshot);
            if let Some(channel) = snapshot.channels.iter().find(|c| c.id == id) {
                events.push(Event::ChannelChanged(channel.clone()));
            }
            for value in rows(&data["recipients"], 64)? {
                events.push(Event::UserChanged(wire::user(value)?));
            }
        }
        "CHANNEL_UPDATE" => {
            let id = ChannelId(wire::id(&data["id"])?);
            if let Some(index) = snapshot.channels.iter().position(|c| c.id == id) {
                if let Some(channel) = patch_channel(&snapshot.channels[index], data)? {
                    snapshot.channels[index] = channel;
                    recompute(snapshot);
                    events.push(Event::ChannelChanged(snapshot.channels[index].clone()));
                } else {
                    remove_channel(snapshot, id);
                    events.push(Event::ChannelRemoved { channel: id });
                }
            }
        }
        "THREAD_CREATE"
        | "THREAD_UPDATE"
        | "THREAD_DELETE"
        | "THREAD_LIST_SYNC"
        | "THREAD_MEMBER_UPDATE"
        | "THREAD_MEMBERS_UPDATE" => {
            // A malformed thread dispatch carries no permission evidence:
            // it is dropped instead of ending the session.
            events = threads::update(snapshot, kind, data).unwrap_or_default();
        }
        "CHANNEL_DELETE" => {
            let channel = ChannelId(wire::id(&data["id"])?);
            remove_channel(snapshot, channel);
            events.push(Event::ChannelRemoved { channel });
        }
        "GUILD_ROLE_CREATE" | "GUILD_ROLE_UPDATE" | "GUILD_ROLE_DELETE" => {
            let id = ServerId(wire::id(&data["guild_id"])?);
            let Some(server) = snapshot.servers.iter_mut().find(|s| s.id == id) else {
                return Ok(events);
            };
            if kind == "GUILD_ROLE_DELETE" {
                let role = RoleId(wire::id(&data["role_id"])?);
                if role.0 == id.0 {
                    return Err(BAD);
                }
                server.roles.retain(|r| r.id != role);
                for member in snapshot.members.iter_mut().filter(|m| m.server == id) {
                    if member.roles.contains(&role) {
                        member.roles.retain(|r| *r != role);
                        events.push(Event::MemberChanged(member.clone()));
                    }
                }
            } else {
                let role = role(&data["role"])?;
                server.roles.retain(|r| r.id != role.id);
                if server.roles.len() >= MAX_ROLES {
                    return Err(BAD);
                }
                server.roles.push(role);
            }
            events.push(Event::ServerChanged(server.clone()));
        }
        "GUILD_MEMBER_ADD" | "GUILD_MEMBER_UPDATE" => {
            let server = ServerId(wire::id(&data["guild_id"])?);
            if !snapshot.servers.iter().any(|s| s.id == server) {
                return Ok(events);
            }
            let user = UserId(wire::id(&data["user"]["id"])?);
            add_member(snapshot, data, server)?;
            if let Some(m) = snapshot
                .members
                .iter()
                .find(|m| m.server == server && m.user == user)
            {
                events.push(Event::MemberChanged(m.clone()));
            } else {
                events.push(Event::MemberRemoved { server, user });
            }
            if data["user"]["username"].is_string() {
                events.push(Event::UserChanged(wire::user(&data["user"])?));
            }
        }
        "GUILD_MEMBER_REMOVE" => {
            let server = ServerId(wire::id(&data["guild_id"])?);
            let user = UserId(wire::id(&data["user"]["id"])?);
            snapshot
                .members
                .retain(|m| !(m.server == server && m.user == user));
            events.push(Event::MemberRemoved { server, user });
        }
        "READY_SUPPLEMENTAL" => {
            let guilds = rows(&data["guilds"], MAX_GUILDS)?;
            let merged = rows(&data["merged_members"], MAX_GUILDS)?;
            if !merged.is_empty() && merged.len() != guilds.len() {
                return Err(BAD);
            }
            // Validate the whole batch before applying it. Repeating a guild
            // must not reset conflict detection and restore denied membership.
            let mut seen = HashSet::new();
            for guild in guilds {
                if !seen.insert(wire::id(&guild["id"])?) {
                    return Err(BAD);
                }
            }
            for (index, guild) in guilds.iter().enumerate() {
                let id = ServerId(wire::id(&guild["id"])?);
                if !snapshot.servers.iter().any(|s| s.id == id) {
                    continue;
                }
                let extra = merged
                    .get(index)
                    .map(|v| rows(v, MAX_ITEMS))
                    .transpose()?
                    .unwrap_or(&[]);
                let members = rows(&guild["members"], MAX_ITEMS)?;
                let me = snapshot.me.as_ref().map(|me| me.id);
                let own_conflict = conflicting_membership(members.iter().chain(extra), me, id)?;
                for data in members.iter().chain(extra) {
                    let who = member_id(data)?;
                    add_member(snapshot, data, id)?;
                    if !snapshot
                        .members
                        .iter()
                        .any(|m| m.server == id && m.user == who)
                    {
                        events.push(Event::MemberRemoved {
                            server: id,
                            user: who,
                        });
                    }
                    if data["user"]["username"].is_string() {
                        events.push(Event::UserChanged(wire::user(&data["user"])?));
                    }
                }
                if own_conflict {
                    snapshot
                        .members
                        .retain(|m| m.server != id || Some(m.user) != me);
                    if let Some(user) = me {
                        events.push(Event::MemberRemoved { server: id, user });
                    }
                }
                events.extend(
                    snapshot
                        .members
                        .iter()
                        .filter(|m| m.server == id)
                        .cloned()
                        .map(Event::MemberChanged),
                );
            }
        }
        _ => return Ok(events),
    }
    recompute(snapshot);
    // Permission changes must immediately reach the UI and REST command gate.
    let changed: HashSet<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::ChannelChanged(c) => Some(c.id),
            _ => None,
        })
        .collect();
    for channel in &snapshot.channels {
        if before
            .get(&channel.id)
            .is_some_and(|p| *p != channel.permissions)
            && !changed.contains(&channel.id)
        {
            events.push(Event::ChannelChanged(channel.clone()));
        }
    }
    Ok(events)
}

#[cfg(test)]
mod tests;
