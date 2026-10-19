//! RUNE's interface in Spanish and English.
//!
//! Interface text is written in Spanish where the code uses it, and that
//! Spanish text is the key of its English translation in this crate's
//! catalog (`src/en/`). [`t`] translates a text, [`tr!`] a text with values
//! in it (`{name}`), and [`es`] marks a text that is translated later, where
//! it is shown: one in a `const` table, or one a lower layer hands up to the
//! interface. A test (`tests/catalog.rs`) reads the workspace's sources and
//! fails when a marked text has no English, or English has no text.
//!
//! The language is the process's ([`set`]), which the host takes from the
//! settings, unless a thread chose its own ([`set_thread`]): the window's
//! thread does each frame, so a test that draws it in English changes no
//! other test, and [`scoped`] does it for a block.
//!
//! Plurals: Spanish and English both say "one" only for exactly one, so the
//! code picks the form (`if n == 1`) and each form is a text of its own.
//! Dates and times are texts too ("{day} de {month} de {year}", "%d/%m/%Y").

use std::cell::Cell;
use std::collections::HashMap;
use std::fmt::{Display, Write};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

mod en;

/// A language RUNE's interface is written in.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Language {
    /// The code's own: every text is its own key.
    #[default]
    Spanish,
    English,
}

impl Language {
    pub const ALL: [Self; 2] = [Self::Spanish, Self::English];

    /// Its code, in the settings and to a helper process: "es", "en".
    pub const fn code(self) -> &'static str {
        match self {
            Self::Spanish => "es",
            Self::English => "en",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.code() == code)
    }

    /// Its name in itself, as a list of languages shows it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Spanish => "Español",
            Self::English => "English",
        }
    }

    /// The language of a POSIX locale ("en_US.UTF-8", "es_AR", "en"), when
    /// RUNE has it.
    pub fn from_locale(locale: &str) -> Option<Self> {
        let language = locale.split(['_', '.', '@', '-']).next()?;
        Self::from_code(&language.to_ascii_lowercase())
    }

    /// The language the session's environment asks for messages in: the
    /// first RUNE has of `LANGUAGE`'s list, else the locale of `LC_ALL`,
    /// `LC_MESSAGES` or `LANG`, the first set; Spanish when it is neither.
    pub fn from_environment(var: impl Fn(&str) -> Option<String>) -> Self {
        let set = |name: &str| var(name).filter(|value| !value.is_empty());
        let preferred =
            set("LANGUAGE").and_then(|list| list.split(':').find_map(Self::from_locale));
        preferred
            .or_else(|| {
                ["LC_ALL", "LC_MESSAGES", "LANG"]
                    .into_iter()
                    .find_map(set)
                    .and_then(|locale| Self::from_locale(&locale))
            })
            .unwrap_or_default()
    }

    /// [`Self::from_environment`] of this process.
    pub fn system() -> Self {
        Self::from_environment(|name| std::env::var(name).ok())
    }
}

static PROCESS: AtomicU8 = AtomicU8::new(0);

thread_local! {
    static THREAD: Cell<Option<Language>> = const { Cell::new(None) };
}

/// The process's language, for every thread that did not choose its own.
pub fn set(language: Language) {
    PROCESS.store(language as u8, Ordering::Relaxed);
}

/// This thread's language; `None` follows the process's.
pub fn set_thread(language: Option<Language>) {
    THREAD.with(|cell| cell.set(language));
}

/// The language texts are written in on this thread.
pub fn current() -> Language {
    THREAD
        .with(Cell::get)
        .unwrap_or_else(|| match PROCESS.load(Ordering::Relaxed) {
            1 => Language::English,
            _ => Language::Spanish,
        })
}

/// This thread writes in `language` until the guard drops.
#[must_use = "the language goes back when this drops"]
pub fn scoped(language: Language) -> Scoped {
    let before = THREAD.with(|cell| cell.replace(Some(language)));
    Scoped { before }
}

/// See [`scoped`].
pub struct Scoped {
    before: Option<Language>,
}

impl Drop for Scoped {
    fn drop(&mut self) {
        set_thread(self.before);
    }
}

/// `text`, Spanish, in the current language. A text the catalog does not
/// have stays as it is: a name, or Spanish the catalog lacks (which the
/// catalog test would catch where it is written).
pub fn t(text: &str) -> &str {
    match current() {
        Language::Spanish => text,
        Language::English => english(text).unwrap_or(text),
    }
}

/// Marks Spanish `text` as interface text that is translated with [`t`]
/// where it is shown, for a `const` table or a text from a lower layer.
pub const fn es(text: &'static str) -> &'static str {
    text
}

/// The English of Spanish `text`, if the catalog has it.
pub fn english(text: &str) -> Option<&'static str> {
    static MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    MAP.get_or_init(|| {
        en::ALL
            .iter()
            .flat_map(|(_, entries)| entries.iter().copied())
            .collect()
    })
    .get(text)
    .copied()
}

/// Every entry of the English catalog with the file it is in, for the
/// catalog test.
#[doc(hidden)]
pub fn catalog() -> impl Iterator<Item = (&'static str, &'static str, &'static str)> {
    en::ALL
        .iter()
        .flat_map(|(file, entries)| entries.iter().map(move |(es, en)| (*file, *es, *en)))
}

/// `template` with each `{name}` replaced by its value; `{{` and `}}` are
/// braces. What [`tr!`] calls.
pub fn fill(template: &str, values: &[(&str, &dyn Display)]) -> String {
    let mut out = String::with_capacity(template.len() + 16);
    let mut rest = template;
    while let Some(at) = rest.find(['{', '}']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
            continue;
        }
        let named = tail
            .strip_prefix('{')
            .and_then(|inner| inner.split_once('}'))
            .and_then(|(name, after)| {
                let value = values.iter().find(|(key, _)| *key == name)?.1;
                Some((value, after))
            });
        match named {
            Some((value, after)) => {
                let _ = write!(out, "{value}");
                rest = after;
            }
            None => {
                debug_assert!(false, "{template:?} has a brace without a value");
                out.push_str(&tail[..1]);
                rest = &tail[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// A Spanish text with values, in the current language:
/// `tr!("Hola, {name}", name)` or `tr!("{n} mensajes", n = list.len())`.
/// Each `{name}` takes the value given by that name, `Display`ed, so the
/// English catalog may put them in another order.
#[macro_export]
macro_rules! tr {
    ($text:literal $(,)?) => {
        ::std::string::String::from($crate::t($text))
    };
    ($text:literal, $($name:ident $(= $value:expr)?),+ $(,)?) => {
        $crate::fill(
            $crate::t($text),
            &[$((
                ::core::stringify!($name),
                &$crate::__value!($name $(= $value)?) as &dyn ::core::fmt::Display,
            )),+],
        )
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __value {
    ($name:ident) => {
        $name
    };
    ($name:ident = $value:expr) => {
        $value
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_fill_their_places_in_any_order() {
        let name = "Ana";
        assert_eq!(fill("Hola, {name}", &[("name", &name)]), "Hola, Ana");
        assert_eq!(
            fill("{b} y {a} {{literal}}", &[("a", &1), ("b", &"dos")]),
            "dos y 1 {literal}"
        );
    }

    #[test]
    fn a_thread_writes_in_its_language_and_spanish_is_the_default() {
        assert_eq!(current(), Language::Spanish);
        assert_eq!(t("Cancelar"), "Cancelar");
        {
            let _english = scoped(Language::English);
            assert_eq!(current(), Language::English);
            assert_eq!(t("Cancelar"), "Cancel");
            // A text the catalog lacks, as a name, stays as it is.
            let name = String::from("Ana");
            assert_eq!(t(&name), "Ana");
            let count = 3;
            assert_eq!(
                tr!("{count} mensajes bloqueados", count),
                "3 blocked messages"
            );
        }
        assert_eq!(current(), Language::Spanish);
        // Another thread follows the process, untouched by this one.
        let _english = scoped(Language::English);
        assert_eq!(
            std::thread::spawn(current).join().unwrap(),
            Language::Spanish
        );
    }

    #[test]
    fn the_system_language_comes_from_the_environment() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| (*value).to_owned())
            }
        };
        assert_eq!(
            Language::from_environment(env(&[("LANG", "en_US.UTF-8")])),
            Language::English
        );
        assert_eq!(
            Language::from_environment(env(&[("LANG", "en_US.UTF-8"), ("LC_MESSAGES", "es_AR")])),
            Language::Spanish
        );
        assert_eq!(
            Language::from_environment(env(&[("LANGUAGE", "fr:en_GB"), ("LANG", "es_ES")])),
            Language::English
        );
        // Neither language, or nothing: Spanish.
        assert_eq!(
            Language::from_environment(env(&[("LANG", "fr_FR.UTF-8")])),
            Language::Spanish
        );
        assert_eq!(
            Language::from_environment(env(&[("LANG", "C")])),
            Language::Spanish
        );
        assert_eq!(Language::from_environment(env(&[])), Language::Spanish);
        assert_eq!(Language::from_locale("es"), Some(Language::Spanish));
        assert_eq!(Language::from_code("pt"), None);
    }
}
