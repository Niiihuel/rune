//! The rest of the server administration on the personal session (P7):
//! what may leave (a server's picture, traits and safety settings, paused
//! invitations, channels reordered at once, a forum's settings, a role's
//! picture, emoji and gradient) and how it reads and travels. The routes
//! are the ones Serein uses (`discord-api` crate: `server_settings.rs`,
//! `server_invites.rs`, `channel_actions.rs`, `server_roles.rs` and
//! `server_admin.rs`, read 2026-10-17): `PATCH /guilds/{g}/profile` and
//! `PATCH /guilds/{g}`, `PATCH /guilds/{g}/channels`, a channel's
//! `PATCH`, `GET /guilds/{g}/roles/member-counts` and the member search.
//! Since 2026-10-19 also a server's banner and moving icon, becoming a
//! community with its rules and updates channels (`PATCH /guilds/{g}` as
//! documented: `banner`, `features`, `rules_channel_id`,
//! `public_updates_channel_id`), the server's own emojis in traits and a
//! forum's reaction, and a forum's `default_auto_archive_duration` and
//! `default_tag_setting` (Serein's `channel_actions.rs`).
//! Fixtures only: none was tried against Discord.

use base64::{engine::general_purpose::STANDARD, Engine};
use rune_i18n::es;
use rune_model::{
    Channel, ChannelKind, ChannelMove, EmojiId, JoinMethod, MediaScan, MemberOrder, MemberQuery,
    Permissions, Rgb, Server, ServerOptions, ServerTrait, SystemMessages, Verification,
    AFK_TIMEOUTS, ARCHIVE_AFTER, MAX_TAGS, MAX_TAG_NAME, MAX_TRAITS,
};
use rune_provider::{ChannelEdit, RoleEdit, RoleIcon, ServerEdit};
use serde_json::{json, Map, Value};

use super::{holds, Standing};
use crate::{wire, LoginError};

/// A forum's flag that asks every post for a tag.
pub(crate) const REQUIRE_TAG: u64 = 1 << 4;
/// The flags of the system channel RUNE shows; the rest stay as they are.
const SYSTEM_MASK: u64 = 0b1111;
/// The features RUNE reads; the rest travel back untouched.
const INVITES_DISABLED: &str = "INVITES_DISABLED";
const ROLE_ICONS: &str = "ROLE_ICONS";
const ENHANCED_ROLE_COLORS: &str = "ENHANCED_ROLE_COLORS";
const COMMUNITY: &str = "COMMUNITY";
// Icon, banner and community (2026-10-19): what the server's boosts unlock.
const ANIMATED_ICON: &str = "ANIMATED_ICON";
const BANNER: &str = "BANNER";
const ANIMATED_BANNER: &str = "ANIMATED_BANNER";
/// What a recent join is, as Serein's filter asks for it.
const RECENT_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// What the safety and profile parts of `edit` need, checked against the session.
pub(super) fn server_edit(standing: &Standing, edit: &ServerEdit) -> Result<(), LoginError> {
    if let Some(traits) = &edit.traits {
        if traits.len() > MAX_TRAITS || !traits.iter().all(ServerTrait::valid) {
            return Err(LoginError::new(es(
                "Hasta 5 rasgos, cada uno de hasta 24 caracteres y un emoji.",
            )));
        }
    }
    for (channel, kind) in [
        (edit.system_channel, ChannelKind::Text),
        (edit.afk_channel, ChannelKind::Voice),
    ] {
        if let Some(Some(channel)) = channel {
            if standing.channel(channel)?.kind != kind {
                return Err(LoginError::new(es("Ese canal no sirve para eso.")));
            }
        }
    }
    if edit
        .afk_timeout
        .is_some_and(|secs| !AFK_TIMEOUTS.contains(&secs))
    {
        return Err(wire::invalid());
    }
    let options = &standing.server.options;
    let community = edit.community.unwrap_or(options.community);
    if community
        && (edit.verification == Some(Verification::None)
            || edit
                .media_scan
                .is_some_and(|scan| scan != MediaScan::Everyone))
    {
        return Err(LoginError::new(es(
            "Una comunidad necesita verificación y revisar el contenido de todos.",
        )));
    }
    looks(standing, edit)
}

/// What a banner, a moving picture, the server's own emojis and becoming
/// a community need of the server (2026-10-19).
fn looks(standing: &Standing, edit: &ServerEdit) -> Result<(), LoginError> {
    let server = &standing.server;
    let options = &server.options;
    let moving = |image: &Option<Option<rune_provider::ProfileImage>>| matches!(image, Some(Some(image)) if image.animated());
    if moving(&edit.icon) && !options.animated_icon {
        return Err(LoginError::new(es(
            "Este servidor no tiene desbloqueado el ícono animado.",
        )));
    }
    if (edit.banner.as_ref().is_some_and(Option::is_some) && !options.banner_allowed)
        || (moving(&edit.banner) && !options.animated_banner)
    {
        return Err(LoginError::new(es(
            "Este servidor no tiene desbloqueado ese banner.",
        )));
    }
    let own = |id: EmojiId| server.emojis.iter().any(|emoji| emoji.id == id);
    if edit
        .traits
        .iter()
        .flatten()
        .filter_map(|found| found.emoji_id)
        .any(|id| !own(id))
    {
        return Err(LoginError::new(es("Ese emoji no es de este servidor.")));
    }
    for channel in [edit.rules_channel, edit.updates_channel]
        .into_iter()
        .flatten()
        .flatten()
    {
        if standing.channel(channel)?.kind != ChannelKind::Text {
            return Err(LoginError::new(es("Ese canal no sirve para eso.")));
        }
    }
    if edit.community.is_some() {
        standing.needs(Permissions::ADMINISTRATOR)?;
    }
    let shown = ServerOptions {
        community: edit.community.unwrap_or(options.community),
        rules_channel: edit.rules_channel.unwrap_or(options.rules_channel),
        updates_channel: edit.updates_channel.unwrap_or(options.updates_channel),
        verification: edit.verification.unwrap_or(options.verification),
        media_scan: edit.media_scan.unwrap_or(options.media_scan),
        ..options.clone()
    };
    if shown.community && !shown.community_ready() {
        return Err(LoginError::new(es(
            "Una comunidad necesita un canal de reglas, uno de novedades, verificación y revisar el contenido de todos.",
        )));
    }
    Ok(())
}

/// What a role's picture, emoji and gradient need of `server`.
pub(super) fn role_style(server: &Server, edit: &RoleEdit) -> Result<(), LoginError> {
    if let Some(icon) = &edit.icon {
        if !server.options.role_icons {
            return Err(LoginError::new(es(
                "Este servidor no tiene desbloqueados los íconos de rol.",
            )));
        }
        if let Some(problem) = icon.as_ref().and_then(RoleIcon::problem) {
            return Err(LoginError::new(problem));
        }
    }
    if let Some(gradient) = &edit.gradient {
        if !server.options.role_gradients {
            return Err(LoginError::new(es(
                "Este servidor no tiene desbloqueados los degradés de rol.",
            )));
        }
        if gradient.len() > 2 || gradient.iter().any(|color| color.0 > 0xFF_FFFF) {
            return Err(wire::invalid());
        }
    }
    Ok(())
}

/// What a forum's tags and defaults need: a forum, known tags, short names,
/// a reaction of its server's own emojis and an archive time Discord offers.
pub(super) fn forum_edit(
    server: &Server,
    channel: &Channel,
    edit: &ChannelEdit,
) -> Result<(), LoginError> {
    if edit.tags.is_none() && edit.forum.is_none() {
        return Ok(());
    }
    if channel.kind != ChannelKind::Forum {
        return Err(LoginError::new(es("Solo un foro tiene etiquetas.")));
    }
    if let Some(tags) = &edit.tags {
        let bad = tags.len() > MAX_TAGS
            || tags.iter().any(|tag| {
                let name = tag.name.trim();
                !(1..=MAX_TAG_NAME).contains(&name.chars().count())
                    || name.chars().any(char::is_control)
                    || (tag.id.0 != 0 && !channel.tags.iter().any(|known| known.id == tag.id))
                    || tag
                        .emoji
                        .as_deref()
                        .is_some_and(|emoji| !short_emoji(emoji))
            });
        if bad {
            return Err(LoginError::new(es(
                "Un foro tiene hasta 20 etiquetas, cada una con un nombre corto.",
            )));
        }
    }
    if let Some(reaction) = edit
        .forum
        .as_ref()
        .and_then(|forum| forum.reaction.as_deref())
    {
        if !short_emoji(reaction) {
            return Err(LoginError::new(es("La reacción es un solo emoji.")));
        }
    }
    // Server emoji, archive and tag filter (2026-10-19).
    if let Some(forum) = &edit.forum {
        if forum
            .reaction_emoji
            .is_some_and(|id| !server.emojis.iter().any(|emoji| emoji.id == id))
        {
            return Err(LoginError::new(es("Ese emoji no es de este servidor.")));
        }
        if forum
            .archive_after
            .is_some_and(|after| !ARCHIVE_AFTER.contains(&after))
        {
            return Err(wire::invalid());
        }
    }
    Ok(())
}

fn short_emoji(emoji: &str) -> bool {
    (1..=8).contains(&emoji.chars().count()) && !emoji.chars().any(char::is_control)
}

/// Channels moved at once: each of this server once, under one of its
/// categories, where the account manages channels.
pub(super) fn moves(standing: &Standing, moves: &[ChannelMove]) -> Result<(), LoginError> {
    standing.needs(Permissions::MANAGE_CHANNELS)?;
    if moves.is_empty() || moves.len() > 500 {
        return Err(wire::invalid());
    }
    let mut seen = std::collections::HashSet::new();
    for step in moves {
        let found = standing.channel(step.channel)?;
        if !seen.insert(step.channel) || step.position < 0 {
            return Err(wire::invalid());
        }
        if let Some(parent) = step.parent {
            let parent = standing.channel(parent)?;
            if found.kind == ChannelKind::Category || parent.kind != ChannelKind::Category {
                return Err(LoginError::new(es(
                    "Un canal solo va dentro de una categoría.",
                )));
            }
            holds(parent, Permissions::MANAGE_CHANNELS)?;
            // Taking the category's permissions, as the official client asks:
            // whoever may change them on both.
            if step.lock_permissions {
                holds(found, Permissions::MANAGE_ROLES)?;
                holds(parent, Permissions::MANAGE_ROLES)?;
            }
        } else if step.lock_permissions {
            return Err(wire::invalid());
        }
    }
    Ok(())
}

/// A server's options from a guild object or a profile, as `get` finds
/// its keys; a key left out, or not as documented, keeps what `known` says.
pub(crate) fn options<'a>(
    get: impl Fn(&str) -> Option<&'a Value>,
    known: &ServerOptions,
) -> ServerOptions {
    let mut options = known.clone();
    let id = |value: &Value| wire::id(value).ok().map(rune_model::ChannelId);
    if let Some(value) = get("system_channel_id") {
        options.system_channel = id(value);
    }
    if let Some(flags) = get("system_channel_flags").and_then(Value::as_u64) {
        options.system_messages = SystemMessages {
            joins: flags & 1 == 0,
            boosts: flags & 1 << 1 == 0,
            tips: flags & 1 << 2 == 0,
            join_replies: flags & 1 << 3 == 0,
        };
    }
    if let Some(level) = get("default_message_notifications").and_then(Value::as_u64) {
        options.mentions_only = level == 1;
    }
    if let Some(value) = get("afk_channel_id") {
        options.afk_channel = id(value);
    }
    if let Some(secs) = get("afk_timeout")
        .and_then(Value::as_u64)
        .and_then(|secs| u32::try_from(secs).ok())
    {
        options.afk_timeout = secs;
    }
    if let Some(level) = get("verification_level")
        .and_then(Value::as_u64)
        .and_then(|level| Verification::ALL.get(level as usize))
    {
        options.verification = *level;
    }
    if let Some(level) = get("explicit_content_filter")
        .and_then(Value::as_u64)
        .and_then(|level| MediaScan::ALL.get(level as usize))
    {
        options.media_scan = *level;
    }
    if let Some(features) = get("features").and_then(Value::as_array) {
        let has = |name: &str| features.iter().any(|feature| feature == name);
        options.invites_paused = has(INVITES_DISABLED);
        options.role_icons = has(ROLE_ICONS);
        options.role_gradients = has(ENHANCED_ROLE_COLORS);
        options.community = has(COMMUNITY);
        // Rules and onboarding (2026-10-18).
        options.rules_screening = has(crate::onboarding::SCREENING);
        options.onboarding = has(crate::onboarding::ONBOARDING);
        // Icon, banner and community (2026-10-19).
        options.animated_icon = has(ANIMATED_ICON);
        options.banner_allowed = has(BANNER);
        options.animated_banner = has(ANIMATED_BANNER);
    }
    if let Some(traits) = get("traits").and_then(traits) {
        options.traits = Some(traits);
    }
    // Icon, banner and community (2026-10-19).
    if let (Some(hash), Some(server)) = (get("banner"), get("id").and_then(|id| wire::id(id).ok()))
    {
        options.banner = wire::image("banners", server, hash);
    }
    if let Some(value) = get("rules_channel_id") {
        options.rules_channel = id(value);
    }
    if let Some(value) = get("public_updates_channel_id") {
        options.updates_channel = id(value);
    }
    options
}

/// A profile's traits in their order; `None` when one is not as documented.
fn traits(value: &Value) -> Option<Vec<ServerTrait>> {
    let mut rows: Vec<&Value> = value.as_array()?.iter().collect();
    if rows.len() > MAX_TRAITS {
        return None;
    }
    rows.sort_by_key(|row| row["position"].as_u64().unwrap_or(0));
    rows.into_iter()
        .map(|row| {
            // One of the server's own emojis (2026-10-19), named by its id.
            let emoji_id = match &row["emoji_id"] {
                Value::Null => None,
                id => Some(EmojiId(wire::id(id).ok()?)),
            };
            Some(ServerTrait {
                label: row["label"].as_str()?.to_owned(),
                emoji: row["emoji_name"]
                    .as_str()
                    .filter(|emoji| !emoji.is_empty())
                    .map(str::to_owned),
                emoji_id,
                emoji_animated: emoji_id.is_some() && row["emoji_animated"] == true,
            })
        })
        .collect()
}

/// A forum's default reaction: its Unicode emoji, or none for one of the
/// server's own, which [`default_reaction_emoji`] reads.
pub(crate) fn default_reaction(value: &Value) -> Option<String> {
    (value["emoji_id"].is_null())
        .then(|| value["emoji_name"].as_str())
        .flatten()
        .filter(|emoji| short_emoji(emoji))
        .map(str::to_owned)
}

/// A forum's default reaction when it is one of the server's own emojis.
pub(crate) fn default_reaction_emoji(value: &Value) -> Option<EmojiId> {
    wire::id(&value["emoji_id"]).ok().map(EmojiId)
}

/// The profile's part of `edit` beyond its name, description and color:
/// its picture as a data URI and its traits, as Serein writes them.
pub(crate) fn profile_fields(body: &mut Value, edit: &ServerEdit) {
    if let Some(icon) = &edit.icon {
        body["icon"] = icon.as_ref().map_or(Value::Null, |image| {
            json!(format!(
                "data:{};base64,{}",
                image.mime(),
                STANDARD.encode(image.bytes())
            ))
        });
    }
    if let Some(traits) = &edit.traits {
        body["traits"] = traits
            .iter()
            .enumerate()
            .map(|(position, found)| {
                json!({"label": found.label.trim(), "emoji_name": found.emoji,
                    "emoji_id": found.emoji_id.map(|id| id.0.to_string()),
                    "emoji_animated": found.emoji_animated, "position": position})
            })
            .collect();
    }
}

/// Whether `edit` changes the guild itself, not only its profile.
pub(crate) fn touches_guild(edit: &ServerEdit) -> bool {
    edit.system_channel.is_some()
        || edit.system_messages.is_some()
        || edit.mentions_only.is_some()
        || edit.afk_channel.is_some()
        || edit.afk_timeout.is_some()
        || edit.verification.is_some()
        || edit.media_scan.is_some()
        || edit.invites_paused.is_some()
        // Icon, banner and community (2026-10-19).
        || edit.banner.is_some()
        || edit.community.is_some()
        || edit.rules_channel.is_some()
        || edit.updates_channel.is_some()
}

/// Whether the guild must be read first: flags and features are written
/// whole, so the ones RUNE does not show must come fresh.
pub(crate) fn needs_fresh_guild(edit: &ServerEdit) -> bool {
    edit.system_messages.is_some() || edit.invites_paused.is_some() || edit.community.is_some()
}

/// The guild's part of `edit`, over `fresh` (the guild as read just now)
/// for its flags and features.
pub(crate) fn guild_body(edit: &ServerEdit, fresh: Option<&Value>) -> Result<Value, LoginError> {
    let mut body = Map::new();
    let channel = |channel: Option<rune_model::ChannelId>| {
        channel.map_or(Value::Null, |id| json!(id.to_string()))
    };
    if let Some(system) = edit.system_channel {
        body.insert("system_channel_id".into(), channel(system));
    }
    if let Some(messages) = edit.system_messages {
        let raw = fresh
            .and_then(|guild| guild["system_channel_flags"].as_u64())
            .ok_or_else(wire::invalid)?;
        let shown = u64::from(!messages.joins)
            | u64::from(!messages.boosts) << 1
            | u64::from(!messages.tips) << 2
            | u64::from(!messages.join_replies) << 3;
        body.insert(
            "system_channel_flags".into(),
            json!((raw & !SYSTEM_MASK) | shown),
        );
    }
    if let Some(mentions) = edit.mentions_only {
        body.insert(
            "default_message_notifications".into(),
            json!(u8::from(mentions)),
        );
    }
    if let Some(afk) = edit.afk_channel {
        body.insert("afk_channel_id".into(), channel(afk));
    }
    if let Some(secs) = edit.afk_timeout {
        body.insert("afk_timeout".into(), json!(secs));
    }
    if let Some(level) = edit.verification {
        let index = Verification::ALL.iter().position(|known| *known == level);
        body.insert("verification_level".into(), json!(index));
    }
    if let Some(scan) = edit.media_scan {
        let index = MediaScan::ALL.iter().position(|known| *known == scan);
        body.insert("explicit_content_filter".into(), json!(index));
    }
    // Both write the features whole: each changes only its own.
    let toggles = [
        (INVITES_DISABLED, edit.invites_paused),
        (COMMUNITY, edit.community),
    ];
    if toggles.iter().any(|(_, on)| on.is_some()) {
        let features = fresh
            .and_then(|guild| guild["features"].as_array())
            .filter(|features| features.len() <= 256)
            .ok_or_else(wire::invalid)?;
        let mut kept: Vec<Value> = features.clone();
        for (feature, on) in toggles {
            if let Some(on) = on {
                kept.retain(|found| found != feature);
                if on {
                    kept.push(json!(feature));
                }
            }
        }
        body.insert("features".into(), Value::Array(kept));
    }
    // Icon, banner and community (2026-10-19).
    if let Some(banner) = &edit.banner {
        body.insert(
            "banner".into(),
            banner.as_ref().map_or(Value::Null, |image| {
                json!(format!(
                    "data:{};base64,{}",
                    image.mime(),
                    STANDARD.encode(image.bytes())
                ))
            }),
        );
    }
    if let Some(rules) = edit.rules_channel {
        body.insert("rules_channel_id".into(), channel(rules));
    }
    if let Some(updates) = edit.updates_channel {
        body.insert("public_updates_channel_id".into(), channel(updates));
    }
    Ok(Value::Object(body))
}

/// The keys of a guild answer that carry its options, for the session.
pub(crate) fn option_keys(value: &Value) -> Value {
    let mut update = Map::new();
    for key in [
        "id",
        "system_channel_id",
        "system_channel_flags",
        "default_message_notifications",
        "afk_channel_id",
        "afk_timeout",
        "verification_level",
        "explicit_content_filter",
        "features",
        // Icon, banner and community (2026-10-19).
        "banner",
        "rules_channel_id",
        "public_updates_channel_id",
    ] {
        if let Some(found) = value.get(key) {
            update.insert(key.into(), found.clone());
        }
    }
    Value::Object(update)
}

/// A role's picture, emoji and gradient in `body`; with a gradient the
/// colors travel together, `current` being the role's color when `edit`
/// does not change it.
pub(crate) fn role_fields(body: &mut Value, edit: &RoleEdit, current: Option<Rgb>) {
    match &edit.icon {
        Some(Some(RoleIcon::Image(image))) => {
            body["icon"] = json!(format!(
                "data:image/png;base64,{}",
                STANDARD.encode(image.bytes())
            ));
            body["unicode_emoji"] = Value::Null;
        }
        Some(Some(RoleIcon::Emoji(emoji))) => {
            body["unicode_emoji"] = json!(emoji);
            body["icon"] = Value::Null;
        }
        Some(None) => {
            body["icon"] = Value::Null;
            body["unicode_emoji"] = Value::Null;
        }
        None => {}
    }
    if let Some(gradient) = &edit.gradient {
        let primary = edit.color.unwrap_or(current).map_or(0, |color| color.0);
        let stop = |index: usize| gradient.get(index).map(|color| color.0);
        if let Some(fields) = body.as_object_mut() {
            fields.remove("color");
        }
        body["colors"] = json!({"primary_color": primary,
            "secondary_color": stop(0), "tertiary_color": stop(1)});
    }
}

/// A forum's tags and defaults in `body`; `flags` is the forum's as read
/// just now, its other flags kept.
pub(crate) fn forum_fields(body: &mut Value, edit: &ChannelEdit, flags: Option<u64>) {
    if let Some(tags) = &edit.tags {
        body["available_tags"] = tags
            .iter()
            .map(|tag| {
                let mut row = json!({"name": tag.name.trim(), "moderated": tag.moderated,
                    "emoji_id": null, "emoji_name": tag.emoji});
                // A tag without an id is made; one left out is deleted.
                if tag.id.0 != 0 {
                    row["id"] = json!(tag.id.0.to_string());
                }
                row
            })
            .collect();
    }
    if let Some(forum) = &edit.forum {
        body["default_forum_layout"] = json!(if forum.gallery { 2 } else { 1 });
        body["default_sort_order"] = json!(u8::from(forum.by_creation));
        body["default_reaction_emoji"] = match (forum.reaction_emoji, &forum.reaction) {
            // One of the server's own (2026-10-19): by its id alone, as documented.
            (Some(id), _) => json!({"emoji_id": id.0.to_string(), "emoji_name": null}),
            (None, Some(emoji)) => json!({"emoji_id": null, "emoji_name": emoji}),
            (None, None) => Value::Null,
        };
        // Archive time and tag filter (2026-10-19), as Serein writes them.
        if let Some(after) = forum.archive_after {
            body["default_auto_archive_duration"] = json!(after);
        }
        body["default_tag_setting"] = json!(if forum.match_all_tags {
            "match_all"
        } else {
            "match_some"
        });
        if let Some(flags) = flags {
            let require = if forum.require_tag { REQUIRE_TAG } else { 0 };
            body["flags"] = json!((flags & !REQUIRE_TAG) | require);
        }
    }
}

/// Channels moved at once, as `PATCH /guilds/{g}/channels` takes them.
pub(crate) fn moves_body(moves: &[ChannelMove]) -> Value {
    moves
        .iter()
        .map(|step| {
            let mut body = json!({"id": step.channel.to_string(), "position": step.position,
                "parent_id": step.parent.map(|parent| parent.to_string())});
            if step.lock_permissions {
                body["lock_permissions"] = json!(true);
            }
            body
        })
        .collect()
}

/// What the session keeps of a channel moved, as the Gateway would say it.
pub(crate) fn moved(server: rune_model::ServerId, step: &ChannelMove) -> Value {
    json!({"id": step.channel.to_string(), "guild_id": server.to_string(),
        "position": step.position, "parent_id": step.parent.map(|parent| parent.to_string())})
}

/// How a member search asks for `query`, at `now` (milliseconds).
pub(crate) fn member_query(body: &mut Value, query: MemberQuery, now: i64) {
    body["sort"] = json!(match query.order {
        MemberOrder::NewestMembers => 1,
        MemberOrder::OldestMembers => 2,
        MemberOrder::NewestAccounts => 3,
        MemberOrder::OldestAccounts => 4,
    });
    if query.recent {
        body["and_query"] =
            json!({"guild_joined_at": {"range": {"gte": now.saturating_sub(RECENT_MS)}}});
    }
}

/// How a member found by the search joined, as Serein names
/// `join_source_type`; an invitation's code when it says.
pub(crate) fn join_method(row: &Value) -> Option<JoinMethod> {
    let code = row["source_invite_code"]
        .as_str()
        .filter(|code| crate::admin::valid_code(code))
        .map(str::to_owned);
    if code.is_some() {
        return Some(JoinMethod::Invite(code));
    }
    Some(match row["join_source_type"].as_u64()? {
        1 => JoinMethod::Bot,
        2 => JoinMethod::Integration,
        3 => JoinMethod::Discovery,
        4 => JoinMethod::Hub,
        5 => JoinMethod::Invite(None),
        6 => JoinMethod::VanityLink,
        7 => JoinMethod::App,
        _ => JoinMethod::Other,
    })
}

/// How many members have each role, from `GET /guilds/{g}/roles/member-counts`.
pub(crate) fn role_counts(value: &Value) -> Result<Vec<(rune_model::RoleId, u32)>, LoginError> {
    let counts = value
        .as_object()
        .filter(|counts| counts.len() <= 1000)
        .ok_or_else(wire::invalid)?;
    counts
        .iter()
        .map(|(role, count)| {
            let role = role.parse::<u64>().ok().filter(|role| *role != 0);
            let count = count.as_u64().and_then(|count| u32::try_from(count).ok());
            role.zip(count)
                .map(|(role, count)| (rune_model::RoleId(role), count))
                .ok_or_else(wire::invalid)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use rune_model::{ForumDefaults, ForumTag, TagId};
    use rune_provider::ExpressionImage;

    use super::*;

    #[test]
    fn options_read_what_the_guild_says_and_keep_the_rest() {
        let guild = json!({"system_channel_id": "30", "system_channel_flags": 0b0101,
            "default_message_notifications": 1, "afk_channel_id": null, "afk_timeout": 900,
            "verification_level": 2, "explicit_content_filter": 9,
            "features": ["COMMUNITY", "INVITES_DISABLED", "ROLE_ICONS", "FUTURE"]});
        let known = ServerOptions::default();
        let read = options(|key| guild.get(key), &known);
        assert_eq!(read.system_channel, Some(rune_model::ChannelId(30)));
        assert!(!read.system_messages.joins && read.system_messages.boosts);
        assert!(!read.system_messages.tips && read.system_messages.join_replies);
        assert!(read.mentions_only && read.afk_channel.is_none());
        assert_eq!(read.afk_timeout, 900);
        assert_eq!(read.verification, Verification::Medium);
        // Not a documented level: what was known stays.
        assert_eq!(read.media_scan, known.media_scan);
        assert!(read.invites_paused && read.role_icons && read.community);
        assert!(!read.role_gradients && read.traits.is_none());
        let profile = json!({"traits": [
            {"label": "Arte", "emoji_name": "🎨", "emoji_id": null, "position": 1},
            {"label": "Rust", "emoji_name": null, "emoji_id": null, "position": 0}]});
        let read = options(|key| profile.get(key), &read);
        let traits = read.traits.unwrap();
        assert_eq!(
            (traits[0].label.as_str(), traits[1].label.as_str()),
            ("Rust", "Arte")
        );
        // A server's own emoji in a trait is read by its id (2026-10-19).
        let custom = json!({"traits": [{"label": "X", "emoji_name": "x", "emoji_id": "5",
            "emoji_animated": true}]});
        let traits = options(|key| custom.get(key), &known).traits.unwrap();
        assert_eq!(traits[0].emoji_id, Some(EmojiId(5)));
        assert!(traits[0].emoji_animated && traits[0].emoji.as_deref() == Some("x"));
        let bad = json!({"traits": [{"label": "X", "emoji_name": "x", "emoji_id": "no"}]});
        assert!(options(|key| bad.get(key), &known).traits.is_none());
    }

    #[test]
    fn boosts_banner_and_community_channels_are_read_from_the_guild() {
        let hash = "0123456789abcdef0123456789abcdef";
        let guild = json!({"id": "10", "banner": hash, "rules_channel_id": "31",
            "public_updates_channel_id": "32", "features": ["ANIMATED_ICON", "BANNER"]});
        let read = options(|key| guild.get(key), &ServerOptions::default());
        assert!(read.animated_icon && read.banner_allowed && !read.animated_banner);
        assert_eq!(
            read.banner.as_ref().unwrap().url,
            format!("https://cdn.discordapp.com/banners/10/{hash}.png?size=512")
        );
        assert_eq!(read.rules_channel, Some(rune_model::ChannelId(31)));
        assert_eq!(read.updates_channel, Some(rune_model::ChannelId(32)));
        // Taken away, they are none; the answer carries the keys.
        let cleared = json!({"id": "10", "banner": null, "rules_channel_id": null});
        let read = options(|key| cleared.get(key), &read);
        assert!(read.banner.is_none() && read.rules_channel.is_none());
        assert!(read.updates_channel.is_some());
        assert_eq!(option_keys(&cleared), cleared);
    }

    #[test]
    fn becoming_a_community_writes_its_feature_channels_and_banner() {
        let mut gif = b"GIF89a".to_vec();
        gif.extend(960u16.to_le_bytes());
        gif.extend(540u16.to_le_bytes());
        let banner = rune_provider::ProfileImage::new(Bytes::from(gif)).unwrap();
        let edit = ServerEdit {
            community: Some(true),
            rules_channel: Some(Some(rune_model::ChannelId(31))),
            updates_channel: Some(Some(rune_model::ChannelId(32))),
            verification: Some(Verification::Low),
            media_scan: Some(MediaScan::Everyone),
            banner: Some(Some(banner)),
            ..ServerEdit::default()
        };
        assert!(touches_guild(&edit) && needs_fresh_guild(&edit));
        let fresh = json!({"features": ["NEWS", "INVITES_DISABLED"]});
        let body = guild_body(&edit, Some(&fresh)).unwrap();
        assert_eq!(
            body["features"],
            json!(["NEWS", "INVITES_DISABLED", "COMMUNITY"])
        );
        assert_eq!(body["rules_channel_id"], "31");
        assert_eq!(body["public_updates_channel_id"], "32");
        assert_eq!(body["verification_level"], 1);
        assert_eq!(body["explicit_content_filter"], 2);
        assert!(body["banner"]
            .as_str()
            .unwrap()
            .starts_with("data:image/gif;base64,"));
        // Stopping keeps the rest of the features; a banner taken away is null.
        let body = guild_body(
            &ServerEdit {
                community: Some(false),
                banner: Some(None),
                ..ServerEdit::default()
            },
            Some(&json!({"features": ["COMMUNITY", "NEWS"]})),
        )
        .unwrap();
        assert_eq!(body, json!({"features": ["NEWS"], "banner": null}));
        // A trait with one of the server's own emojis says so.
        let mut profile = json!({});
        profile_fields(
            &mut profile,
            &ServerEdit {
                traits: Some(vec![ServerTrait {
                    label: "Propio".into(),
                    emoji: Some("rune".into()),
                    emoji_id: Some(EmojiId(5)),
                    emoji_animated: true,
                }]),
                ..ServerEdit::default()
            },
        );
        assert_eq!(
            profile,
            json!({"traits": [{"label": "Propio", "emoji_name": "rune", "emoji_id": "5",
                "emoji_animated": true, "position": 0}]})
        );
    }

    #[test]
    fn the_guild_s_flags_and_features_travel_whole() {
        let fresh = json!({"system_channel_flags": 0b1_0000_0001,
            "features": ["COMMUNITY", "NEWS"]});
        let edit = ServerEdit {
            system_messages: Some(SystemMessages {
                joins: true,
                boosts: false,
                tips: true,
                join_replies: true,
            }),
            invites_paused: Some(true),
            verification: Some(Verification::High),
            afk_channel: Some(None),
            ..ServerEdit::default()
        };
        let body = guild_body(&edit, Some(&fresh)).unwrap();
        assert_eq!(
            body,
            json!({"system_channel_flags": 0b1_0000_0010, "afk_channel_id": null,
                "verification_level": 3, "features": ["COMMUNITY", "NEWS", "INVITES_DISABLED"]})
        );
        // Without a fresh read nothing is guessed.
        assert!(guild_body(&edit, None).is_err());
        let resumed = guild_body(
            &ServerEdit {
                invites_paused: Some(false),
                ..ServerEdit::default()
            },
            Some(&json!({"features": ["INVITES_DISABLED", "NEWS"]})),
        )
        .unwrap();
        assert_eq!(resumed, json!({"features": ["NEWS"]}));
    }

    #[test]
    fn a_role_s_gradient_carries_its_color_and_an_emoji_drops_the_picture() {
        let mut body = json!({"color": 5});
        role_fields(
            &mut body,
            &RoleEdit {
                gradient: Some(vec![Rgb(0x00FF00)]),
                icon: Some(Some(RoleIcon::Emoji("🛡️".into()))),
                ..RoleEdit::default()
            },
            Some(Rgb(0x112233)),
        );
        assert_eq!(
            body,
            json!({"unicode_emoji": "🛡️", "icon": null, "colors": {"primary_color": 0x112233,
                "secondary_color": 0x00FF00, "tertiary_color": null}})
        );
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(64u32.to_be_bytes());
        png.extend(64u32.to_be_bytes());
        let image =
            ExpressionImage::png(rune_provider::ExpressionKind::Emoji, Bytes::from(png)).unwrap();
        let mut body = json!({});
        role_fields(
            &mut body,
            &RoleEdit {
                icon: Some(Some(RoleIcon::Image(image))),
                ..RoleEdit::default()
            },
            None,
        );
        assert!(body["icon"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,"));
        assert!(body["unicode_emoji"].is_null() && body.get("colors").is_none());
    }

    #[test]
    fn a_forum_s_tags_and_defaults_keep_its_other_flags() {
        let mut body = json!({});
        let edit = ChannelEdit {
            tags: Some(vec![
                ForumTag {
                    id: TagId(7),
                    name: " Ayuda ".into(),
                    emoji: None,
                    moderated: true,
                },
                ForumTag {
                    id: TagId(0),
                    name: "Nueva".into(),
                    emoji: Some("✨".into()),
                    moderated: false,
                },
            ]),
            forum: Some(ForumDefaults {
                gallery: true,
                by_creation: true,
                require_tag: true,
                reaction: Some("👍".into()),
                ..ForumDefaults::default()
            }),
            ..ChannelEdit::default()
        };
        forum_fields(&mut body, &edit, Some(1 << 1));
        assert_eq!(
            body,
            json!({"available_tags": [
                    {"id": "7", "name": "Ayuda", "moderated": true, "emoji_id": null, "emoji_name": null},
                    {"name": "Nueva", "moderated": false, "emoji_id": null, "emoji_name": "✨"}],
                "default_forum_layout": 2, "default_sort_order": 1,
                "default_reaction_emoji": {"emoji_id": null, "emoji_name": "👍"},
                "default_tag_setting": "match_some",
                "flags": (1 << 1) | REQUIRE_TAG})
        );
        assert_eq!(
            default_reaction(&json!({"emoji_id": "9", "emoji_name": "custom"})),
            None
        );
        // One of the server's own, an archive time and every tag (2026-10-19).
        let mut body = json!({});
        let edit = ChannelEdit {
            forum: Some(ForumDefaults {
                reaction: Some("👍".into()),
                reaction_emoji: Some(EmojiId(9)),
                archive_after: Some(10080),
                match_all_tags: true,
                ..ForumDefaults::default()
            }),
            ..ChannelEdit::default()
        };
        forum_fields(&mut body, &edit, None);
        assert_eq!(
            body["default_reaction_emoji"],
            json!({"emoji_id": "9", "emoji_name": null})
        );
        assert_eq!(body["default_auto_archive_duration"], 10080);
        assert_eq!(body["default_tag_setting"], "match_all");
        assert_eq!(
            default_reaction_emoji(&body["default_reaction_emoji"]),
            Some(EmojiId(9))
        );
    }

    #[test]
    fn a_member_search_asks_for_its_order_and_says_how_each_joined() {
        let mut body = json!({"limit": 25});
        member_query(
            &mut body,
            MemberQuery {
                order: MemberOrder::OldestAccounts,
                recent: true,
            },
            RECENT_MS + 5,
        );
        assert_eq!(
            body,
            json!({"limit": 25, "sort": 4,
                "and_query": {"guild_joined_at": {"range": {"gte": 5}}}})
        );
        assert_eq!(
            join_method(&json!({"join_source_type": 5, "source_invite_code": "abc"})),
            Some(JoinMethod::Invite(Some("abc".into())))
        );
        assert_eq!(
            join_method(&json!({"join_source_type": 6})),
            Some(JoinMethod::VanityLink)
        );
        assert_eq!(join_method(&json!({})), None);
        assert_eq!(role_counts(&json!({"11": 3, "12": 0})).unwrap().len(), 2);
        assert!(role_counts(&json!({"x": 1})).is_err());
    }
}
