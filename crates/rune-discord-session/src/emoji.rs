//! Bounded translation of the documented guild emoji catalog. Artwork uses a
//! fixed CDN reference and is fetched separately, without session credentials.

use std::collections::HashSet;

use rune_model::{
    CustomEmoji, EmojiId, RemoteImage, RoleId, ServerId, TextPicture, MAX_TEXT_PICTURES,
};
use serde_json::Value;

use crate::{wire, LoginError};

const MAX_EMOJI: usize = 500;
const MAX_ROLES: usize = 512;

pub(crate) fn catalog(value: &Value, server: ServerId) -> Result<Vec<CustomEmoji>, LoginError> {
    if value.is_null() {
        return Ok(Vec::new());
    }
    let rows = value
        .as_array()
        .filter(|rows| rows.len() <= MAX_EMOJI)
        .ok_or_else(wire::invalid)?;
    let mut seen = HashSet::new();
    rows.iter()
        .map(|row| {
            let id = EmojiId(wire::id(&row["id"])?);
            if !seen.insert(id) {
                return Err(wire::invalid());
            }
            let name = row["name"]
                .as_str()
                .filter(|name| {
                    (2..=32).contains(&name.len())
                        && name
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                })
                .ok_or_else(wire::invalid)?;
            let flag = |key: &str, default| {
                row.get(key).map_or(Ok(default), |value| {
                    value.as_bool().ok_or_else(wire::invalid)
                })
            };
            let animated = flag("animated", false)?;
            let available = flag("available", true)?;
            let mut roles = Vec::new();
            let mut seen_roles = HashSet::new();
            if let Some(value) = row.get("roles") {
                for role in value
                    .as_array()
                    .filter(|roles| roles.len() <= MAX_ROLES)
                    .ok_or_else(wire::invalid)?
                {
                    let role = RoleId(wire::id(role)?);
                    if !seen_roles.insert(role) {
                        return Err(wire::invalid());
                    }
                    roles.push(role);
                }
            }
            Ok(CustomEmoji {
                id,
                server,
                name: name.to_owned(),
                animated,
                available,
                roles,
                insert: format!("<{}:{name}:{id}>", if animated { "a" } else { "" }),
                image: Some(picture(id.0)),
            })
        })
        .collect()
}

/// A server emoji's still picture, for the catalog and wherever a message
/// names one; an animated one shows its first frame, as in the picker.
pub(crate) fn picture(id: u64) -> RemoteImage {
    RemoteImage {
        url: format!("https://cdn.discordapp.com/emojis/{id}.webp?size=128"),
        kind: Default::default(),
    }
}

/// The server emoji `texts` name (`<:name:id>`, `<a:name:id>` when it
/// moves), each once, as pictures a profile draws in place of their token:
/// an animated one moves (`animated=true`), as in the official client's
/// bios. At most [`MAX_TEXT_PICTURES`]; a token that does not parse stays
/// text.
pub(crate) fn pictures<'a>(texts: impl IntoIterator<Item = &'a str>) -> Vec<TextPicture> {
    let mut found: Vec<TextPicture> = Vec::new();
    for text in texts {
        let mut rest = text;
        while let Some(start) = rest.find('<') {
            rest = &rest[start..];
            let Some(end) = rest[1..].find(['<', '>']).map(|end| end + 1) else {
                break;
            };
            if &rest[end..=end] == "<" {
                rest = &rest[end..];
                continue;
            }
            let token = &rest[..=end];
            rest = &rest[end + 1..];
            let (animated, inner) = match token[1..token.len() - 1].strip_prefix("a:") {
                Some(inner) => (true, inner),
                None => match token[1..token.len() - 1].strip_prefix(':') {
                    Some(inner) => (false, inner),
                    None => continue,
                },
            };
            let Some((name, digits)) = inner.split_once(':') else {
                continue;
            };
            let named = (2..=32).contains(&name.len())
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
            let Some(id) = digits
                .parse::<u64>()
                .ok()
                .filter(|id| named && *id != 0 && id.to_string() == digits)
            else {
                continue;
            };
            if found.iter().any(|known| known.token == token) {
                continue;
            }
            if found.len() >= MAX_TEXT_PICTURES {
                return found;
            }
            found.push(TextPicture {
                token: token.to_owned(),
                name: name.to_owned(),
                image: inline(id, animated),
            });
        }
    }
    found
}

/// A server emoji's picture where a text names it (a message, a bio, a
/// status): 48 px, what the official client asks for one in a line
/// (`size` 44 at a scale of 1); one that moves, moving (`animated=true`).
pub(crate) fn inline(id: u64, animated: bool) -> RemoteImage {
    sized(id, animated, 48)
}

/// The same picture for a message of emoji alone, drawn 48 px: 96, what
/// the official client asks for one at a scale of 1 (`jumbo: 96`, times
/// the screen's scale), so it stays sharp on a screen scaled up to 2.
/// RUNE keeps an emoji's pixels at 128 a side whatever it asks for, so a
/// larger size would add nothing.
pub(crate) fn jumbo(id: u64, animated: bool) -> RemoteImage {
    sized(id, animated, 96)
}

/// A server emoji's picture on an app's part, a button or a menu's option:
/// the catalog's size, moving when the emoji does (`animated=true`)
/// (limn-teclado-y-superficies).
pub(crate) fn part(id: u64, animated: bool) -> RemoteImage {
    sized(id, animated, 128)
}

fn sized(id: u64, animated: bool, size: u32) -> RemoteImage {
    RemoteImage {
        url: format!(
            "https://cdn.discordapp.com/emojis/{id}.webp?size={size}{}",
            if animated { "&animated=true" } else { "" }
        ),
        kind: Default::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_bio_names_its_emoji_once_each_and_only_well_formed_ones() {
        let bio =
            "hola <a:Rana_Saltarina:1234567890123456789>\n<a:Rana_Saltarina:1234567890123456789> \
                   y <:rust:22> <:x:5> <a:bad name:7> <:ok:0> <@123> <:ok:08> <<:in:9>";
        let found = pictures([bio, "<:rust:22>"]);
        let tokens: Vec<&str> = found.iter().map(|picture| picture.token.as_str()).collect();
        assert_eq!(
            tokens,
            [
                "<a:Rana_Saltarina:1234567890123456789>",
                "<:rust:22>",
                "<:in:9>"
            ]
        );
        assert_eq!(
            found[0].image.url,
            "https://cdn.discordapp.com/emojis/1234567890123456789.webp?size=48&animated=true"
        );
        assert_eq!(found[1].name, "rust");
        assert!(!found[1].image.url.contains("animated"));
        let many: String = (1..=MAX_TEXT_PICTURES as u64 + 5)
            .map(|id| format!("<:e{id}:{id}>"))
            .collect();
        assert_eq!(pictures([many.as_str()]).len(), MAX_TEXT_PICTURES);
    }

    #[test]
    fn catalog_preserves_identity_role_limits_and_static_artwork() {
        let emojis = catalog(
            &json!([{"id":"22","name":"hello","animated":true,
            "available":false,"roles":["12"]}]),
            ServerId(10),
        )
        .unwrap();
        let emoji = &emojis[0];
        assert_eq!(emoji.insert, "<a:hello:22>");
        assert_eq!(emoji.server, ServerId(10));
        assert_eq!(emoji.roles, vec![RoleId(12)]);
        assert!(!emoji.available);
        assert_eq!(
            emoji.image.as_ref().unwrap().url,
            "https://cdn.discordapp.com/emojis/22.webp?size=128"
        );
        assert!(catalog(&Value::Null, ServerId(10)).unwrap().is_empty());
    }

    #[test]
    fn malformed_or_duplicate_catalog_rows_fail_closed() {
        for row in [
            json!({"id":"0","name":"hello"}),
            json!({"id":"22","name":"../asset"}),
            json!({"id":"22","name":"hello","available":"true"}),
            json!({"id":"22","name":"hello","roles":["12","12"]}),
        ] {
            assert!(catalog(&json!([row]), ServerId(10)).is_err());
        }
        let row = json!({"id":"22","name":"hello"});
        assert!(catalog(&json!([row.clone(), row.clone()]), ServerId(10)).is_err());
        assert!(catalog(&json!(vec![row; MAX_EMOJI + 1]), ServerId(10)).is_err());
    }
}
