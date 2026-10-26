//! Someone's full profile, read from the undocumented profile route that
//! Serein uses (owner-authorized 2026-10-16). Unofficial and not verified
//! against Discord: only the shapes of local fixtures are known to parse.

use rune_i18n::es;
use rune_model::{ProfileBanner, Rgb, ServerId, UserId, UserProfile, MAX_MUTUALS};
use rune_provider::{Command, Event, RequestId, Snapshot};
use serde_json::Value;

use crate::{wire, LoginError};

/// Whether the account may change its profile as `command` asks: in a
/// server, only one it is in and not restricted in. Refused here before
/// anything is sent.
pub(crate) fn edit_allowed(snapshot: &Snapshot, command: &Command) -> Result<(), LoginError> {
    let me = snapshot.me.as_ref().ok_or_else(wire::invalid)?.id;
    match command {
        Command::EditProfile(_) => Ok(()),
        Command::EditServerProfile { server, .. } => {
            let member = snapshot
                .members
                .iter()
                .find(|m| m.server == *server && m.user == me)
                .ok_or_else(|| LoginError::new(es("Ya no estás en ese servidor.")))?;
            match member.restrictions.at(crate::messages::wall_now()) {
                Some(_) => Err(LoginError::new(es(
                    "Tu cuenta está restringida en este servidor: no puedes cambiar tu perfil ahí.",
                ))),
                None => Ok(()),
            }
        }
        _ => Err(wire::invalid()),
    }
}

/// The profile answer is read up to this size.
pub(crate) const MAX_RESPONSE: usize = 256 * 1024;

/// The route of `user`'s profile, as seen in `server`. Mutual servers and
/// friends are asked for only about someone else.
pub(crate) fn path(user: UserId, server: Option<ServerId>, mutuals: bool) -> String {
    let mut path = format!(
        "/users/{user}/profile?with_mutual_guilds={mutuals}&with_mutual_friends={mutuals}&with_mutual_friends_count=false"
    );
    if let Some(server) = server {
        path.push_str(&format!("&guild_id={server}"));
    }
    path
}

/// Text the profile shows, cut to `chars`; empty is not reported.
pub(crate) fn text(value: &Value, chars: usize) -> Option<String> {
    let text = value.as_str()?.trim();
    (!text.is_empty()).then(|| {
        text.chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(chars)
            .collect()
    })
}

fn accent(value: &Value) -> Option<Rgb> {
    value
        .as_u64()
        .filter(|color| *color <= 0xFF_FFFF)
        .map(|color| Rgb(color as u32))
}

/// A banner field: missing is unknown, `null` is none, a valid hash is one.
fn banner(value: Option<&Value>, path: &str) -> ProfileBanner {
    match value {
        None => ProfileBanner::Unknown,
        Some(Value::Null) => ProfileBanner::Absent,
        Some(hash) => wire::cdn(path, true, hash)
            .map(ProfileBanner::Image)
            .unwrap_or_default(),
    }
}

/// The first of two places that reports a banner, else the second.
fn either(first: ProfileBanner, second: ProfileBanner) -> ProfileBanner {
    match first {
        ProfileBanner::Image(_) => first,
        _ if second != ProfileBanner::Unknown => second,
        _ => first,
    }
}

/// A list of the mutuals: `None` unless it is an array within bounds.
fn mutual(value: &Value) -> Option<&[Value]> {
    value
        .as_array()
        .filter(|list| list.len() <= MAX_MUTUALS)
        .map(Vec::as_slice)
}

/// The people the profile names, then the profile itself. The answer must
/// be about `user` and, inside `server`, about their membership there.
pub(crate) fn profile(
    value: &Value,
    request: RequestId,
    user: UserId,
    server: Option<ServerId>,
    mutuals: bool,
) -> Result<Vec<Event>, LoginError> {
    let account = &value["user"];
    if wire::id(&account["id"])? != user.0 {
        return Err(wire::invalid());
    }
    let global = &value["user_profile"];
    let mut profile = UserProfile {
        user,
        server,
        created: wire::created(user),
        about: text(&global["bio"], 1024).or_else(|| text(&account["bio"], 1024)),
        pronouns: text(&global["pronouns"], 64),
        banner: either(
            banner(global.get("banner"), &format!("banners/{user}")),
            banner(account.get("banner"), &format!("banners/{user}")),
        ),
        accent: accent(&global["accent_color"]).or_else(|| accent(&account["accent_color"])),
        // Presence, identity and expressions (P5).
        badges: crate::identity::badges(&value["badges"], &value["guild_badges"]),
        connections: crate::identity::connections(&value["connected_accounts"]),
        ..Default::default()
    };
    let member = &value["guild_member"];
    if let (Some(server), true) = (server, member.is_object()) {
        if !member["user"].is_null() && wire::id(&member["user"]["id"])? != user.0 {
            return Err(wire::invalid());
        }
        let scoped = &value["guild_member_profile"];
        if !scoped["guild_id"].is_null() && wire::id(&scoped["guild_id"])? != server.0 {
            return Err(wire::invalid());
        }
        let path = format!("guilds/{server}/users/{user}/banners");
        profile.nick = text(&member["nick"], 128);
        profile.server_about = text(&scoped["bio"], 1024).or_else(|| text(&member["bio"], 1024));
        profile.server_pronouns = text(&scoped["pronouns"], 64);
        profile.server_banner = either(
            banner(scoped.get("banner"), &path),
            banner(member.get("banner"), &path),
        );
        profile.joined = wire::timestamp(&member["joined_at"]).ok();
    }
    // The server's emoji the texts name, drawn in place (2026-10-17).
    profile.pictures = crate::emoji::pictures(
        [&profile.about, &profile.server_about]
            .into_iter()
            .flatten()
            .map(String::as_str),
    );
    let mut events = Vec::new();
    if mutuals {
        profile.mutual_servers = mutual(&value["mutual_guilds"]).and_then(|list| {
            list.iter()
                .map(|guild| wire::id(&guild["id"]).map(ServerId).ok())
                .collect()
        });
        // A friend who cannot be shown by name makes the list unknown, not shorter.
        let friends: Option<Vec<_>> = mutual(&value["mutual_friends"])
            .and_then(|list| list.iter().map(|friend| wire::user(friend).ok()).collect());
        if let Some(friends) = friends {
            profile.mutual_friends = Some(friends.iter().map(|friend| friend.id).collect());
            events.extend(friends.into_iter().map(Event::UserChanged));
        }
    }
    events.push(Event::ProfileLoaded { request, profile });
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HASH: &str = "0123456789abcdef0123456789abcdef";

    fn loaded(events: &[Event]) -> &UserProfile {
        match events.last() {
            Some(Event::ProfileLoaded { profile, .. }) => profile,
            _ => panic!("profile last"),
        }
    }

    #[test]
    fn the_route_asks_for_mutuals_only_about_someone_else() {
        assert_eq!(
            path(UserId(43), Some(ServerId(10)), true),
            "/users/43/profile?with_mutual_guilds=true&with_mutual_friends=true&with_mutual_friends_count=false&guild_id=10"
        );
        assert!(path(UserId(42), None, false).ends_with(
            "with_mutual_guilds=false&with_mutual_friends=false&with_mutual_friends_count=false"
        ));
    }

    #[test]
    fn a_full_answer_keeps_the_server_profile_apart_and_names_mutual_friends() {
        let value = json!({
            "user": {"id":"43","username":"peer","bio":"old bio","banner":null,"accent_color":255},
            "user_profile": {"bio":"  Hola  ","pronouns":"elle","banner":HASH,"accent_color":16711680},
            "guild_member": {"user":{"id":"43"},"nick":"Apodo","joined_at":"2024-03-12T10:00:00Z","roles":[]},
            "guild_member_profile": {"guild_id":"10","bio":"En este servidor","pronouns":null,"banner":null},
            "mutual_guilds": [{"id":"10","nick":null},{"id":"11"}],
            "mutual_friends": [{"id":"44","username":"friend"}]
        });
        let events = profile(&value, 7, UserId(43), Some(ServerId(10)), true).unwrap();
        assert!(matches!(&events[0], Event::UserChanged(user) if user.id == UserId(44)));
        let loaded = loaded(&events);
        assert_eq!(loaded.about.as_deref(), Some("Hola"));
        assert!(loaded.pictures.is_empty(), "no emoji named");
        assert_eq!(loaded.pronouns.as_deref(), Some("elle"));
        assert!(
            matches!(&loaded.banner, ProfileBanner::Image(image) if image.url.contains("banners/43/"))
        );
        assert_eq!(loaded.accent, Some(Rgb(0xFF0000)));
        assert_eq!(loaded.nick.as_deref(), Some("Apodo"));
        assert_eq!(loaded.server_about.as_deref(), Some("En este servidor"));
        assert_eq!(loaded.server_pronouns, None);
        assert_eq!(loaded.server_banner, ProfileBanner::Absent);
        assert!(
            loaded.joined.is_some() && loaded.created.is_none(),
            "43 >> 22 is 0"
        );
        assert_eq!(
            loaded.mutual_servers,
            Some(vec![ServerId(10), ServerId(11)])
        );
        assert_eq!(loaded.mutual_friends, Some(vec![UserId(44)]));
        assert!(loaded.answers(UserId(43), Some(ServerId(10))));
    }

    #[test]
    fn the_emoji_a_bio_names_come_as_pictures_the_servers_too() {
        let value = json!({
            "user": {"id":"43","username":"peer"},
            "user_profile": {"bio":"hola <a:Rana:1234567890123456789>\nchau 🌱"},
            "guild_member": {"user":{"id":"43"},"roles":[]},
            "guild_member_profile": {"guild_id":"10","bio":"<:rust:22> aquí"}
        });
        let events = profile(&value, 7, UserId(43), Some(ServerId(10)), true).unwrap();
        let loaded = loaded(&events);
        let tokens: Vec<&str> = loaded.pictures.iter().map(|p| p.token.as_str()).collect();
        assert_eq!(tokens, ["<a:Rana:1234567890123456789>", "<:rust:22>"]);
        assert!(loaded.answers(UserId(43), Some(ServerId(10))));
    }

    #[test]
    fn unknown_stays_apart_from_none_and_floods_are_unknown() {
        let value = json!({
            "user": {"id":"43","username":"peer"},
            "mutual_guilds": [],
            "mutual_friends": vec![json!({"id":"44","username":"f"}); MAX_MUTUALS + 1]
        });
        let events = profile(&value, 1, UserId(43), None, true).unwrap();
        let loaded = loaded(&events);
        assert_eq!(
            loaded.mutual_servers,
            Some(vec![]),
            "an empty list is none in common"
        );
        assert_eq!(loaded.mutual_friends, None, "too many is not reported");
        assert_eq!(loaded.banner, ProfileBanner::Unknown);
        assert_eq!(events.len(), 1);
        let missing = json!({"user": {"id":"43","username":"peer"},
            "mutual_friends": [{"id":"bad"}]});
        let loaded = profile(&missing, 1, UserId(43), None, true).unwrap();
        let loaded = self::loaded(&loaded);
        assert_eq!(loaded.mutual_servers, None, "missing is unknown");
        assert_eq!(loaded.mutual_friends, None, "a malformed friend is unknown");
        let own = profile(&value, 1, UserId(43), None, false).unwrap();
        assert_eq!(self::loaded(&own).mutual_servers, None, "not asked");
    }

    #[test]
    fn an_answer_about_someone_or_somewhere_else_is_refused_and_text_is_bounded() {
        let other = json!({"user": {"id":"44","username":"peer"}});
        assert!(profile(&other, 1, UserId(43), None, true).is_err());
        let elsewhere = json!({"user": {"id":"43","username":"peer"},
            "guild_member": {"user":{"id":"43"}}, "guild_member_profile": {"guild_id":"11"}});
        assert!(profile(&elsewhere, 1, UserId(43), Some(ServerId(10)), true).is_err());
        let someone = json!({"user": {"id":"43","username":"peer"},
            "guild_member": {"user":{"id":"44"}}});
        assert!(profile(&someone, 1, UserId(43), Some(ServerId(10)), true).is_err());
        let long = json!({"user": {"id":"43","username":"peer","bio":"x".repeat(5000)},
            "user_profile": {"pronouns":"p".repeat(500)}});
        let events = profile(&long, 1, UserId(43), None, true).unwrap();
        let loaded = loaded(&events);
        assert_eq!(loaded.about.as_ref().map(|t| t.chars().count()), Some(1024));
        assert_eq!(
            loaded.pronouns.as_ref().map(|t| t.chars().count()),
            Some(64)
        );
    }
}
