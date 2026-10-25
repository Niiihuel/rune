//! Presence, identity and expressions (P5), as Discord sends them: rich
//! activities and the platforms of a presence, the marks of a user or a
//! member (avatar decoration, nameplate, server tag), a role's icon,
//! emoji and gradient, the plan's perks, a profile's badges and connected
//! accounts, the standard sticker packs and Discord's own GIF search.
//!
//! Field names and routes come from Serein (`discord-protocol/src/
//! presence.rs`, `collectibles.rs`, `profile.rs`, `gifs.rs`, read
//! 2026-10-17) and from the official web client (REA of
//! `web.c5b338cc7e610833.js`: `GIFS_SUGGEST` with `q`, `limit` and
//! `locale`; search and trending with `media_format` and `locale`;
//! `clan-badges/{guild}/{badge}.png`; `badge-icons/{icon}.png`;
//! `avatar-decoration-presets/{asset}.png`), reimplemented. Pictures are
//! built here, on Discord's own hosts, but for an activity's Spotify,
//! Twitch or YouTube cover, on that service's image host (2026-10-18).
//! Unofficial and only tried with fixtures.

use rune_i18n::t;
use std::collections::HashSet;

use rune_model::{
    Activity, ActivityKind, Badge, Connection, Identity, ImageKind, Nameplate, Perks, Platforms,
    RemoteImage, Rgb, RoleStyle, ServerId, ServerTag, StickerPack, Timestamp, MAX_ACTIVITIES,
};
use rune_provider::{
    Command, GifAttribution, GifCategory, GifItem, GifPage, GifQuery, MAX_GIF_CATEGORIES,
    MAX_GIF_SUGGESTIONS,
};
use serde_json::Value;

use crate::{wire, LoginError};

/// The longest line of an activity kept.
const TEXT: usize = 128;
/// Discord's plan, as `premium_type` numbers it.
const UPGRADE: &str = "Nitro";
/// GIFs of one page, as Serein and the official client ask for them.
pub(crate) const GIF_PAGE: u32 = 50;
/// The last page RUNE asks for: the tenth of [`GIF_PAGE`] (2026-10-18:
/// «Más GIF» paged nothing).
pub(crate) const MAX_GIF_OFFSET: u32 = 9 * GIF_PAGE;
/// The answers read, at most.
pub(crate) const MAX_GIFS: usize = 256 * 1024;
pub(crate) const MAX_PACKS: usize = 1024 * 1024;
/// The language the GIF search answers in.
const LOCALE: &str = "es-ES";

/// A line of text: trimmed, without control characters, cut short.
fn line(value: &Value) -> Option<String> {
    let text: String = value
        .as_str()?
        .chars()
        .filter(|c| !c.is_control())
        .take(TEXT)
        .collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// A hash naming a CDN picture: 32 hex digits, maybe after `a_`.
fn hash(value: &Value) -> Option<&str> {
    let hash = value.as_str()?;
    let digits = hash.strip_prefix("a_").unwrap_or(hash);
    (digits.len() == 32 && digits.bytes().all(|byte| byte.is_ascii_hexdigit())).then_some(hash)
}

fn picture(url: String) -> RemoteImage {
    RemoteImage {
        url,
        kind: ImageKind::Artwork,
    }
}

/// A picture kept at its own shape: wide, tall or moving (a GIF, an effect).
fn shaped(url: String, kind: ImageKind) -> RemoteImage {
    RemoteImage { url, kind }
}

/// A time of an activity, in Unix milliseconds, within what JSON keeps exact.
fn moment(value: &Value) -> Option<Timestamp> {
    value
        .as_u64()
        .filter(|ms| *ms > 0 && *ms < (1 << 53))
        .map(|ms| Timestamp(ms as i64))
}

/// An activity's picture: an application's asset on the CDN, or a picture
/// Discord's media proxy keeps (`mp:`), asked for small (160 px, the
/// official client's large size) so that it fits the artwork budget; a GIF
/// as an animated WebP, as that client asks. An external address keeps its
/// percent-encoding (2026-10-17: such pictures were refused). A Spotify,
/// Twitch or YouTube cover comes from that service's own image host, as
/// the official client builds it (owner-approved 2026-10-18; see
/// SECURITY.md): only its exact host, over HTTPS, as a still JPEG within
/// a banner's limits (4 MiB, 2048 px), not an attachment's.
fn asset(application: Option<u64>, key: &Value) -> Option<RemoteImage> {
    let key = key.as_str().filter(|key| key.len() <= 1024)?;
    if let Some(url) = cover(key) {
        return Some(shaped(url, ImageKind::Banner));
    }
    if let Some(path) = key.strip_prefix("mp:") {
        let clean = !path.is_empty()
            && (path.starts_with("external/") || path.starts_with("attachments/"))
            && !path.contains("..")
            && !path
                .chars()
                .any(|c| c.is_control() || c.is_whitespace() || matches!(c, '\\' | '?' | '#'));
        let format = if path.to_ascii_lowercase().ends_with(".gif") {
            "format=webp&animated=true"
        } else {
            "format=png"
        };
        return clean.then(|| {
            picture(format!(
                "https://media.discordapp.net/{path}?{format}&width=160&height=160"
            ))
        });
    }
    let application = application?;
    (!key.is_empty() && key.len() <= 20 && key.bytes().all(|byte| byte.is_ascii_digit())).then(
        || {
            picture(format!(
                "https://cdn.discordapp.com/app-assets/{application}/{key}.png?size=128"
            ))
        },
    )
}

/// The cover a `spotify:`, `twitch:` or `youtube:` key names, at its
/// service's host (REA of the official client's asset keys, 2026-10-18):
/// a Spotify image ID, a Twitch login's live preview at 160 px as the
/// activity card asks, a YouTube video's live thumbnail. Nothing else of
/// those services is read: never a connection's token.
fn cover(key: &str) -> Option<String> {
    let (service, id) = key.split_once(':')?;
    let plain = |extra: &[u8]| {
        !id.is_empty()
            && id.len() <= 64
            && id
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || extra.contains(&byte))
    };
    match service {
        "spotify" if plain(b"") => Some(format!("https://i.scdn.co/image/{id}")),
        "twitch" if plain(b"_") => Some(format!(
            "https://static-cdn.jtvnw.net/previews-ttv/live_user_{id}-160x160.jpg"
        )),
        "youtube" if plain(b"_-") => {
            Some(format!("https://i.ytimg.com/vi/{id}/hqdefault_live.jpg"))
        }
        _ => None,
    }
}

/// One activity of a presence as RUNE shows it; a custom status (type 4),
/// an unknown type or one without a name is none.
pub(crate) fn activity(value: &Value) -> Option<Activity> {
    let kind = match value["type"].as_u64()? {
        0 => ActivityKind::Playing,
        1 => ActivityKind::Streaming,
        2 => ActivityKind::Listening,
        3 => ActivityKind::Watching,
        5 => ActivityKind::Competing,
        _ => return None,
    };
    let application = wire::id(&value["application_id"]).ok();
    let assets = &value["assets"];
    let started = moment(&value["timestamps"]["start"]);
    let image = asset(application, &assets["large_image"]);
    let small_image = asset(application, &assets["small_image"]).filter(|small| {
        // A badge that is the same picture adds nothing.
        image.as_ref() != Some(small)
    });
    Some(Activity {
        kind,
        name: line(&value["name"])?,
        detail: line(&value["details"]),
        state: line(&value["state"]),
        image,
        small_image,
        started,
        ends: moment(&value["timestamps"]["end"]).filter(|end| started.is_none_or(|s| *end > s)),
        application,
    })
}

/// The activities of a presence, in order, at most [`MAX_ACTIVITIES`]: a
/// game reported twice under one name is kept once.
pub(crate) fn activities(values: &[Value]) -> Vec<Activity> {
    let mut seen = HashSet::new();
    values
        .iter()
        .filter_map(activity)
        .filter(|activity| seen.insert((activity.kind, activity.name.to_lowercase())))
        .take(MAX_ACTIVITIES)
        .collect()
}

/// Where someone is connected from: `client_status`, each place around.
pub(crate) fn platforms(value: &Value) -> Platforms {
    let around = |place: &str| matches!(value[place].as_str(), Some("online" | "idle" | "dnd"));
    Platforms {
        desktop: around("desktop"),
        mobile: around("mobile"),
        web: around("web"),
    }
}

/// Whether a collectible whose `expires_at` (Unix seconds) is `value` is
/// still worn.
fn worn(value: &Value) -> bool {
    match value.as_u64() {
        None => true,
        Some(seconds) => {
            let now = crate::messages::wall_now().0 / 1000;
            i64::try_from(seconds).is_ok_and(|seconds| seconds > now)
        }
    }
}

/// The frame over someone's picture (`avatar_decoration_data`).
fn decoration(value: &Value) -> Option<RemoteImage> {
    let asset = hash(&value["asset"]).filter(|_| worn(&value["expires_at"]))?;
    // The animated preset plays only through `passthrough`.
    let animated = asset.starts_with("a_");
    Some(picture(format!(
        "https://cdn.discordapp.com/avatar-decoration-presets/{asset}.png?size=96&passthrough={animated}"
    )))
}

/// A nameplate's ground, by the palette it names: RUNE's own pairs, dark
/// to light, close to each name.
fn palette(name: &str) -> Option<(Rgb, Rgb)> {
    let (dark, light) = match name {
        "crimson" => (0x5C_10_1A, 0xC2_2E_3D),
        "berry" => (0x4E_14_40, 0xB0_3A_8C),
        "sky" => (0x15_3E_63, 0x4F_A3_E0),
        "teal" => (0x0F_45_45, 0x2F_A8_A0),
        "forest" => (0x1B_3D_23, 0x4F_9A_5A),
        "bubble_gum" => (0x6B_2A_55, 0xF0_8C_C8),
        "violet" => (0x2F_1F_63, 0x8A_6C_E8),
        "cobalt" => (0x14_25_5E, 0x3D_6B_E0),
        "clover" => (0x1F_4A_1A, 0x6C_C2_4A),
        "lemon" => (0x5E_55_12, 0xE8_D4_3C),
        "white" => (0x6E_6E_78, 0xE6_E6_EE),
        _ => return None,
    };
    Some((Rgb(dark), Rgb(light)))
}

/// A collectible's picture by its product (`sku_id`), as the official
/// client builds it (REA, 2026-10-18): `static`, or `animated` where it
/// does not play video (Linux).
fn collectible(sku: u64, format: &str) -> RemoteImage {
    shaped(
        format!("https://cdn.discordapp.com/media/v1/collectibles-shop/{sku}/{format}"),
        ImageKind::AttachmentPreview,
    )
}

/// The artwork behind someone's name (`collectibles.nameplate`): the still
/// from its asset path, else by its product, and the moving one by its
/// product (2026-10-18: by asset path only, never moving).
fn nameplate(value: &Value) -> Option<Nameplate> {
    let asset = value["asset"].as_str().filter(|asset| {
        asset.len() <= 128
            && asset.starts_with("nameplates/")
            && asset.ends_with('/')
            && !asset.contains("..")
            && asset
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'/'))
    });
    let sku = wire::id(&value["sku_id"]).ok();
    let image = match (asset, sku) {
        (Some(asset), _) => shaped(
            format!("https://cdn.discordapp.com/assets/collectibles/{asset}static.png"),
            ImageKind::AttachmentPreview,
        ),
        (None, Some(sku)) => collectible(sku, "static"),
        (None, None) => return None,
    };
    worn(&value["expires_at"]).then(|| Nameplate {
        image,
        colors: value["palette"].as_str().and_then(palette),
        label: line(&value["label"]),
        animated: sku.map(|sku| collectible(sku, "animated")),
    })
}

/// The server tag someone shows (`primary_guild`, formerly `clan`).
fn tag(value: &Value) -> Option<ServerTag> {
    if value["identity_enabled"].as_bool() != Some(true) {
        return None;
    }
    let server = ServerId(wire::id(&value["identity_guild_id"]).ok()?);
    let text = value["tag"].as_str().filter(|tag| {
        !tag.trim().is_empty()
            && tag.chars().count() <= rune_model::identity::TAG_MAX
            && tag.len() <= 16
            && !tag.chars().any(char::is_control)
    })?;
    Some(ServerTag {
        server,
        tag: text.to_owned(),
        badge: hash(&value["badge"]).map(|badge| {
            picture(format!(
                "https://cdn.discordapp.com/clan-badges/{server}/{badge}.png?size=32"
            ))
        }),
    })
}

/// The marks of a user or a member object. A field it does not carry, or
/// carries broken, is none: a bad mark never fails the account.
pub(crate) fn identity(value: &Value) -> Identity {
    let primary = value
        .get("primary_guild")
        .filter(|tag| !tag.is_null())
        .unwrap_or(&value["clan"]);
    Identity {
        decoration: decoration(&value["avatar_decoration_data"]),
        nameplate: nameplate(&value["collectibles"]["nameplate"]),
        tag: tag(primary),
    }
}

/// What a role shows beside names and its gradient (`icon`,
/// `unicode_emoji`, `colors`).
pub(crate) fn role_style(role: u64, value: &Value) -> RoleStyle {
    let color = |key: &str| {
        value["colors"][key]
            .as_u64()
            .filter(|color| (1..=0xFF_FFFF).contains(color))
            .map(|color| Rgb(color as u32))
    };
    let mut gradient: Vec<Rgb> = color("secondary_color").into_iter().collect();
    if !gradient.is_empty() {
        gradient.extend(color("tertiary_color"));
    }
    RoleStyle {
        icon: hash(&value["icon"]).map(|icon| {
            picture(format!(
                "https://cdn.discordapp.com/role-icons/{role}/{icon}.png?size=64"
            ))
        }),
        emoji: value["unicode_emoji"]
            .as_str()
            .filter(|emoji| {
                !emoji.is_empty()
                    && emoji.chars().count() <= 16
                    && emoji.len() <= 64
                    && !emoji.chars().any(|c| c.is_control() || c.is_whitespace())
            })
            .map(str::to_owned),
        gradient,
    }
}

/// What the account's plan (`premium_type`) lets it use: Nitro Classic
/// (1) and Basic (3) emoji anywhere, Nitro (2) stickers too, as Serein
/// reads it. Of the profile, as the official client's product catalog
/// grants it (REA of its bundle, 2026-10-18): a moving avatar with Classic
/// or Nitro, a banner and a profile per server only with Nitro.
pub(crate) fn perks(user: &Value) -> Perks {
    let plan = user["premium_type"].as_u64().unwrap_or(0);
    Perks {
        emoji_anywhere: (1..=3).contains(&plan),
        animated_emoji: (1..=3).contains(&plan),
        stickers_anywhere: matches!(plan, 2 | 3),
        upgrade: Some(UPGRADE.into()),
        animated_avatar: matches!(plan, 1 | 2),
        profile_banner: plan == 2,
        server_profiles: plan == 2,
    }
}

/// Where a connected account's public page is, given what the profile
/// reports of it.
#[derive(Clone, Copy)]
enum Page {
    /// The service shows no page for it.
    Hidden,
    /// The account's handle (`name`) after this address.
    Handle(&'static str),
    /// The service's own id (`id`) between these two parts.
    Numbered(&'static str, &'static str),
    /// `https://{name}`: a domain the account proved it owns.
    Website,
    /// `@user@host`: a page on the account's own server.
    Fediverse,
}

/// The services a profile can show, by key: the name people know them by
/// and where an account's page is (the same pages Serein opens).
const SERVICES: [(&str, &str, Page); 23] = [
    ("battlenet", "Battle.net", Page::Hidden),
    (
        "bluesky",
        "Bluesky",
        Page::Handle("https://bsky.app/profile/"),
    ),
    ("crunchyroll", "Crunchyroll", Page::Hidden),
    ("domain", "Dominio", Page::Website),
    ("ebay", "eBay", Page::Handle("https://www.ebay.com/usr/")),
    ("epicgames", "Epic Games", Page::Hidden),
    ("facebook", "Facebook", Page::Hidden),
    ("github", "GitHub", Page::Handle("https://github.com/")),
    (
        "instagram",
        "Instagram",
        Page::Handle("https://www.instagram.com/"),
    ),
    ("leagueoflegends", "League of Legends", Page::Hidden),
    ("mastodon", "Mastodon", Page::Fediverse),
    ("paypal", "PayPal", Page::Hidden),
    ("playstation", "PlayStation", Page::Hidden),
    (
        "reddit",
        "Reddit",
        Page::Handle("https://www.reddit.com/user/"),
    ),
    ("riotgames", "Riot Games", Page::Hidden),
    (
        "roblox",
        "Roblox",
        Page::Numbered("https://www.roblox.com/users/", "/profile"),
    ),
    (
        "spotify",
        "Spotify",
        Page::Numbered("https://open.spotify.com/user/", ""),
    ),
    (
        "steam",
        "Steam",
        Page::Numbered("https://steamcommunity.com/profiles/", ""),
    ),
    ("tiktok", "TikTok", Page::Handle("https://www.tiktok.com/@")),
    ("twitch", "Twitch", Page::Handle("https://www.twitch.tv/")),
    ("twitter", "X", Page::Handle("https://x.com/")),
    ("xbox", "Xbox", Page::Hidden),
    (
        "youtube",
        "YouTube",
        Page::Numbered("https://www.youtube.com/channel/", ""),
    ),
];

fn service(key: &str) -> Option<&'static (&'static str, &'static str, Page)> {
    SERVICES.iter().find(|(known, ..)| *known == key)
}

/// A piece of an address that can only name a page: letters, digits, `-`,
/// `_` and `.`, never empty and never climbing out with `..`, so it cannot
/// change the host the page is on.
fn path_safe(piece: &str) -> bool {
    !piece.is_empty()
        && !piece.contains("..")
        && piece
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}

/// An account's public page on its service, when it has one.
fn link(key: &str, id: &str, name: &str) -> Option<String> {
    match service(key)?.2 {
        Page::Hidden => None,
        Page::Handle(base) => path_safe(name).then(|| format!("{base}{name}")),
        Page::Numbered(base, end) => path_safe(id).then(|| format!("{base}{id}{end}")),
        Page::Website => (path_safe(name) && name.contains('.')).then(|| format!("https://{name}")),
        Page::Fediverse => match name.split('@').collect::<Vec<_>>()[..] {
            ["", user, host] if path_safe(user) && path_safe(host) && host.contains('.') => {
                Some(format!("https://{host}/@{user}"))
            }
            _ => None,
        },
    }
}

/// A service as people know it, by its key.
fn label(key: &str) -> String {
    service(key).map_or(key, |(_, name, _)| name).to_owned()
}

/// A profile's connected accounts (`connected_accounts`); `None` when not
/// reported or not a list.
pub(crate) fn connections(value: &Value) -> Option<Vec<Connection>> {
    let list = value.as_array()?;
    Some(
        list.iter()
            .filter_map(|account| {
                let service = account["type"]
                    .as_str()
                    .filter(|kind| {
                        !kind.is_empty()
                            && kind.len() <= 32
                            && kind.bytes().all(|b| b.is_ascii_lowercase() || b == b'_')
                    })?
                    .to_owned();
                let name = line(&account["name"])?;
                let id = account["id"].as_str().unwrap_or_default();
                Some(Connection {
                    link: link(&service, id, &name),
                    label: label(&service),
                    verified: account["verified"].as_bool() == Some(true),
                    service,
                    name,
                })
            })
            .take(rune_model::identity::MAX_CONNECTIONS)
            .collect(),
    )
}

/// A profile's badges, the account's then the server's (`badges` and
/// `guild_badges`); `None` when neither was reported.
pub(crate) fn badges(account: &Value, server: &Value) -> Option<Vec<Badge>> {
    if !account.is_array() && !server.is_array() {
        return None;
    }
    let lists = [account, server];
    Some(
        lists
            .iter()
            .filter_map(|list| list.as_array())
            .flatten()
            .filter_map(|badge| {
                let id = badge["id"].as_str().filter(|id| {
                    !id.is_empty()
                        && id.len() <= 64
                        && id
                            .bytes()
                            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                })?;
                let icon = badge["icon"].as_str().filter(|icon| {
                    hash(&Value::from(*icon)).is_some()
                        || (icon.len() <= 64
                            && icon
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-')))
                });
                Some(Badge {
                    id: id.to_owned(),
                    description: line(&badge["description"]).unwrap_or_else(|| id.to_owned()),
                    icon: icon.map(|icon| {
                        picture(format!("https://cdn.discordapp.com/badge-icons/{icon}.png"))
                    }),
                    link: badge["link"]
                        .as_str()
                        .filter(|link| {
                            link.starts_with("https://")
                                && link.len() <= 512
                                && !link.chars().any(|c| c.is_control() || c.is_whitespace())
                        })
                        .map(str::to_owned),
                })
            })
            .take(rune_model::identity::MAX_BADGES)
            .collect(),
    )
}

/// The standard packs of `GET /sticker-packs`. A pack that carries a
/// server's sticker makes the answer unreadable.
pub(crate) fn sticker_packs(value: &Value) -> Result<Vec<StickerPack>, LoginError> {
    let packs = value["sticker_packs"]
        .as_array()
        .filter(|packs| packs.len() <= rune_model::identity::MAX_PACKS)
        .ok_or_else(wire::invalid)?;
    packs
        .iter()
        .map(|pack| {
            let items = &pack["stickers"];
            if items
                .as_array()
                .is_some_and(|items| items.iter().any(|item| !item["guild_id"].is_null()))
            {
                return Err(wire::invalid());
            }
            let stickers = crate::stickers::decode(items, true)?;
            Ok(StickerPack {
                id: wire::id(&pack["id"])?,
                name: line(&pack["name"]).ok_or_else(wire::invalid)?,
                stickers,
            })
        })
        .collect()
}

/// A host a GIF's picture or address may come from: KLIPY's and Tenor's,
/// as Serein allows them.
fn gif_host(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split('/').next().unwrap_or_default();
    url.len() <= 512
        && !url.contains("..")
        && !url
            .chars()
            .any(|c| c.is_control() || c.is_whitespace() || c == '\\' || c == '#')
        && matches!(
            host,
            "klipy.com"
                | "static.klipy.com"
                | "static1.klipy.com"
                | "static2.klipy.com"
                | "tenor.com"
                | "media.tenor.com"
                | "c.tenor.com"
        )
}

/// The GIFs of an answer: a picture to show (`src`, the `tinygif` asked
/// for, else `preview`) and the address sent (`url`).
fn gif_items(value: &Value) -> Vec<GifItem> {
    let mut seen = HashSet::new();
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|gif| {
            let url = gif["url"].as_str().filter(|url| gif_host(url))?;
            let preview = [&gif["src"], &gif["preview"]]
                .into_iter()
                .filter_map(Value::as_str)
                .find(|source| gif_host(source))?;
            let id = match &gif["id"] {
                Value::String(id) => id.clone(),
                Value::Number(id) => id.to_string(),
                _ => url.to_owned(),
            };
            seen.insert(id.clone()).then(|| GifItem {
                title: line(&gif["title"]).unwrap_or_else(|| "GIF".into()),
                creator: None,
                url: url.to_owned(),
                preview: shaped(preview.to_owned(), ImageKind::AttachmentPreview),
                thumbnail: None,
                id,
            })
        })
        .take(GIF_PAGE as usize)
        .collect()
}

/// What the search answered for the page at `offset`: GIFs, and for
/// trending also what is trending to search for (`categories`). A full
/// page offers the next one, up to [`MAX_GIF_OFFSET`].
pub(crate) fn gif_page(gifs: &Value, trending: Option<&Value>, offset: u32) -> GifPage {
    let full = gifs
        .as_array()
        .is_some_and(|gifs| gifs.len() >= GIF_PAGE as usize);
    let categories = trending
        .and_then(|trending| trending["categories"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|category| {
            Some(GifCategory {
                name: line(&category["name"])?,
                image: category["src"]
                    .as_str()
                    .filter(|source| gif_host(source))
                    .map(|source| shaped(source.to_owned(), ImageKind::AttachmentPreview)),
            })
        })
        .take(MAX_GIF_CATEGORIES)
        .collect();
    GifPage {
        items: gif_items(gifs),
        next_offset: (full && offset < MAX_GIF_OFFSET).then_some(offset + GIF_PAGE),
        attribution: GifAttribution {
            label: t("GIF de Discord").into(),
            url: String::new(),
            image: None,
        },
        categories,
    }
}

/// The terms of a suggestion answer: a list of strings.
pub(crate) fn suggestions(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(line)
        .take(MAX_GIF_SUGGESTIONS)
        .collect()
}

/// The routes of a GIF command: one, or the two trending reads of its
/// first page. A later page adds its `offset`; the official client never
/// pages (its picker shows one answer), so whether the service honors it
/// is unproven, and the interface stops on a page that repeats.
pub(crate) fn gif_paths(command: &Command) -> Vec<String> {
    let encode =
        |text: &str| -> String { url::form_urlencoded::byte_serialize(text.as_bytes()).collect() };
    let page = |offset: u32| match offset {
        0 => String::new(),
        offset => format!("&offset={offset}"),
    };
    match command {
        Command::SearchGifs {
            query: GifQuery::Search(text),
            offset,
            ..
        } => vec![format!(
            "/gifs/search?q={}&media_format=tinygif&locale={LOCALE}&limit={GIF_PAGE}{}",
            encode(text.trim()),
            page(*offset)
        )],
        Command::SearchGifs {
            query: GifQuery::Trending,
            offset,
            ..
        } => {
            let mut paths = vec![format!(
                "/gifs/trending-gifs?media_format=tinygif&locale={LOCALE}&limit={GIF_PAGE}{}",
                page(*offset)
            )];
            if *offset == 0 {
                paths.push(format!(
                    "/gifs/trending?media_format=tinygif&locale={LOCALE}"
                ));
            }
            paths
        }
        Command::SuggestGifs { query, .. } if query.trim().is_empty() => {
            vec![format!("/gifs/trending-search?locale={LOCALE}")]
        }
        Command::SuggestGifs { query, .. } => vec![format!(
            "/gifs/suggest?q={}&limit=5&locale={LOCALE}",
            encode(query.trim())
        )],
        _ => Vec::new(),
    }
}

/// Whether `command` is one of these reads.
pub(crate) fn is_identity(command: &Command) -> bool {
    matches!(
        command,
        Command::LoadStickerPacks | Command::SearchGifs { .. } | Command::SuggestGifs { .. }
    )
}

/// A query RUNE sends: 1 to 50 characters without control characters.
pub(crate) fn valid_query(command: &Command) -> bool {
    let fits = |text: &str| {
        let text = text.trim();
        text.chars().count() <= 50 && !text.chars().any(char::is_control)
    };
    let page = |offset: u32| offset.is_multiple_of(GIF_PAGE) && offset <= MAX_GIF_OFFSET;
    match command {
        Command::SearchGifs {
            query: GifQuery::Search(text),
            offset,
            ..
        } => page(*offset) && !text.trim().is_empty() && fits(text),
        Command::SearchGifs { offset, .. } => page(*offset),
        Command::SuggestGifs { query, .. } => fits(query),
        _ => true,
    }
}

/// The product of the effect someone wears on their profile, the server's
/// over the account's: an equipped collectible of type 1, or the older
/// `profile_effect`, while it has not expired.
pub(crate) fn effect_sku(profile: &Value) -> Option<u64> {
    let now = crate::messages::wall_now();
    let equipped = |scoped: &Value| {
        scoped["collectibles"]
            .as_array()?
            .iter()
            .take(16)
            .find(|item| {
                item["type"].as_u64() == Some(1)
                    && wire::timestamp(&item["expires_at"]).map_or(true, |at| at > now)
            })
            .and_then(|item| wire::id(&item["sku_id"]).ok())
    };
    let legacy = &profile["user_profile"]["profile_effect"];
    equipped(&profile["guild_member_profile"])
        .or_else(|| equipped(&profile["user_profile"]))
        .or_else(|| {
            worn(&legacy["expires_at"])
                .then(|| wire::id(&legacy["id"]).ok())
                .flatten()
        })
}

/// A picture of an effect, only from the collectibles' own folders.
fn effect_picture(value: &Value) -> Option<RemoteImage> {
    let url = value.as_str().filter(|url| url.len() <= 256)?;
    let name = url
        .strip_prefix("https://cdn.discordapp.com/assets/content/")
        .or_else(|| url.strip_prefix("https://cdn.discordapp.com/media/v1/collectibles-shop/"))?;
    let clean = !name.is_empty()
        && !name.contains("..")
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'));
    // An effect keeps its shape and moves: decoded as a preview.
    clean.then(|| shaped(url.to_owned(), ImageKind::AttachmentPreview))
}

/// The effect `sku` of a `GET /collectibles-products/{sku}` answer, as
/// Serein reads it: its item of type 1, up to eight layers by `zIndex`,
/// each at its place and in its time (`start`, `duration`, `loop`,
/// `loopDelay`, in milliseconds, as the official client plays them), and
/// its still for reduced motion (`reducedMotionSrc`, as that client shows).
pub(crate) fn effect(product: &Value, sku: u64) -> Option<rune_model::ProfileEffect> {
    let item = product["items"].as_array()?.iter().take(8).find(|item| {
        item["type"].as_u64() == Some(1) && wire::id(&item["sku_id"]).ok() == Some(sku)
    })?;
    let dimension = |value: &Value, default: u32| {
        value
            .as_u64()
            .and_then(|side| u32::try_from(side).ok())
            .filter(|side| (1..=4096).contains(side))
            .unwrap_or(default)
    };
    let mut layers: Vec<(i64, rune_model::EffectLayer)> = item["effects"]
        .as_array()
        .into_iter()
        .flatten()
        .take(rune_model::identity::MAX_EFFECT_LAYERS)
        .filter_map(|layer| {
            // One of its random pictures, the same every time.
            let image = effect_picture(&layer["src"])
                .or_else(|| effect_picture(&layer["randomizedSources"][0]["src"]))?;
            let at = |key: &str| {
                layer["position"][key]
                    .as_i64()
                    .and_then(|at| i32::try_from(at).ok())
                    .filter(|at| at.abs() <= 4096)
                    .unwrap_or(0)
            };
            let millis = |key: &str| {
                layer[key]
                    .as_u64()
                    .and_then(|ms| u32::try_from(ms).ok())
                    .filter(|ms| *ms <= 3_600_000)
                    .unwrap_or(0)
            };
            let duration = millis("duration");
            Some((
                layer["zIndex"].as_i64().unwrap_or(0),
                rune_model::EffectLayer {
                    image,
                    x: at("x"),
                    y: at("y"),
                    width: dimension(&layer["width"], 450),
                    height: dimension(&layer["height"], 880),
                    start: millis("start"),
                    duration,
                    // Without a length it cannot end: it stays.
                    looping: layer["loop"].as_bool().unwrap_or(true) || duration == 0,
                    loop_delay: millis("loopDelay"),
                },
            ))
        })
        .collect();
    layers.sort_by_key(|(order, _)| *order);
    let still = effect_picture(&item["reducedMotionSrc"])
        .or_else(|| effect_picture(&item["staticFrameSrc"]));
    (!layers.is_empty() || still.is_some()).then(|| rune_model::ProfileEffect {
        width: 450,
        height: 880,
        layers: layers.into_iter().map(|(_, layer)| layer).collect(),
        still,
    })
}

/// What the account may send beyond the channel's server: whatever its
/// plan allows, and the standard packs read.
#[derive(Debug, Default)]
pub(crate) struct Reach {
    pub stickers_anywhere: bool,
    pub pack_stickers: HashSet<rune_model::StickerId>,
}

impl Reach {
    pub(crate) fn observe(&mut self, event: &rune_provider::Event) {
        match event {
            rune_provider::Event::Perks(perks) => self.stickers_anywhere = perks.stickers_anywhere,
            rune_provider::Event::StickerPacks(packs) => {
                self.pack_stickers = packs
                    .iter()
                    .flat_map(|pack| &pack.stickers)
                    .filter(|sticker| sticker.available)
                    .map(|sticker| sticker.id)
                    .collect();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const HASH: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn activities_keep_pictures_on_known_hosts_and_bounded_times() {
        let value = json!([
            {"type": 4, "name": "Custom Status", "state": "hola"},
            {"type": 0, "name": "Factorio", "application_id": "55", "details": "Fábrica",
             "state": "\u{7}Partida", "timestamps": {"start": 1_700_000_000_000u64},
             "assets": {"large_image": "123456", "small_image": "mp:external/abc/https/x.test/a%20b.png",
                        "large_text": "ignored"},
             "buttons": ["Unirme"]},
            {"type": 2, "name": "Spotify", "details": "Tema", "state": "Artista",
             "assets": {"large_image": "spotify:ab67616d0000b273"},
             "timestamps": {"start": 1000, "end": 500}},
            {"type": 0, "name": "factorio"},
            {"type": 3, "name": "Video", "assets": {"large_image": "mp:../../etc"}},
            {"type": 9, "name": "Futuro"}
        ]);
        let found = activities(value.as_array().unwrap());
        assert_eq!(
            found.len(),
            3,
            "custom, a repeated game and unknown types go"
        );
        let game = &found[0];
        assert_eq!(game.state.as_deref(), Some("Partida"));
        assert_eq!(
            game.image.as_ref().unwrap().url,
            "https://cdn.discordapp.com/app-assets/55/123456.png?size=128"
        );
        assert_eq!(
            game.small_image.as_ref().unwrap().url,
            "https://media.discordapp.net/external/abc/https/x.test/a%20b.png?format=png&width=160&height=160"
        );
        let moving = asset(None, &json!("mp:external/x/https/y.test/baile.GIF")).unwrap();
        assert!(moving
            .url
            .ends_with("baile.GIF?format=webp&animated=true&width=160&height=160"));
        assert!(asset(None, &json!("mp:external/x/https/y.test/a.png?q=1")).is_none());
        assert_eq!(game.started, Some(Timestamp(1_700_000_000_000)));
        let music = &found[1];
        let cover = music.image.as_ref().unwrap();
        assert_eq!(cover.url, "https://i.scdn.co/image/ab67616d0000b273");
        assert_eq!(
            cover.kind,
            ImageKind::Banner,
            "a JPEG, under 1 MiB and 2048 px"
        );
        assert_eq!(music.ends, None, "an end before its start");
        // Covers of the other services, on their own hosts (2026-10-18).
        let url = |key: &str| asset(None, &json!(key)).map(|image| image.url);
        assert_eq!(
            url("twitch:some_streamer").as_deref(),
            Some("https://static-cdn.jtvnw.net/previews-ttv/live_user_some_streamer-160x160.jpg")
        );
        assert_eq!(
            url("youtube:dQw4w9WgXcQ").as_deref(),
            Some("https://i.ytimg.com/vi/dQw4w9WgXcQ/hqdefault_live.jpg")
        );
        for cover in [cover, &asset(None, &json!("twitch:x")).unwrap()] {
            let origin = url::Url::parse(&cover.url)
                .unwrap()
                .origin()
                .ascii_serialization();
            assert!(
                crate::provider::IMAGE_ORIGINS.contains(&origin.as_str()),
                "{origin}"
            );
        }
        for refused in [
            "spotify:../token",
            "spotify:a b",
            "spotify:",
            "twitch:evil.example/x",
            "youtube:x?y=1",
            "soundcloud:123",
            "spotify:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            assert!(url(refused).is_none(), "{refused}");
        }
        assert!(found[2].image.is_none(), "a path that climbs is refused");
        let platforms = platforms(&json!({"desktop": "idle", "mobile": "offline", "web": "dnd"}));
        assert!(platforms.desktop && !platforms.mobile && platforms.web);
    }

    #[test]
    fn marks_are_read_from_users_and_broken_ones_are_none() {
        let user = json!({
            "avatar_decoration_data": {"asset": format!("a_{HASH}"), "sku_id": "1", "expires_at": null},
            "collectibles": {"nameplate": {"asset": "nameplates/nameplates_v3/bonsai/",
                "palette": "forest", "label": "Bonsái", "sku_id": "2"}},
            "primary_guild": {"identity_guild_id": "77", "identity_enabled": true,
                "tag": "RUST", "badge": HASH}
        });
        let marks = identity(&user);
        assert_eq!(
            marks.decoration.unwrap().url,
            format!("https://cdn.discordapp.com/avatar-decoration-presets/a_{HASH}.png?size=96&passthrough=true")
        );
        let plate = marks.nameplate.unwrap();
        assert_eq!(
            plate.image.url,
            "https://cdn.discordapp.com/assets/collectibles/nameplates/nameplates_v3/bonsai/static.png"
        );
        assert_eq!(
            plate.animated.as_ref().unwrap().url,
            "https://cdn.discordapp.com/media/v1/collectibles-shop/2/animated",
            "it moves by its product"
        );
        // Without a usable asset path, its product names the still too.
        let by_sku = identity(&json!({"collectibles": {"nameplate":
            {"asset": "../escape/", "sku_id": "5", "palette": "nope"}}}))
        .nameplate
        .unwrap();
        assert_eq!(
            by_sku.image.url,
            "https://cdn.discordapp.com/media/v1/collectibles-shop/5/static"
        );
        assert!(by_sku.colors.is_none());
        let still = identity(&json!({"collectibles": {"nameplate":
            {"asset": "nameplates/a/", "sku_id": "x"}}}));
        assert!(
            still.nameplate.unwrap().animated.is_none(),
            "a broken product"
        );
        assert!(plate.colors.is_some() && plate.label.as_deref() == Some("Bonsái"));
        let tag = marks.tag.unwrap();
        assert_eq!((tag.server, tag.tag.as_str()), (ServerId(77), "RUST"));
        assert_eq!(
            tag.badge.unwrap().url,
            format!("https://cdn.discordapp.com/clan-badges/77/{HASH}.png?size=32")
        );
        let broken = json!({
            "avatar_decoration_data": {"asset": "nope", "expires_at": 1},
            "collectibles": {"nameplate": {"asset": "../escape/"}},
            "clan": {"identity_guild_id": "77", "identity_enabled": false, "tag": "RUST"}
        });
        assert_eq!(identity(&broken), Identity::default());
        let expired = json!({"avatar_decoration_data": {"asset": HASH, "expires_at": 1}});
        assert!(identity(&expired).decoration.is_none());
        let legacy =
            json!({"clan": {"identity_guild_id": "5", "identity_enabled": true, "tag": "LONGER"}});
        assert!(identity(&legacy).tag.is_none(), "longer than four");
    }

    #[test]
    fn roles_perks_badges_and_connections_follow_the_service() {
        let role = role_style(
            9,
            &json!({"icon": HASH, "unicode_emoji": null,
                "colors": {"primary_color": 1, "secondary_color": 2, "tertiary_color": 3}}),
        );
        assert_eq!(role.gradient, vec![Rgb(2), Rgb(3)]);
        assert!(role.icon.unwrap().url.contains("/role-icons/9/"));
        let emoji = role_style(
            9,
            &json!({"unicode_emoji": "🛡️", "colors": {"tertiary_color": 3}}),
        );
        assert_eq!(emoji.emoji.as_deref(), Some("🛡️"));
        assert!(emoji.gradient.is_empty(), "a third color needs a second");
        let none = perks(&json!({"premium_type": 0}));
        assert!(!none.emoji_anywhere && !none.stickers_anywhere);
        let classic = perks(&json!({"premium_type": 1}));
        assert!(classic.emoji_anywhere && !classic.stickers_anywhere);
        assert!(perks(&json!({"premium_type": 2})).stickers_anywhere);
        // The profile: a moving avatar with Classic; banner and servers with Nitro.
        assert!(!none.animated_avatar && !none.profile_banner && !none.server_profiles);
        assert!(classic.animated_avatar && !classic.profile_banner && !classic.server_profiles);
        let basic = perks(&json!({"premium_type": 3}));
        assert!(!basic.animated_avatar && !basic.profile_banner && !basic.server_profiles);
        let nitro = perks(&json!({"premium_type": 2}));
        assert!(nitro.animated_avatar && nitro.profile_banner && nitro.server_profiles);
        let found = badges(
            &json!([{"id": "hypesquad", "description": "HypeSquad", "icon": HASH,
                "link": "https://discord.com/settings/hypesquad-online"},
                {"id": "bad id!", "description": "x"}]),
            &json!([{"id": "guild_booster_lvl1", "description": "Mejora", "icon": "javascript:"}]),
        )
        .unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(
            found[0].icon.as_ref().unwrap().url,
            format!("https://cdn.discordapp.com/badge-icons/{HASH}.png")
        );
        assert!(found[1].icon.is_none() && found[1].link.is_none());
        assert_eq!(badges(&Value::Null, &Value::Null), None);
        let accounts = connections(&json!([
            {"type": "github", "id": "1", "name": "ada", "verified": true},
            {"type": "steam", "id": "7656", "name": "Ada"},
            {"type": "mastodon", "id": "x", "name": "@ada@social.example"},
            {"type": "domain", "id": "x", "name": "ada"},
            {"type": "github", "id": "1", "name": "../evil"}
        ]))
        .unwrap();
        assert_eq!(accounts[0].link.as_deref(), Some("https://github.com/ada"));
        assert!(accounts[0].verified && accounts[0].label == "GitHub");
        assert_eq!(
            accounts[1].link.as_deref(),
            Some("https://steamcommunity.com/profiles/7656")
        );
        assert_eq!(
            accounts[2].link.as_deref(),
            Some("https://social.example/@ada")
        );
        assert_eq!(accounts[3].link, None, "a domain needs a dot");
        assert_eq!(accounts[4].link, None, "a name that climbs");
        assert_eq!(
            link("roblox", "42", "Ada").as_deref(),
            Some("https://www.roblox.com/users/42/profile")
        );
        assert_eq!(
            link("domain", "x", "ada.example").as_deref(),
            Some("https://ada.example")
        );
        assert_eq!(
            link("mastodon", "x", "ada@social.example"),
            None,
            "no leading @"
        );
        assert_eq!(link("facebook", "1", "ada"), None, "no public page");
        assert_eq!(
            (label("twitter"), label("bungie")),
            ("X".into(), "bungie".into())
        );
    }

    #[test]
    fn packs_gifs_and_suggestions_are_bounded_and_on_known_hosts() {
        let packs = sticker_packs(&json!({"sticker_packs": [{"id": "1", "name": "Wumpus",
            "stickers": [{"id": "10", "name": "Hola", "format_type": 3},
                         {"id": "11", "name": "Chau", "format_type": 1}]}]}))
        .unwrap();
        assert!(packs[0].stickers[0].available);
        assert_eq!(
            packs[0].stickers[0].image.as_ref().unwrap().url,
            "https://cdn.discordapp.com/stickers/10.json",
            "drawn from its vectors"
        );
        assert!(packs[0].stickers[1].image.is_some());
        assert!(
            sticker_packs(&json!({"sticker_packs": [{"id": "1", "name": "x",
            "stickers": [{"id": "10", "name": "Robado", "format_type": 1, "guild_id": "5"}]}]}))
            .is_err()
        );
        let page = gif_page(
            &json!([
                {"id": 1, "title": "Gato", "url": "https://klipy.com/gifs/gato",
                 "src": "https://static.klipy.com/gato.gif"},
                {"id": "2", "url": "https://evil.test/x", "src": "https://static.klipy.com/x.gif"},
                {"id": "1", "url": "https://klipy.com/gifs/otro", "src": "https://static.klipy.com/o.gif"}
            ]),
            Some(
                &json!({"categories": [{"name": "aplausos", "src": "https://static.klipy.com/a.gif"},
                {"name": "", "src": "https://static.klipy.com/b.gif"}]}),
            ),
            0,
        );
        assert_eq!(page.items.len(), 1, "an unknown host and a repeated id go");
        assert_eq!(page.next_offset, None, "a short page is the last");
        assert_eq!(
            page.items[0].preview.url,
            "https://static.klipy.com/gato.gif"
        );
        assert_eq!(page.categories.len(), 1);
        assert_eq!(suggestions(&json!(["gato", 3, "gatos"])), ["gato", "gatos"]);
        let search = Command::SearchGifs {
            request: 1,
            query: GifQuery::Search("gato feliz&".into()),
            offset: 0,
        };
        assert_eq!(
            gif_paths(&search),
            ["/gifs/search?q=gato+feliz%26&media_format=tinygif&locale=es-ES&limit=50"]
        );
        assert!(valid_query(&search));
        let suggest = Command::SuggestGifs {
            request: 2,
            query: String::new(),
        };
        assert_eq!(gif_paths(&suggest), ["/gifs/trending-search?locale=es-ES"]);
        for offset in [49, MAX_GIF_OFFSET + GIF_PAGE] {
            assert!(!valid_query(&Command::SearchGifs {
                request: 1,
                query: GifQuery::Trending,
                offset,
            }));
        }
    }

    #[test]
    fn a_full_page_offers_the_next_one_asked_for_by_its_offset() {
        let full: Vec<Value> = (0..GIF_PAGE)
            .map(|id| {
                json!({"id": id, "url": format!("https://klipy.com/gifs/{id}"),
                    "src": format!("https://static.klipy.com/{id}.gif")})
            })
            .collect();
        let first = gif_page(&json!(full), None, 0);
        assert_eq!(first.items.len(), GIF_PAGE as usize);
        assert_eq!(first.next_offset, Some(GIF_PAGE));
        assert_eq!(
            gif_page(&json!(full), None, MAX_GIF_OFFSET).next_offset,
            None,
            "the tenth page is the last asked for"
        );
        let next = Command::SearchGifs {
            request: 3,
            query: GifQuery::Search("gato".into()),
            offset: GIF_PAGE,
        };
        assert!(valid_query(&next));
        assert_eq!(
            gif_paths(&next),
            ["/gifs/search?q=gato&media_format=tinygif&locale=es-ES&limit=50&offset=50"]
        );
        let trending = Command::SearchGifs {
            request: 4,
            query: GifQuery::Trending,
            offset: 100,
        };
        assert!(valid_query(&trending));
        assert_eq!(
            gif_paths(&trending),
            ["/gifs/trending-gifs?media_format=tinygif&locale=es-ES&limit=50&offset=100"],
            "the categories come with the first page only"
        );
    }
}
