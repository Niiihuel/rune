//! Documented public GET only. This transport cannot access account credentials.

use std::time::Duration;

use rune_model::{ChannelId, RemoteImage, ServerId, Timestamp, UserId};
use rune_provider::{
    InvitationDestination as Destination, InvitationError as Error, InvitationPreview,
    InvitationTarget,
};
use serde_json::Value;
use url::Url;

const MAX_BODY: usize = 64 * 1024;
/// The invitation's flag of a guest's way into one voice channel (`IS_GUEST_INVITE`).
const GUEST: u64 = 1;

/// Where invitation codes live: each host and the path just before the code.
const INVITE_PAGES: [(&str, &str); 8] = [
    ("discord.gg", "/"),
    ("www.discord.gg", "/"),
    ("discord.com", "/invite/"),
    ("www.discord.com", "/invite/"),
    ("ptb.discord.com", "/invite/"),
    ("canary.discord.com", "/invite/"),
    ("discordapp.com", "/invite/"),
    ("www.discordapp.com", "/invite/"),
];

pub(crate) fn target(raw: &str) -> Option<InvitationTarget> {
    if raw.len() > 2048 || raw.chars().any(char::is_control) {
        return None;
    }
    let url = Url::parse(raw).ok()?;
    // Only a bare https address: a user, password or port could point the
    // same text somewhere else.
    let bare = url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none();
    let host = url.host_str().filter(|_| bare)?;
    let (_, before) = INVITE_PAGES.iter().find(|(known, _)| *known == host)?;
    let code = url.path().trim_end_matches('/').strip_prefix(before)?;
    let well_formed = (1..=100).contains(&code.len())
        && code
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte));
    if !well_formed {
        return None;
    }
    // Scheduled events (eventos-programados): `?event={id}` names one of
    // the server's events, as the official client reads it.
    let event = url
        .query_pairs()
        .find(|(name, _)| name == "event")
        .and_then(|(_, id)| {
            (id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit()))
                .then(|| id.parse::<u64>().ok())
                .flatten()
        })
        .filter(|id| *id != 0);
    Some(InvitationTarget {
        key: code.into(),
        // The public page where a person can accept it in a browser.
        url: match event {
            Some(event) => format!("https://discord.com/invite/{code}?event={event}"),
            None => format!("https://discord.com/invite/{code}"),
        },
    })
}

/// The event an invitation's page names, if it names one.
fn named_event(invitation: &InvitationTarget) -> Option<u64> {
    invitation.url.split_once("?event=")?.1.parse().ok()
}

pub(crate) async fn inspect(invitation: InvitationTarget) -> Result<InvitationPreview, Error> {
    fetch(invitation, "https://discord.com/api/v10").await
}

async fn fetch(invitation: InvitationTarget, base: &str) -> Result<InvitationPreview, Error> {
    if target(&invitation.url).as_ref() != Some(&invitation) {
        return Err(Error::Invalid);
    }
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(12))
        .build()
        .map_err(|_| Error::Unavailable)?;
    let mut query = vec![("with_counts", "true".to_owned())];
    // Scheduled events (eventos-programados): the event it names comes along.
    if let Some(event) = named_event(&invitation) {
        query.push(("guild_scheduled_event_id", event.to_string()));
    }
    let mut response = client
        .get(format!("{base}/invites/{}", invitation.key))
        .headers(crate::browser::api_headers(&reqwest::Method::GET))
        .query(&query)
        .send()
        .await
        .map_err(|_| Error::Unavailable)?;
    match response.status().as_u16() {
        200 => {}
        404 => return Err(Error::Invalid),
        429 => {
            let retry_after_secs = response
                .headers()
                .get("retry-after")
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<f64>().ok())
                .filter(|value| value.is_finite() && *value >= 0.0)
                .map(|value| value.ceil().max(1.0) as u64)
                .unwrap_or(60);
            return Err(Error::RateLimited { retry_after_secs });
        }
        _ => return Err(Error::Unavailable),
    }
    if response
        .content_length()
        .is_some_and(|bytes| bytes > MAX_BODY as u64)
    {
        return Err(Error::Unavailable);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Error::Unavailable)? {
        if body.len().saturating_add(chunk.len()) > MAX_BODY {
            return Err(Error::Unavailable);
        }
        body.extend_from_slice(&chunk);
    }
    let mut preview = decode(&invitation.key, &body)?;
    // Only the event it asked for, of the server it leads to.
    if preview
        .event
        .as_ref()
        .is_some_and(|event| Some(event.id.0) != named_event(&invitation))
    {
        preview.event = None;
    }
    Ok(preview)
}

fn decode(code: &str, bytes: &[u8]) -> Result<InvitationPreview, Error> {
    let data: Value = serde_json::from_slice(bytes).map_err(|_| Error::Unavailable)?;
    if data["code"].as_str() != Some(code) {
        return Err(Error::Invalid);
    }
    let destination = destination(&data).ok_or(Error::Invalid)?;
    let picture = |path: String, hash: &Value| {
        hash.as_str()
            .filter(|hash| {
                let plain = hash.strip_prefix("a_").unwrap_or(hash);
                plain.len() == 32 && plain.bytes().all(|c| c.is_ascii_hexdigit())
            })
            .map(|hash| RemoteImage {
                url: format!("https://cdn.discordapp.com/{path}/{hash}.png?size=128"),
                kind: Default::default(),
            })
    };
    let (name, description, image) = match destination {
        Destination::Server(id) | Destination::Guest { server: id, .. } => {
            let guild = &data["guild"];
            (
                text(&guild["name"], 400).ok_or(Error::Unavailable)?,
                description(&guild["description"])?,
                picture(format!("icons/{id}"), &guild["icon"]),
            )
        }
        // A group without a name goes by its people, as the reference
        // client shows it; with none known, the interface names it.
        Destination::Group(id) => {
            let channel = &data["channel"];
            let people = || {
                let names: Vec<String> = channel["recipients"]
                    .as_array()?
                    .iter()
                    .filter_map(|person| text(&person["username"], 32))
                    .take(10)
                    .collect();
                (!names.is_empty()).then(|| names.join(", "))
            };
            (
                text(&channel["name"], 100)
                    .or_else(people)
                    .unwrap_or_default(),
                None,
                picture(format!("channel-icons/{id}"), &channel["icon"]),
            )
        }
        Destination::Friend(id) => {
            let inviter = &data["inviter"];
            (
                text(&inviter["global_name"], 100)
                    .or_else(|| text(&inviter["username"], 32))
                    .ok_or(Error::Unavailable)?,
                None,
                picture(format!("avatars/{id}"), &inviter["avatar"]),
            )
        }
    };
    let count = |field: &str| {
        if data[field].is_null() {
            Ok(None)
        } else {
            data[field].as_u64().map(Some).ok_or(Error::Unavailable)
        }
    };
    let expires_at = expiration(&data)?;
    Ok(InvitationPreview {
        destination,
        name,
        description,
        image,
        members: count("approximate_member_count")?,
        online: count("approximate_presence_count")?,
        expires_at,
        // A malformed or unknown event leaves the server's invitation as it is.
        event: destination
            .server()
            .filter(|_| data["guild_scheduled_event"].is_object())
            .and_then(|server| {
                crate::events::event(&data["guild_scheduled_event"], Some(server))
                    .ok()
                    .flatten()
            }),
    })
}

/// Where an invitation leads, by its type and the guest flag: a server
/// (0), a group (1) or a friendship (2). Other kinds, a guest flag outside a
/// server, or ids that are not ids, lead nowhere RUNE can take the account.
pub(crate) fn destination(data: &Value) -> Option<Destination> {
    let id = |value: &Value| crate::wire::id(value).ok();
    let flags = match &data["flags"] {
        Value::Null => 0,
        flags => flags.as_u64()?,
    };
    let guest = flags & GUEST != 0;
    Some(match (data["type"].as_u64()?, guest) {
        (0, false) => Destination::Server(ServerId(id(&data["guild"]["id"])?)),
        (0, true) => Destination::Guest {
            server: ServerId(id(&data["guild"]["id"])?),
            channel: ChannelId(id(&data["channel"]["id"])?),
        },
        (1, false) => Destination::Group(ChannelId(id(&data["channel"]["id"])?)),
        (2, false) => Destination::Friend(UserId(id(&data["inviter"]["id"])?)),
        _ => return None,
    })
}

/// A short single-line text of at most `max` bytes.
fn text(value: &Value, max: usize) -> Option<String> {
    value
        .as_str()
        .filter(|text| !text.is_empty() && text.len() <= max && !text.chars().any(char::is_control))
        .map(str::to_owned)
}

/// A server's description: absent, or text that may span lines.
fn description(value: &Value) -> Result<Option<String>, Error> {
    Ok(if value.is_null() {
        None
    } else {
        let description = value
            .as_str()
            .filter(|value| {
                value.len() <= 2048
                    && !value
                        .chars()
                        .any(|c| c.is_control() && c != '\n' && c != '\t')
            })
            .ok_or(Error::Unavailable)?;
        (!description.is_empty()).then(|| description.to_owned())
    })
}

/// Shared by display decoding and acceptance preflight: absent is unknown;
/// malformed reported expiration is never treated as absent.
pub(crate) fn expiration(data: &Value) -> Result<Option<Timestamp>, Error> {
    if data["expires_at"].is_null() {
        Ok(None)
    } else {
        let value = data["expires_at"]
            .as_str()
            .filter(|value| value.len() <= 128)
            .ok_or(Error::Unavailable)?;
        Ok(Some(Timestamp(
            chrono::DateTime::parse_from_rfc3339(value)
                .map_err(|_| Error::Unavailable)?
                .timestamp_millis(),
        )))
    }
}

#[cfg(test)]
mod tests;
