//! Emoticons typed in a message become emoji when it is sent or edited, as
//! Serein's «convert emoticons» setting does, with its own switch, off until
//! turned on. Code is found first and kept as written; in the rest, only a
//! whitespace-separated word that is exactly an emoticon changes, so a link,
//! a mention or an escaped one (`\:)`) never does.

use std::ops::Range;

/// What the account sends for `text`: converted when it asked for it.
pub(crate) fn sent(ui: &crate::Ui, text: &str) -> String {
    if ui.prefs.convert_emoticons {
        convert(text)
    } else {
        text.to_owned()
    }
}

/// `text` with each emoticon that stands as a word of its own, outside
/// code, turned into its emoji.
pub(crate) fn convert(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut prose = 0;
    for span in code_spans(text) {
        convert_words(&text[prose..span.start], &mut out);
        out.push_str(&text[span.clone()]);
        prose = span.end;
    }
    convert_words(&text[prose..], &mut out);
    out
}

/// The emoji `word` stands for, if it is an emoticon.
fn emoji(word: &str) -> Option<&'static str> {
    Some(match word {
        "<3" => "❤️",
        ":'(" => "😢",
        ":)" | ":-)" => "🙂",
        ":(" | ":-(" => "🙁",
        ";)" | ";-)" => "😉",
        ":D" | ":-D" => "😃",
        ":/" | ":-/" => "😕",
        ":P" | ":p" | ":-P" | ":-p" => "😛",
        ":o" | ":O" | ":-o" | ":-O" => "😮",
        _ => return None,
    })
}

/// `prose` onto `out`, word by word, its whitespace untouched.
fn convert_words(prose: &str, out: &mut String) {
    for piece in prose.split_inclusive(char::is_whitespace) {
        let (word, gap) = piece.split_at(piece.find(char::is_whitespace).unwrap_or(piece.len()));
        out.push_str(emoji(word).unwrap_or(word));
        out.push_str(gap);
    }
}

/// Where `text` has code, in order: a run of backticks opens a span that
/// the next run as long closes. Left open, a fence (three or more) runs to
/// the end and a shorter run is plain text, as is a run after an odd
/// number of backslashes.
fn code_spans(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut runs = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let length = bytes[at..].iter().take_while(|byte| **byte == b'`').count();
        if length == 0 {
            at += 1;
        } else {
            runs.push(at..at + length);
            at += length;
        }
    }
    let mut spans = Vec::new();
    let mut open = 0;
    // shortcut: an unclosed run looks at every later one, quadratic in
    // backtick runs; a message's length keeps that small.
    while let Some(run) = runs.get(open) {
        let escapes = bytes[..run.start]
            .iter()
            .rev()
            .take_while(|byte| **byte == b'\\')
            .count();
        if escapes % 2 == 1 {
            open += 1;
            continue;
        }
        let close = runs[open + 1..]
            .iter()
            .position(|other| other.len() == run.len())
            .map(|found| open + 1 + found);
        match close {
            Some(close) => {
                spans.push(run.start..runs[close].end);
                open = close + 1;
            }
            None if run.len() >= 3 => {
                spans.push(run.start..text.len());
                break;
            }
            None => open += 1,
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::convert;

    #[test]
    fn each_emoticon_alone_becomes_its_emoji() {
        assert_eq!(convert("<3"), "❤️");
        assert_eq!(convert("chau :'("), "chau 😢");
        assert_eq!(convert(":-) :( ;-) :-D :/ :-p :O"), "🙂 🙁 😉 😃 😕 😛 😮");
        // Whitespace of any kind stays as it was.
        assert_eq!(convert("  :D\t;)\r\n:o "), "  😃\t😉\r\n😮 ");
        assert_eq!(convert(""), "");
        assert_eq!(convert("ningún emoticón acá"), "ningún emoticón acá");
    }

    #[test]
    fn an_emoticon_inside_a_longer_word_stays() {
        for kept in [
            "https://rune.test/a:)",
            "<@42>:)",
            "<:feliz:7>",
            "\\:)",
            "(:)",
            "jaja:D",
            ":Dxd",
            "8)",
        ] {
            assert_eq!(convert(kept), kept);
        }
    }

    #[test]
    fn code_keeps_its_emoticons() {
        assert_eq!(convert("ok `:)` <3"), "ok `:)` ❤️");
        // A span closes only on a run as long as the one that opened it.
        assert_eq!(convert(":P ``a ` :P`` :P"), "😛 ``a ` :P`` 😛");
        assert_eq!(convert("```\n;)\n```\n;)"), "```\n;)\n```\n😉");
        // A fence left open keeps the rest; a lone tick keeps nothing.
        assert_eq!(convert(":( ```\n:("), "🙁 ```\n:(");
        assert_eq!(convert("` :( "), "` 🙁 ");
        // An escaped tick opens no span, a doubly escaped one does.
        assert_eq!(convert("\\` :/ `"), "\\` 😕 `");
        assert_eq!(convert("\\\\` :/ `"), "\\\\` :/ `");
    }
}
