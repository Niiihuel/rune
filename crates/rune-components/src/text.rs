//! What a message says beyond limn's Markdown: mentions of people, roles and
//! channels drawn as pills, a server's emoji as its picture in the line,
//! spoilers hidden until clicked and the "(editado)" mark, all added to the
//! tree Markdown builds; and the inks a draft is painted with while it is
//! written. Markdown itself draws underline (`__x__`), strikethrough
//! (`~~x~~`), bold italic (`***x***`) and subtext (`-# x`) since
//! 2026-10-18.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};

use limn::{Color, Element, ElementKind, FontFamily, RasterImage, TextHighlight, TextSpan};
use rune_i18n::{t, tr};
use rune_model::{RemoteImage, RoleId, TextReference, TextTarget, Timestamp, UserId};

use crate::Look;

/// What a message can mention, as RUNE draws it.
#[derive(Clone, Debug, Default)]
pub struct Names {
    /// The handle of each person mentioned in the loaded messages.
    pub people: HashMap<UserId, String>,
    /// The server's roles that can be mentioned, with their ink.
    pub roles: Vec<(String, Color)>,
    pub role_ids: HashMap<RoleId, (String, Color)>,
    pub display_names: HashMap<UserId, String>,
    /// The server's conversations, with their id.
    pub channels: Vec<(String, u64)>,
    /// The authors whose `@everyone` and `@here` notify: those who may
    /// mention everyone where they wrote. Anyone else's are plain text.
    pub everyone: HashSet<UserId>,
    // Message actions (P4).
    /// How each conversation a link names reads on its pill, by its id:
    /// «#general», «Servidor › #canal», «@persona». One missing is unknown.
    pub places: HashMap<u64, String>,
    /// Now, for the moments written relative to it.
    pub now: Timestamp,
}

/// The action of a link to a conversation, or to one of its messages.
pub fn place_action(
    channel: rune_model::ChannelId,
    message: Option<rune_model::MessageId>,
) -> String {
    format!("place:{channel}:{}", message.map_or(0, |message| message.0))
}

/// The action of a link whose text is not its address.
pub const MASKED_LINK: &str = "link_masked:";
/// The action of a moment written in a message: its milliseconds follow.
/// It gives the moment the keyboard focus, which shows its hint.
pub const MOMENT: &str = "moment:";

/// The spoilers of one message: which ones are shown, and the prefix of the
/// action that shows each: `{prefix}{index}`.
pub struct Spoilers<'a> {
    pub revealed: &'a [usize],
    pub action: String,
    /// The number of this text's first spoiler; the rest follow it, so the
    /// texts of a card each number theirs apart.
    pub first: usize,
}

/// The pictures the emoji of a text may show: those loaded, by their
/// source, and the digests of the still first frames whose animation the
/// host plays (on [`crate::message::animation_surface`]).
#[derive(Clone, Copy, Default)]
pub struct Pictures<'a> {
    pub loaded: Option<&'a HashMap<RemoteImage, RasterImage>>,
    pub playing: Option<&'a HashSet<String>>,
    /// A message of emoji alone: their larger pictures, and its Unicode
    /// emoji drawn as pictures too (2026-10-18).
    pub jumbo: bool,
    /// What each emoji reports when clicked, its card: this, its number
    /// in the text and itself, `{prefix}{n}:{emoji}`; `None`, nothing.
    pub emoji_action: Option<&'a str>,
}

/// Draw the mentions, emoji and spoilers of the text Markdown built:
/// `mentioned` are the people the message mentions, as its provider says;
/// `everyone`, whether its `@everyone` and `@here` notify. Returns the
/// number after its last spoiler, where another text of the same card goes
/// on.
pub fn decorate(
    element: &mut Element,
    names: &Names,
    mentioned: &[UserId],
    references: &[TextReference],
    everyone: bool,
    spoilers: &Spoilers<'_>,
    pictures: Pictures<'_>,
    look: &Look,
) -> usize {
    let people: Vec<&str> = mentioned
        .iter()
        .filter_map(|user| names.people.get(user).map(String::as_str))
        .collect();
    let mut count = spoilers.first;
    // The emoji are numbered across the text, for their actions.
    let emoji = Cell::new(0);
    walk(element, &mut |spans| {
        *spans = spoil(std::mem::take(spans), spoilers, &mut count, look)
            .into_iter()
            .map(|span| link(span, names, references, look))
            .flat_map(|span| resolve(span, names, references, pictures, &emoji, look))
            .flat_map(|span| split(span, names, &people, everyone, look))
            .flat_map(|span| unicode(span, pictures, &emoji))
            .collect();
    });
    count
}

/// The action of the next emoji of a text, `emoji` itself in it.
fn emoji_target(pictures: Pictures<'_>, count: &Cell<usize>, emoji: &str) -> Option<String> {
    let prefix = pictures.emoji_action?;
    let number = count.get();
    count.set(number + 1);
    Some(format!("{prefix}{number}:{emoji}"))
}

/// Each Unicode emoji of a plain run as a run of its own that reports its
/// action, drawn as its picture in the line in a message of emoji alone,
/// the size of a server's (2026-10-18; before, its letter was enlarged and
/// limn-egui's color emoji painted over it, smaller). The emoji stays what
/// a selection copies.
fn unicode(span: TextSpan, pictures: Pictures<'_>, count: &Cell<usize>) -> Vec<TextSpan> {
    if !plain(&span) || (pictures.emoji_action.is_none() && !pictures.jumbo) {
        return vec![span];
    }
    let text = span.text.clone();
    let mut out = Vec::new();
    let (mut from, mut at) = (0, 0);
    while at < text.len() {
        let rest = &text[at..];
        let Some(length) = crate::twemoji::emoji_len(rest) else {
            at += rest.chars().next().map_or(1, char::len_utf8);
            continue;
        };
        if from < at {
            out.push(TextSpan {
                text: text[from..at].into(),
                ..span.clone()
            });
        }
        let found = &rest[..length];
        let mut run = TextSpan {
            text: found.into(),
            action: emoji_target(pictures, count, found),
            ..span.clone()
        };
        // Made only for a message of emoji alone: each costs a digest.
        if let Some(image) = pictures
            .jumbo
            .then(|| crate::twemoji::image(found))
            .flatten()
        {
            run = run.image(image);
        }
        out.push(run);
        at += length;
        from = at;
    }
    if from < text.len() {
        out.push(TextSpan {
            text: text[from..].into(),
            ..span
        });
    }
    out
}

/// A link of the text: one to a conversation of the provider opens in RUNE,
/// as a pill naming it when written as its bare address; one whose text is
/// not its address says so, to be asked about before it opens (P4).
fn link(span: TextSpan, names: &Names, references: &[TextReference], look: &Look) -> TextSpan {
    let Some(url) = span
        .action
        .as_deref()
        .and_then(|action| action.strip_prefix("markdown_link:"))
        .and_then(|rest| rest.splitn(3, ':').nth(2))
        .map(str::to_owned)
    else {
        return span;
    };
    let bare = span.text.trim() == url;
    let place = references
        .iter()
        .find_map(|reference| match reference.target {
            TextTarget::Link {
                channel, message, ..
            } if reference.source == url => Some((channel, message)),
            _ => None,
        });
    match place {
        Some((channel, message)) if bare => TextSpan {
            text: place_label(names, channel, message),
            color: look.accent,
            background: Some(Look::alpha(look.accent, 40)),
            action: Some(place_action(channel, message)),
            ..span
        },
        Some((channel, message)) => TextSpan {
            action: Some(place_action(channel, message)),
            ..span
        },
        None if !bare => TextSpan {
            action: Some(format!("{MASKED_LINK}{url}")),
            ..span
        },
        None => span,
    }
}

/// What a pill of a link says: where it leads, and « › mensaje» for a message.
fn place_label(
    names: &Names,
    channel: rune_model::ChannelId,
    message: Option<rune_model::MessageId>,
) -> String {
    let place = names
        .places
        .get(&channel.0)
        .map_or(t("conversación desconocida"), String::as_str);
    match message {
        Some(_) => tr!("{place} › mensaje", place),
        None => place.to_owned(),
    }
}

/// Resolve only provider-supplied literal tokens. Code, links and hidden
/// spoilers keep their original text, and labels never become Markdown.
fn resolve(
    span: TextSpan,
    names: &Names,
    references: &[TextReference],
    pictures: Pictures<'_>,
    count: &Cell<usize>,
    look: &Look,
) -> Vec<TextSpan> {
    if !plain(&span) || references.is_empty() {
        return vec![span];
    }
    let mut out = Vec::new();
    let mut rest = span.text.as_str();
    while let Some((at, reference)) = references
        .iter()
        .filter(|reference| !reference.source.is_empty())
        .filter(|reference| reference.target != TextTarget::Invitation)
        .filter_map(|reference| rest.find(&reference.source).map(|at| (at, reference)))
        .min_by_key(|(at, _)| *at)
    {
        if at > 0 {
            out.push(TextSpan {
                text: rest[..at].into(),
                ..span.clone()
            });
        }
        if let TextTarget::Emoji { name, .. } = &reference.target {
            let picture = reference
                .target
                .emoji_picture(pictures.jumbo)
                .and_then(|image| pictures.loaded?.get(image));
            let playing = picture.is_some_and(|picture| {
                pictures
                    .playing
                    .is_some_and(|playing| playing.contains(picture.sha256()))
            });
            out.push(TextSpan {
                action: emoji_target(pictures, count, &reference.source),
                ..emoji(span.clone(), name, picture, playing)
            });
            rest = &rest[at + reference.source.len()..];
            continue;
        }
        // A moment's whole date shows in its hint, and on a click or Enter.
        let mut hint = None;
        let (label, ink, action) = match &reference.target {
            TextTarget::User(id) => (
                format!(
                    "@{}",
                    names
                        .display_names
                        .get(id)
                        .or_else(|| names.people.get(id))
                        .map_or(t("usuario"), String::as_str)
                ),
                look.accent,
                Some(format!("member:{id}")),
            ),
            TextTarget::Role(id) => {
                let role = names.role_ids.get(id);
                (
                    format!("@{}", role.map_or(t("rol"), |(name, _)| name.as_str())),
                    role.map_or(look.accent, |(_, ink)| *ink),
                    None,
                )
            }
            TextTarget::Channel(id) => {
                let name = names.channels.iter().find(|(_, known)| *known == id.0);
                (
                    format!("#{}", name.map_or(t("canal"), |(name, _)| name.as_str())),
                    look.accent,
                    name.map(|_| format!("channel:{id}")),
                )
            }
            // Drawn above as its picture.
            TextTarget::Emoji { name, .. } => (format!(":{name}:"), look.text, None),
            TextTarget::Invitation => (reference.source.clone(), look.text, None),
            // Message actions (P4).
            TextTarget::Link {
                channel, message, ..
            } => (
                place_label(names, *channel, *message),
                look.accent,
                Some(place_action(*channel, *message)),
            ),
            TextTarget::Time { at, style } => {
                hint = Some(crate::time::full_date(*at));
                (
                    crate::time::moment(*at, *style, names.now),
                    look.text,
                    Some(format!("{MOMENT}{}", at.0)),
                )
            }
        };
        let pill = TextSpan {
            text: label,
            color: ink,
            background: Some(Look::alpha(ink, 40)),
            action,
            ..span.clone()
        };
        out.push(match hint {
            Some(hint) => pill.tooltip(hint, look.menu_palette()),
            None => pill,
        });
        rest = &rest[at + reference.source.len()..];
    }
    if !rest.is_empty() {
        out.push(TextSpan {
            text: rest.into(),
            ..span
        });
    }
    out
}

/// Plain text: not code, a link, a picture or something already filled.
fn plain(span: &TextSpan) -> bool {
    span.action.is_none()
        && span.background.is_none()
        && span.image.is_none()
        && span.font_family != FontFamily::Monospace
}

/// An emoji `name` of a text, in `span`'s place and size: its picture in
/// the line, on the surface the host plays it on while it moves
/// (`playing`), and `:name:` until it loads or when it fails. `:name:` is
/// what a selection copies and a screen reader reads (2026-10-17: it was
/// always the pill `:name:`).
pub fn emoji(span: TextSpan, name: &str, picture: Option<&RasterImage>, playing: bool) -> TextSpan {
    let span = TextSpan {
        text: format!(":{name}:"),
        ..span
    };
    match picture {
        Some(picture) if playing => {
            span.surface(crate::message::animation_surface(picture.sha256()))
        }
        Some(picture) => span.image(picture.clone()),
        None => span,
    }
}

/// Put each of `pictures` in place of its token in the plain runs of
/// `element`, as [`emoji`] draws a message's: a profile's «Sobre mí».
pub fn with_pictures(element: &mut Element, pictures: &[crate::identity::TextPicture]) {
    if pictures.is_empty() {
        return;
    }
    walk(element, &mut |spans| {
        *spans = std::mem::take(spans)
            .into_iter()
            .flat_map(|span| {
                if !plain(&span) {
                    return vec![span];
                }
                let mut out = Vec::new();
                let mut rest = span.text.as_str();
                while let Some((at, picture)) = pictures
                    .iter()
                    .filter(|picture| !picture.token.is_empty())
                    .filter_map(|picture| rest.find(&picture.token).map(|at| (at, picture)))
                    .min_by_key(|(at, _)| *at)
                {
                    if at > 0 {
                        out.push(TextSpan {
                            text: rest[..at].into(),
                            ..span.clone()
                        });
                    }
                    out.push(emoji(
                        span.clone(),
                        &picture.name,
                        picture.image.as_ref(),
                        picture.playing,
                    ));
                    rest = &rest[at + picture.token.len()..];
                }
                if !rest.is_empty() {
                    out.push(TextSpan {
                        text: rest.into(),
                        ..span.clone()
                    });
                }
                out
            })
            .collect();
    });
}

/// `value` in one line, `size` px in `ink`, with the pictures it names in
/// place of their tokens, each `scale` times its letters; what does not fit
/// is left out and the line ends in «…». `None` when it names none, for a
/// plain line.
pub fn line_with_pictures(
    value: &str,
    pictures: &[crate::identity::TextPicture],
    size: f32,
    ink: Color,
    scale: f32,
) -> Option<Element> {
    if !pictures
        .iter()
        .any(|picture| !picture.token.is_empty() && value.contains(&picture.token))
    {
        return None;
    }
    let mut element =
        Element::formatted_text(vec![TextSpan::new(value, ink, size, FontFamily::Sans)]);
    with_pictures(&mut element, pictures);
    walk(&mut element, &mut |spans| {
        for span in spans.iter_mut().filter(|span| span.image.is_some()) {
            span.font_size = size * scale / limn::INLINE_IMAGE_SCALE;
        }
    });
    Some(element.max_lines(1))
}

/// How much larger the emoji of a message that says nothing else are: 48
/// px against 1.375 times 16 px, as the official client draws them
/// (`--custom-emoji-size-jumbo-emoji: 3rem` against `1.375em`).
pub const JUMBO: f32 = 48.0 / 22.0;
/// The most emoji such a message has; one more and they are a line's size
/// (`MAX_EMOJI_TO_BE_JUMBO` in the official client).
pub const JUMBO_MAX: usize = 30;

/// Whether `content` is emoji alone, between spaces: a server's, among its
/// `references`, or Unicode ones, from one to [`JUMBO_MAX`].
pub fn only_emoji(content: &str, references: &[TextReference]) -> bool {
    let mut rest = content.to_owned();
    let mut count = 0;
    for reference in references
        .iter()
        .filter(|reference| matches!(reference.target, TextTarget::Emoji { .. }))
        .filter(|reference| !reference.source.is_empty())
    {
        count += rest.matches(&reference.source).count();
        rest = rest.replace(&reference.source, " ");
    }
    unicode_emoji(&rest).is_some_and(|unicode| (1..=JUMBO_MAX).contains(&(count + unicode)))
}

/// How many Unicode emoji `text` has when it has nothing else but spaces,
/// by the sequences of the Twemoji set (2026-10-18; before, by code point
/// ranges, a keycap such as 1️⃣ read as text): a family, a skin tone or a
/// flag counts once.
fn unicode_emoji(text: &str) -> Option<usize> {
    let mut count = 0;
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        rest = rest[crate::twemoji::emoji_len(rest)?..].trim_start();
        count += 1;
    }
    Some(count)
}

/// Every run of `element` `factor` times its size, as a message of emoji
/// alone is drawn.
pub fn enlarge(element: &mut Element, factor: f32) {
    walk(element, &mut |spans| {
        for span in spans {
            span.font_size *= factor;
        }
    });
}

/// Hide what each `||…||` of a paragraph encloses, though Markdown cut it
/// into runs: bold or code inside a spoiler hides with the rest. `count`
/// numbers the spoilers across the whole message.
fn spoil(
    spans: Vec<TextSpan>,
    spoilers: &Spoilers<'_>,
    count: &mut usize,
    look: &Look,
) -> Vec<TextSpan> {
    // Every `||` of plain text, as (run, byte), paired in order; one left
    // over, or a pair with nothing between, stays as written.
    let marks: Vec<(usize, usize)> = spans
        .iter()
        .enumerate()
        .filter(|(_, span)| plain(span))
        .flat_map(|(index, span)| {
            span.text
                .match_indices("||")
                .map(move |(at, _)| (index, at))
        })
        .collect();
    let cuts: Vec<(usize, usize)> = marks
        .chunks_exact(2)
        .filter(|pair| pair[1] != (pair[0].0, pair[0].1 + 2))
        .flatten()
        .copied()
        .collect();
    let mut out = Vec::new();
    let mut inside: Option<usize> = None;
    let mut cuts = cuts.into_iter().peekable();
    for (index, span) in spans.into_iter().enumerate() {
        let mut push = |start: usize, end: usize, inside: Option<usize>| {
            let mut run = TextSpan {
                text: span.text[start..end].to_owned(),
                ..span.clone()
            };
            match inside {
                Some(number) if !spoilers.revealed.contains(&number) => {
                    run.color = look.faint;
                    run.background = Some(look.faint);
                    run.action = Some(format!("{}{number}", spoilers.action));
                }
                Some(_) => run.background = Some(look.selected),
                None => {}
            }
            out.push(run);
        };
        let mut start = 0;
        while let Some((_, at)) = cuts.next_if(|(run, _)| *run == index) {
            push(start, at, inside);
            inside = match inside {
                Some(_) => None,
                None => {
                    *count += 1;
                    Some(*count - 1)
                }
            };
            start = at + 2;
        }
        push(start, span.text.len(), inside);
    }
    out.retain(|span| !span.text.is_empty());
    out
}

pub(crate) fn walk(element: &mut Element, visit: &mut impl FnMut(&mut Vec<TextSpan>)) {
    match &mut element.kind {
        ElementKind::FormattedText(spans) => visit(spans),
        ElementKind::Column(children)
        | ElementKind::Row(children)
        | ElementKind::Stack(children) => {
            for child in children {
                walk(child, visit);
            }
        }
        ElementKind::Panel(child) | ElementKind::Clickable { child, .. } => walk(child, visit),
        _ => {}
    }
}

/// One run of plain text cut where a mention starts and ends. Code, links,
/// spoilers and other filled runs are left as they are.
fn split(
    span: TextSpan,
    names: &Names,
    people: &[&str],
    everyone: bool,
    look: &Look,
) -> Vec<TextSpan> {
    if !plain(&span) {
        return vec![span];
    }
    let text = span.text.clone();
    let mut out = Vec::new();
    let mut plain = String::new();
    let mut rest = text.as_str();
    let piece = |text: &str| TextSpan {
        text: text.to_owned(),
        ..span.clone()
    };
    while let Some(next) = rest.chars().next() {
        let at_word = plain
            .chars()
            .next_back()
            .or_else(|| {
                out.last()
                    .and_then(|last: &TextSpan| last.text.chars().next_back())
            })
            .is_none_or(|before| before.is_whitespace() || matches!(before, '(' | '['));
        if at_word && matches!(next, '@' | '#') {
            let token = word(&rest[1..]);
            let found = match next {
                '@' if everyone && (token == "everyone" || token == "here") => {
                    Some((look.accent, None))
                }
                '@' if people
                    .iter()
                    .any(|handle| handle.eq_ignore_ascii_case(token)) =>
                {
                    Some((look.accent, None))
                }
                '@' => names
                    .roles
                    .iter()
                    .find(|(role, _)| !token.is_empty() && role == token)
                    .map(|(_, ink)| (*ink, None)),
                _ => names
                    .channels
                    .iter()
                    .find(|(name, _)| !token.is_empty() && name == token)
                    .map(|(_, id)| (look.accent, Some(format!("channel:{id}")))),
            };
            if let Some((ink, action)) = found {
                let mut pill = piece(&rest[..1 + token.len()]);
                pill.color = ink;
                pill.background = Some(Look::alpha(ink, 40));
                pill.action = action;
                out.push(piece(&std::mem::take(&mut plain)));
                out.push(pill);
                rest = &rest[1 + token.len()..];
                continue;
            }
        }
        plain.push(next);
        rest = &rest[next.len_utf8()..];
    }
    out.push(piece(&plain));
    out.retain(|span| !span.text.is_empty());
    out
}

/// The name after a sigil: letters, digits and `_ . -`, without a dot or
/// dash at its end (the end of a sentence).
fn word(text: &str) -> &str {
    let end = text
        .char_indices()
        .find(|(_, character)| {
            !(character.is_alphanumeric() || matches!(character, '_' | '.' | '-'))
        })
        .map_or(text.len(), |(index, _)| index);
    text[..end].trim_end_matches(['.', '-'])
}

/// Whether `content` mentions everyone who can read it.
pub fn mentions_everyone(content: &str) -> bool {
    content.split_whitespace().any(|token| {
        let token = token.trim_end_matches(|character: char| !character.is_alphanumeric());
        token == "@everyone" || token == "@here"
    })
}

/// What `source` says outside code and links, as Markdown lays it out:
/// where a token stands for what it names (2026-10-18: a token in code
/// counted as the emoji).
pub fn outside_code(source: &str) -> String {
    let mut element = limn::Markdown::new(source, Look::DARK.palette())
        .render()
        .element;
    let mut said = String::new();
    walk(&mut element, &mut |spans| {
        for span in spans.iter().filter(|span| plain(span)) {
            said.push_str(&span.text);
        }
        said.push('\n');
    });
    said
}

/// " (editado)", `size` px, with when the edit was made, `at`, in its hint,
/// as the official client shows it.
pub fn edited_mark(at: Timestamp, size: f32, look: &Look) -> TextSpan {
    TextSpan::new(
        format!(" {}", t("(editado)")),
        look.faint,
        size,
        FontFamily::Sans,
    )
    .tooltip(crate::time::full_date(at), look.menu_palette())
}

/// Put [`edited_mark`] at the end of the text's last line. False when the
/// text does not end in a paragraph, so the caller shows it on its own.
pub fn append_edited(element: &mut Element, at: Timestamp, size: f32, look: &Look) -> bool {
    match &mut element.kind {
        ElementKind::FormattedText(spans) => {
            spans.push(edited_mark(at, size, look));
            true
        }
        ElementKind::Column(children)
        | ElementKind::Row(children)
        | ElementKind::Stack(children) => children
            .last_mut()
            .is_some_and(|last| append_edited(last, at, size, look)),
        ElementKind::Panel(child) => append_edited(child, at, size, look),
        _ => false,
    }
}

/// The inks of a draft while it is written: every `@word` and `#word` in
/// the accent over a pill's fill, quotes and subtext muted. The syntax
/// stays. It does not know who or what exists, so a word that names nobody
/// is filled too and is sent as plain text.
pub fn draft_highlights(draft: &str, look: &Look) -> Vec<TextHighlight> {
    let mut found = Vec::new();
    let mut offset = 0;
    for line in draft.split('\n') {
        let length = line.chars().count();
        if line.starts_with("> ") || line.starts_with("-# ") {
            found.push(TextHighlight {
                range: offset..offset + length,
                color: look.muted,
                italic: false,
                background: None,
            });
        }
        let mut before = ' ';
        for (index, (at, character)) in line.char_indices().enumerate() {
            if matches!(character, '@' | '#') && before.is_whitespace() {
                let token = word(&line[at + 1..]).chars().count();
                if token > 0 {
                    found.push(TextHighlight {
                        range: offset + index..offset + index + 1 + token,
                        color: look.accent,
                        italic: false,
                        background: Some(Look::alpha(look.accent, 40)),
                    });
                }
            }
            before = character;
        }
        offset += length + 1;
    }
    found.truncate(64);
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use limn::Markdown;

    fn spans(element: &mut Element) -> Vec<TextSpan> {
        let mut all = Vec::new();
        walk(element, &mut |spans| all.extend(spans.iter().cloned()));
        all
    }

    fn written(
        source: &str,
        names: &Names,
        mentioned: &[UserId],
        revealed: &[usize],
    ) -> Vec<TextSpan> {
        let look = Look::DARK;
        let mut element = Markdown::new(source, look.palette())
            .width(500.0)
            .render()
            .element;
        decorate(
            &mut element,
            names,
            mentioned,
            &[],
            true,
            &Spoilers {
                revealed,
                action: "msg:1:spoiler:".into(),
                first: 0,
            },
            Pictures::default(),
            &look,
        );
        spans(&mut element)
    }

    #[test]
    fn a_line_names_its_pictures_at_their_scale_and_stays_one_line() {
        let picture = |image| crate::identity::TextPicture {
            token: "<:rust:22>".into(),
            name: "rust".into(),
            image,
            playing: false,
        };
        let look = Look::DARK;
        assert!(line_with_pictures("sin emoji", &[picture(None)], 14.0, look.text, 1.25).is_none());
        let image = RasterImage::new(
            "c".repeat(64),
            limn::RasterImageFormat::Png,
            3,
            std::sync::Arc::from("AAAA"),
            "rust",
        )
        .unwrap();
        let mut line = line_with_pictures(
            "Preguntas <:rust:22>: el error",
            &[picture(Some(image))],
            14.0,
            look.text,
            limn::INLINE_IMAGE_SCALE,
        )
        .unwrap();
        // One line, ending in «…» where it does not fit.
        assert_eq!(line.style.max_lines, Some(1));
        let (_, drawn) = crate::testing::frame(line.clone(), limn::Size::new(120.0, 100.0));
        let painted = crate::testing::texts(&drawn);
        assert_eq!(painted.last().map(String::as_str), Some("…"), "{painted:?}");
        assert!(
            !painted.iter().any(|text| text.contains("error")),
            "{painted:?}"
        );
        let all = spans(&mut line);
        let texts: Vec<&str> = all.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(texts, ["Preguntas ", ":rust:", ": el error"]);
        assert!(all[1].image.is_some() && all[1].font_size == 14.0);
        // Loading: its name, never its token.
        let mut waiting =
            line_with_pictures("a <:rust:22>", &[picture(None)], 14.0, look.text, 1.25).unwrap();
        assert_eq!(spans(&mut waiting)[1].text, ":rust:");
    }

    #[test]
    fn a_token_in_code_or_a_link_is_not_what_the_text_names() {
        let said = outside_code(
            "hola <:a:1> `<:b:2>`\n```\n<:c:3>\n```\n**<:d:4>** ||<:f:6>|| [<:e:5>](https://x.test)",
        );
        for named in ["<:a:1>", "<:d:4>", "<:f:6>"] {
            assert!(said.contains(named), "{named} in {said:?}");
        }
        for coded in ["<:b:2>", "<:c:3>", "<:e:5>"] {
            assert!(!said.contains(coded), "{coded} in {said:?}");
        }
    }

    #[test]
    fn provider_references_show_names_without_changing_code_links_or_hidden_spoilers() {
        let look = Look::DARK;
        let names = Names {
            display_names: HashMap::from([(UserId(7), "Ada Lovelace".into())]),
            role_ids: HashMap::from([(RoleId(8), ("Equipo".into(), look.accent))]),
            channels: vec![("general".into(), 9)],
            ..Default::default()
        };
        let references = [
            TextReference {
                source: "<@7>".into(),
                target: TextTarget::User(UserId(7)),
            },
            TextReference {
                source: "<@&8>".into(),
                target: TextTarget::Role(RoleId(8)),
            },
            TextReference {
                source: "<#9>".into(),
                target: TextTarget::Channel(rune_model::ChannelId(9)),
            },
        ];
        let mut element = Markdown::new("<@7> <@&8> <#9> `<@7>` ||<@7>||", look.palette())
            .width(500.0)
            .render()
            .element;
        decorate(
            &mut element,
            &names,
            &[UserId(7)],
            &references,
            false,
            &Spoilers {
                revealed: &[],
                action: "reveal:".into(),
                first: 0,
            },
            Pictures::default(),
            &look,
        );
        let all = spans(&mut element);
        assert!(
            all.iter()
                .any(|span| span.text == "@Ada Lovelace"
                    && span.action.as_deref() == Some("member:7"))
        );
        assert!(all.iter().any(|span| span.text == "@Equipo"));
        assert!(all
            .iter()
            .any(|span| span.text == "#general" && span.action.as_deref() == Some("channel:9")));
        assert!(all
            .iter()
            .any(|span| span.text == "<@7>" && span.font_family == FontFamily::Monospace));
        assert!(all
            .iter()
            .any(|span| span.text == "<@7>" && span.action.as_deref() == Some("reveal:0")));
    }

    #[test]
    fn links_to_places_become_pills_masked_links_say_so_and_moments_read_locally() {
        use rune_model::{ChannelId, MessageId, ServerId, TimeStyle};
        let look = Look::DARK;
        let names = Names {
            places: HashMap::from([(2, "Taller › #anuncios".to_owned())]),
            now: Timestamp(10 * 60_000),
            ..Default::default()
        };
        let place = "https://chat.test/channels/1/2/3";
        let other = "https://chat.test/channels/1/4";
        let references = [
            TextReference {
                source: place.into(),
                target: TextTarget::Link {
                    server: Some(ServerId(1)),
                    channel: ChannelId(2),
                    message: Some(MessageId(3)),
                },
            },
            TextReference {
                source: other.into(),
                target: TextTarget::Link {
                    server: Some(ServerId(1)),
                    channel: ChannelId(4),
                    message: None,
                },
            },
            TextReference {
                source: "<t:0:R>".into(),
                target: TextTarget::Time {
                    at: Timestamp(0),
                    style: TimeStyle::Relative,
                },
            },
        ];
        let source = format!(
            "Mirá {place} y [esto]({other}), [acá](https://evil.test/x) o https://ok.test/y, <t:0:R>"
        );
        let mut element = Markdown::new(&source, look.palette())
            .width(900.0)
            .render()
            .element;
        decorate(
            &mut element,
            &names,
            &[],
            &references,
            false,
            &Spoilers {
                revealed: &[],
                action: "reveal:".into(),
                first: 0,
            },
            Pictures::default(),
            &look,
        );
        let all = spans(&mut element);
        let find = |text: &str| all.iter().find(|span| span.text == text).cloned();
        let pill = find("Taller › #anuncios › mensaje").expect("a pill");
        assert_eq!(pill.action.as_deref(), Some("place:2:3"));
        assert!(pill.background.is_some());
        // A masked link to a place opens in RUNE and keeps its text.
        assert_eq!(find("esto").unwrap().action.as_deref(), Some("place:4:0"));
        assert_eq!(
            find("acá").unwrap().action.as_deref(),
            Some("link_masked:https://evil.test/x")
        );
        let bare = find("https://ok.test/y").unwrap();
        assert!(bare.action.unwrap().starts_with("markdown_link:"));
        // A moment says its whole date in its hint, and takes the keyboard
        // focus to show it.
        let moment = find("hace 10 minutos").unwrap();
        assert_eq!(moment.action.as_deref(), Some("moment:0"));
        assert_eq!(
            moment.tooltip.map(|hint| hint.title),
            Some(crate::time::full_date(Timestamp(0)))
        );
    }

    #[test]
    fn mentions_of_people_roles_everyone_and_channels_become_pills() {
        let names = Names {
            people: HashMap::from([(UserId(7), "ada".to_owned())]),
            roles: vec![("Moderación".into(), Color::rgb(52, 152, 219))],
            channels: vec![("general".into(), 3)],
            ..Names::default()
        };
        let all = written(
            "Hola @ada y @Moderación y @everyone en #general, no @bruno ni correo@ada",
            &names,
            &[UserId(7)],
            &[],
        );
        let pills: Vec<&str> = all
            .iter()
            .filter(|span| span.background.is_some())
            .map(|span| span.text.as_str())
            .collect();
        assert_eq!(
            pills,
            ["@ada", "@Moderación", "@everyone", "#general"],
            "{all:?}"
        );
        let channel = all.iter().find(|span| span.text == "#general").unwrap();
        assert_eq!(channel.action.as_deref(), Some("channel:3"));
        let role = all.iter().find(|span| span.text == "@Moderación").unwrap();
        assert_eq!(role.color, Color::rgb(52, 152, 219), "a role in its color");
        let text: String = all.iter().map(|span| span.text.as_str()).collect();
        assert!(
            text.contains("no @bruno ni correo@ada"),
            "nothing is lost: {text}"
        );
    }

    #[test]
    fn a_spoiler_hides_until_revealed() {
        let names = Names::default();
        let hidden = written("el final: ||muere|| y ||nadie||", &names, &[], &[]);
        let first = hidden.iter().find(|span| span.text == "muere").unwrap();
        assert_eq!(first.action.as_deref(), Some("msg:1:spoiler:0"));
        assert_eq!(
            first.background,
            Some(first.color),
            "ink and fill alike: hidden"
        );
        let second = hidden.iter().find(|span| span.text == "nadie").unwrap();
        assert_eq!(second.action.as_deref(), Some("msg:1:spoiler:1"));
        let shown = written("el final: ||muere||", &names, &[], &[0]);
        let first = shown.iter().find(|span| span.text == "muere").unwrap();
        assert!(first.action.is_none() && first.color != first.background.unwrap());
        assert!(!hidden.iter().any(|span| span.text.contains("||")));
        // Markdown cuts bold and code into runs of their own: hidden too.
        let cut = written("||el **asesino** es|| y ||es `x`|| fin", &names, &[], &[]);
        assert!(!cut.iter().any(|span| span.text.contains("||")), "{cut:?}");
        let spoiler = |text: &str| {
            cut.iter()
                .find(|span| span.text == text)
                .and_then(|span| span.action.clone())
        };
        assert_eq!(spoiler("asesino").as_deref(), Some("msg:1:spoiler:0"));
        assert_eq!(spoiler(" es").as_deref(), Some("msg:1:spoiler:0"));
        assert_eq!(spoiler("x").as_deref(), Some("msg:1:spoiler:1"));
        assert_eq!(spoiler(" fin"), None);
        let open = written("a || b", &names, &[], &[]);
        let text: String = open.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(text, "a || b", "a lone mark stays as written");
        // A bare link inside one hides with it, and works once shown
        // (2026-10-18: Markdown took the closing marks into the link).
        let link = written("mirá ||https://ok.test/a||", &names, &[], &[]);
        assert!(
            !link.iter().any(|span| span.text.contains("||")),
            "{link:?}"
        );
        let hidden = link.iter().find(|span| span.text == "https://ok.test/a");
        assert_eq!(
            hidden.and_then(|span| span.action.as_deref()),
            Some("msg:1:spoiler:0")
        );
        let shown = written("mirá ||https://ok.test/a||", &names, &[], &[0]);
        let shown = shown.iter().find(|span| span.text == "https://ok.test/a");
        assert!(shown
            .and_then(|span| span.action.as_deref())
            .is_some_and(|action| action.starts_with("markdown_link:")));
    }

    #[test]
    fn discord_marks_reach_the_message() {
        let all = written(
            "__sub__ ~~fuera~~ ***ambas*** **negrita**\n-# al pie",
            &Names::default(),
            &[],
            &[],
        );
        let find = |text: &str| all.iter().find(|span| span.text == text).unwrap();
        assert!(find("sub").underline);
        assert!(find("fuera").strikethrough);
        assert_eq!(find("ambas").font_family, FontFamily::SansBoldItalic);
        assert_eq!(find("negrita").font_family, FontFamily::SansBold);
        let subtext = find("al pie");
        assert!(subtext.font_size < 14.0 && subtext.color == Look::DARK.muted);
    }

    #[test]
    fn an_emoji_named_with_two_underscores_stays_whole_beside_an_underline() {
        let look = Look::DARK;
        let references = [TextReference {
            source: "<:a__b:1>".into(),
            target: TextTarget::Emoji {
                name: "a__b".into(),
                image: None,
                jumbo: None,
                id: None,
            },
        }];
        let mut element = Markdown::new("<:a__b:1> y __x__", look.palette())
            .width(500.0)
            .render()
            .element;
        decorate(
            &mut element,
            &Names::default(),
            &[],
            &references,
            false,
            &Spoilers {
                revealed: &[],
                action: "reveal:".into(),
                first: 0,
            },
            Pictures::default(),
            &look,
        );
        let all = spans(&mut element);
        let shown: Vec<(&str, bool)> = all
            .iter()
            .map(|span| (span.text.as_str(), span.underline))
            .collect();
        assert_eq!(shown, [(":a__b:", false), (" y ", false), ("x", true)]);
    }

    #[test]
    fn everyone_is_found_as_a_word_only() {
        assert!(mentions_everyone("atención @everyone!"));
        assert!(mentions_everyone("@here"));
        assert!(!mentions_everyone("correo@everyone.com no"));
        assert!(!mentions_everyone("hola"));
    }

    #[test]
    fn edited_goes_at_the_end_of_the_last_paragraph_only() {
        let look = Look::DARK;
        let mut text = Markdown::new("hola", look.palette()).render().element;
        let at = Timestamp(1_700_000_000_000);
        assert!(append_edited(&mut text, at, 11.0, &look));
        let mark = spans(&mut text).pop().unwrap();
        assert_eq!(mark.text, " (editado)");
        // When it was edited, in its hint.
        assert_eq!(
            mark.tooltip.map(|hint| hint.title),
            Some(crate::time::full_date(at))
        );
        let mut code = Markdown::new("```\nfn main() {}\n```", look.palette())
            .render()
            .element;
        assert!(!append_edited(&mut code, at, 11.0, &look));
    }

    #[test]
    fn a_draft_paints_mentions_and_quotes() {
        let look = Look::DARK;
        let found = draft_highlights("hola @ada en #general\n> cita", &look);
        let ranges: Vec<_> = found
            .iter()
            .map(|highlight| highlight.range.clone())
            .collect();
        assert_eq!(ranges, [5..9, 13..21, 22..28]);
        // Mentions over the fill of their pill; a quote without one.
        let fills: Vec<_> = found.iter().map(|highlight| highlight.background).collect();
        let pill = Some(Look::alpha(look.accent, 40));
        assert_eq!(fills, [pill, pill, None]);
    }
}
