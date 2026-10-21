//! Message kinds, cards, forwards and polls. They are optional decoration:
//! a malformed one is dropped, never the message that carries it.

use rune_model::{
    ChannelId, Embed, EmbedField, EmbedImage, EmbedKind, Forwarded, ImageKind, MessageKind, Poll,
    PollAnswer, RemoteImage, Timestamp,
};
use serde_json::Value;

use crate::wire::{attachments, timestamp};

/// The service's message types, as listed in its public API documentation.
pub(crate) fn kind(value: &Value) -> MessageKind {
    match value.as_u64() {
        // Replies and app commands read as ordinary messages.
        None | Some(0 | 19 | 20 | 23) => MessageKind::Default,
        Some(1) => MessageKind::RecipientAdd,
        Some(2) => MessageKind::RecipientRemove,
        Some(3) => MessageKind::Call,
        Some(4) => MessageKind::ChannelRename,
        Some(5) => MessageKind::ChannelIcon,
        Some(6) => MessageKind::Pinned,
        Some(7) => MessageKind::Join,
        Some(8) => MessageKind::Boost { tier: 0 },
        Some(tier @ 9..=11) => MessageKind::Boost {
            tier: tier as u8 - 8,
        },
        Some(18) => MessageKind::ThreadCreated,
        Some(46) => MessageKind::PollResult,
        Some(other) => MessageKind::Other(u16::try_from(other).unwrap_or(u16::MAX)),
    }
}

/// Whether a `message_reference` forwards its message instead of answering it.
pub(crate) fn forwards(reference: &Value) -> bool {
    reference["type"].as_u64() == Some(1)
}

/// Where card pictures may load from: the service's image proxy, so a page
/// never learns who looked at its preview.
pub(crate) const IMAGE_PROXIES: [&str; 3] = [
    "https://media.discordapp.net",
    "https://images-ext-1.discordapp.net",
    "https://images-ext-2.discordapp.net",
];

/// All of a message's cards keep within this many bytes of text and addresses.
const CARD_BUDGET: usize = 64 * 1024;

/// The card list, at most ten as the service allows and within
/// [`CARD_BUDGET`]; an absent or malformed list is no cards.
pub(crate) fn embeds(value: &Value) -> Vec<Embed> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };
    let mut spent = 0;
    items
        .iter()
        .take(10)
        .map(embed)
        .filter(|embed| !embed.is_empty())
        .map(|mut embed| {
            let mut references = Vec::new();
            let parts = crate::components::texts(&embed.components);
            for text in embed
                .description
                .iter()
                .chain(embed.fields.iter().map(|field| &field.value))
                .map(String::as_str)
                .chain(parts)
            {
                for reference in crate::text::references(text) {
                    if !references.contains(&reference) {
                        references.push(reference);
                    }
                }
            }
            embed.text_references = references;
            embed
        })
        .take_while(|embed| {
            spent += size(embed);
            spent <= CARD_BUDGET
        })
        .collect()
}

fn size(embed: &Embed) -> usize {
    let text = [
        &embed.provider,
        &embed.author,
        &embed.title,
        &embed.url,
        &embed.description,
        &embed.footer,
    ];
    text.into_iter().flatten().map(String::len).sum::<usize>()
        + embed
            .fields
            .iter()
            .map(|field| field.name.len() + field.value.len())
            .sum::<usize>()
        + [&embed.image, &embed.thumbnail]
            .into_iter()
            .flatten()
            .map(|image| image.source.url.len())
            .sum::<usize>()
        + crate::components::texts(&embed.components)
            .iter()
            .map(|text| text.len())
            .sum::<usize>()
}

/// The card types the service uses for its own notices about the account,
/// as its client names them. Their fields are machine data, not text to read.
const NOTICES: [&str; 3] = [
    "safety_system_notification",
    "safety_policy_notice",
    "age_verification_system_notification",
];

fn embed(value: &Value) -> Embed {
    let kind = match value["type"].as_str() {
        None | Some("rich" | "article" | "link") => EmbedKind::Rich,
        Some("image") => EmbedKind::Image,
        Some("gifv") => EmbedKind::Gifv,
        Some("video") => EmbedKind::Video,
        Some(notice) if NOTICES.contains(&notice) => EmbedKind::SystemNotice,
        Some("poll_result") => EmbedKind::PollResult,
        Some("voice_channel") => EmbedKind::VoiceChannel,
        Some("components") => EmbedKind::Components,
        Some(_) => EmbedKind::Other,
    };
    let embed = Embed {
        kind,
        image: still(&value["image"]),
        text_references: Vec::new(),
        thumbnail: still(&value["thumbnail"]),
        provider: text(&value["provider"]["name"], 256),
        author: text(&value["author"]["name"], 256),
        title: text(&value["title"], 256),
        url: link(&value["url"]),
        description: text(&value["description"], 4096),
        fields: value["fields"]
            .as_array()
            .map(|fields| {
                fields
                    .iter()
                    .take(25)
                    .filter_map(|field| {
                        Some(EmbedField {
                            name: text(&field["name"], 256)?,
                            value: text(&field["value"], 1024)?,
                            inline: field["inline"].as_bool().unwrap_or(false),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        footer: text(&value["footer"]["text"], 2048),
        timestamp: timestamp(&value["timestamp"]).ok(),
        color: value["color"]
            .as_u64()
            .filter(|color| *color <= 0xFF_FF_FF)
            .map(|color| color as u32),
        components: if kind == EmbedKind::Components {
            crate::components::parts(&value["components"])
        } else {
            Vec::new()
        },
        channel: None,
        // A GIF from a GIF site arrives as a short MP4 through the proxy.
        video: (kind == EmbedKind::Gifv)
            .then(|| media(&value["video"]))
            .flatten()
            .map(|mut clip| {
                clip.source.kind = ImageKind::Clip;
                clip
            }),
        explicit: value["flags"]
            .as_u64()
            .is_some_and(|flags| flags & crate::wire::EXPLICIT_MEDIA != 0),
    };
    match kind {
        EmbedKind::SystemNotice => notice(embed, value),
        EmbedKind::VoiceChannel => voice(embed),
        EmbedKind::PollResult => poll_result(embed),
        _ => embed,
    }
}

/// A poll's result keeps its winner's server emoji, when it has one, as its
/// thumbnail; the reference client reads it from `victor_answer_emoji_id`.
fn poll_result(mut embed: Embed) -> Embed {
    embed.thumbnail = embed
        .field("victor_answer_emoji_id")
        .and_then(|id| crate::wire::id(&Value::from(id)).ok())
        .map(|id| EmbedImage {
            source: crate::emoji::picture(id),
            dimensions: (128, 128),
        });
    embed
}

/// A voice card's channel, from its link to it.
// shortcut: the reference client keeps this card's raw fields but draws and
// reads none of them (docs/DISCORD.md), so the channel is read only from a link
// `https://discord.com/channels/{server}/{channel}`; read its fields once a
// real card shows which name the channel.
fn voice(mut embed: Embed) -> Embed {
    embed.channel = embed
        .url
        .as_deref()
        .and_then(|url| url::Url::parse(url).ok())
        .filter(|url| url.scheme() == "https" && official(url.as_str()))
        .and_then(|url| {
            let mut path = url.path_segments()?;
            (path.next()? == "channels").then_some(())?;
            path.next()?.parse::<u64>().ok()?;
            let channel = path.next()?.parse().ok()?;
            path.next().is_none().then_some(ChannelId(channel))
        });
    embed
}

/// A notice reads its header, body, date and explanation page from named
/// fields, and keeps none of the fields themselves.
fn notice(mut embed: Embed, value: &Value) -> Embed {
    let field = |name: &str| {
        value["fields"]
            .as_array()?
            .iter()
            .find(|field| field["name"] == name)
            .map(|field| &field["value"])
    };
    if let Some(header) = field("header").and_then(|header| text(header, 256)) {
        embed.title = Some(header);
    }
    if let Some(body) = field("body").and_then(|body| text(body, 4096)) {
        embed.description = Some(body);
    }
    embed.url = field("learn_more_link")
        .and_then(link)
        .or(embed.url)
        .filter(|url| official(url));
    // The service writes its date as seconds since the epoch, with decimals.
    let seconds = field("timestamp").and_then(|at| {
        at.as_f64()
            .or_else(|| at.as_str().and_then(|at| at.trim().parse().ok()))
    });
    if let Some(seconds) = seconds.filter(|s| s.is_finite() && (1.0..1e11).contains(s)) {
        embed.timestamp = Some(Timestamp((seconds * 1000.0) as i64));
    }
    embed.fields.clear();
    embed
}

/// Whether `url` is a page of the service itself, which a notice may send
/// the account to.
fn official(url: &str) -> bool {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_default();
    ["discord.com", "discordapp.com", "discord.gg", "dis.gd"]
        .iter()
        .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

/// The first snapshot of a forward. Its files keep their addresses on the
/// service's file host, checked against the channel they came from, so
/// their pictures show as the original's do.
pub(crate) fn forwarded(value: &Value) -> Option<Forwarded> {
    let reference = &value["message_reference"];
    if !forwards(reference) {
        return None;
    }
    let snapshot = &value["message_snapshots"].as_array()?.first()?["message"];
    let source = reference["channel_id"]
        .as_str()
        .and_then(|id| id.parse().ok())
        .map(ChannelId)
        .unwrap_or_default();
    let files = attachments(&snapshot["attachments"], source).unwrap_or_default();
    let content = text(&snapshot["content"], 32_768).unwrap_or_default();
    Some(Forwarded {
        timestamp: timestamp(&snapshot["timestamp"]).ok()?,
        edited: timestamp(&snapshot["edited_timestamp"]).ok(),
        attachments: files,
        embeds: embeds(&snapshot["embeds"]),
        components: crate::components::forwarded(snapshot),
        // Its mentions, channels and emoji, as a message's own (contenido-de-mensajes).
        text_references: crate::text::references(&content),
        content,
    })
}

/// The people a forward's copy mentions, so their names show in it; a
/// malformed one is left out (contenido-de-mensajes).
pub(crate) fn forwarded_mentions(value: &Value) -> Vec<rune_model::User> {
    if !forwards(&value["message_reference"]) {
        return Vec::new();
    }
    value["message_snapshots"][0]["message"]["mentions"]
        .as_array()
        .map(|mentions| {
            mentions
                .iter()
                .take(100)
                .filter_map(|mention| crate::wire::user(mention).ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Who joined a call's message and when the call ended; a malformed one
/// tells nothing (contenido-de-mensajes).
pub(crate) fn call(value: &Value) -> Option<rune_model::MessageCall> {
    let participants = value["participants"]
        .as_array()?
        .iter()
        .take(256)
        .map(|id| crate::wire::id(id).map(rune_model::UserId))
        .collect::<Result<_, _>>()
        .ok()?;
    Some(rune_model::MessageCall {
        participants,
        ended: timestamp(&value["ended_timestamp"]).ok(),
    })
}

/// A card's picture through the proxy, never the page's own address, with
/// the bounded size an attachment preview has.
pub(crate) fn media(value: &Value) -> Option<EmbedImage> {
    let raw = value["proxy_url"].as_str().filter(|raw| {
        raw.len() <= 2048 && !raw.chars().any(char::is_control) && !raw.contains('\\')
    })?;
    let url = url::Url::parse(raw).ok()?;
    let origin = url.origin().ascii_serialization();
    let path = url.path();
    let proxied = path.starts_with("/external/")
        || (origin == IMAGE_PROXIES[0] && path.starts_with("/attachments/"));
    if url.scheme() != "https"
        || !IMAGE_PROXIES.contains(&origin.as_str())
        || !proxied
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return None;
    }
    let side = |key: &str| {
        value[key]
            .as_u64()
            .and_then(|side| u32::try_from(side).ok())
            .filter(|side| (1..=4096).contains(side))
    };
    let (width, height) = (side("width")?, side("height")?);
    (u64::from(width) * u64::from(height) <= 16_777_216).then(|| EmbedImage {
        source: RemoteImage {
            url: raw.to_owned(),
            kind: ImageKind::AttachmentPreview,
        },
        dimensions: (width, height),
    })
}

/// The longest side a card's still picture is asked for in.
const STILL_SIDE: u32 = 1024;

/// [`media`] for a card's still picture: one the proxy fetched from a page
/// is asked for as the official client asks, re-encoded to PNG at a
/// bounded size (2026-10-21: the bare address of a GitHub preview answered
/// 400 now and then, and the card kept a broken picture). A GIF keeps its
/// bare address, so it still moves; an address with a query is left as is.
fn still(value: &Value) -> Option<EmbedImage> {
    let mut image = media(value)?;
    let url = &image.source.url;
    let external = IMAGE_PROXIES.iter().any(|proxy| {
        url.strip_prefix(proxy)
            .is_some_and(|path| path.starts_with("/external/"))
    });
    if external && !url.contains('?') && !url.to_ascii_lowercase().ends_with(".gif") {
        let (width, height) = image.dimensions;
        let scale = (f64::from(STILL_SIDE) / f64::from(width.max(height))).min(1.0);
        let side = |side: u32| ((f64::from(side) * scale).round() as u32).max(1);
        image.source.url = format!(
            "{url}?format=png&width={}&height={}",
            side(width),
            side(height)
        );
    }
    Some(image)
}

/// A poll with two to ten answers. Without `results` the counts are unknown,
/// not zero.
pub(crate) fn poll(value: &Value) -> Option<Poll> {
    let question = text(&value["question"]["text"], 300)?;
    let mut answers: Vec<PollAnswer> = Vec::new();
    for answer in value["answers"].as_array()?.iter().take(10) {
        let id = answer["answer_id"]
            .as_u64()
            .and_then(|id| u32::try_from(id).ok())
            .filter(|id| *id != 0 && !answers.iter().any(|known| known.id == *id))?;
        let media = &answer["poll_media"];
        let emoji_name = media["emoji"]["name"]
            .as_str()
            .filter(|name| !name.is_empty() && name.len() <= 64);
        // A server's own emoji has no picture here yet: its name stands in.
        let custom = !media["emoji"]["id"].is_null();
        let emoji = emoji_name.filter(|_| !custom).map(str::to_owned);
        let text = text(&media["text"], 55)
            .or_else(|| {
                emoji_name
                    .filter(|_| custom)
                    .map(|name| format!(":{name}:"))
            })
            .unwrap_or_default();
        if text.is_empty() && emoji.is_none() {
            return None;
        }
        answers.push(PollAnswer {
            id,
            text,
            emoji,
            votes: 0,
            me: false,
        });
    }
    if answers.len() < 2 {
        return None;
    }
    let results = &value["results"];
    for count in results["answer_counts"].as_array().into_iter().flatten() {
        let id = count["id"].as_u64();
        if let Some(answer) = answers
            .iter_mut()
            .find(|answer| Some(u64::from(answer.id)) == id)
        {
            answer.votes = count["count"]
                .as_u64()
                .and_then(|votes| u32::try_from(votes).ok())
                .unwrap_or(0);
            answer.me = count["me_voted"].as_bool().unwrap_or(false);
        }
    }
    Some(Poll {
        question,
        answers,
        expiry: timestamp(&value["expiry"]).ok(),
        multiselect: value["allow_multiselect"].as_bool().unwrap_or(false),
        results_known: results.is_object(),
        finalized: results["is_finalized"].as_bool().unwrap_or(false),
    })
}

/// Text without control characters other than line breaks and tabs, cut
/// to `limit` characters; blank is absent.
pub(crate) fn text(value: &Value, limit: usize) -> Option<String> {
    let text: String = value
        .as_str()?
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .take(limit)
        .collect();
    (!text.trim().is_empty()).then_some(text)
}

/// An address a person may open: http(s), no credentials, bounded.
pub(crate) fn link(value: &Value) -> Option<String> {
    let raw = value
        .as_str()
        .filter(|raw| raw.len() <= 2048 && !raw.chars().any(char::is_control))?;
    let url = url::Url::parse(raw).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none())
    .then(|| raw.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn kinds_follow_the_service_numbers_and_keep_unknown_ones() {
        assert_eq!(kind(&Value::Null), MessageKind::Default);
        for said in [0, 19, 20, 23] {
            assert_eq!(kind(&json!(said)), MessageKind::Default);
        }
        assert_eq!(kind(&json!(6)), MessageKind::Pinned);
        assert_eq!(kind(&json!(7)), MessageKind::Join);
        assert_eq!(kind(&json!(10)), MessageKind::Boost { tier: 2 });
        assert_eq!(kind(&json!(18)), MessageKind::ThreadCreated);
        assert_eq!(kind(&json!(46)), MessageKind::PollResult);
        assert_eq!(kind(&json!(24)), MessageKind::Other(24));
        assert_eq!(kind(&json!(1u64 << 40)), MessageKind::Other(u16::MAX));
    }

    #[test]
    fn cards_are_bounded_and_malformed_parts_are_dropped() {
        let long = "x".repeat(5000);
        let cards = embeds(&json!([
            {"title": "Título\u{0007}", "url": "javascript:alert(1)", "color": 0x5865F2,
             "description": long, "provider": {"name": "Sitio"},
             "fields": [{"name": "A", "value": "1", "inline": true}, {"name": "", "value": "x"}],
             "timestamp": "2026-10-14T12:00:00Z"},
            {"type": "image", "thumbnail": {"url": "https://x.test/a.png", "width": 9, "height": 9}},
            {"title": "Dos", "url": "https://user:pw@x.test/", "color": 0x1_000_000},
            "not a card"
        ]));
        assert_eq!(cards.len(), 2, "empty and malformed cards are dropped");
        assert_eq!(cards[0].title.as_deref(), Some("Título"));
        assert_eq!(cards[0].url, None, "only http(s) titles are links");
        assert_eq!(cards[0].description.as_ref().unwrap().len(), 4096);
        assert_eq!(cards[0].fields.len(), 1);
        assert!(cards[0].fields[0].inline && cards[0].timestamp.is_some());
        assert_eq!(cards[0].color, Some(0x5865F2));
        assert_eq!((cards[1].url.as_ref(), cards[1].color), (None, None));
        assert_eq!(embeds(&json!({"title": "x"})), Vec::new());
        let many: Vec<_> = (0..12).map(|i| json!({"title": i.to_string()})).collect();
        assert_eq!(embeds(&Value::Array(many)).len(), 10);
    }

    #[test]
    fn a_service_notice_reads_its_fields_and_hides_the_machine_ones() {
        // Shaped like the notice the system account sends; values synthetic.
        let field = |name: &str, value: &str| json!({"name": name, "value": value});
        let payload = json!({
            "type": "safety_system_notification",
            "fields": [
                field("client_version_message", "Actualizá la app para ver este mensaje."),
                field("body", "Detectamos actividad inusual en tu cuenta."),
                field("icon_type", "warning"),
                field("header", "Mensaje importante de Discord acerca de tu cuenta"),
                field("timestamp", "1760572800.5"),
                field("theme", "danger"),
                field("ctas", "[]"),
                field("learn_more_link", "https://support.discord.com/hc/articles/1"),
                field("classification_id", "123456"),
            ]
        });
        let cards = embeds(&json!([payload]));
        let notice = &cards[0];
        assert_eq!(notice.kind, EmbedKind::SystemNotice);
        assert_eq!(
            notice.title.as_deref(),
            Some("Mensaje importante de Discord acerca de tu cuenta")
        );
        assert_eq!(
            notice.description.as_deref(),
            Some("Detectamos actividad inusual en tu cuenta.")
        );
        assert_eq!(
            notice.url.as_deref(),
            Some("https://support.discord.com/hc/articles/1")
        );
        assert_eq!(notice.timestamp, Some(Timestamp(1_760_572_800_500)));
        assert!(notice.fields.is_empty(), "{:?}", notice.fields);

        for elsewhere in [
            "https://discord.com.example.test/x",
            "javascript:alert(1)",
            "https://user:pw@discord.com/x",
        ] {
            let mut other = payload.clone();
            other["fields"][7]["value"] = json!(elsewhere);
            assert_eq!(embed(&other).url, None, "{elsewhere}");
        }
        let mut short = payload.clone();
        short["fields"][7]["value"] = json!("https://dis.gd/notice");
        assert_eq!(embed(&short).url.as_deref(), Some("https://dis.gd/notice"));

        // An app's card with the same field names keeps them all.
        let mut bot = payload;
        bot["type"] = json!("rich");
        assert_eq!(embed(&bot).fields.len(), 9);
        assert_eq!(embed(&bot).kind, EmbedKind::Rich);
        bot["type"] = json!("auto_moderation_message");
        let other = embed(&bot);
        assert_eq!((other.kind, other.fields.len()), (EmbedKind::Other, 9));
    }

    #[test]
    fn result_voice_and_parts_cards_keep_what_they_draw() {
        // Field names as the reference client reads them; values synthetic.
        let result = embed(&json!({"type": "poll_result", "fields": [
            {"name": "poll_question_text", "value": "¿Pizza?"},
            {"name": "victor_answer_id", "value": "1"},
            {"name": "victor_answer_text", "value": "Sí"},
            {"name": "victor_answer_votes", "value": "3"},
            {"name": "total_votes", "value": "5"}]}));
        assert_eq!(result.kind, EmbedKind::PollResult);
        assert_eq!(result.field("victor_answer_text"), Some("Sí"));
        assert_eq!(result.field("total_votes"), Some("5"));
        assert_eq!(result.thumbnail, None, "a Unicode winner has no picture");
        let own = embed(
            &json!({"type": "poll_result", "thumbnail": {"proxy_url": "x"},
            "fields": [{"name": "victor_answer_emoji_id", "value": "22"},
            {"name": "victor_answer_emoji_name", "value": "pizza"}]}),
        );
        assert_eq!(
            own.thumbnail.unwrap().source.url,
            "https://cdn.discordapp.com/emojis/22.webp?size=128"
        );
        let bad = embed(&json!({"type": "poll_result",
            "fields": [{"name": "victor_answer_emoji_id", "value": "../22"}]}));
        assert_eq!(bad.thumbnail, None);

        let voice =
            |url: &str| embed(&json!({"type": "voice_channel", "title": "Sala", "url": url}));
        let card = voice("https://discord.com/channels/10/20");
        assert_eq!(
            (card.kind, card.channel),
            (EmbedKind::VoiceChannel, Some(ChannelId(20)))
        );
        for elsewhere in [
            "https://discord.com.example.test/channels/10/20",
            "http://discord.com/channels/10/20",
            "https://discord.com/channels/10/20/30",
            "https://discord.com/invite/abc",
            "https://discord.com/channels/@me/20",
        ] {
            assert_eq!(voice(elsewhere).channel, None, "{elsewhere}");
        }

        let parts = embed(&json!({"type": "components", "components": [
            {"type": 10, "content": "Hola <@5>"}]}));
        assert_eq!(parts.kind, EmbedKind::Components);
        assert_eq!(parts.components.len(), 1);
        let cards = embeds(&json!([{"type": "components", "components": [
            {"type": 10, "content": "Hola <@5>"}]}]));
        assert!(!cards[0].text_references.is_empty(), "its texts' mentions");
        assert!(embeds(&json!([{"type": "components", "components": "x"}])).is_empty());
        let rich = embed(&json!({"title": "x", "components": [{"type": 10, "content": "y"}]}));
        assert!(
            rich.components.is_empty(),
            "only a card of parts keeps them"
        );
    }

    #[test]
    fn a_forward_keeps_its_snapshot_and_its_files_from_their_channel() {
        let value = json!({
            "message_reference": {"type": 1, "channel_id": "30", "message_id": "300"},
            "message_snapshots": [{"message": {
                "content": "reenviado", "timestamp": "2026-10-14T12:00:00Z",
                "attachments": [
                    {"id": "400", "filename": "a.png", "size": 3, "width": 8, "height": 6,
                        "content_type": "image/png",
                        "url": "https://cdn.discordapp.com/attachments/30/300/400/a.png?ex=1&is=2&hm=3"},
                    {"id": "401", "filename": "b.png", "size": 3, "width": 8, "height": 6,
                        "content_type": "image/png",
                        "url": "https://cdn.discordapp.com/attachments/31/401/b.png"}],
                "embeds": [{"title": "Tarjeta"}],
                "flags": 32768, "components": [{"type": 10, "content": "Parte"}]}}]
        });
        let copy = forwarded(&value).unwrap();
        assert!(copy.components.whole);
        assert_eq!(
            copy.components.parts,
            [rune_model::Component::Text("Parte".into())]
        );
        assert_eq!(copy.content, "reenviado");
        assert_eq!(copy.attachments[0].filename, "a.png");
        // Its file under the channel it came from, as forwarded files are
        // addressed (with the source message), previews and downloads.
        let file = &copy.attachments[0];
        assert!(file.download.is_some() && file.preview.is_some());
        // One claiming another channel loses its address, not the card.
        let foreign = &copy.attachments[1];
        assert!(foreign.download.is_none() && foreign.preview.is_none());
        assert_eq!(copy.embeds[0].title.as_deref(), Some("Tarjeta"));
        let mut reply = value.clone();
        reply["message_reference"]["type"] = json!(0);
        assert_eq!(forwarded(&reply), None);
    }

    #[test]
    fn pictures_load_only_through_the_proxy_within_bounds() {
        let proxied = "https://images-ext-1.discordapp.net/external/abc/https/x.test/a.png";
        let card = |image: Value| embed(&json!({"type": "image", "image": image}));
        let shown = card(json!({"url": "https://x.test/a.png", "proxy_url": proxied,
            "width": 640, "height": 480}));
        assert_eq!(shown.kind, EmbedKind::Image);
        assert!(shown.is_media());
        let image = shown.image.unwrap();
        assert_eq!(
            (image.source.url.as_str(), image.dimensions),
            (
                format!("{proxied}?format=png&width=640&height=480").as_str(),
                (640, 480)
            )
        );
        assert_eq!(image.source.kind, ImageKind::AttachmentPreview);
        // A page's preview is asked for re-encoded and no larger than needed,
        // as the official client asks; a GIF and an address with a query are
        // left as they came.
        let preview = embed(&json!({"type": "article", "thumbnail": {"proxy_url":
            "https://images-ext-2.discordapp.net/external/s/https/opengraph.githubassets.test/a/b",
            "width": 1200, "height": 600}}));
        assert_eq!(
            preview.thumbnail.unwrap().source.url,
            "https://images-ext-2.discordapp.net/external/s/https/opengraph.githubassets.test/a/b?format=png&width=1024&height=512"
        );
        for kept in [
            "https://images-ext-1.discordapp.net/external/g/https/x.test/a.GIF",
            "https://images-ext-1.discordapp.net/external/q/https/x.test/a.png?format=webp",
            "https://media.discordapp.net/attachments/1/2/p.jpg",
        ] {
            let card = embed(&json!({"type": "rich", "image": {"proxy_url": kept,
                "width": 2000, "height": 2000}}));
            assert_eq!(card.image.unwrap().source.url, kept);
        }
        for bad in [
            json!({"url": proxied, "width": 640, "height": 480}),
            json!({"proxy_url": "https://x.test/a.png", "width": 640, "height": 480}),
            json!({"proxy_url": "http://images-ext-1.discordapp.net/external/a", "width": 1, "height": 1}),
            json!({"proxy_url": "https://images-ext-1.discordapp.net/other/a", "width": 1, "height": 1}),
            json!({"proxy_url": "https://media.discordapp.net/stickers/a.png", "width": 1, "height": 1}),
            json!({"proxy_url": "https://images-ext-1.discordapp.net:8443/external/a", "width": 1, "height": 1}),
            json!({"proxy_url": proxied, "width": 4097, "height": 1}),
            json!({"proxy_url": proxied}),
        ] {
            assert!(card(bad).image.is_none());
        }
        let video = embed(&json!({"type": "video", "title": "Clip",
            "thumbnail": {"proxy_url": "https://media.discordapp.net/attachments/1/2/p.jpg",
                "width": 1280, "height": 720}}));
        assert_eq!(video.kind, EmbedKind::Video);
        assert!(video.thumbnail.is_some() && !video.is_media());
        assert!(video.video.is_none(), "only a GIF card plays its clip");
        // A GIF site's card: its still and the MP4 it loops, both proxied.
        let clip = "https://images-ext-2.discordapp.net/external/c/https/media.tenor.test/a.mp4";
        let gif = embed(&json!({"type": "gifv", "url": "https://tenor.test/view/a",
            "thumbnail": {"proxy_url": proxied, "width": 220, "height": 160},
            "video": {"url": "https://media.tenor.test/a.mp4", "proxy_url": clip,
                "width": 220, "height": 160}}));
        assert!(gif.is_media());
        let looped = gif.video.unwrap();
        assert_eq!(
            (looped.source.url.as_str(), looped.source.kind),
            (clip, ImageKind::Clip)
        );
        let direct = embed(&json!({"type": "gifv",
            "thumbnail": {"proxy_url": proxied, "width": 220, "height": 160},
            "video": {"url": "https://media.tenor.test/a.mp4", "width": 220, "height": 160}}));
        assert!(direct.video.is_none(), "never from the site itself");
    }

    #[test]
    fn all_cards_keep_within_the_budget() {
        let big: Vec<_> = (0..10)
            .map(|_| json!({"description": "x".repeat(4096),
                "fields": (0..25).map(|_| json!({"name": "n", "value": "v".repeat(1024)})).collect::<Vec<_>>()}))
            .collect();
        let cards = embeds(&Value::Array(big));
        assert!(!cards.is_empty() && cards.len() < 10);
        assert!(cards.iter().map(size).sum::<usize>() <= CARD_BUDGET);
    }

    #[test]
    fn polls_keep_answers_counts_and_unknown_results_apart() {
        let value = json!({
            "question": {"text": "¿Cuándo?"},
            "answers": [
                {"answer_id": 1, "poll_media": {"text": "Hoy", "emoji": {"id": null, "name": "🔥"}}},
                {"answer_id": 2, "poll_media": {"emoji": {"id": "9", "name": "propio"}}},
            ],
            "expiry": "2026-10-16T12:00:00Z", "allow_multiselect": true,
            "results": {"is_finalized": false,
                "answer_counts": [{"id": 1, "count": 3, "me_voted": true}, {"id": 7, "count": 9}]}
        });
        let mapped = poll(&value).unwrap();
        assert_eq!(mapped.answers[0].emoji.as_deref(), Some("🔥"));
        assert_eq!(mapped.answers[1].text, ":propio:");
        assert_eq!((mapped.answers[0].votes, mapped.answers[0].me), (3, true));
        assert_eq!(mapped.votes(), 3, "counts for unknown answers are ignored");
        assert!(mapped.multiselect && mapped.results_known && mapped.expiry.is_some());
        let mut unknown = value.clone();
        unknown.as_object_mut().unwrap().remove("results");
        let mut later = poll(&unknown).unwrap();
        assert!(!later.results_known && later.votes() == 0);
        later.retain_results(&mapped);
        assert_eq!(
            later.votes(),
            3,
            "an update without counts keeps the known ones"
        );
        let mut duplicate = value.clone();
        duplicate["answers"][1]["answer_id"] = json!(1);
        assert_eq!(poll(&duplicate), None);
        let mut single = value;
        single["answers"].as_array_mut().unwrap().pop();
        assert_eq!(poll(&single), None);
    }
}
