//! The interface's language (P6): Español, English or the system's, a
//! setting of this device, not of the account. A choice shows from the next
//! frame on, without restarting: the view writes every text in
//! [`Ui::language`], and the host hands it to the process for what other
//! threads write (the core, the tray).

use rune_i18n::Language;
use rune_state::Store;

use crate::settings::Prefs;
use crate::{Intent, Ui};

/// The setting: "es", "en", or none for the system's.
pub(crate) const KEY: &str = "language";

impl Ui {
    /// The language the interface is in: the one chosen, or the system's.
    pub fn language(&self) -> Language {
        self.prefs.language.unwrap_or(self.desktop.system_language)
    }
}

pub(crate) fn kept(prefs: &Prefs) -> (&'static str, Option<String>) {
    (KEY, prefs.language.map(|language| language.code().into()))
}

pub(crate) fn restore(prefs: &mut Prefs, setting: &impl Fn(&str) -> Option<String>) {
    prefs.language = setting(KEY).as_deref().and_then(Language::from_code);
}

/// `us:lang={es|en|system}`: choose, and write in it from now on. What the
/// window keeps already written (the inbox's rows) is written again.
pub(crate) fn clicked(ui: &mut Ui, store: &Store, rest: &str) -> Option<Vec<Intent>> {
    let code = rest.strip_prefix("lang=")?;
    ui.prefs.language = Language::from_code(code);
    rune_i18n::set_thread(Some(ui.language()));
    crate::inbox::rewrite(ui, store);
    let (key, value) = kept(&ui.prefs);
    Some(vec![Intent::SaveSetting {
        key: key.into(),
        value,
    }])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_s_language_is_the_default_and_a_choice_is_kept() {
        let mut ui = Ui::new();
        let store = Store::new();
        assert_eq!(ui.language(), Language::Spanish, "tests write Spanish");
        ui.desktop.system_language = Language::English;
        assert_eq!(ui.language(), Language::English);
        let saved = clicked(&mut ui, &store, "lang=es").unwrap();
        assert_eq!(ui.language(), Language::Spanish);
        assert!(matches!(
            &saved[..],
            [Intent::SaveSetting { key, value: Some(value) }] if key == KEY && value == "es"
        ));
        assert_eq!(rune_i18n::current(), Language::Spanish);
        clicked(&mut ui, &store, "lang=system").unwrap();
        assert_eq!(ui.language(), Language::English);
        assert_eq!(rune_i18n::current(), Language::English, "at once");
        rune_i18n::set_thread(None);
        // Kept as a setting of the device, back as it was.
        let kept: Vec<_> = ui.settings_to_keep();
        let value = kept.iter().find(|(key, _)| *key == KEY).unwrap().1.clone();
        assert_eq!(value, None);
        let mut other = Ui::new();
        other.restore(|key| (key == KEY).then(|| "en".to_owned()));
        assert_eq!(other.prefs.language, Some(Language::English));
        assert!(!crate::ACCOUNT_SETTINGS.contains(&KEY));
    }
}
