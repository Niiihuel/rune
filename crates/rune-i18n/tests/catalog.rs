//! Every interface text marked in the workspace's code has its English, and
//! every English has its text: the two languages hold the same texts.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The `.rs` files of every crate's and app's `src`.
fn sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut files = Vec::new();
    for group in ["crates", "apps"] {
        let Ok(entries) = std::fs::read_dir(root.join(group)) else {
            continue;
        };
        for entry in entries.flatten() {
            walk(&entry.path().join("src"), &mut files);
        }
    }
    files.sort();
    assert!(
        files.len() > 100,
        "the workspace's sources: {}",
        files.len()
    );
    files
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

/// The Spanish texts `source` marks: the string literal right after
/// `t(`, `es(` or `tr!(`, unescaped, outside comments.
fn marked(source: &str) -> Vec<String> {
    let bytes = source.as_bytes();
    let mut found = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                i = source[i..].find('\n').map_or(bytes.len(), |end| i + end);
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                i = source[i..]
                    .find("*/")
                    .map_or(bytes.len(), |end| i + end + 2);
            }
            b'\'' => {
                // A char literal ('"' among them) or a lifetime.
                let rest = &source[i + 1..];
                let close = if rest.starts_with('\\') {
                    rest[2..].find('\'').map(|at| at + 2)
                } else {
                    rest.char_indices()
                        .nth(1)
                        .filter(|(_, c)| *c == '\'')
                        .map(|(at, _)| at)
                };
                i += close.map_or(1, |at| at + 2);
            }
            b'r' if raw_start(source, i).is_some() => {
                let (hashes, open) = raw_start(source, i).unwrap();
                let close = format!("\"{}", "#".repeat(hashes));
                i = source[open..]
                    .find(&close)
                    .map_or(bytes.len(), |end| open + end + close.len());
            }
            b'"' => {
                let mut end = i + 1;
                while end < bytes.len() && bytes[end] != b'"' {
                    end += if bytes[end] == b'\\' { 2 } else { 1 };
                }
                let before = source[..i].trim_end();
                let call = ["t(", "es(", "tr!("].into_iter().find(|call| {
                    before.ends_with(call)
                        && !before[..before.len() - call.len()]
                            .ends_with(|c: char| c.is_alphanumeric() || c == '_')
                });
                if call.is_some() {
                    found.push(unescape(&source[i + 1..end]));
                }
                i = end + 1;
            }
            _ => i += 1,
        }
    }
    found
}

/// Where a raw string literal starting at `at` opens: its `#`s and the byte
/// after its quote.
fn raw_start(source: &str, at: usize) -> Option<(usize, usize)> {
    let before = source[..at].chars().next_back();
    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
        return None;
    }
    let rest = &source[at + 1..];
    let hashes = rest.len() - rest.trim_start_matches('#').len();
    rest[hashes..]
        .starts_with('"')
        .then_some((hashes, at + 1 + hashes + 1))
}

fn unescape(literal: &str) -> String {
    let mut out = String::new();
    let mut chars = literal.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('0') => out.push('\0'),
            Some('u') => {
                let code: String = chars
                    .by_ref()
                    .skip_while(|c| *c == '{')
                    .take_while(|c| *c != '}')
                    .collect();
                out.extend(u32::from_str_radix(&code, 16).ok().and_then(char::from_u32));
            }
            Some('\n') => {
                while chars.peek().is_some_and(|c| c.is_whitespace()) {
                    chars.next();
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// The `{name}`s of a text, without `{{` braces.
fn places(text: &str) -> BTreeSet<&str> {
    let mut found = BTreeSet::new();
    let mut rest = text;
    while let Some(at) = rest.find('{') {
        let tail = &rest[at..];
        if let Some(after) = tail.strip_prefix("{{") {
            rest = after;
            continue;
        }
        match tail[1..].split_once('}') {
            Some((name, after)) => {
                found.insert(name);
                rest = after;
            }
            None => break,
        }
    }
    found
}

#[test]
fn every_marked_text_has_its_english_and_every_english_its_text() {
    let mut used: BTreeMap<String, String> = BTreeMap::new();
    for file in sources() {
        let source = std::fs::read_to_string(&file).unwrap();
        for text in marked(&source) {
            used.entry(text)
                .or_insert_with(|| file.display().to_string());
        }
    }
    let mut english: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
    let mut problems = Vec::new();
    for (file, es, en) in rune_i18n::catalog() {
        if let Some((other, before)) = english.insert(es, (file, en)) {
            if other == file {
                problems.push(format!("{file}: {es:?} is there twice"));
            } else if before != en {
                problems.push(format!(
                    "{es:?} is {before:?} in {other} and {en:?} in {file}"
                ));
            }
        }
        if en.trim().is_empty() {
            problems.push(format!("{file}: {es:?} has no English"));
        }
        if places(es) != places(en) {
            problems.push(format!("{file}: {es:?} and {en:?} have other {{places}}"));
        }
    }
    for (text, file) in &used {
        if !english.contains_key(text.as_str()) {
            problems.push(format!("{file}: {text:?} has no English"));
        }
    }
    for (text, (file, _)) in &english {
        if !used.contains_key(*text) {
            problems.push(format!("{file}: {text:?} is not in the code"));
        }
    }
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn english_has_no_spanish_left() {
    let spanish: Vec<String> = rune_i18n::catalog()
        .filter(|(_, _, en)| en.contains(['á', 'é', 'í', 'ó', 'ú', 'ñ', '¿', '¡']))
        // Names that keep their letters in English.
        .filter(|(_, _, en)| !["Español"].iter().any(|name| en.contains(name)))
        .map(|(file, es, en)| format!("{file}: {es:?} → {en:?}"))
        .collect();
    assert!(spanish.is_empty(), "{}", spanish.join("\n"));
}

#[test]
fn the_scanner_finds_marked_texts_only() {
    let source = r##"
        // t("comentario")
        let a = t("Hola");
        let b = tr!("Hola, {name}", name);
        const C: &str = es("Cargando…");
        let d = format!("t(\"no\")");
        let e = rune_i18n::t("Línea \
                    seguida");
        let f = '"'; let g = t("Después \"citado\"");
        let h = start("no");
        let i = r#"t("crudo")"#;
    "##;
    assert_eq!(
        marked(source),
        [
            "Hola",
            "Hola, {name}",
            "Cargando…",
            "Línea seguida",
            "Después \"citado\""
        ]
    );
}

/// Spanish that reads two ways where English has a word for each: the place
/// that means the other sense says it in Spanish too, and each sense keeps
/// its English (2026-10-17).
#[test]
fn a_word_with_two_senses_says_which_one_where_it_is_used() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (file, spanish, english, other, others) in [
        // A thread no one can write in; «Bloqueado» is a blocked person.
        (
            "crates/rune-components/src/threads.rs",
            "Hilo bloqueado",
            "Locked thread",
            "Bloqueado",
            "Blocked",
        ),
        // The player's sound; «Desactivar ensordecimiento» undeafens a call.
        (
            "crates/rune-components/src/message/media.rs",
            "Activar el sonido",
            "Unmute",
            "Desactivar ensordecimiento",
            "Undeafen",
        ),
        // Who may send friend requests; «Todos» is a filter's "All".
        (
            "crates/rune-ui/src/account.rs",
            "Todo el mundo",
            "Everyone",
            "Todos",
            "All",
        ),
    ] {
        let texts = marked(&std::fs::read_to_string(root.join(file)).unwrap());
        assert!(
            texts.iter().any(|text| text == spanish),
            "{file}: «{spanish}»"
        );
        assert!(!texts.iter().any(|text| text == other), "{file}: «{other}»");
        assert_eq!(rune_i18n::english(spanish), Some(english));
        assert_eq!(rune_i18n::english(other), Some(others));
    }
}
