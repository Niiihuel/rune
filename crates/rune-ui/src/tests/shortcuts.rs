//! Editable shortcuts (atajos-y-navegacion): recording another key, its
//! conflicts and refusals, «Restablecer», and the composer answering the
//! keys chosen, over the laid-out limn tree.

use crate::desktop::Shortcut;
use crate::keybinds::{Action, Chord};

use super::*;

fn saved(intents: &[Intent], key: &str) -> Option<Option<String>> {
    intents.iter().find_map(|intent| match intent {
        Intent::SaveSetting { key: saved, value } if saved == key => Some(value.clone()),
        _ => None,
    })
}

impl Host {
    fn settings_click(&mut self, id: &str) -> Vec<Intent> {
        let intents = update(
            &mut self.ui,
            &mut self.store,
            &UiAction::Clicked { id: id.into() },
            self.now,
        );
        self.run(intents.clone());
        intents
    }

    /// Record `key`, as the host names it, for `action`.
    fn record(&mut self, action: Action, key: &str) -> Vec<Intent> {
        self.ui.settings = Some(crate::SettingsPage::System);
        self.settings_click(&format!("us:key-record={}", action.key()));
        assert!(crate::recording_key(&self.ui));
        let intents = crate::record_key(&mut self.ui, Some(key.into()));
        self.run(intents.clone());
        intents
    }

    fn notice(&self, action: Action) -> Option<&str> {
        self.ui
            .settings_state
            .key_notice
            .as_ref()
            .filter(|(refused, _)| *refused == action)
            .map(|(_, why)| why.as_str())
    }
}

#[test]
fn another_key_is_kept_and_reset_takes_it_back() {
    let mut host = Host::new();
    let intents = host.record(Action::Back, "F9");
    assert_eq!(saved(&intents, "shortcuts"), Some(Some("back=F9".into())));
    assert_eq!(
        crate::keybinds::binding(&host.ui.prefs, Action::Back),
        Some(Chord::plain(Key::F9))
    );
    // The settings show it with its reset.
    host.ui.settings_state.query = "historial".into();
    let painted = host.painted();
    assert!(painted.iter().any(|text| text == "Atrás en el historial"));
    assert!(painted.iter().any(|text| text == "Restablecer"));
    let intents = host.settings_click("us:key-reset=back");
    assert_eq!(saved(&intents, "shortcuts"), Some(None));
    assert_eq!(
        crate::keybinds::binding(&host.ui.prefs, Action::Back),
        Action::Back.default_chord()
    );
    // Recording the default again keeps nothing.
    let intents = host.record(Action::Back, "Alt+Left");
    assert_eq!(saved(&intents, "shortcuts"), Some(None));
}

#[test]
fn a_key_another_action_has_is_refused_with_its_name() {
    let mut host = Host::new();
    let intents = host.record(Action::Back, "Ctrl+K");
    assert!(intents.is_empty());
    assert_eq!(
        host.notice(Action::Back),
        Some("Ya lo usa «Selector rápido de conversaciones».")
    );
    assert_eq!(
        crate::keybinds::binding(&host.ui.prefs, Action::Back),
        Action::Back.default_chord()
    );
    host.ui.settings_state.query = "historial".into();
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Ya lo usa «Selector rápido de conversaciones»."));
    // Push to talk, recorded from Voz y video, says why there too.
    host.ui.prefs.push_to_talk = true;
    host.ui.settings = Some(crate::SettingsPage::Voice);
    host.settings_click("us:ptt-record");
    let intents = crate::record_key(&mut host.ui, Some("Ctrl+K".into()));
    assert!(intents.is_empty());
    host.ui.settings_state.query = "grabar".into();
    assert!(host
        .painted()
        .iter()
        .any(|text| text == "Ya lo usa «Selector rápido de conversaciones»."));
    assert_eq!(host.ui.prefs.push_to_talk_key, None);
    // A voice shortcut is refused the same.
    host.record(Action::Voice(Shortcut::Mute), "Ctrl+F");
    assert_eq!(host.ui.prefs.mute_key, None);
    assert_eq!(
        host.notice(Action::Voice(Shortcut::Mute)),
        Some("Ya lo usa «Buscar en la conversación».")
    );
    // Shift does not make another chord of a typed character.
    host.record(Action::Search, "Ctrl+Shift+Slash");
    assert_eq!(
        host.notice(Action::Search),
        Some("Ya lo usa «Mostrar esta lista de atajos».")
    );
}

#[test]
fn keys_that_type_or_edit_are_refused_and_recording_again_clears_why() {
    let mut host = Host::new();
    host.record(Action::Switcher, "K");
    assert_eq!(
        host.notice(Action::Switcher),
        Some("Sola escribe o mueve el cursor: súmale Ctrl o Alt.")
    );
    host.record(Action::Switcher, "Ctrl+V");
    assert_eq!(
        host.notice(Action::Switcher),
        Some("La usa la edición de texto: elige otra.")
    );
    host.record(Action::Switcher, "Ctrl+J");
    assert_eq!(host.notice(Action::Switcher), None);
    // Push to talk is held: a letter alone is as before.
    host.record(Action::Voice(Shortcut::PushToTalk), "V");
    assert_eq!(host.ui.prefs.push_to_talk_key.as_deref(), Some("V"));
}

#[test]
fn reset_waits_while_another_action_took_the_default() {
    let mut host = Host::new();
    host.record(Action::Switcher, "Ctrl+J");
    host.record(Action::Search, "Ctrl+K");
    let intents = host.settings_click("us:key-reset=switcher");
    assert!(intents.is_empty());
    assert_eq!(
        host.notice(Action::Switcher),
        Some("Ya lo usa «Buscar en la conversación».")
    );
    // The quick switcher answers its new key, the search the old one's.
    host.ui.settings = None;
    host.open_channel("general");
    host.key(Key::K, false, true);
    assert!(host.ui.switcher.is_none());
    assert!(host.ui.search.is_some());
    host.key(Key::Escape, false, false);
    host.key(Key::J, false, true);
    assert!(host.ui.switcher.is_some());
}

#[test]
fn the_composer_sends_and_breaks_the_line_where_the_table_says() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.ui.prefs.shortcuts = vec![(Action::Send, Chord::ctrl(Key::Enter))];
    host.frame();
    let sends = |host: &Host| {
        host.sent
            .iter()
            .filter(|command| matches!(command, Command::SendMessage { .. }))
            .count()
    };
    let before = sends(&host);
    host.type_text("hola");
    // Enter no longer sends: it breaks the line.
    host.key(Key::Enter, false, false);
    assert_eq!(sends(&host), before);
    assert_eq!(
        host.ui.drafts.get(&general).map(String::as_str),
        Some("hola\n")
    );
    host.type_text("chau");
    host.key(Key::Enter, false, true);
    assert_eq!(sends(&host), before + 1, "Ctrl+Enter sends");
    // A send elsewhere, and a new line elsewhere.
    host.ui.prefs.shortcuts = vec![
        (Action::Send, Chord::ctrl(Key::S)),
        (Action::NewLine, Chord::ctrl(Key::L)),
    ];
    host.frame();
    host.type_text("uno");
    host.key(Key::L, false, true);
    host.type_text("dos");
    assert_eq!(
        host.ui.drafts.get(&general).map(String::as_str),
        Some("uno\ndos")
    );
    host.key(Key::S, false, true);
    assert_eq!(sends(&host), before + 2);
}

#[test]
fn formatting_and_editing_the_last_message_follow_their_keys() {
    let mut host = Host::new();
    let general = host.open_channel("general");
    host.confirm = true;
    host.type_text("mío");
    host.key(Key::Enter, false, false);
    host.frame();
    host.ui.prefs.shortcuts = vec![
        (Action::Bold, Chord::ctrl_shift(Key::B)),
        (Action::EditLast, Chord::ctrl(Key::ArrowUp)),
    ];
    // Up alone moves the caret; Ctrl+Up edits.
    host.key(Key::ArrowUp, false, false);
    assert_eq!(host.ui.editing, None);
    host.key(Key::ArrowUp, false, true);
    assert!(host.ui.editing.is_some());
    host.key(Key::Escape, false, false);
    host.frame();
    host.tree.focus(&editor_id(general));
    host.type_text("hola");
    host.frame();
    host.tree.set_input_selection(&editor_id(general), 0, 4);
    host.key(Key::B, false, true);
    assert_eq!(
        host.ui.drafts.get(&general).map(String::as_str),
        Some("hola"),
        "Ctrl+B is no longer bold"
    );
    host.frame();
    host.tree.set_input_selection(&editor_id(general), 0, 4);
    host.key(Key::B, true, true);
    assert_eq!(
        host.ui.drafts.get(&general).map(String::as_str),
        Some("**hola**")
    );
}
