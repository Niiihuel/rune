//! Twemoji 17.0.3 artwork for limn-egui's color emoji: the upstream 72 px
//! PNGs back to back, and an index by sequence. `scripts/twemoji.py` builds
//! both; `assets/licenses/TWEMOJI.md` has their provenance and license.

use std::collections::HashMap;
use std::sync::OnceLock;

const PNGS: &[u8] = include_bytes!("../../../assets/twemoji/pngs.bin");
const INDEX: &str = include_str!("../../../assets/twemoji/index.tsv");

const VS15: char = '\u{FE0E}';
const VS16: char = '\u{FE0F}';

/// Where one picture's bytes are, and whether its lone code point is text
/// unless VS16 asks for an emoji.
struct Entry {
    start: usize,
    end: usize,
    text_default: bool,
}

/// Sequences without VS16, as text.
fn index() -> &'static HashMap<String, Entry> {
    static INDEXED: OnceLock<HashMap<String, Entry>> = OnceLock::new();
    INDEXED.get_or_init(|| {
        INDEX
            .lines()
            .filter_map(|line| {
                let mut fields = line.split('\t');
                let sequence = fields
                    .next()?
                    .split('-')
                    .map(|point| char::from_u32(u32::from_str_radix(point, 16).ok()?))
                    .collect::<Option<String>>()?;
                let start: usize = fields.next()?.parse().ok()?;
                let end = start.checked_add(fields.next()?.parse().ok()?)?;
                let text_default = fields.next()? == "t";
                (end <= PNGS.len()).then_some((
                    sequence,
                    Entry {
                        start,
                        end,
                        text_default,
                    },
                ))
            })
            .collect()
    })
}

/// The PNG for one grapheme cluster, or `None` to keep the font's glyph:
/// a cluster outside the set, one with VS15, or a lone code point that is
/// text by default (`❤`, `©`, `↔`) without VS16.
pub fn picture(cluster: &str) -> Option<&'static [u8]> {
    // The longest sequence, a family with skin tones, is under 40 bytes.
    if cluster.len() > 64 || cluster.contains(VS15) {
        return None;
    }
    let key: String = cluster.chars().filter(|point| *point != VS16).collect();
    let entry = index().get(&key)?;
    if entry.text_default && !cluster.contains(VS16) {
        return None;
    }
    PNGS.get(entry.start..entry.end)
}

// Emoji and states (2026-10-18): the set as a table of what is an emoji,
// and its pictures for a line of text.

/// What the official client never draws as an emoji, with or without VS16.
const TEXT_ONLY: [char; 3] = ['\u{2122}', '\u{A9}', '\u{AE}'];

/// The most code points of a sequence of the set, VS16 left out.
fn longest() -> usize {
    static LONGEST: OnceLock<usize> = OnceLock::new();
    *LONGEST.get_or_init(|| {
        index()
            .keys()
            .map(|key| key.chars().count())
            .max()
            .unwrap_or(1)
    })
}

/// How many bytes of `text` the emoji it starts with takes, its variation
/// selectors included; `None` when it starts with something else. The
/// longest sequence of the set wins, so a keycap (`1️⃣`), a flag or a family
/// is one emoji and a lone digit is text. A lone code point that is text
/// by default (`❤`, `➡`, `↔`) counts, as the chat services draw it; `©`,
/// `®` and `™` never do, nor an emoji VS15 asks to be text, as [`picture`]
/// keeps it.
pub fn emoji_len(text: &str) -> Option<usize> {
    let first = text.chars().next()?;
    // Only keycaps start with ASCII.
    if first.is_ascii() && !matches!(first, '0'..='9' | '#' | '*') || TEXT_ONLY.contains(&first) {
        return None;
    }
    let mut key = String::new();
    let mut found = None;
    for (at, point) in text.char_indices() {
        let end = at + point.len_utf8();
        if point == VS15 && found == Some(at) {
            return None;
        }
        if point == VS16 || point == VS15 {
            if found == Some(at) {
                found = Some(end);
            }
            continue;
        }
        key.push(point);
        if key.chars().count() > longest() {
            break;
        }
        if index().contains_key(&key) {
            found = Some(end);
        }
    }
    found
}

/// The picture of the emoji `sequence` (one [`emoji_len`] found) to draw
/// in a line in its place, as a message of emoji alone shows them; the
/// same one is handed out again while the few kept last. None with VS15.
// shortcut: the pack's 72 px PNG, drawn 48 px; past a scale of 1.5 it is
// upscaled (a server's asks 96). Upgrade with larger art in the pack.
pub fn image(sequence: &str) -> Option<limn::RasterImage> {
    use base64::Engine;
    use sha2::Digest;
    static MADE: std::sync::Mutex<Option<HashMap<String, limn::RasterImage>>> =
        std::sync::Mutex::new(None);
    if sequence.contains(VS15) {
        return None;
    }
    let mut made = MADE.lock().ok()?;
    let made = made.get_or_insert_with(HashMap::new);
    if let Some(image) = made.get(sequence) {
        return Some(image.clone());
    }
    let key: String = sequence
        .chars()
        .filter(|point| *point != VS16 && *point != VS15)
        .collect();
    let entry = index().get(&key)?;
    let png = PNGS.get(entry.start..entry.end)?;
    let image = limn::RasterImage::new(
        format!("{:x}", sha2::Sha256::digest(png)),
        limn::RasterImageFormat::Png,
        u32::try_from(png.len()).ok()?,
        std::sync::Arc::from(base64::engine::general_purpose::STANDARD.encode(png)),
        sequence,
    )?;
    // shortcut: the kept pictures are dropped all at once past 256; a page
    // of messages rarely shows that many different emoji.
    if made.len() >= 256 {
        made.clear();
    }
    made.insert(sequence.to_owned(), image.clone());
    Some(image)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_set_tells_an_emoji_from_text_by_its_longest_sequence() {
        let len = |text: &str| emoji_len(text);
        assert_eq!(len("1\u{fe0f}\u{20e3} y"), Some(7), "a keycap is one emoji");
        assert_eq!(len("#\u{20e3}"), Some(4));
        assert_eq!(len("1"), None, "a lone digit is text");
        assert_eq!(len("12"), None);
        assert_eq!(len("👍🏽!"), Some(8), "with its skin tone");
        assert_eq!(len("🇦🇷🇧🇷"), Some(8), "one flag at a time");
        assert_eq!(len("🇿🇿"), Some(4), "no flag: a letter, as its emoji");
        assert_eq!(len("👨\u{200d}👩\u{200d}👧 "), Some(18), "a family is one");
        assert_eq!(len("🏳\u{fe0f}\u{200d}🌈"), Some(14), "VS16 inside");
        assert_eq!(len("❤\u{fe0f}"), Some(6));
        assert_eq!(len("❤"), Some(3), "text by default, still an emoji");
        // The arrows of the set are emoji (2026-10-18); `→` is not in it.
        assert_eq!(len("➡"), Some(3));
        assert_eq!(len("↔ y"), Some(3));
        assert_eq!(len("😀\u{fe0e}"), None, "VS15 asks for its text");
        assert_eq!(len("➡\u{fe0e}"), None);
        assert!(image("😀\u{fe0e}").is_none());
        for text in ["©", "©\u{fe0f}", "™", "®", "→", "a", "", "\u{200d}"] {
            assert_eq!(len(text), None, "{text:?}");
        }
    }

    #[test]
    fn an_emoji_s_picture_is_its_png_made_once() {
        let first = image("👍").unwrap();
        assert_eq!(first.alt(), "👍");
        assert_eq!(first.format(), limn::RasterImageFormat::Png);
        assert_eq!(image("👍").unwrap().sha256(), first.sha256());
        assert_ne!(image("👍🏽").unwrap().sha256(), first.sha256());
        assert_eq!(
            image("❤\u{fe0f}").unwrap().sha256(),
            image("❤").unwrap().sha256()
        );
        assert!(image("a").is_none());
    }

    #[test]
    fn every_entry_is_a_72_pixel_png_inside_the_pack() {
        assert_eq!(index().len(), 4009);
        assert_eq!(INDEX.lines().count(), 4009, "no line was dropped");
        let mut covered = 0;
        for entry in index().values() {
            let png = &PNGS[entry.start..entry.end];
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n") && &png[12..16] == b"IHDR");
            assert_eq!(png[16..24], [0, 0, 0, 72, 0, 0, 0, 72]);
            covered += png.len();
        }
        assert_eq!(covered, PNGS.len(), "the pictures tile the pack");
    }

    #[test]
    fn sequences_are_found_whole() {
        for cluster in [
            "😀",
            "👨\u{200d}👩\u{200d}👧\u{200d}👦",
            "🧑🏽\u{200d}💻",
            "🏳\u{fe0f}\u{200d}🌈",
            "🏳\u{200d}🌈",
            "🇦🇷",
            "🏴\u{e0067}\u{e0062}\u{e0073}\u{e0063}\u{e0074}\u{e007f}",
            "1\u{fe0f}\u{20e3}",
            "#\u{20e3}",
            "👍🏽",
            "👍",
        ] {
            assert!(picture(cluster).is_some(), "{cluster}");
        }
        assert_ne!(
            picture("👍🏽"),
            picture("👍"),
            "a skin tone is its own picture"
        );
    }

    #[test]
    fn presentation_follows_the_variation_selector() {
        assert!(picture("❤\u{fe0f}").is_some());
        assert!(picture("❤").is_none(), "text by default");
        assert!(picture("😀\u{fe0e}").is_none(), "VS15 asks for text");
        assert!(picture("©").is_none() && picture("©\u{fe0f}").is_some());
        assert!(picture("☀\u{fe0f}").is_some() && picture("☀").is_none());
    }

    #[test]
    fn what_is_not_in_the_set_keeps_its_glyph() {
        for cluster in [
            "a",
            "1",
            "#",
            "→",
            "é",
            "\u{200d}",
            "🇿🇿",
            "",
            &"😀".repeat(20),
        ] {
            assert!(picture(cluster).is_none(), "{cluster}");
        }
    }
}
