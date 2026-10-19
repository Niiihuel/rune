//! Translate bounded wire tokens into provider-independent display references.

use rune_model::{
    ChannelId, MessageId, RoleId, ServerId, TextReference, TextTarget, TimeStyle, Timestamp, UserId,
};

/// The most invitations of one message that get a card: the reference
/// client keeps the first ten of a message's links of this sort.
const INVITATIONS: usize = 10;

pub(crate) fn references(content: &str) -> Vec<TextReference> {
    let mut out = invitations(content);
    let mut rest = content;
    while let Some(start) = rest.find('<') {
        rest = &rest[start..];
        let Some(end) = rest.find('>') else {
            break;
        };
        let token = &rest[1..end];
        let number = |text: &str| {
            text.parse::<u64>()
                .ok()
                .filter(|id| *id != 0 && id.to_string() == text)
        };
        let target = if let Some(id) = token.strip_prefix("@&").and_then(number) {
            Some(TextTarget::Role(RoleId(id)))
        } else if let Some(id) = token
            .strip_prefix("@!")
            .or_else(|| token.strip_prefix('@'))
            .and_then(number)
        {
            Some(TextTarget::User(UserId(id)))
        } else if let Some(id) = token.strip_prefix('#').and_then(number) {
            Some(TextTarget::Channel(ChannelId(id)))
        } else if let Some(moment) = token.strip_prefix("t:").and_then(moment) {
            // Message actions (P4).
            Some(moment)
        } else {
            let animated = token.starts_with("a:");
            token
                .strip_prefix("a:")
                .or_else(|| token.strip_prefix(':'))
                .and_then(|emoji| emoji.split_once(':'))
                .filter(|(name, _)| {
                    !name.is_empty()
                        && name.len() <= 32
                        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
                })
                .and_then(|(name, id)| Some((name, number(id)?)))
                .map(|(name, id)| TextTarget::Emoji {
                    name: name.into(),
                    image: Some(crate::emoji::inline(id, animated)),
                    jumbo: Some(crate::emoji::jumbo(id, animated)),
                    id: Some(rune_model::EmojiId(id)),
                })
        };
        if let Some(target) = target {
            let source = &rest[..=end];
            if !out.iter().any(|item: &TextReference| item.source == source) {
                out.push(TextReference {
                    source: source.into(),
                    target,
                });
                if out.len() == 256 {
                    break;
                }
            }
        }
        rest = &rest[end + 1..];
    }
    links(content, &mut out);
    out
}

/// The invitation links of `content` wherever they are written (bare,
/// `<…>` or the address of `[text](…)`), each invitation once and at most
/// [`INVITATIONS`]. Those in code are text, as for the reference client;
/// those in a spoiler stay hidden, so they get no card either.
fn invitations(content: &str) -> Vec<TextReference> {
    let mut out: Vec<TextReference> = Vec::new();
    let mut seen = Vec::new();
    let open = outside_code_and_spoilers(content);
    let mut rest = open.as_str();
    while let Some(start) = rest.find("https://") {
        rest = &rest[start..];
        let end = rest
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '<' | '>' | '(' | ')' | '[' | ']' | '"' | '\'')
            })
            .unwrap_or(rest.len());
        let url = rest[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', '*', '_', '~', '|']);
        rest = &rest[end..];
        let Some(target) = crate::invitations::target(url) else {
            continue;
        };
        if !seen.contains(&target.url) {
            seen.push(target.url);
            out.push(TextReference {
                source: url.into(),
                target: TextTarget::Invitation,
            });
            if out.len() == INVITATIONS {
                break;
            }
        }
    }
    out
}

/// `content` with its code (fenced or inline) and spoilers blanked out,
/// so what is left can be searched for links.
fn outside_code_and_spoilers(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    loop {
        // The earliest marker; at the same place, the longest.
        let next = ["```", "``", "`", "||"]
            .iter()
            .filter_map(|mark| rest.find(mark).map(|at| (at, *mark)))
            .min_by_key(|(at, mark)| (*at, std::cmp::Reverse(mark.len())));
        let Some((at, mark)) = next else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..at]);
        let inside = &rest[at + mark.len()..];
        match inside.find(mark) {
            Some(end) => {
                out.extend(std::iter::repeat_n(' ', mark.len() * 2 + end));
                rest = &inside[end + mark.len()..];
            }
            // An unclosed fence runs to the end; any other mark is text.
            None if mark == "```" => return out,
            None => {
                out.push_str(mark);
                rest = inside;
            }
        }
    }
}

/// `<t:seconds:style>` without its brackets and `t:`: a moment for each
/// reader's own clock. No style is `f`.
fn moment(body: &str) -> Option<TextTarget> {
    if body.len() > 24 {
        return None;
    }
    let (seconds, style) = body.split_once(':').unwrap_or((body, "f"));
    let style = match style {
        "t" => TimeStyle::ShortTime,
        "T" => TimeStyle::LongTime,
        "d" => TimeStyle::ShortDate,
        "D" => TimeStyle::LongDate,
        "f" => TimeStyle::ShortDateTime,
        "F" => TimeStyle::LongDateTime,
        "R" => TimeStyle::Relative,
        _ => return None,
    };
    let seconds: i64 = seconds.parse().ok()?;
    Some(TextTarget::Time {
        at: Timestamp(seconds.checked_mul(1000)?),
        style,
    })
}

/// The links to Discord's conversations and messages in `content`, as
/// written (bare, `<…>` or the address of `[text](…)`), added to `out`.
fn links(content: &str, out: &mut Vec<TextReference>) {
    let mut rest = content;
    while let Some(start) = rest.find("https://") {
        rest = &rest[start..];
        let end = rest
            .find(|c: char| {
                c.is_whitespace() || matches!(c, '<' | '>' | '(' | ')' | '[' | ']' | '"' | '\'')
            })
            .unwrap_or(rest.len());
        // As limn's Markdown reads it: a `|` at its end closes a `||spoiler||`.
        let url = rest[..end].trim_end_matches(['.', ',', ';', ':', '!', '?', '|']);
        rest = &rest[end..];
        if let Some(target) = link(url) {
            if out.len() < 256 && !out.iter().any(|known| known.source == url) {
                out.push(TextReference {
                    source: url.into(),
                    target,
                });
            }
        }
    }
}

/// `url` as a link to a conversation or a message of Discord, when it is
/// one: `https://(www.|ptb.|canary.)discord(app).com/channels/{server or
/// @me}/{channel}[/{message}]`, its query and fragment ignored.
pub(crate) fn link(url: &str) -> Option<TextTarget> {
    if url.len() > 2048 || url.chars().any(|c| c.is_control() || c == '\\') {
        return None;
    }
    let rest = url.strip_prefix("https://")?;
    let (host, path) = rest.split_once('/')?;
    let host = host
        .strip_suffix(":443")
        .unwrap_or(host)
        .to_ascii_lowercase();
    let known = ["discord.com", "discordapp.com"].iter().any(|domain| {
        ["", "www.", "ptb.", "canary."]
            .iter()
            .any(|sub| host == format!("{sub}{domain}"))
    });
    if !known {
        return None;
    }
    let path = path.split(['?', '#']).next()?;
    let path = path.strip_suffix('/').unwrap_or(path);
    let id = |text: &str| {
        (text.len() <= 20 && !text.starts_with('0') && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse::<u64>().ok())
            .flatten()
            .filter(|id| *id != 0)
    };
    let parts: Vec<&str> = path.split('/').collect();
    let (scope, channel, message) = match parts.as_slice() {
        ["channels", scope, channel] => (*scope, id(channel)?, None),
        ["channels", scope, channel, message] => (*scope, id(channel)?, Some(id(message)?)),
        _ => return None,
    };
    let server = match scope {
        "@me" => None,
        other => Some(ServerId(id(other)?)),
    };
    Some(TextTarget::Link {
        server,
        channel: ChannelId(channel),
        message: message.map(MessageId),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_to_conversations_and_moments_are_references() {
        let found = references(
            "Mirá https://discord.com/channels/1/2/3. y <https://PTB.discordapp.com:443/channels/@me/4/> \
             o [esto](https://canary.discord.com/channels/1/5?x=1#y) <t:1700000000:R> <t:-5>",
        );
        let targets: Vec<&TextTarget> = found.iter().map(|found| &found.target).collect();
        assert!(targets.contains(&&TextTarget::Time {
            at: Timestamp(1_700_000_000_000),
            style: TimeStyle::Relative,
        }));
        assert!(targets.contains(&&TextTarget::Time {
            at: Timestamp(-5_000),
            style: TimeStyle::ShortDateTime,
        }));
        let links: Vec<(&str, &TextTarget)> = found
            .iter()
            .filter(|found| matches!(found.target, TextTarget::Link { .. }))
            .map(|found| (found.source.as_str(), &found.target))
            .collect();
        assert_eq!(
            links,
            vec![
                (
                    "https://discord.com/channels/1/2/3",
                    &TextTarget::Link {
                        server: Some(ServerId(1)),
                        channel: ChannelId(2),
                        message: Some(MessageId(3)),
                    }
                ),
                (
                    "https://PTB.discordapp.com:443/channels/@me/4/",
                    &TextTarget::Link {
                        server: None,
                        channel: ChannelId(4),
                        message: None,
                    }
                ),
                (
                    "https://canary.discord.com/channels/1/5?x=1#y",
                    &TextTarget::Link {
                        server: Some(ServerId(1)),
                        channel: ChannelId(5),
                        message: None,
                    }
                ),
            ]
        );
        for url in [
            "http://discord.com/channels/1/2",
            "https://discord.com.evil.test/channels/1/2",
            "https://evil.test/channels/1/2",
            "https://discord.com/channels/01/2",
            "https://discord.com/channels/1/0",
            "https://discord.com/channels/1",
            "https://discord.com/channels/1/2/3/4",
            "https://discord.com:444/channels/1/2",
            "https://discord.com/guilds/1/2",
        ] {
            assert_eq!(link(url), None, "{url}");
        }
        assert!(references("<t:1:X> <t:abc:R>").is_empty());
        // Inside a spoiler, without the marks that close it.
        let hidden = references("||https://discord.com/channels/1/2/3||");
        assert_eq!(
            hidden
                .iter()
                .map(|found| found.source.as_str())
                .collect::<Vec<_>>(),
            ["https://discord.com/channels/1/2/3"]
        );
    }

    fn invitations_in(content: &str) -> Vec<String> {
        references(content)
            .into_iter()
            .filter(|reference| reference.target == TextTarget::Invitation)
            .map(|reference| reference.source)
            .collect()
    }

    #[test]
    fn every_validated_invitation_link_gets_a_card_even_amid_text() {
        for url in [
            "https://discord.gg/hello-world",
            "https://discord.com/invite/hello-world",
            "https://canary.discord.com/invite/hello-world",
        ] {
            assert_eq!(invitations_in(&format!("  {url}\n")), vec![url]);
        }
        assert_eq!(
            invitations_in(
                "Vengan: https://discord.gg/hello-world. Y al grupo <https://discord.gg/another>, \
                 o [acá](https://discord.com/invite/third) <@7>"
            ),
            vec![
                "https://discord.gg/hello-world",
                "https://discord.gg/another",
                "https://discord.com/invite/third"
            ]
        );
        // The other references of the text are still there.
        assert!(references("https://discord.gg/hello-world <@7>")
            .iter()
            .any(|reference| reference.target == TextTarget::User(UserId(7))));
        // One card per invitation, however it is written.
        assert_eq!(
            invitations_in("https://discord.gg/same https://discord.com/invite/same"),
            vec!["https://discord.gg/same"]
        );
        // The first ten, as the reference client.
        let many: Vec<String> = (0..12)
            .map(|index| format!("https://discord.gg/code{index}"))
            .collect();
        let found = invitations_in(&many.join(" y "));
        assert_eq!(found, many[..INVITATIONS].to_vec());
        for content in [
            "hello-world",
            "`https://discord.gg/hello-world`",
            "``https://discord.gg/hello-world``",
            "```\nhttps://discord.gg/hello-world\n```",
            "```\nhttps://discord.gg/hello-world",
            "mirá ||https://discord.gg/hello-world|| ahí",
            "https://discord.gg.evil.invalid/hello-world",
            "https://user:pass@discord.gg/hello-world",
            "https://discord.gg:444/hello-world",
            "http://discord.gg/hello-world",
        ] {
            assert!(invitations_in(content).is_empty(), "{content}");
        }
        // Code and spoilers end; what follows them is text again.
        assert_eq!(
            invitations_in("`a` ||b|| ``c`` https://discord.gg/after"),
            vec!["https://discord.gg/after"]
        );
        assert_eq!(
            invitations_in("1 ` 2 https://discord.gg/lonely-tick"),
            vec!["https://discord.gg/lonely-tick"]
        );
    }

    #[test]
    fn tokens_keep_their_source_but_resolve_to_typed_references() {
        let parsed = references("<@7> <@!7> <@&8> <#9> <:wave:10> <a:wave:10> <@7>");
        assert_eq!(parsed.len(), 6);
        assert_eq!(parsed[0].target, TextTarget::User(UserId(7)));
        assert_eq!(parsed[2].target, TextTarget::Role(RoleId(8)));
        assert_eq!(parsed[3].target, TextTarget::Channel(ChannelId(9)));
        assert_eq!(
            parsed[4].target,
            TextTarget::Emoji {
                name: "wave".into(),
                image: Some(rune_model::RemoteImage {
                    url: "https://cdn.discordapp.com/emojis/10.webp?size=48".into(),
                    kind: Default::default(),
                }),
                // A message of emoji alone asks twice the size (2026-10-18).
                jumbo: Some(rune_model::RemoteImage {
                    url: "https://cdn.discordapp.com/emojis/10.webp?size=96".into(),
                    kind: Default::default(),
                }),
                id: Some(rune_model::EmojiId(10)),
            }
        );
        // The same emoji, moving, is a picture that moves.
        assert!(matches!(
            &parsed[5].target,
            TextTarget::Emoji { image: Some(image), jumbo: Some(jumbo), .. }
                if image.url.ends_with("size=48&animated=true")
                    && jumbo.url.ends_with("size=96&animated=true")
        ));
        assert!(references("<@0> <@007> <@-1> <:bad name:10>").is_empty());
    }
}
