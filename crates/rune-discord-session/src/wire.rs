//! Bounded, provider-local translations of documented message and user objects.

use chrono::DateTime;
use rune_i18n::es;
use rune_model::{
    Attachment, AttachmentId, ChannelId, Delivery, ImageKind, Message, MessageId, MessagePatch,
    ProfileBanner, Reaction, RemoteFile, RemoteImage, Rgb, Timestamp, User, UserId, VoiceClip,
};
use serde_json::Value;

use crate::LoginError;

pub(crate) fn invalid() -> LoginError {
    LoginError::new(es(
        "Discord devolvió datos incompatibles con esta versión de RUNE.",
    ))
}

pub(crate) fn id(value: &Value) -> Result<u64, LoginError> {
    let parsed = match value {
        Value::String(text) => {
            let parsed = text.parse::<u64>().map_err(|_| invalid())?;
            if parsed.to_string() != *text {
                return Err(invalid());
            }
            parsed
        }
        Value::Number(number) => number.as_u64().ok_or_else(invalid)?,
        _ => return Err(invalid()),
    };
    (parsed != 0).then_some(parsed).ok_or_else(invalid)
}

fn name(value: &Value) -> Result<String, LoginError> {
    let value = value.as_str().ok_or_else(invalid)?;
    if value.trim().is_empty()
        || value.len() > 512
        || value.chars().count() > 128
        || value.chars().any(char::is_control)
    {
        return Err(invalid());
    }
    Ok(value.to_owned())
}

pub(crate) fn user(value: &Value) -> Result<User, LoginError> {
    let accent = match value.get("accent_color").filter(|value| !value.is_null()) {
        Some(value) => u32::try_from(value.as_u64().ok_or_else(invalid)?)
            .ok()
            .filter(|color| *color <= 0xFF_FFFF)
            .ok_or_else(invalid)?,
        None => 0x8F_83_E8,
    };
    Ok(User {
        // Presence, identity and expressions (P5).
        identity: crate::identity::identity(value),
        id: UserId(id(&value["id"])?),
        avatar: image("avatars", id(&value["id"])?, &value["avatar"]),
        banner: match value.get("banner") {
            None => ProfileBanner::Unknown,
            Some(Value::Null) => ProfileBanner::Absent,
            Some(hash) => image("banners", id(&value["id"])?, hash)
                .map(ProfileBanner::Image)
                .unwrap_or_default(),
        },
        name: name(&value["username"])?,
        display_name: value
            .get("global_name")
            .filter(|value| !value.is_null())
            .map(name)
            .transpose()?,
        accent: Rgb(accent),
        bot: value
            .get("bot")
            .map_or(Ok(false), |v| v.as_bool().ok_or_else(invalid))?,
        pronouns: None,
        about: None,
        default_avatar: Some(default_avatar(
            id(&value["id"])?,
            &value["discriminator"],
            value["bot"] == true,
        )),
    })
}

/// The picture Discord shows for an account without one of its own, from
/// its public CDN, chosen as the official client does: one of six by when
/// the account was made, or of five by its old four-digit tag («0» or none
/// is a new-style name); a bot tagged «0000», as a webhook writes, the first.
pub(crate) fn default_avatar(id: u64, discriminator: &Value, bot: bool) -> RemoteImage {
    let tag = discriminator.as_str();
    let index = match tag.and_then(|tag| tag.parse::<u64>().ok()) {
        Some(tag) if tag != 0 => tag % 5,
        _ if bot && tag == Some("0000") => 0,
        _ => (id >> 22) % 6,
    };
    RemoteImage {
        url: format!("https://cdn.discordapp.com/embed/avatars/{index}.png"),
        kind: ImageKind::Artwork,
    }
}

/// Construct documented public CDN paths only. Bad optional artwork keeps
/// the initials fallback without failing an otherwise valid account.
pub(crate) fn image(folder: &str, id: u64, hash: &Value) -> Option<RemoteImage> {
    if !matches!(folder, "avatars" | "icons" | "banners" | "channel-icons") || id == 0 {
        return None;
    }
    cdn(&format!("{folder}/{id}"), folder == "banners", hash)
}

/// The hash that names a CDN image this module built: what a remembered
/// account keeps of its picture, rebuilt with [`image`] when listed.
pub(crate) fn image_key(image: &RemoteImage) -> Option<&str> {
    let name = image.url.split('?').next()?.rsplit('/').next()?;
    name.strip_suffix(".png")
}

/// The public CDN image at `path`/`hash`, once the hash is a valid one.
/// A CDN picture asked with `size=` (avatars, icons, banners, emoji,
/// stickers' artwork) at the size a screen of `density` draws it: the size
/// times the density, up to the next power of two the CDN serves, at most
/// 4096. Anything else, or the same size, is `None`: fetched as it is.
pub(crate) fn sized(url: &str, density: f32) -> Option<String> {
    let mut url = url::Url::parse(url).ok()?;
    if url.host_str() != Some("cdn.discordapp.com") || !density.is_finite() || density <= 1.0 {
        return None;
    }
    let size: u32 = url
        .query_pairs()
        .find(|(key, _)| key == "size")?
        .1
        .parse()
        .ok()?;
    let wanted = ((size as f32 * density).ceil() as u32)
        .next_power_of_two()
        .clamp(16, 4096);
    if wanted <= size {
        return None;
    }
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .map(|(key, value)| {
            let value = if key == "size" {
                wanted.to_string()
            } else {
                value.into_owned()
            };
            (key.into_owned(), value)
        })
        .collect();
    url.query_pairs_mut().clear().extend_pairs(pairs);
    Some(url.into())
}

pub(crate) fn cdn(path: &str, banner: bool, hash: &Value) -> Option<RemoteImage> {
    let hash = hash.as_str()?;
    let digest = hash.strip_prefix("a_").unwrap_or(hash);
    if digest.len() != 32 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let size = if banner { 512 } else { 128 };
    Some(RemoteImage {
        url: format!("https://cdn.discordapp.com/{path}/{hash}.png?size={size}"),
        kind: if banner {
            ImageKind::Banner
        } else {
            ImageKind::Artwork
        },
    })
}

/// Discord IDs carry their creation time: milliseconds since 2015 in their top bits.
const ID_EPOCH_MS: i64 = 1_420_070_400_000;

/// When the account `user` was created, read from its ID.
pub(crate) fn created(user: UserId) -> Option<Timestamp> {
    (user.0 >> 22 > 0).then(|| Timestamp((user.0 >> 22) as i64 + ID_EPOCH_MS))
}

pub(crate) fn timestamp(value: &Value) -> Result<Timestamp, LoginError> {
    let text = value
        .as_str()
        .filter(|text| text.len() <= 64)
        .ok_or_else(invalid)?;
    DateTime::parse_from_rfc3339(text)
        .map(|time| Timestamp(time.timestamp_millis()))
        .map_err(|_| invalid())
}

fn content(value: &Value) -> Result<String, LoginError> {
    value
        .as_str()
        .filter(|text| text.len() <= 32_768)
        .map(str::to_owned)
        .ok_or_else(invalid)
}

fn array(value: &Value, limit: usize) -> Result<&[Value], LoginError> {
    value
        .as_array()
        .filter(|items| items.len() <= limit)
        .map(Vec::as_slice)
        .ok_or_else(invalid)
}

pub(crate) fn attachments(
    value: &Value,
    channel: ChannelId,
) -> Result<Vec<Attachment>, LoginError> {
    let mut seen = std::collections::HashSet::new();
    array(value, 64)?
        .iter()
        .map(|item| {
            let dimension = |key| {
                item.get(key)
                    .and_then(Value::as_u64)
                    .and_then(|size| u32::try_from(size).ok())
                    .filter(|size| *size > 0)
            };
            let filename = item["filename"]
                .as_str()
                .filter(|name| !name.is_empty() && name.len() <= 1024)
                .ok_or_else(invalid)?;
            let content_type = item.get("content_type").and_then(Value::as_str);
            if content_type.is_some_and(|value| value.len() > 256) {
                return Err(invalid());
            }
            let id = AttachmentId(id(&item["id"])?);
            if !seen.insert(id) {
                return Err(invalid());
            }
            let dimensions = dimension("width").zip(dimension("height"));
            let size_bytes = item["size"].as_u64().ok_or_else(invalid)?;
            let original = attachment_url(&item["url"], channel, id);
            let preview_url = original
                .clone()
                .or_else(|| attachment_url(&item["proxy_url"], channel, id));
            let raster = match content_type {
                Some(mime) => matches!(
                    mime,
                    "image/png" | "image/jpeg" | "image/webp" | "image/gif"
                ),
                None => filename.rsplit_once('.').is_some_and(|(_, suffix)| {
                    matches!(
                        suffix.to_ascii_lowercase().as_str(),
                        "png" | "jpg" | "jpeg" | "webp" | "gif"
                    )
                }),
            };
            // A spoiler keeps its preview reference, but the interface requests
            // pixels only after the account reveals it.
            let spoiler = filename.starts_with("SPOILER_")
                || item["flags"].as_u64().is_some_and(|flags| flags & 8 != 0);
            let preview = (raster
                && size_bytes <= 8 * 1024 * 1024
                && dimensions.is_some_and(|(w, h)| {
                    w <= 4096 && h <= 4096 && u64::from(w) * u64::from(h) <= 16_777_216
                }))
            .then_some(preview_url)
            .flatten()
            .map(|url| RemoteImage {
                url,
                kind: ImageKind::AttachmentPreview,
            });
            Ok(Attachment {
                id,
                filename: filename.into(),
                size_bytes,
                content_type: content_type.map(str::to_owned),
                dimensions,
                preview,
                download: original.map(|url| RemoteFile { url }),
                spoiler,
                voice: voice(item),
                placeholder: placeholder(item),
                explicit: item["flags"]
                    .as_u64()
                    .is_some_and(|flags| flags & EXPLICIT_MEDIA != 0),
            })
        })
        .collect()
}

/// `CONTAINS_EXPLICIT_MEDIA`, the same bit in an attachment's and a card's
/// `flags`, as the official client reads them (docs/REA.md).
pub(crate) const EXPLICIT_MEDIA: u64 = 1 << 4;

/// The ThumbHash that stands in for a picture while it loads, as base64
/// without padding. Only `placeholder_version` 1 is a known format; one
/// that is too long or does not decode is dropped (contenido-de-mensajes).
fn placeholder(item: &Value) -> Option<RemoteImage> {
    use base64::Engine as _;
    let hash = item.get("placeholder")?.as_str()?.trim_end_matches('=');
    (item.get("placeholder_version").and_then(Value::as_u64) == Some(1)
        && hash.len() <= 64
        && base64::engine::general_purpose::STANDARD_NO_PAD
            .decode(hash)
            .is_ok_and(|bytes| bytes.len() >= 5))
    .then(|| RemoteImage {
        url: hash.to_owned(),
        kind: ImageKind::ThumbHash,
    })
}

/// A voice message's length and loudness. Only its uploads write
/// `duration_secs` and `waveform`; a length out of bounds keeps the file as a
/// plain attachment, and a bad waveform only loses the drawing (docs/REA.md).
fn voice(item: &Value) -> Option<VoiceClip> {
    use base64::Engine as _;
    let duration = item.get("duration_secs")?.as_f64()?;
    if !(duration.is_finite() && duration > 0.0 && duration <= f64::from(VoiceClip::MAX_SECS)) {
        return None;
    }
    let waveform = item
        .get("waveform")
        .and_then(Value::as_str)
        .filter(|text| text.len() <= VoiceClip::MAX_WAVEFORM.div_ceil(3) * 4)
        .and_then(|text| base64::engine::general_purpose::STANDARD.decode(text).ok())
        .filter(|samples| samples.len() <= VoiceClip::MAX_WAVEFORM)
        .unwrap_or_default();
    Some(VoiceClip {
        duration_secs: duration as f32,
        waveform,
    })
}

/// Preserve service-provided signed URLs; never construct or refresh signatures.
/// Optional malformed media must not discard the surrounding message.
fn attachment_url(value: &Value, channel: ChannelId, attachment: AttachmentId) -> Option<String> {
    let raw = value.as_str().filter(|raw| {
        raw.len() <= 2048 && !raw.chars().any(char::is_control) && !raw.contains('\\')
    })?;
    let url = url::Url::parse(raw).ok()?;
    if url.scheme() != "https"
        || !matches!(
            url.host_str(),
            Some("cdn.discordapp.com" | "media.discordapp.net")
        )
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let mut parts: Vec<_> = url.path_segments()?.collect();
    // A forwarded file keeps its source message's ID before its own.
    if parts.len() == 5 && parts[2].parse::<u64>().is_ok() {
        parts.remove(2);
    }
    if parts.len() != 4
        || parts[0] != "attachments"
        || parts[1] != channel.0.to_string()
        || parts[2] != attachment.0.to_string()
        || parts[3].is_empty()
        || parts[3].to_ascii_lowercase().contains("%2f")
        || parts[3].to_ascii_lowercase().contains("%5c")
    {
        return None;
    }
    Some(raw.to_owned())
}

/// A reaction's Unicode emoji; `None` for a custom one.
pub(crate) fn reaction_emoji(value: &Value) -> Result<Option<String>, LoginError> {
    // The current domain renders Unicode reactions. Custom assets need
    // a separate model before they can be represented without guessing.
    if value.get("id").is_some_and(|id| !id.is_null()) {
        return Ok(None);
    }
    value["name"]
        .as_str()
        .filter(|emoji| !emoji.is_empty() && emoji.len() <= 128)
        .map(|emoji| Some(emoji.to_owned()))
        .ok_or_else(invalid)
}

/// The colors of a super reaction, `#rrggbb` each; others are left out.
pub(crate) fn burst_colors(value: &Value) -> Vec<rune_model::Rgb> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|color| {
            let hex = color.as_str()?;
            let hex = hex.strip_prefix('#').unwrap_or(hex);
            (hex.len() == 6)
                .then(|| u32::from_str_radix(hex, 16).ok())
                .flatten()
        })
        .take(rune_model::MAX_BURST_COLORS)
        .map(rune_model::Rgb)
        .collect()
}

fn reactions(value: &Value) -> Result<Vec<Reaction>, LoginError> {
    let count = |value: &Value| {
        value
            .as_u64()
            .and_then(|count| u32::try_from(count).ok())
            .ok_or_else(invalid)
    };
    let mut output = Vec::new();
    for item in array(value, 100)? {
        let Some(emoji) = reaction_emoji(&item["emoji"])? else {
            continue;
        };
        let total = count(&item["count"])?;
        // `count` adds super reactions to normal ones; the details split
        // them, as the web client reads them.
        let details = &item["count_details"];
        let (normal, burst) = if details.is_object() {
            let part = |key: &str| match &details[key] {
                Value::Null => Ok(0),
                value => count(value),
            };
            (part("normal")?, part("burst")?)
        } else {
            (total, 0)
        };
        output.push(Reaction {
            emoji,
            count: normal,
            me: item.get("me").and_then(Value::as_bool).unwrap_or(false),
            burst,
            me_burst: item
                .get("me_burst")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            burst_colors: burst_colors(&item["burst_colors"]),
        });
    }
    Ok(output)
}

pub(crate) fn message(value: &Value) -> Result<(Message, Vec<User>), LoginError> {
    let author = user(&value["author"])?;
    let mut result = Message {
        id: MessageId(id(&value["id"])?),
        channel: ChannelId(id(&value["channel_id"])?),
        author: author.id,
        content: content(&value["content"])?,
        timestamp: timestamp(&value["timestamp"])?,
        edited: None,
        kind: crate::rich::kind(&value["type"]),
        reply_to: None,
        mentions: Vec::new(),
        reactions: Vec::new(),
        attachments: Vec::new(),
        stickers: Vec::new(),
        text_references: Vec::new(),
        delivery: Delivery::Sent,
        pinned: false,
        embeds: Vec::new(),
        forwarded: crate::rich::forwarded(value),
        poll: None,
        components: Default::default(),
        call: None,
        mention_roles: Vec::new(),
        mention_everyone: false,
    };
    let (patch, mut users) = message_patch(value)?;
    users.extend(crate::rich::forwarded_mentions(value));
    if !patch.apply(&mut result) {
        return Err(invalid());
    }
    // Its app and flags, with parts or without: a reply only the account
    // sees may be text alone (P3).
    let (app, flags) = crate::components::marks(value);
    result.components.app = app;
    result.components.flags = flags;
    result.components.ephemeral = flags & crate::components::EPHEMERAL != 0;
    result.components.invoked = crate::components::invoked(value);
    Ok((result, users))
}

/// Gateway message updates are patches: absence must never erase old fields.
pub(crate) fn message_patch(value: &Value) -> Result<(MessagePatch, Vec<User>), LoginError> {
    let mut patch = MessagePatch {
        id: MessageId(id(&value["id"])?),
        channel: ChannelId(id(&value["channel_id"])?),
        ..Default::default()
    };
    let mut users = Vec::new();
    if let Some(value) = value.get("author") {
        patch.author = Some(UserId(id(&value["id"])?));
        // A partial author asserts identity without inventing profile fields.
        if value.get("username").is_some() {
            users.push(user(value)?);
        }
    }
    if let Some(value) = value.get("content") {
        let text = content(value)?;
        patch.text_references = Some(crate::text::references(&text));
        patch.content = Some(text);
    }
    if let Some(value) = value.get("edited_timestamp") {
        patch.edited = Some((!value.is_null()).then(|| timestamp(value)).transpose()?);
    }
    if let Some(value) = value.get("pinned") {
        patch.pinned = Some(value.as_bool().ok_or_else(invalid)?);
    }
    if let Some(value) = value.get("message_reference") {
        if !value.is_null() && !value.is_object() {
            return Err(invalid());
        }
        // A forward points at its source without answering it.
        patch.reply_to = Some(if crate::rich::forwards(value) {
            None
        } else {
            value
                .get("message_id")
                .filter(|value| !value.is_null())
                .map(id)
                .transpose()?
                .map(MessageId)
        });
    }
    if let Some(value) = value.get("mentions") {
        let mut mentions = Vec::new();
        for mention in array(value, 100)? {
            let mention = user(mention)?;
            mentions.push(mention.id);
            users.push(mention);
        }
        patch.mentions = Some(mentions);
    }
    // Rail and notifications: the roles and everyone told, which the text
    // alone does not say. Only what notifies: a malformed list tells no role
    // rather than dropping the message or the session.
    if let Some(value) = value.get("mention_roles") {
        patch.mention_roles = Some(
            array(value, 250)
                .map(|roles| {
                    roles
                        .iter()
                        .filter_map(|role| id(role).ok().map(rune_model::RoleId))
                        .collect()
                })
                .unwrap_or_default(),
        );
    }
    if let Some(value) = value.get("mention_everyone") {
        patch.mention_everyone = Some(value.as_bool().unwrap_or(false));
    }
    if let Some(value) = value.get("attachments") {
        patch.attachments = Some(attachments(value, patch.channel)?);
    }
    if let Some(value) = value.get("sticker_items") {
        patch.stickers = Some(crate::stickers::decode(value, false)?);
    }
    if let Some(value) = value.get("reactions") {
        patch.reactions = Some(reactions(value)?);
    }
    // A link's preview usually arrives in a later update holding only cards.
    if let Some(value) = value.get("embeds") {
        patch.embeds = Some(crate::rich::embeds(value));
    }
    if let Some(value) = value.get("poll") {
        patch.poll = crate::rich::poll(value);
    }
    if value.get("components").is_some() {
        patch.components = Some(crate::components::message(value));
    }
    // A call's message learns its end in an update (contenido-de-mensajes).
    if let Some(value) = value.get("call") {
        patch.call = crate::rich::call(value);
    }
    Ok((patch, users))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn attachments_keep_signed_urls_without_guessing_signatures_and_validate_paths() {
        let mut value = sample();
        let url = "https://cdn.discordapp.com/attachments/20/200/test.png?ex=ffffffff&is=1234&hm=private-signature";
        value["attachments"][0]["url"] = json!(url);
        let (mapped, _) = message(&value).unwrap();
        let attachment = &mapped.attachments[0];
        assert_eq!(attachment.download.as_ref().unwrap().url, url);
        assert_eq!(attachment.preview.as_ref().unwrap().url, url);
        assert_eq!(
            attachment.preview.as_ref().unwrap().kind,
            ImageKind::AttachmentPreview
        );
        assert!(!format!("{mapped:?}").contains("private-signature"));
        for bad in [
            "http://cdn.discordapp.com/attachments/20/200/test.png",
            "https://cdn.discordapp.com.evil.test/attachments/20/200/test.png",
            "https://user:password@cdn.discordapp.com/attachments/20/200/test.png",
            "https://cdn.discordapp.com:8443/attachments/20/200/test.png",
            "https://cdn.discordapp.com/attachments/21/200/test.png",
            "https://cdn.discordapp.com/attachments/20/201/test.png",
            "https://cdn.discordapp.com/attachments/20/200/test%2fescape.png",
            "https://cdn.discordapp.com/attachments/20/200/test.png#fragment",
            "https://cdn.discordapp.com/attachments/20/200/test\\escape.png",
        ] {
            value["attachments"][0]["url"] = json!(bad);
            let (mapped, _) = message(&value).unwrap();
            assert!(mapped.attachments[0].preview.is_none());
            assert!(mapped.attachments[0].download.is_none());
            assert_eq!(mapped.attachments[0].filename, "test.png");
        }
        value["attachments"][0]["proxy_url"] =
            json!("https://media.discordapp.net/attachments/20/200/test.png?hm=supplied");
        let (mapped, _) = message(&value).unwrap();
        assert!(mapped.attachments[0].preview.is_some());
        assert!(
            mapped.attachments[0].download.is_none(),
            "a proxy is not the original file"
        );
    }

    #[test]
    fn unsupported_large_and_spoiler_attachments_remain_explicit_downloads() {
        for (field, value) in [
            ("width", json!(4097)),
            ("size", json!(8 * 1024 * 1024 + 1)),
            ("content_type", json!("application/pdf")),
        ] {
            let mut data = sample();
            data["attachments"][0]["url"] =
                json!("https://cdn.discordapp.com/attachments/20/200/test.png");
            data["attachments"][0][field] = value;
            let (mapped, _) = message(&data).unwrap();
            assert!(mapped.attachments[0].preview.is_none());
            assert!(mapped.attachments[0].download.is_some());
            assert!(!mapped.attachments[0].spoiler);
        }
        // Spoilers keep their preview reference for the reveal to request.
        for (field, value) in [("filename", json!("SPOILER_test.png")), ("flags", json!(8))] {
            let mut data = sample();
            data["attachments"][0]["url"] =
                json!("https://cdn.discordapp.com/attachments/20/200/test.png");
            data["attachments"][0][field] = value;
            let (mapped, _) = message(&data).unwrap();
            assert!(mapped.attachments[0].spoiler);
            assert!(mapped.attachments[0].preview.is_some());
        }
        let mut duplicate = sample();
        let attachment = duplicate["attachments"][0].clone();
        duplicate["attachments"]
            .as_array_mut()
            .unwrap()
            .push(attachment);
        assert!(
            message(&duplicate).is_err(),
            "attachment action ids must be unique"
        );
    }

    #[test]
    fn voice_messages_keep_bounded_length_and_loudness_and_never_drop_the_message() {
        let mut data = sample();
        let attachment = &mut data["attachments"][0];
        attachment["filename"] = json!("voice-message.ogg");
        attachment["content_type"] = json!("audio/ogg");
        attachment["url"] =
            json!("https://cdn.discordapp.com/attachments/20/200/voice-message.ogg");
        attachment["duration_secs"] = json!(4.5);
        // 0, 128 and 255.
        attachment["waveform"] = json!("AID/");
        let (mapped, _) = message(&data).unwrap();
        let voice = mapped.attachments[0].voice.as_ref().unwrap();
        assert_eq!(voice.duration_secs, 4.5);
        assert_eq!(voice.waveform, [0, 128, 255]);
        assert!(mapped.attachments[0].download.is_some());
        // A bad or oversized waveform draws flat; the clip still plays.
        for waveform in [json!("no es base64"), json!(7), json!("A".repeat(400))] {
            data["attachments"][0]["waveform"] = waveform;
            let (mapped, _) = message(&data).unwrap();
            assert!(mapped.attachments[0]
                .voice
                .as_ref()
                .unwrap()
                .waveform
                .is_empty());
        }
        // A length that cannot be true leaves a plain file, and the message.
        for duration in [json!(0), json!(-1), json!(3601), json!("4"), json!(null)] {
            data["attachments"][0]["duration_secs"] = duration;
            let (mapped, _) = message(&data).unwrap();
            assert!(mapped.attachments[0].voice.is_none());
            assert_eq!(mapped.attachments[0].filename, "voice-message.ogg");
        }
        assert!(message(&sample()).unwrap().0.attachments[0].voice.is_none());
    }

    #[test]
    fn gif_attachments_request_a_bounded_preview_without_changing_the_download() {
        for mime in [Some("image/gif"), None] {
            let mut data = sample();
            let attachment = &mut data["attachments"][0];
            attachment["filename"] = json!("motion.GIF");
            attachment["url"] = json!("https://cdn.discordapp.com/attachments/20/200/motion.gif");
            if let Some(mime) = mime {
                attachment["content_type"] = json!(mime);
            } else {
                attachment.as_object_mut().unwrap().remove("content_type");
            }
            let (mapped, _) = message(&data).unwrap();
            let preview = mapped.attachments[0].preview.as_ref().unwrap();
            assert_eq!(preview.kind, ImageKind::AttachmentPreview);
            assert_eq!(
                preview.url,
                mapped.attachments[0].download.as_ref().unwrap().url
            );
            data["attachments"][0]["filename"] = json!("SPOILER_motion.gif");
            assert!(message(&data).unwrap().0.attachments[0].spoiler);
        }
    }

    #[test]
    fn profile_banner_distinguishes_partial_null_and_valid_artwork() {
        let hash = "a_0123456789abcdef0123456789abcdef";
        let mut value = json!({"id":"42", "username":"Owner"});
        assert_eq!(user(&value).unwrap().banner, ProfileBanner::Unknown);
        value["banner"] = Value::Null;
        assert_eq!(user(&value).unwrap().banner, ProfileBanner::Absent);
        value["banner"] = json!(hash);
        assert_eq!(
            user(&value).unwrap().banner,
            ProfileBanner::Image(RemoteImage {
                url: format!("https://cdn.discordapp.com/banners/42/{hash}.png?size=512"),
                kind: ImageKind::Banner,
            })
        );
        for invalid in [json!("../secret"), json!("x"), json!(true), json!({})] {
            value["banner"] = invalid;
            assert_eq!(user(&value).unwrap().banner, ProfileBanner::Unknown);
        }
        assert_eq!(
            message(&sample()).unwrap().1[0].banner,
            ProfileBanner::Unknown
        );
    }

    #[test]
    fn artwork_uses_fixed_public_png_paths_and_bad_hashes_keep_initials() {
        let hash = "a_0123456789abcdef0123456789abcdef";
        let owner = user(&json!({"id":"42","username":"Owner","avatar":hash})).unwrap();
        assert_eq!(
            owner.avatar.unwrap().url,
            format!("https://cdn.discordapp.com/avatars/42/{hash}.png?size=128")
        );
        for hash in [
            "../secret",
            "x",
            "https://evil.test/image",
            "0123456789abcdef0123456789abcde/x",
        ] {
            let user = user(&json!({"id":"42","username":"Owner","avatar":hash})).unwrap();
            assert!(user.avatar.is_none());
        }
        assert!(image("attachments", 42, &json!("0".repeat(32))).is_none());
    }

    #[test]
    fn without_a_picture_of_their_own_people_wear_discord_s_default_one() {
        let url = |value: Value| user(&value).unwrap().default_avatar.unwrap().url;
        // A new-style name: by when the account was made, (id >> 22) % 6.
        let id = (1_234_567u64 << 22) | 99;
        assert_eq!(
            url(json!({"id": id.to_string(), "username":"ada", "discriminator":"0"})),
            format!(
                "https://cdn.discordapp.com/embed/avatars/{}.png",
                1_234_567 % 6
            )
        );
        assert_eq!(
            url(json!({"id": id.to_string(), "username":"ada"})),
            "https://cdn.discordapp.com/embed/avatars/1.png"
        );
        // An old four-digit tag: by the tag, % 5.
        assert_eq!(
            url(json!({"id":"42", "username":"bot", "discriminator":"1337"})),
            "https://cdn.discordapp.com/embed/avatars/2.png"
        );
        // A webhook's author: a bot tagged «0000».
        assert_eq!(
            url(
                json!({"id": id.to_string(), "username":"hook", "discriminator":"0000", "bot":true})
            ),
            "https://cdn.discordapp.com/embed/avatars/0.png"
        );
        // Their own picture still wins where it is shown.
        let hash = "0123456789abcdef0123456789abcdef";
        let owner = user(&json!({"id":"42","username":"Owner","avatar":hash})).unwrap();
        assert_eq!(owner.picture(), owner.avatar.as_ref());
        assert!(owner.default_avatar.is_some());
    }

    pub(crate) fn sample() -> Value {
        json!({"id":"100", "channel_id":"20", "author":{"id":"42","username":"Owner"},
            "content":"hello", "timestamp":"2026-10-14T12:34:56.000Z", "pinned":true,
            "reactions":[{"emoji":{"id":null,"name":"👍"},"count":2,"me":true}],
            "attachments":[{"id":"200","filename":"test.png","size":32,"width":20,"height":30}]})
    }

    #[test]
    fn complete_message_preserves_author_timestamp_and_attachment_metadata() {
        let (message, users) = message(&sample()).unwrap();
        assert_eq!(users[0].id, message.author);
        assert_eq!(message.timestamp, Timestamp(1_791_981_296_000));
        assert_eq!(message.attachments[0].dimensions, Some((20, 30)));
        assert!(message.pinned && message.reactions[0].me);
    }

    #[test]
    fn super_reactions_are_counted_apart_from_normal_ones() {
        let mut value = sample();
        value["reactions"] = json!([
            {"emoji":{"id":null,"name":"🎉"},"count":5,"me":true,"me_burst":true,
                "count_details":{"normal":3,"burst":2},"burst_colors":["#f9a62b","#E8433A","x"]},
            {"emoji":{"id":null,"name":"🚀"},"count":1,"count_details":{"burst":1}},
            // A history without the details: every one is normal.
            {"emoji":{"id":null,"name":"👍"},"count":2}
        ]);
        let (message, _) = message(&value).unwrap();
        let counts: Vec<_> = message
            .reactions
            .iter()
            .map(|reaction| {
                (
                    reaction.count,
                    reaction.burst,
                    reaction.me,
                    reaction.me_burst,
                )
            })
            .collect();
        assert_eq!(
            counts,
            [
                (3, 2, true, true),
                (0, 1, false, false),
                (2, 0, false, false)
            ]
        );
        assert_eq!(
            message.reactions[0].burst_colors,
            [rune_model::Rgb(0xf9a62b), rune_model::Rgb(0xe8433a)]
        );
        value["reactions"][0]["count_details"]["burst"] = json!(-1);
        assert!(super::message(&value).is_err(), "a count must be a count");
    }

    #[test]
    fn the_roles_a_message_tells_are_read_and_kept_by_an_edit_without_them() {
        let mut value = sample();
        value["mention_roles"] = json!(["9", "10"]);
        let (mut told, _) = message(&value).unwrap();
        assert_eq!(
            told.mention_roles,
            [rune_model::RoleId(9), rune_model::RoleId(10)]
        );
        let (patch, _) =
            message_patch(&json!({"id": "100", "channel_id": "20", "content": "otro"})).unwrap();
        assert!(patch.apply(&mut told));
        assert_eq!(told.mention_roles.len(), 2, "absent: kept");
        let (patch, _) =
            message_patch(&json!({"id": "100", "channel_id": "20", "mention_roles": []})).unwrap();
        assert!(patch.apply(&mut told));
        assert!(told.mention_roles.is_empty());
        // Malformed, it tells no role and keeps the message.
        value["mention_roles"] = json!(["nine", "9"]);
        assert_eq!(
            message(&value).unwrap().0.mention_roles,
            [rune_model::RoleId(9)]
        );
        for malformed in [
            json!(null),
            json!((0..251).map(|n| n.to_string()).collect::<Vec<_>>()),
        ] {
            value["mention_roles"] = malformed;
            assert!(message(&value).unwrap().0.mention_roles.is_empty());
        }
        assert!(message(&sample()).unwrap().0.mention_roles.is_empty());
    }

    #[test]
    fn telling_everyone_is_read_and_kept_by_an_edit_without_it() {
        let mut value = sample();
        assert!(!message(&value).unwrap().0.mention_everyone);
        value["mention_everyone"] = json!(true);
        let (mut told, _) = message(&value).unwrap();
        assert!(told.mention_everyone);
        let (patch, _) =
            message_patch(&json!({"id": "100", "channel_id": "20", "content": "otro"})).unwrap();
        assert!(patch.apply(&mut told));
        assert!(told.mention_everyone, "absent: kept");
        let (patch, _) =
            message_patch(&json!({"id": "100", "channel_id": "20", "mention_everyone": "true"}))
                .unwrap();
        assert!(patch.apply(&mut told));
        assert!(!told.mention_everyone, "malformed: nobody");
    }

    #[test]
    fn forwards_are_not_replies_and_cards_arrive_by_update() {
        let mut value = sample();
        value["message_reference"] = json!({"type": 1, "channel_id": "30", "message_id": "300"});
        value["message_snapshots"] =
            json!([{"message": {"content": "copia", "timestamp": "2026-10-14T12:00:00Z"}}]);
        let (forward, _) = message(&value).unwrap();
        assert_eq!(forward.reply_to, None);
        assert_eq!(forward.forwarded.unwrap().content, "copia");
        value["message_reference"]["type"] = json!(0);
        let (reply, _) = message(&value).unwrap();
        assert_eq!(reply.reply_to, Some(MessageId(300)));
        assert!(reply.forwarded.is_none());
        let (mut before, _) = message(&sample()).unwrap();
        let (patch, _) = message_patch(
            &json!({"id": "100", "channel_id": "20", "embeds": [{"title": "Vista previa"}]}),
        )
        .unwrap();
        assert!(patch.apply(&mut before));
        assert_eq!(before.embeds[0].title.as_deref(), Some("Vista previa"));
        assert_eq!(before.content, "hello");
    }

    /// Message content (contenido-de-mensajes): a forward's text keeps its
    /// references and the people it names; files carry their ThumbHash and
    /// the explicit mark; a call's record who joined and when it ended.
    #[test]
    fn forwards_name_their_references_files_their_placeholder_and_calls_their_end() {
        use rune_model::{TextTarget, UserId};
        let mut value = sample();
        value["message_reference"] = json!({"type": 1, "channel_id": "30", "message_id": "300"});
        value["message_snapshots"] = json!([{"message": {
            "content": "<:rust:55> <@77> en <#31>", "timestamp": "2026-10-14T12:00:00Z",
            "mentions": [{"id": "77", "username": "bruno"}, {"id": "bad"}]}}]);
        let (forward, users) = message(&value).unwrap();
        let copy = forward.forwarded.unwrap();
        let targets: Vec<_> = copy
            .text_references
            .iter()
            .map(|reference| &reference.target)
            .collect();
        assert!(targets
            .iter()
            .any(|target| matches!(target, TextTarget::Emoji { name, .. } if name == "rust")));
        assert!(targets.contains(&&TextTarget::User(UserId(77))));
        assert!(targets.contains(&&TextTarget::Channel(ChannelId(31))));
        assert!(users
            .iter()
            .any(|user| user.id == UserId(77) && user.name == "bruno"));

        let file = |extra: Value| {
            let mut value = sample();
            for (key, item) in extra.as_object().unwrap() {
                value["attachments"][0][key] = item.clone();
            }
            message(&value).unwrap().0.attachments.remove(0)
        };
        let hash = "oPcNHZpwd3dxeHiHh3eHh9jQDPh4";
        let found = file(json!({"placeholder": hash, "placeholder_version": 1, "flags": 16}));
        let placeholder = found.placeholder.unwrap();
        assert_eq!(
            (placeholder.url.as_str(), placeholder.kind),
            (hash, ImageKind::ThumbHash)
        );
        assert!(found.explicit && !found.spoiler);
        // Padding is dropped; another version, a bad or long one, is no placeholder.
        let padded = file(
            json!({"placeholder": "XwiCAwIzkISK/neQiQepeDd4h3iAi7c=", "placeholder_version": 1}),
        );
        assert_eq!(
            padded.placeholder.unwrap().url,
            "XwiCAwIzkISK/neQiQepeDd4h3iAi7c"
        );
        for (placeholder, version) in [(hash, 2), ("not base64!", 1), ("AAAA", 1)] {
            let found = file(json!({"placeholder": placeholder, "placeholder_version": version}));
            assert!(found.placeholder.is_none(), "{placeholder} {version}");
        }
        assert!(
            file(json!({"placeholder": "A".repeat(200), "placeholder_version": 1}))
                .placeholder
                .is_none()
        );
        assert!(!file(json!({"flags": 8})).explicit);
        let mut card = sample();
        card["embeds"] = json!([{"title": "x", "flags": 16}, {"title": "y"}]);
        let cards = message(&card).unwrap().0.embeds;
        assert!(cards[0].explicit && !cards[1].explicit);

        let mut call = sample();
        call["type"] = json!(3);
        call["call"] = json!({"participants": ["42", "77"], "ended_timestamp": null});
        let (mut record, _) = message(&call).unwrap();
        let started = record.call.clone().unwrap();
        assert_eq!(started.participants, [UserId(42), UserId(77)]);
        assert_eq!(started.ended, None);
        // Its end arrives in an update; a malformed one tells nothing.
        let (patch, _) = message_patch(&json!({"id": "100", "channel_id": "20",
            "call": {"participants": ["42"], "ended_timestamp": "2026-10-14T12:46:56.000Z"}}))
        .unwrap();
        assert!(patch.apply(&mut record));
        let ended = record.call.unwrap();
        assert_eq!(ended.participants, [UserId(42)]);
        assert_eq!(
            ended.ended,
            Some(Timestamp(1_791_981_296_000 + 12 * 60_000))
        );
        call["call"] = json!({"participants": ["x"]});
        assert!(message(&call).unwrap().0.call.is_none());
    }

    #[test]
    fn partial_update_preserves_unmentioned_fields_and_handles_explicit_null() {
        let (before, _) = message(&sample()).unwrap();
        let (patch, _) = message_patch(&json!({"id":"100","channel_id":"20",
            "content":"edited", "edited_timestamp":"2026-10-14T12:35:00Z"}))
        .unwrap();
        let mut after = before.clone();
        assert!(patch.apply(&mut after));
        assert_eq!(after.content, "edited");
        assert_eq!(after.attachments, before.attachments);
        assert_eq!(after.reactions, before.reactions);
        assert_eq!(after.timestamp, before.timestamp);
        assert!(after.pinned && after.edited.is_some());
        let (patch, _) = message_patch(&json!({"id":"100","channel_id":"20",
            "edited_timestamp":null,"attachments":[]}))
        .unwrap();
        assert_eq!(patch.edited, Some(None));
        assert!(
            !patch.apply(&mut after),
            "explicit null cannot undo a confirmed edit"
        );
        let mut unedited = before;
        assert!(patch.apply(&mut unedited));
        assert!(unedited.edited.is_none() && unedited.attachments.is_empty());
    }

    #[test]
    fn untrusted_ids_and_mismatched_partial_messages_are_rejected() {
        for value in [json!("0"), json!("01"), json!("-1"), json!(0), json!(true)] {
            assert!(id(&value).is_err());
        }
        let (mut message, _) = message(&sample()).unwrap();
        let (patch, _) = message_patch(&json!({"id":"101","channel_id":"20"})).unwrap();
        assert!(!patch.apply(&mut message));
        let mut oversized = sample();
        oversized["content"] = json!("x".repeat(32_769));
        assert!(super::message(&oversized).is_err());
    }

    #[test]
    fn sparse_patches_preserve_absence_and_decode_explicit_empty_fields() {
        let (patch, users) = message_patch(&json!({"id":"100","channel_id":"20"})).unwrap();
        assert_eq!(
            patch,
            MessagePatch {
                id: MessageId(100),
                channel: ChannelId(20),
                ..Default::default()
            }
        );
        assert!(users.is_empty());
        let (patch, users) = message_patch(&json!({"id":"100","channel_id":"20",
            "author":{"id":"42"}, "content":"", "edited_timestamp":null,
            "message_reference":null, "mentions":[], "reactions":[], "attachments":[],
            "sticker_items":[], "pinned":false}))
        .unwrap();
        assert_eq!(patch.author, Some(UserId(42)));
        assert_eq!(patch.content.as_deref(), Some(""));
        assert_eq!(patch.text_references, Some(Vec::new()));
        assert_eq!(patch.edited, Some(None));
        assert_eq!(patch.reply_to, Some(None));
        assert_eq!(patch.mentions, Some(Vec::new()));
        assert_eq!(patch.reactions, Some(Vec::new()));
        assert_eq!(patch.attachments, Some(Vec::new()));
        assert_eq!(patch.stickers, Some(Vec::new()));
        assert_eq!(patch.pinned, Some(false));
        assert!(users.is_empty());
        for (key, value) in [
            ("content", json!(null)),
            ("pinned", json!(null)),
            ("author", json!({"id":"0"})),
            ("message_reference", json!("bad")),
            ("attachments", json!(vec![json!({}); 65])),
            ("mentions", json!(vec![json!({}); 101])),
        ] {
            let mut data = json!({"id":"100","channel_id":"20"});
            data[key] = value;
            assert!(message_patch(&data).is_err(), "{key}");
        }
    }

    #[test]
    fn an_apps_parts_arrive_with_the_message_and_malformed_ones_keep_it() {
        let mut value = sample();
        value["flags"] = json!(1 << 15);
        value["components"] = json!([{"type": 10, "content": "Hola"}, {"type": "x"}]);
        let (mapped, _) = message(&value).unwrap();
        assert!(mapped.components.whole);
        assert_eq!(mapped.components.parts.len(), 1);
        value["components"] = json!("not a list");
        let (mapped, _) = message(&value).unwrap();
        assert!(mapped.components.is_empty(), "the message stays");
        let (patch, _) = message_patch(&json!({"id":"100","channel_id":"20"})).unwrap();
        assert_eq!(patch.components, None, "absent parts keep the old ones");
    }

    #[test]
    fn a_cdn_picture_is_asked_at_the_size_a_dense_screen_draws_it() {
        let avatar = "https://cdn.discordapp.com/avatars/42/a_0123.png?size=128";
        assert_eq!(
            super::sized(avatar, 1.0),
            None,
            "density 1 asks what it says"
        );
        assert_eq!(
            super::sized(avatar, 2.0).as_deref(),
            Some("https://cdn.discordapp.com/avatars/42/a_0123.png?size=256")
        );
        assert_eq!(
            super::sized(avatar, 1.5).as_deref(),
            Some("https://cdn.discordapp.com/avatars/42/a_0123.png?size=256")
        );
        let decoration =
            "https://cdn.discordapp.com/avatar-decoration-presets/a_1.png?size=96&passthrough=true";
        assert_eq!(
            super::sized(decoration, 2.0).as_deref(),
            Some("https://cdn.discordapp.com/avatar-decoration-presets/a_1.png?size=256&passthrough=true")
        );
        assert_eq!(
            super::sized("https://cdn.discordapp.com/banners/1/b.png?size=4096", 2.0),
            None,
            "the largest the CDN serves"
        );
        // An attachment, the media proxy or another origin: as they are.
        for other in [
            "https://cdn.discordapp.com/attachments/1/2/a.png?ex=1",
            "https://media.discordapp.net/external/a/b.png?width=160&height=160",
            "https://example.test/a.png?size=128",
        ] {
            assert_eq!(super::sized(other, 2.0), None, "{other}");
        }
    }
}
